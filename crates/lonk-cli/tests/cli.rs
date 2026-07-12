use std::process::Command;

fn lonk() -> Command {
  Command::new(env!("CARGO_BIN_EXE_lonk"))
}

#[test]
fn zero_args_prints_help_and_exits_1() {
  let out = lonk().output().expect("run lonk");
  assert_eq!(out.status.code(), Some(1));
  let text = String::from_utf8_lossy(&out.stderr);
  assert!(text.contains("Usage:"), "help not shown: {text}");
}

#[test]
fn dash_h_prints_help_and_exits_0() {
  let out = lonk().arg("-h").output().expect("run lonk");
  assert_eq!(out.status.code(), Some(0));
  assert!(String::from_utf8_lossy(&out.stdout).contains("Usage:"));
}

#[test]
fn valid_flag_reports_each_url_and_exit_code() {
  let out = lonk()
    .args([
      "--valid",
      "https://ok.example/x",
      "ftp://bad.example",
      "also not a url",
    ])
    .output()
    .expect("run lonk");
  assert_eq!(out.status.code(), Some(1));
  let stdout = String::from_utf8_lossy(&out.stdout);
  let lines: Vec<&str> = stdout.lines().collect();
  assert_eq!(lines.len(), 3);
  assert_eq!(lines[0], "valid");
  assert!(lines[1].starts_with("invalid: url scheme must be http or https"));
  assert!(lines[2].starts_with("invalid: invalid url:"));

  let out = lonk()
    .args(["--valid", "https://ok.example/x"])
    .output()
    .expect("run lonk");
  assert_eq!(out.status.code(), Some(0));
}

fn read_config(dir: &std::path::Path) -> String {
  std::fs::read_to_string(dir.join("config.toml")).expect("config.toml written")
}

#[test]
fn setup_writes_default_profile() {
  let dir = tempfile::tempdir().unwrap();
  let out = lonk()
    .env("LONK_CONFIG_DIR", dir.path())
    .args(["setup", "https://s.example.com/"])
    .output()
    .expect("run lonk setup");
  assert_eq!(
    out.status.code(),
    Some(0),
    "stderr: {}",
    String::from_utf8_lossy(&out.stderr)
  );
  let cfg = read_config(dir.path());
  assert!(cfg.contains("[profiles.default]"), "{cfg}");
  // trailing slash trimmed
  assert!(
    cfg.contains("base_url = \"https://s.example.com\""),
    "{cfg}"
  );
}

#[test]
fn setup_with_profile_flag_writes_named_profile() {
  let dir = tempfile::tempdir().unwrap();
  let out = lonk()
    .env("LONK_CONFIG_DIR", dir.path())
    .args(["setup", "--profile", "work", "https://links.corp.example"])
    .output()
    .expect("run lonk setup");
  assert_eq!(out.status.code(), Some(0));
  let cfg = read_config(dir.path());
  assert!(cfg.contains("[profiles.work]"), "{cfg}");
}

#[test]
fn setup_rejects_invalid_base_url() {
  let dir = tempfile::tempdir().unwrap();
  let out = lonk()
    .env("LONK_CONFIG_DIR", dir.path())
    .args(["setup", "ftp://s.example.com"])
    .output()
    .expect("run lonk setup");
  assert_eq!(out.status.code(), Some(1));
  assert!(String::from_utf8_lossy(&out.stderr).contains("scheme"));
}

#[test]
fn setup_rejects_base_url_with_query_or_fragment() {
  // endpoints append paths to the base, so ?query or #fragment would
  // corrupt every request the profile makes
  let dir = tempfile::tempdir().unwrap();
  for (bad, needle) in [
    ("https://s.example.com/?utm=x", "query"),
    ("https://s.example.com/#section", "fragment"),
  ] {
    let out = lonk()
      .env("LONK_CONFIG_DIR", dir.path())
      .args(["setup", bad])
      .output()
      .expect("run lonk setup");
    assert_eq!(out.status.code(), Some(1), "accepted {bad}");
    assert!(
      String::from_utf8_lossy(&out.stderr).contains(needle),
      "stderr for {bad} missing {needle:?}"
    );
    assert!(!dir.path().join("config.toml").exists(), "saved {bad}");
  }
}

#[test]
fn setup_accepts_base_url_with_path() {
  // reverse-proxy subpath deployments are legitimate base urls
  let dir = tempfile::tempdir().unwrap();
  let out = lonk()
    .env("LONK_CONFIG_DIR", dir.path())
    .args(["setup", "https://example.com/lonk/"])
    .output()
    .expect("run lonk setup");
  assert_eq!(out.status.code(), Some(0));
  let cfg = read_config(dir.path());
  assert!(
    cfg.contains("base_url = \"https://example.com/lonk\""),
    "{cfg}"
  );
}

#[test]
fn setup_without_url_and_without_tty_errors_with_hint() {
  let dir = tempfile::tempdir().unwrap();
  let out = lonk()
    .env("LONK_CONFIG_DIR", dir.path())
    .arg("setup")
    .stdin(std::process::Stdio::piped()) // not a TTY
    .output()
    .expect("run lonk setup");
  assert_eq!(out.status.code(), Some(1));
  assert!(String::from_utf8_lossy(&out.stderr).contains("lonk setup"));
}

#[test]
fn qr_flag_without_urls_is_a_usage_error() {
  let dir = tempfile::tempdir().unwrap();
  let out = lonk()
    .env("LONK_CONFIG_DIR", dir.path())
    .arg("--qr")
    .stdin(std::process::Stdio::piped()) // not a TTY; must not trigger setup prompt
    .output()
    .expect("run lonk");
  assert_eq!(out.status.code(), Some(1));
  assert!(
    String::from_utf8_lossy(&out.stderr).contains("URL"),
    "stderr: {}",
    String::from_utf8_lossy(&out.stderr)
  );
}

#[test]
fn setup_preserves_other_profiles() {
  let dir = tempfile::tempdir().unwrap();
  for args in [
    vec!["setup", "https://one.example"],
    vec!["setup", "--profile", "work", "https://two.example"],
  ] {
    let out = lonk()
      .env("LONK_CONFIG_DIR", dir.path())
      .args(&args)
      .output()
      .unwrap();
    assert_eq!(out.status.code(), Some(0));
  }
  let cfg = read_config(dir.path());
  assert!(
    cfg.contains("https://one.example") && cfg.contains("https://two.example"),
    "{cfg}"
  );
}

#[test]
fn invalid_header_flag_fails_fast_without_config() {
  let dir = tempfile::tempdir().unwrap();
  // no colon at all
  let out = lonk()
    .env("LONK_CONFIG_DIR", dir.path())
    .args(["-H", "no-colon-here", "https://example.com/x"])
    .output()
    .unwrap();
  assert_eq!(out.status.code(), Some(1));
  assert!(String::from_utf8_lossy(&out.stderr).contains("expected"));

  // denylisted header, rejected locally before any config/network use
  let out = lonk()
    .env("LONK_CONFIG_DIR", dir.path())
    .args([
      "-H",
      "Location: https://evil.example",
      "https://example.com/x",
    ])
    .output()
    .unwrap();
  assert_eq!(out.status.code(), Some(1));
  assert!(String::from_utf8_lossy(&out.stderr).contains("not allowed"));
  // config dir untouched proves fail-fast happened before setup/prompt logic
  assert!(!dir.path().join("config.toml").exists());
}
