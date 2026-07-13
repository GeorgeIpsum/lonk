use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const PORT: u16 = 8911;

struct ServerGuard(Child);

impl Drop for ServerGuard {
  fn drop(&mut self) {
    let _ = self.0.kill();
    let _ = self.0.wait();
  }
}

fn start_server(dbdir: &std::path::Path) -> ServerGuard {
  let child = Command::new(env!("CARGO_BIN_EXE_lonkd"))
    .env("LONK_DB", dbdir.join("status.db"))
    .env("ROCKET_ADDRESS", "127.0.0.1")
    .env("ROCKET_PORT", PORT.to_string())
    .env("ROCKET_LOG_LEVEL", "off")
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .spawn()
    .expect("spawn lonkd");
  let deadline = Instant::now() + Duration::from_secs(15);
  while Instant::now() < deadline {
    if TcpStream::connect(("127.0.0.1", PORT)).is_ok() {
      return ServerGuard(child);
    }
    std::thread::sleep(Duration::from_millis(100));
  }
  panic!("lonkd did not start listening on {PORT}");
}

fn create_link(base: &str, url: &str) -> String {
  let resp = ureq::post(&format!("{base}/api/links"))
    .send_json(serde_json::json!({ "url": url }))
    .expect("create link");
  let body: serde_json::Value = resp.into_json().expect("json");
  body["id"].as_str().expect("id").to_string()
}

// The spawned server is its own status target: its `/` is alive, its
// `/zzzzzzz` is a 404, and 127.0.0.1:1 refuses connections.
#[test]
fn status_reports_alive_and_dead_destinations() {
  let tmp = tempfile::tempdir().unwrap();
  let _server = start_server(tmp.path());
  let base = format!("http://127.0.0.1:{PORT}");

  let alive_id = create_link(&base, &format!("{base}/"));
  let body: serde_json::Value = ureq::get(&format!("{base}/{alive_id}/status"))
    .call()
    .expect("status call")
    .into_json()
    .expect("json");
  assert_eq!(body["alive"], true, "body: {body}");
  assert_eq!(body["http_status"], 200);
  assert!(body.get("error").is_none() || body["error"].is_null());

  let dead_id = create_link(&base, &format!("{base}/zzzzzzz"));
  let body: serde_json::Value = ureq::get(&format!("{base}/{dead_id}/status"))
    .call()
    .expect("status call")
    .into_json()
    .expect("json");
  assert_eq!(body["alive"], false);
  assert_eq!(body["http_status"], 404);

  let refused_id = create_link(&base, "http://127.0.0.1:1/");
  let body: serde_json::Value = ureq::get(&format!("{base}/{refused_id}/status"))
    .call()
    .expect("status call")
    .into_json()
    .expect("json");
  assert_eq!(body["alive"], false);
  assert!(body.get("http_status").is_none() || body["http_status"].is_null());
  assert!(!body["error"].as_str().expect("error").is_empty());
}
