use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const PORT: u16 = 8907;

struct ServerGuard(Child);

impl Drop for ServerGuard {
  fn drop(&mut self) {
    let _ = self.0.kill();
    let _ = self.0.wait();
  }
}

fn start_server(dbdir: &std::path::Path) -> ServerGuard {
  let child = Command::new(env!("CARGO_BIN_EXE_lonkd"))
    .env("LONK_DB", dbdir.join("live.db"))
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

fn lonk_with_config(dir: &std::path::Path) -> Command {
  let mut c = Command::new(env!("CARGO_BIN_EXE_lonk"));
  c.env("LONK_CONFIG_DIR", dir);
  c
}

fn base() -> String {
  format!("http://127.0.0.1:{PORT}")
}

// Single #[test] so the server/port is used by exactly one test at a time.
#[test]
fn shorten_end_to_end() {
  let tmp = tempfile::tempdir().unwrap();
  let _server = start_server(tmp.path());

  // setup
  let out = lonk_with_config(tmp.path())
    .args(["setup", &base()])
    .output()
    .unwrap();
  assert_eq!(out.status.code(), Some(0));

  // multiple URLs -> one short link per line, input order
  let out = lonk_with_config(tmp.path())
    .args(["https://example.com/first", "https://example.com/second"])
    .output()
    .unwrap();
  assert_eq!(
    out.status.code(),
    Some(0),
    "stderr: {}",
    String::from_utf8_lossy(&out.stderr)
  );
  let stdout = String::from_utf8_lossy(&out.stdout);
  let lines: Vec<&str> = stdout.lines().collect();
  assert_eq!(lines.len(), 2);
  for line in &lines {
    assert!(line.starts_with(&format!("{}/", base())), "line: {line}");
  }

  // a short link actually redirects to the original
  let resp = ureq::AgentBuilder::new()
    .redirects(0)
    .build()
    .get(lines[0])
    .call()
    .expect("GET short link");
  assert_eq!(resp.status(), 303);
  assert_eq!(resp.header("Location"), Some("https://example.com/first"));

  // --qr: link line, then a unicode qr block
  let out = lonk_with_config(tmp.path())
    .args(["--qr", "https://example.com/qr"])
    .output()
    .unwrap();
  assert_eq!(out.status.code(), Some(0));
  let stdout = String::from_utf8_lossy(&out.stdout);
  assert!(stdout
    .lines()
    .next()
    .unwrap()
    .starts_with(&format!("{}/", base())));
  assert!(
    stdout.contains('\u{2588}'),
    "no unicode blocks in: {stdout}"
  ); // █

  // --qr-svg: stdout is exactly an svg document
  let out = lonk_with_config(tmp.path())
    .args(["--qr-svg", "https://example.com/qrsvg"])
    .output()
    .unwrap();
  assert_eq!(out.status.code(), Some(0));
  let stdout = String::from_utf8_lossy(&out.stdout);
  assert!(stdout.trim_start().starts_with("<?xml") || stdout.trim_start().starts_with("<svg"));
  assert!(String::from_utf8_lossy(&out.stderr).contains(&format!("{}/", base())));

  // --qr-svg with two urls is a usage error
  let out = lonk_with_config(tmp.path())
    .args(["--qr-svg", "https://a.example/1", "https://a.example/2"])
    .output()
    .unwrap();
  assert_eq!(out.status.code(), Some(1));

  // fail fast on invalid input: nothing shortened, exit 1
  let out = lonk_with_config(tmp.path())
    .args(["ftp://nope.example", "https://example.com/never-sent"])
    .output()
    .unwrap();
  assert_eq!(out.status.code(), Some(1));
  assert!(out.stdout.is_empty());
  assert!(String::from_utf8_lossy(&out.stderr).contains("scheme"));

  // unknown profile: exit 1, mentions the profile name
  let out = lonk_with_config(tmp.path())
    .args(["--profile", "nope", "https://example.com/x"])
    .output()
    .unwrap();
  assert_eq!(out.status.code(), Some(1));
  assert!(String::from_utf8_lossy(&out.stderr).contains("nope"));
}

#[test]
fn shorten_unconfigured_without_tty_hints_setup() {
  let tmp = tempfile::tempdir().unwrap();
  let out = lonk_with_config(tmp.path())
    .arg("https://example.com/x")
    .stdin(Stdio::piped())
    .output()
    .unwrap();
  assert_eq!(out.status.code(), Some(1));
  assert!(String::from_utf8_lossy(&out.stderr).contains("lonk setup"));
}

#[test]
fn shorten_with_trailing_slash_config_base_url() {
  let tmp = tempfile::tempdir().unwrap();
  let _server = start_server(tmp.path());

  // Hand-write a config with a trailing slash on base_url, bypassing
  // `setup`'s own normalization, so this exercises the read path in
  // resolve_base_url. If that path didn't trim the slash, the CLI would
  // POST to "{base}//api/links", which doesn't match lonkd's route and
  // fails with a network/server error (exit 2) instead of succeeding.
  std::fs::write(
    tmp.path().join("config.toml"),
    format!("[profiles.default]\nbase_url = \"{}/\"\n", base()),
  )
  .unwrap();

  let out = lonk_with_config(tmp.path())
    .arg("https://example.com/trailing-slash-config")
    .output()
    .unwrap();
  assert_eq!(
    out.status.code(),
    Some(0),
    "stderr: {}",
    String::from_utf8_lossy(&out.stderr)
  );
  let stdout = String::from_utf8_lossy(&out.stdout);
  let short = stdout.trim();
  assert!(
    short.starts_with(&format!("{}/", base())) && !short.contains("//api/"),
    "short url: {short}"
  );

  // the short link must actually resolve, proving the create request landed
  // on the real /api/links route rather than a doubled-slash 404.
  let resp = ureq::AgentBuilder::new()
    .redirects(0)
    .build()
    .get(short)
    .call()
    .expect("GET short link");
  assert_eq!(resp.status(), 303);
  assert_eq!(
    resp.header("Location"),
    Some("https://example.com/trailing-slash-config")
  );
}

#[test]
fn shorten_with_server_down_is_exit_2() {
  let tmp = tempfile::tempdir().unwrap();
  // configure a base url where nothing listens (server NOT started)
  let out = lonk_with_config(tmp.path())
    .args(["setup", "http://127.0.0.1:8996"])
    .output()
    .unwrap();
  assert_eq!(out.status.code(), Some(0));
  let out = lonk_with_config(tmp.path())
    .arg("https://example.com/x")
    .output()
    .unwrap();
  assert_eq!(out.status.code(), Some(2));
}
