use rocket::http::Status;
use rocket::response::status;
use rocket::serde::json::Json;
use rocket::serde::{Deserialize, Serialize};
use rocket::State;
use url::Url;

use crate::db::{gen_slug, Db};

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

#[derive(Deserialize)]
#[serde(crate = "rocket::serde")]
pub struct CreateReq {
  url: String,
}

#[derive(Serialize)]
#[serde(crate = "rocket::serde")]
pub struct LinkResp {
  id: String,
  url: String,
  short_url: String,
  qr_url: String,
}

#[rocket::post("/api/links", data = "<body>")]
pub fn create_link(
  db: &State<Db>,
  body: Json<CreateReq>,
) -> Result<status::Created<Json<LinkResp>>, ApiError> {
  let parsed = Url::parse(&body.url)
    .map_err(|e| api_error(Status::BadRequest, &format!("invalid url: {e}")))?;
  if !matches!(parsed.scheme(), "http" | "https") {
    return Err(api_error(
      Status::BadRequest,
      "url scheme must be http or https",
    ));
  }
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

#[rocket::catch(default)]
pub fn default_catcher(status: Status, _req: &rocket::Request<'_>) -> Json<ErrorBody> {
  Json(ErrorBody {
    error: status.reason_lossy().to_string(),
  })
}
