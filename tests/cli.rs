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
