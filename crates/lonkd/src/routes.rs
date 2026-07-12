use qrcode::render::svg;
use qrcode::QrCode;
use rocket::http::uri::Host;
use rocket::http::{ContentType, Status};
use rocket::response::{status, Redirect};
use rocket::serde::json::Json;
use rocket::serde::Serialize;
use rocket::State;

use crate::db::{gen_slug, Db};
use lonk_core::types::{CreateLinkReq, LinkResp, ValidResp};

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

#[rocket::post("/api/links", data = "<body>")]
pub fn create_link(
  db: &State<Db>,
  body: Json<CreateLinkReq>,
) -> Result<status::Created<Json<LinkResp>>, ApiError> {
  let parsed = lonk_core::validate_url(&body.url)
    .map_err(|e| api_error(Status::BadRequest, &e.to_string()))?;
  for _ in 0..8 {
    let id = gen_slug(7);
    if db.insert(&id, parsed.as_str()).map_err(db_error)? {
      let resp = LinkResp {
        short_url: format!("/{id}"),
        qr_url: format!("/{id}/qr"),
        url: parsed.into(),
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
pub fn follow_link(db: &State<Db>, id: &str) -> Result<Redirect, ApiError> {
  match db.get_url(id).map_err(db_error)? {
    Some(url) => Ok(Redirect::to(url)),
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
