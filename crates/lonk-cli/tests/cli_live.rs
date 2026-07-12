use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

// Tests in this binary run concurrently under the default harness, so each
// test that spawns a server must own a unique port: two lonkd processes
// racing to bind the same port would leave the loser silently unbound
// (stderr is nulled) while its readiness poll connects to the winner,
// letting the test pass by luck against the wrong server/DB.
const PORT_END_TO_END: u16 = 8907;
const PORT_TRAILING_SLASH: u16 = 8908;
const PORT_HEADERS: u16 = 8909;

struct ServerGuard(Child);

impl Drop for ServerGuard {
  fn drop(&mut self) {
    let _ = self.0.kill();
    let _ = self.0.wait();
  }
}

fn lonkd_bin() -> std::path::PathBuf {
  // test executables live in target/debug/deps/; the workspace's binaries in target/debug/
  let mut dir = std::env::current_exe().expect("test exe path");
  dir.pop(); // deps/
  dir.pop(); // debug/
  let bin = dir.join(format!("lonkd{}", std::env::consts::EXE_SUFFIX));
  assert!(
    bin.exists(),
    "lonkd binary not found at {} - run: cargo test --workspace (or cargo build -p lonkd)",
    bin.display()
  );
  bin
}

fn start_server(dbdir: &std::path::Path, port: u16) -> ServerGuard {
  let child = Command::new(lonkd_bin())
    .env("LONK_DB", dbdir.join("live.db"))
    .env("ROCKET_ADDRESS", "127.0.0.1")
    .env("ROCKET_PORT", port.to_string())
    .env("ROCKET_LOG_LEVEL", "off")
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .spawn()
    .expect("spawn lonkd");
  let deadline = Instant::now() + Duration::from_secs(15);
  while Instant::now() < deadline {
    if TcpStream::connect(("127.0.0.1", port)).is_ok() {
      return ServerGuard(child);
    }
    std::thread::sleep(Duration::from_millis(100));
  }
  panic!("lonkd did not start listening on {port}");
}

fn lonk_with_config(dir: &std::path::Path) -> Command {
  let mut c = Command::new(env!("CARGO_BIN_EXE_lonk"));
  c.env("LONK_CONFIG_DIR", dir);
  c
}

fn base(port: u16) -> String {
  format!("http://127.0.0.1:{port}")
}

// Owns PORT_STATUS: it must spawn its own server, and sharing the
// end-to-end test's port would race under the parallel default harness.
#[test]
fn status_subcommand_end_to_end() {
  const PORT_STATUS: u16 = 8910;
  let tmp = tempfile::tempdir().unwrap();
  let _server = start_server(tmp.path(), PORT_STATUS);
  let base = base(PORT_STATUS);

  let out = lonk_with_config(tmp.path())
    .args(["setup", &base])
    .output()
    .unwrap();
  assert_eq!(out.status.code(), Some(0));

  // a link whose destination is the server's own index page: alive
  let out = lonk_with_config(tmp.path())
    .arg(format!("{base}/"))
    .output()
    .unwrap();
  assert_eq!(out.status.code(), Some(0));
  let short = String::from_utf8_lossy(&out.stdout).trim().to_string();
  let slug = short.rsplit('/').next().unwrap().to_string();

  // by slug
  let out = lonk_with_config(tmp.path())
    .args(["status", &slug])
    .output()
    .unwrap();
  assert_eq!(
    out.status.code(),
    Some(0),
    "stderr: {}",
    String::from_utf8_lossy(&out.stderr)
  );
  assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "alive (200)");

  // by full short URL
  let out = lonk_with_config(tmp.path())
    .args(["status", &short])
    .output()
    .unwrap();
  assert_eq!(out.status.code(), Some(0));
  assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "alive (200)");

  // dead destination: the server's own 404 page
  let out = lonk_with_config(tmp.path())
    .arg(format!("{base}/zzzzzzz"))
    .output()
    .unwrap();
  let dead_short = String::from_utf8_lossy(&out.stdout).trim().to_string();
  let dead_slug = dead_short.rsplit('/').next().unwrap().to_string();
  let out = lonk_with_config(tmp.path())
    .args(["status", &dead_slug])
    .output()
    .unwrap();
  assert_eq!(out.status.code(), Some(1));
  assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "dead (404)");

  // unknown slug
  let out = lonk_with_config(tmp.path())
    .args(["status", "zzzzzzz"])
    .output()
    .unwrap();
  assert_eq!(out.status.code(), Some(1));
  assert!(String::from_utf8_lossy(&out.stderr).contains("zzzzzzz"));
}

// All end-to-end scenarios against one server share this single #[test];
// its port (PORT_END_TO_END) is owned by this test alone.
#[test]
fn shorten_end_to_end() {
  let base = base(PORT_END_TO_END);
  let tmp = tempfile::tempdir().unwrap();
  let _server = start_server(tmp.path(), PORT_END_TO_END);

  // setup
  let out = lonk_with_config(tmp.path())
    .args(["setup", &base])
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
    assert!(line.starts_with(&format!("{base}/")), "line: {line}");
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
    .starts_with(&format!("{base}/")));
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
  assert!(String::from_utf8_lossy(&out.stderr).contains(&format!("{base}/")));

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

// Owns PORT_TRAILING_SLASH: it must spawn its own server, and sharing the
// end-to-end test's port would race under the parallel default harness.
#[test]
fn shorten_with_trailing_slash_config_base_url() {
  let base = base(PORT_TRAILING_SLASH);
  let tmp = tempfile::tempdir().unwrap();
  let _server = start_server(tmp.path(), PORT_TRAILING_SLASH);

  // Hand-write a config with a trailing slash on base_url, bypassing
  // `setup`'s own normalization, so this exercises the read path in
  // resolve_base_url. If that path didn't trim the slash, the CLI would
  // POST to "{base}//api/links", which doesn't match lonkd's route and
  // fails with a network/server error (exit 2) instead of succeeding.
  std::fs::write(
    tmp.path().join("config.toml"),
    format!("[profiles.default]\nbase_url = \"{base}/\"\n"),
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
    short.starts_with(&format!("{base}/")) && !short.contains("//api/"),
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

#[test]
fn shorten_with_headers_end_to_end() {
  let tmp = tempfile::tempdir().unwrap();
  let _server = start_server(tmp.path(), PORT_HEADERS);
  let base = base(PORT_HEADERS);

  let out = lonk_with_config(tmp.path())
    .args(["setup", &base])
    .output()
    .unwrap();
  assert_eq!(out.status.code(), Some(0));

  let out = lonk_with_config(tmp.path())
    .args([
      "-H",
      "X-Demo: 1",
      "-H",
      "Set-Cookie: a=1",
      "-H",
      "Set-Cookie: b=2",
      "https://example.com/with-headers",
    ])
    .output()
    .unwrap();
  assert_eq!(
    out.status.code(),
    Some(0),
    "stderr: {}",
    String::from_utf8_lossy(&out.stderr)
  );
  let stdout = String::from_utf8_lossy(&out.stdout);
  let short = stdout.lines().next().unwrap();

  let resp = ureq::AgentBuilder::new()
    .redirects(0)
    .build()
    .get(short)
    .call()
    .expect("GET short link");
  assert_eq!(resp.status(), 303);
  assert_eq!(resp.header("X-Demo"), Some("1"));
  let cookies: Vec<&str> = resp.all("Set-Cookie");
  assert_eq!(cookies, vec!["a=1", "b=2"]);
}
