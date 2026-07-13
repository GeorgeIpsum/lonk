use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

// Port registry: 8912 belongs to this file. One server alive at a time.
const PORT: u16 = 8912;

struct ServerGuard(Child);

impl Drop for ServerGuard {
  fn drop(&mut self) {
    let _ = self.0.kill();
    let _ = self.0.wait();
  }
}

fn start_server(dbdir: &std::path::Path, web_dir: Option<&std::path::Path>) -> ServerGuard {
  let mut cmd = Command::new(env!("CARGO_BIN_EXE_lonkd"));
  cmd
    .env("LONK_DB", dbdir.join("web.db"))
    .env("ROCKET_ADDRESS", "127.0.0.1")
    .env("ROCKET_PORT", PORT.to_string())
    .env("ROCKET_LOG_LEVEL", "off")
    .env_remove("LONK_WEB_DIR")
    .stdout(Stdio::null())
    .stderr(Stdio::null());
  if let Some(dir) = web_dir {
    cmd.env("LONK_WEB_DIR", dir);
  }
  let child = cmd.spawn().expect("spawn lonkd");
  let deadline = Instant::now() + Duration::from_secs(15);
  while Instant::now() < deadline {
    if TcpStream::connect(("127.0.0.1", PORT)).is_ok() {
      return ServerGuard(child);
    }
    std::thread::sleep(Duration::from_millis(100));
  }
  panic!("lonkd did not start listening on {PORT}");
}

fn get_index() -> String {
  ureq::get(&format!("http://127.0.0.1:{PORT}/"))
    .call()
    .expect("GET /")
    .into_string()
    .expect("body")
}

#[test]
fn web_dir_overrides_embedded_ui() {
  let tmp = tempfile::tempdir().unwrap();
  let custom = tmp.path().join("custom-ui");
  std::fs::create_dir(&custom).unwrap();
  std::fs::write(
    custom.join("index.html"),
    "<!DOCTYPE html><html><body>custom-ui-marker</body></html>",
  )
  .unwrap();
  std::fs::write(custom.join("extra.css"), "body{}").unwrap();

  // override: serves the bespoke directory
  {
    let _server = start_server(tmp.path(), Some(&custom));
    let body = get_index();
    assert!(body.contains("custom-ui-marker"), "body: {body}");
    let css = ureq::get(&format!("http://127.0.0.1:{PORT}/extra.css"))
      .call()
      .expect("GET css");
    let content_type = css.header("Content-Type").expect("content-type header");
    assert!(content_type.starts_with("text/css"), "got {content_type}");
  } // ServerGuard dropped -> port free

  // default: embedded UI
  let _server = start_server(tmp.path(), None);
  let body = get_index();
  assert!(body.contains("id=\"url\""), "body: {body}");
}
