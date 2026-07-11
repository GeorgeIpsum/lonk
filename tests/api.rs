use rocket::http::{ContentType, Status};
use rocket::local::blocking::Client;

fn client() -> Client {
  Client::tracked(lonk::rocket_app(":memory:")).expect("valid rocket instance")
}

#[test]
fn create_link_returns_201_with_expected_shape() {
  let client = client();
  let res = client
    .post("/api/links")
    .header(ContentType::JSON)
    .body(r#"{"url": "https://example.com/some/long/path"}"#)
    .dispatch();
  assert_eq!(res.status(), Status::Created);
  let body: serde_json::Value = res.into_json().expect("json body");
  let id = body["id"].as_str().expect("id is a string");
  assert_eq!(id.len(), 7);
  assert_eq!(body["url"], "https://example.com/some/long/path");
  assert_eq!(body["short_url"], format!("/{id}"));
  assert_eq!(body["qr_url"], format!("/{id}/qr"));
}

#[test]
fn create_link_rejects_unparseable_url() {
  let client = client();
  let res = client
    .post("/api/links")
    .header(ContentType::JSON)
    .body(r#"{"url": "not a url"}"#)
    .dispatch();
  assert_eq!(res.status(), Status::BadRequest);
  let body: serde_json::Value = res.into_json().expect("json body");
  assert!(!body["error"].as_str().expect("error message").is_empty());
}

#[test]
fn redirect_round_trip() {
  let client = client();
  let res = client
    .post("/api/links")
    .header(ContentType::JSON)
    .body(r#"{"url": "https://example.com/target"}"#)
    .dispatch();
  let body: serde_json::Value = res.into_json().expect("json body");
  let short_url = body["short_url"].as_str().unwrap().to_string();

  let res = client.get(&short_url).dispatch();
  assert_eq!(res.status(), Status::SeeOther);
  assert_eq!(
    res.headers().get_one("Location"),
    Some("https://example.com/target")
  );
}

#[test]
fn redirect_unknown_id_is_404() {
  let client = client();
  let res = client.get("/zzzzzzz").dispatch();
  assert_eq!(res.status(), Status::NotFound);
  let body: serde_json::Value = res.into_json().expect("json body");
  assert!(body["error"].as_str().is_some());
}

#[test]
fn create_link_rejects_non_http_scheme() {
  let client = client();
  let res = client
    .post("/api/links")
    .header(ContentType::JSON)
    .body(r#"{"url": "ftp://example.com/file"}"#)
    .dispatch();
  assert_eq!(res.status(), Status::BadRequest);
}
