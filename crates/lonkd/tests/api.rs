use rocket::http::{ContentType, Header, Status};
use rocket::local::blocking::Client;

fn client() -> Client {
  Client::tracked(lonkd::rocket_app(":memory:")).expect("valid rocket instance")
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
fn qr_returns_svg_for_known_id() {
  let client = client();
  let res = client
    .post("/api/links")
    .header(ContentType::JSON)
    .body(r#"{"url": "https://example.com/qr-me"}"#)
    .dispatch();
  let body: serde_json::Value = res.into_json().expect("json body");
  let qr_url = body["qr_url"].as_str().unwrap().to_string();

  let res = client
    .get(&qr_url)
    .header(Header::new("Host", "lonk.example"))
    .dispatch();
  assert_eq!(res.status(), Status::Ok);
  assert_eq!(res.content_type(), Some(ContentType::SVG));
  let svg = res.into_string().expect("svg body");
  assert!(svg.contains("<svg"));
}

#[test]
fn qr_unknown_id_is_404() {
  let client = client();
  let res = client.get("/zzzzzzz/qr").dispatch();
  assert_eq!(res.status(), Status::NotFound);
}

#[test]
fn index_serves_create_form() {
  let client = client();
  let res = client.get("/").dispatch();
  assert_eq!(res.status(), Status::Ok);
  assert_eq!(res.content_type(), Some(ContentType::HTML));
  let html = res.into_string().expect("html body");
  assert!(html.contains("id=\"url\""));
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

#[test]
fn valid_endpoint_accepts_good_url() {
  let client = client();
  let res = client
    .post("/api/valid")
    .header(ContentType::JSON)
    .body(r#"{"url": "https://example.com/ok"}"#)
    .dispatch();
  assert_eq!(res.status(), Status::Ok);
  let body: serde_json::Value = res.into_json().expect("json body");
  assert_eq!(body["valid"], true);
  assert!(body.get("error").is_none() || body["error"].is_null());
}

#[test]
fn valid_endpoint_rejects_bad_url() {
  let client = client();
  for bad in [
    r#"{"url": "not a url"}"#,
    r#"{"url": "ftp://example.com/f"}"#,
  ] {
    let res = client
      .post("/api/valid")
      .header(ContentType::JSON)
      .body(bad)
      .dispatch();
    assert_eq!(res.status(), Status::Ok);
    let body: serde_json::Value = res.into_json().expect("json body");
    assert_eq!(body["valid"], false);
    assert!(!body["error"].as_str().expect("error message").is_empty());
  }
}

#[test]
fn create_link_with_headers_echoes_and_redirect_carries_them() {
  let client = client();
  let res = client
    .post("/api/links")
    .header(ContentType::JSON)
    .body(
      r#"{"url": "https://example.com/hdr",
          "headers": [["Set-Cookie","a=1"],["Set-Cookie","b=2"],["X-Track","yes"]]}"#,
    )
    .dispatch();
  assert_eq!(res.status(), Status::Created);
  let body: serde_json::Value = res.into_json().expect("json body");
  assert_eq!(body["headers"][0][0], "Set-Cookie");
  let short_url = body["short_url"].as_str().unwrap().to_string();

  let res = client.get(&short_url).dispatch();
  assert_eq!(res.status(), Status::SeeOther);
  assert_eq!(
    res.headers().get_one("Location"),
    Some("https://example.com/hdr")
  );
  let cookies: Vec<_> = res.headers().get("Set-Cookie").collect();
  assert_eq!(cookies, vec!["a=1", "b=2"]);
  assert_eq!(res.headers().get_one("X-Track"), Some("yes"));
}

#[test]
fn create_link_without_headers_redirect_is_plain() {
  let client = client();
  let res = client
    .post("/api/links")
    .header(ContentType::JSON)
    .body(r#"{"url": "https://example.com/plain"}"#)
    .dispatch();
  let body: serde_json::Value = res.into_json().expect("json body");
  assert_eq!(body["headers"].as_array().unwrap().len(), 0);
  let res = client.get(body["short_url"].as_str().unwrap()).dispatch();
  assert_eq!(res.status(), Status::SeeOther);
  assert_eq!(res.headers().get_one("X-Track"), None);
}

#[test]
fn create_link_rejects_invalid_headers() {
  let client = client();
  let cases = [
    r#"{"url":"https://e.com/x","headers":[["Bad Name","v"]]}"#,
    r#"{"url":"https://e.com/x","headers":[["X-Ok","a\r\nInjected: yes"]]}"#,
    r#"{"url":"https://e.com/x","headers":[["Location","https://evil.example"]]}"#,
  ];
  for case in cases {
    let res = client
      .post("/api/links")
      .header(ContentType::JSON)
      .body(case)
      .dispatch();
    assert_eq!(res.status(), Status::BadRequest, "accepted: {case}");
  }
}

#[test]
fn create_link_rejects_too_many_headers() {
  let client = client();
  let headers: Vec<String> = (0..17).map(|i| format!(r#"["X-H{i}","v"]"#)).collect();
  let body = format!(
    r#"{{"url":"https://e.com/x","headers":[{}]}}"#,
    headers.join(",")
  );
  let res = client
    .post("/api/links")
    .header(ContentType::JSON)
    .body(body)
    .dispatch();
  assert_eq!(res.status(), Status::BadRequest);
  let body: serde_json::Value = res.into_json().expect("json body");
  assert!(body["error"].as_str().unwrap().contains("max 16"));
}
