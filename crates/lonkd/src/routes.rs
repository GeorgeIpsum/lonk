use qrcode::render::svg;
use qrcode::QrCode;
use rocket::http::uri::Host;
use rocket::http::{ContentType, Status};
use rocket::response::status;
use rocket::serde::json::Json;
use rocket::serde::Serialize;
use rocket::State;

use crate::db::{gen_slug, Db};
use lonk_core::types::{CreateLinkReq, LinkResp, StatusResp, ValidResp};

#[derive(Serialize)]
#[serde(crate = "rocket::serde")]
pub struct ErrorBody {
  pub error: String,
}

pub type ApiError = status::Custom<Json<ErrorBody>>;

pub fn api_error(status: Status, msg: &str) -> ApiError {
  status::Custom(
    status,
    Json(ErrorBody {
      error: msg.to_string(),
    }),
  )
}

fn db_error(_: rusqlite::Error) -> ApiError {
  api_error(Status::InternalServerError, "database error")
}

/// 303 redirect that also carries a link's stored custom response headers.
pub struct RedirectWithHeaders {
  location: String,
  headers: Vec<(String, String)>,
}

impl<'r> rocket::response::Responder<'r, 'static> for RedirectWithHeaders {
  fn respond_to(self, _req: &'r rocket::Request<'_>) -> rocket::response::Result<'static> {
    let mut builder = rocket::Response::build();
    builder
      .status(Status::SeeOther)
      .raw_header("Location", self.location);
    for (name, value) in self.headers {
      builder.header_adjoin(rocket::http::Header::new(name, value));
    }
    Ok(builder.finalize())
  }
}

#[rocket::post("/api/links", data = "<body>")]
pub fn create_link(
  db: &State<Db>,
  body: Json<CreateLinkReq>,
) -> Result<status::Created<Json<LinkResp>>, ApiError> {
  let body = body.into_inner();
  let parsed = lonk_core::validate_url(&body.url)
    .map_err(|e| api_error(Status::BadRequest, &e.to_string()))?;
  if body.headers.len() > lonk_core::MAX_HEADERS_PER_LINK {
    return Err(api_error(
      Status::BadRequest,
      &format!("too many headers (max {})", lonk_core::MAX_HEADERS_PER_LINK),
    ));
  }
  for (name, value) in &body.headers {
    lonk_core::validate_header(name, value)
      .map_err(|e| api_error(Status::BadRequest, &e.to_string()))?;
  }
  let headers_json = serde_json::to_string(&body.headers)
    .map_err(|_| api_error(Status::InternalServerError, "header encoding failed"))?;
  for _ in 0..8 {
    let id = gen_slug(7);
    if db
      .insert(&id, parsed.as_str(), &headers_json)
      .map_err(db_error)?
    {
      let resp = LinkResp {
        short_url: format!("/{id}"),
        qr_url: format!("/{id}/qr"),
        url: parsed.into(),
        headers: body.headers.clone(),
        id,
      };
      return Ok(status::Created::new(resp.short_url.clone()).body(Json(resp)));
    }
  }
  Err(api_error(
    Status::InternalServerError,
    "could not allocate a unique id",
  ))
}

#[rocket::get("/<id>")]
pub fn follow_link(db: &State<Db>, id: &str) -> Result<RedirectWithHeaders, ApiError> {
  match db.get_link(id).map_err(db_error)? {
    Some((url, headers)) => Ok(RedirectWithHeaders {
      location: url,
      headers,
    }),
    None => Err(api_error(Status::NotFound, "no such link")),
  }
}

#[rocket::get("/<id>/qr")]
pub fn qr_svg(
  db: &State<Db>,
  id: &str,
  host: Option<&Host<'_>>,
  proto: ForwardedProto,
) -> Result<(ContentType, String), ApiError> {
  if db.get_url(id).map_err(db_error)?.is_none() {
    return Err(api_error(Status::NotFound, "no such link"));
  }
  let host = host.map_or_else(|| "localhost:8000".to_string(), |h| h.to_string());
  let short = format!("{}://{}/{}", proto.0, host, id);
  let code = QrCode::new(short.as_bytes())
    .map_err(|_| api_error(Status::InternalServerError, "qr encoding failed"))?;
  let image = code.render::<svg::Color>().min_dimensions(256, 256).build();
  Ok((ContentType::SVG, image))
}

#[rocket::post("/api/valid", data = "<body>")]
pub fn valid_url(body: Json<CreateLinkReq>) -> Json<ValidResp> {
  match lonk_core::validate_url(&body.url) {
    Ok(_) => Json(ValidResp {
      valid: true,
      error: None,
    }),
    Err(e) => Json(ValidResp {
      valid: false,
      error: Some(e.to_string()),
    }),
  }
}

/// Live destination check: (alive, final http status if any, transport error if any).
/// HEAD with a one-shot GET retry on 405/501; <=5 redirects; 5s timeout; alive = final 2xx.
fn check_destination(url: &str) -> (bool, Option<u16>, Option<String>) {
  let agent = ureq::AgentBuilder::new()
    .redirects(5)
    .timeout(std::time::Duration::from_secs(5))
    .build();
  let result = match agent.head(url).call() {
    Err(ureq::Error::Status(405 | 501, _)) => agent.get(url).call(),
    other => other,
  };
  match result {
    Ok(resp) => (resp.status() / 100 == 2, Some(resp.status()), None),
    Err(ureq::Error::Status(code, _)) => (false, Some(code), None),
    Err(ureq::Error::Transport(t)) => (false, None, Some(t.to_string())),
  }
}

#[rocket::get("/<id>/status")]
pub async fn link_status(db: &State<Db>, id: &str) -> Result<Json<StatusResp>, ApiError> {
  let url = match db.get_url(id).map_err(db_error)? {
    Some(url) => url,
    None => return Err(api_error(Status::NotFound, "no such link")),
  };
  let check_url = url.clone();
  // ureq is blocking; keep the 5s worst case off Rocket's async workers.
  let (alive, http_status, error) =
    rocket::tokio::task::spawn_blocking(move || check_destination(&check_url))
      .await
      .map_err(|_| api_error(Status::InternalServerError, "status check failed"))?;
  Ok(Json(StatusResp {
    id: id.to_string(),
    url,
    alive,
    http_status,
    error,
  }))
}

/// "https" when a proxy says so via X-Forwarded-Proto, else "http".
pub struct ForwardedProto(pub &'static str);

#[rocket::async_trait]
impl<'r> rocket::request::FromRequest<'r> for ForwardedProto {
  type Error = std::convert::Infallible;

  async fn from_request(
    req: &'r rocket::Request<'_>,
  ) -> rocket::request::Outcome<Self, Self::Error> {
    let proto = match req.headers().get_one("X-Forwarded-Proto") {
      Some("https") => "https",
      _ => "http",
    };
    rocket::request::Outcome::Success(ForwardedProto(proto))
  }
}

#[rocket::catch(default)]
pub fn default_catcher(status: Status, _req: &rocket::Request<'_>) -> Json<ErrorBody> {
  Json(ErrorBody {
    error: status.reason_lossy().to_string(),
  })
}
