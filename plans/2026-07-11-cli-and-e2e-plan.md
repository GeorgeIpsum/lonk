# lonk CLI + Playwright e2e Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `lonk` CLI client (one-time setup, profiles, QR output) and Playwright e2e tests to the lonk link shortener, per `docs/superpowers/specs/2026-07-11-cli-and-e2e-design.md`.

**Architecture:** The repo becomes a Cargo workspace: the existing `lonk` package keeps the server (binary renamed `lonkd`) and gains a second binary `lonk` (the CLI, under `src/cli/`); a new `crates/lonk-validate` crate holds URL validation shared by server and CLI. A new `e2e/` npm project drives the real server with Playwright.

**Tech Stack:** Rust (Rocket 0.5, rusqlite, qrcode, clap 4, ureq 2, toml), Playwright (`@playwright/test`, TypeScript, Chromium).

## Global Constraints

- Rust edition 2021; **2-space indentation** (repo has `rustfmt.toml`; run `cargo fmt` before every commit).
- Server binary is `lonkd` (from Task 4 on, launch with `cargo run --bin lonkd`); CLI binary is `lonk`.
- Config dir resolution (shared, exact order): `$LONK_CONFIG_DIR` → `$XDG_CONFIG_HOME/lonk` → `~/.config/lonk`.
- CLI exit codes: `0` success, `1` validation/usage/config errors, `2` network/server errors.
- CLI URL args are sent **as-is** (no percent-decoding); all args validated locally via `lonk-validate` before any network call; **fail fast** on first error.
- `POST /api/valid` always returns HTTP 200 for well-formed JSON; invalidity is `{"valid": false, "error": "..."}`.
- Validation rule (single source of truth in `lonk-validate`): parseable by `url::Url` AND scheme is `http` or `https`.
- Tests must not touch the real `~/.config/lonk` — always set `LONK_CONFIG_DIR` and `LONK_DB` in test processes.

---

### Task 1: Workspace + `lonk-validate` crate

**Files:**
- Modify: `Cargo.toml` (add workspace + dependency)
- Create: `crates/lonk-validate/Cargo.toml`
- Create: `crates/lonk-validate/src/lib.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: `lonk_validate::validate_url(input: &str) -> Result<url::Url, ValidateError>`; `ValidateError` implements `Display` + `std::error::Error` (messages: `invalid url: <parse error>` / `url scheme must be http or https (got <scheme>)`). Also re-exports `pub use url::Url;`.

- [ ] **Step 1: Declare the workspace and dependency**

In `Cargo.toml`, append to the bottom:

```toml
[workspace]
members = ["crates/lonk-validate"]
```

and add to `[dependencies]`:

```toml
lonk-validate = { path = "crates/lonk-validate" }
```

- [ ] **Step 2: Create the crate with a failing test**

`crates/lonk-validate/Cargo.toml`:

```toml
[package]
name = "lonk-validate"
version = "0.1.0"
edition = "2021"

[dependencies]
url = "2"
```

`crates/lonk-validate/src/lib.rs`:

```rust
use std::fmt;

pub use url::Url;

#[derive(Debug, PartialEq, Eq)]
pub enum ValidateError {
  Parse(String),
  Scheme(String),
}

impl fmt::Display for ValidateError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      ValidateError::Parse(msg) => write!(f, "invalid url: {msg}"),
      ValidateError::Scheme(s) => write!(f, "url scheme must be http or https (got {s})"),
    }
  }
}

impl std::error::Error for ValidateError {}

/// The single source of truth for what lonk accepts as a shortenable URL.
pub fn validate_url(input: &str) -> Result<Url, ValidateError> {
  todo!()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn accepts_http_and_https() {
    assert!(validate_url("http://example.com/a").is_ok());
    assert!(validate_url("https://example.com/a?b=c&d=e").is_ok());
  }

  #[test]
  fn rejects_unparseable() {
    match validate_url("not a url") {
      Err(ValidateError::Parse(msg)) => assert!(!msg.is_empty()),
      other => panic!("expected Parse error, got {other:?}"),
    }
  }

  #[test]
  fn rejects_non_http_scheme() {
    match validate_url("ftp://example.com/f") {
      Err(ValidateError::Scheme(s)) => assert_eq!(s, "ftp"),
      other => panic!("expected Scheme error, got {other:?}"),
    }
  }

  #[test]
  fn error_messages_are_human_readable() {
    let e = validate_url("ftp://x.example").unwrap_err();
    assert_eq!(e.to_string(), "url scheme must be http or https (got ftp)");
  }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p lonk-validate`
Expected: FAIL — panics with `not yet implemented` (the `todo!()`).

- [ ] **Step 4: Implement `validate_url`**

Replace the `todo!()` body:

```rust
pub fn validate_url(input: &str) -> Result<Url, ValidateError> {
  let parsed = Url::parse(input).map_err(|e| ValidateError::Parse(e.to_string()))?;
  if !matches!(parsed.scheme(), "http" | "https") {
    return Err(ValidateError::Scheme(parsed.scheme().to_string()));
  }
  Ok(parsed)
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p lonk-validate`
Expected: 4 passed.

Run: `cargo test` (whole workspace)
Expected: all pre-existing lonk tests still pass.

- [ ] **Step 6: Commit**

```bash
cargo fmt && git add Cargo.toml Cargo.lock crates/ && git commit -m "feat: add lonk-validate crate in a cargo workspace"
```

---

### Task 2: Server uses `lonk-validate` in `create_link`

**Files:**
- Modify: `src/routes.rs:49-61` (the `create_link` validation block and the `use url::Url;` import)

**Interfaces:**
- Consumes: `lonk_validate::validate_url` (Task 1).
- Produces: unchanged HTTP behavior — this is a pure refactor guarded by the existing test suite (`tests/api.rs::create_link_rejects_unparseable_url`, `create_link_rejects_non_http_scheme`).

- [ ] **Step 1: Refactor validation**

In `src/routes.rs`, delete the import `use url::Url;` and replace the validation block at the top of `create_link`:

```rust
  let parsed = Url::parse(&body.url)
    .map_err(|e| api_error(Status::BadRequest, &format!("invalid url: {e}")))?;
  if !matches!(parsed.scheme(), "http" | "https") {
    return Err(api_error(
      Status::BadRequest,
      "url scheme must be http or https",
    ));
  }
```

with:

```rust
  let parsed = lonk_validate::validate_url(&body.url)
    .map_err(|e| api_error(Status::BadRequest, &e.to_string()))?;
```

(`parsed` is still a `url::Url`; the later `url: parsed.into()` keeps compiling.)

- [ ] **Step 2: Run the full test suite**

Run: `cargo test`
Expected: all tests pass, including `create_link_rejects_unparseable_url` and `create_link_rejects_non_http_scheme` (their assertions only check status 400 + non-empty error, which the new messages satisfy).

- [ ] **Step 3: Check `url` is still needed as a direct dependency**

Run: `grep -rn "use url::" src/`
Expected: no matches → remove the line `url = "2"` from `[dependencies]` in `Cargo.toml`, then `cargo test` again to confirm it builds.

- [ ] **Step 4: Commit**

```bash
cargo fmt && git add -A && git commit -m "refactor: create_link validates via lonk-validate"
```

---

### Task 3: `POST /api/valid` endpoint

**Files:**
- Modify: `src/routes.rs` (new response type + handler)
- Modify: `src/lib.rs:13` (mount the route)
- Test: `tests/api.rs`

**Interfaces:**
- Consumes: `lonk_validate::validate_url`, existing `CreateReq` body type.
- Produces: `POST /api/valid` with JSON body `{"url": "<string>"}` → `200 {"valid": true}` or `200 {"valid": false, "error": "<why>"}` (the `error` key is absent when valid).

- [ ] **Step 1: Write failing tests**

Append to `tests/api.rs`:

```rust
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
  for bad in [r#"{"url": "not a url"}"#, r#"{"url": "ftp://example.com/f"}"#] {
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test api valid_endpoint`
Expected: FAIL — both tests get 404 (route not mounted).

- [ ] **Step 3: Implement the endpoint**

In `src/routes.rs`, add after the `LinkResp` definition:

```rust
#[derive(Serialize)]
#[serde(crate = "rocket::serde")]
pub struct ValidResp {
  valid: bool,
  #[serde(skip_serializing_if = "Option::is_none")]
  error: Option<String>,
}

#[rocket::post("/api/valid", data = "<body>")]
pub fn valid_url(body: Json<CreateReq>) -> Json<ValidResp> {
  match lonk_validate::validate_url(&body.url) {
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
```

In `src/lib.rs`, extend the routes list:

```rust
      rocket::routes![
        routes::create_link,
        routes::follow_link,
        routes::qr_svg,
        routes::valid_url
      ],
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test`
Expected: all pass, including the two new tests.

- [ ] **Step 5: Commit**

```bash
cargo fmt && git add -A && git commit -m "feat: add POST /api/valid url validation endpoint"
```

---

### Task 4: Rename server binary to `lonkd`; default DB in the config dir

**Files:**
- Modify: `Cargo.toml` (explicit `[[bin]]`)
- Create: `src/paths.rs`
- Modify: `src/lib.rs` (declare `pub mod paths;`)
- Modify: `src/main.rs`

**Interfaces:**
- Consumes: nothing new.
- Produces: `lonk::paths::config_dir() -> std::path::PathBuf` (resolution: `$LONK_CONFIG_DIR` → `$XDG_CONFIG_HOME/lonk` → `~/.config/lonk`). Server binary is now named `lonkd`; when `LONK_DB` is unset it creates the config dir and uses `<config-dir>/lonk.db`.

- [ ] **Step 1: Declare the binary name**

Add to `Cargo.toml` (below `[package]`):

```toml
[[bin]]
name = "lonkd"
path = "src/main.rs"
```

- [ ] **Step 2: Write failing test for `config_dir`**

Create `src/paths.rs`:

```rust
use std::path::PathBuf;

/// Resolution order: $LONK_CONFIG_DIR, else $XDG_CONFIG_HOME/lonk, else ~/.config/lonk.
pub fn config_dir() -> PathBuf {
  todo!()
}

#[cfg(test)]
mod tests {
  use super::*;

  // One test exercising all branches sequentially: env vars are process-global,
  // so splitting into parallel #[test]s would race.
  #[test]
  fn resolution_order() {
    std::env::set_var("LONK_CONFIG_DIR", "/explicit/dir");
    std::env::set_var("XDG_CONFIG_HOME", "/xdg");
    assert_eq!(config_dir(), PathBuf::from("/explicit/dir"));

    std::env::remove_var("LONK_CONFIG_DIR");
    assert_eq!(config_dir(), PathBuf::from("/xdg/lonk"));

    std::env::remove_var("XDG_CONFIG_HOME");
    let home = std::env::var("HOME").unwrap();
    assert_eq!(config_dir(), PathBuf::from(home).join(".config").join("lonk"));
  }
}
```

Add to `src/lib.rs` after `pub mod db;`:

```rust
pub mod paths;
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test --lib paths`
Expected: FAIL with `not yet implemented`.

- [ ] **Step 4: Implement `config_dir`**

```rust
pub fn config_dir() -> PathBuf {
  if let Ok(dir) = std::env::var("LONK_CONFIG_DIR") {
    return PathBuf::from(dir);
  }
  if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
    return PathBuf::from(xdg).join("lonk");
  }
  PathBuf::from(std::env::var("HOME").expect("HOME is not set"))
    .join(".config")
    .join("lonk")
}
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test --lib paths`
Expected: PASS.

- [ ] **Step 6: Use it in the server default**

Replace `src/main.rs` entirely:

```rust
#[rocket::launch]
fn rocket() -> _ {
  let db_path = std::env::var("LONK_DB").unwrap_or_else(|_| {
    let dir = lonk::paths::config_dir();
    std::fs::create_dir_all(&dir).expect("failed to create config dir");
    dir.join("lonk.db").to_string_lossy().into_owned()
  });
  lonk::rocket_app(&db_path)
}
```

- [ ] **Step 7: Verify the binary end-to-end**

```bash
TESTDIR=$(mktemp -d)
LONK_CONFIG_DIR=$TESTDIR ROCKET_PORT=8899 cargo run --bin lonkd &
sleep 3
ls "$TESTDIR"          # expect: lonk.db  (created in the config dir by default)
curl -s http://127.0.0.1:8899/api/valid -X POST \
  -H 'Content-Type: application/json' -d '{"url":"https://x.example"}'
                       # expect: {"valid":true}
kill %1
```

Run: `cargo test`
Expected: all pass.

- [ ] **Step 8: Commit**

```bash
cargo fmt && git add -A && git commit -m "feat: rename server binary to lonkd, default db in config dir"
```

---

### Task 5: CLI skeleton — arg parsing, help, exit codes, `--valid`

**Files:**
- Modify: `Cargo.toml` (second `[[bin]]`, new deps)
- Create: `src/cli/main.rs`
- Create: `src/cli/args.rs`
- Test: `tests/cli.rs`

**Interfaces:**
- Consumes: `lonk_validate::validate_url`.
- Produces: binary `lonk`. `args::Cli { urls: Vec<String>, qr: bool, qr_svg: bool, valid: bool, profile: Option<String>, cmd: Option<Cmd> }`, `enum Cmd { Setup { base_url: Option<String> } }`. `main` maps clap errors → exit 1 (help/version → 0). `run_valid(urls: &[String]) -> i32` prints `valid` / `invalid: <reason>` per line. Later tasks add functions to `src/cli/main.rs` and modules alongside `args.rs`.

- [ ] **Step 1: Add binary target and dependencies**

In `Cargo.toml`, add below the `lonkd` `[[bin]]`:

```toml
[[bin]]
name = "lonk"
path = "src/cli/main.rs"
```

Add to `[dependencies]`:

```toml
clap = { version = "4", features = ["derive"] }
ureq = { version = "2", features = ["json"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
```

Remove `serde_json = "1"` from `[dev-dependencies]` (it is now a regular dependency and stays visible to tests). Add to `[dev-dependencies]`:

```toml
tempfile = "3"
```

- [ ] **Step 2: Write the parser with unit tests**

Create `src/cli/args.rs`:

```rust
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
  name = "lonk",
  about = "Shorten URLs via a lonk server",
  arg_required_else_help = true
)]
pub struct Cli {
  /// URLs to shorten (http:// or https://)
  #[arg(value_name = "URL")]
  pub urls: Vec<String>,

  /// Print a scannable unicode QR code after each short link
  #[arg(long, conflicts_with = "qr_svg")]
  pub qr: bool,

  /// Print the QR code as SVG text to stdout (exactly one URL)
  #[arg(long = "qr-svg")]
  pub qr_svg: bool,

  /// Validate URLs locally and exit; no config or network needed
  #[arg(long)]
  pub valid: bool,

  /// Use a named profile from the config file (default: "default")
  #[arg(long, global = true, value_name = "NAME")]
  pub profile: Option<String>,

  #[command(subcommand)]
  pub cmd: Option<Cmd>,
}

#[derive(Subcommand, Debug)]
pub enum Cmd {
  /// Configure the server base URL (first-time setup or reconfigure)
  Setup {
    /// e.g. https://s.example.com (prompts if omitted)
    base_url: Option<String>,
  },
}

#[cfg(test)]
mod tests {
  use super::*;
  use clap::Parser;

  #[test]
  fn parses_urls_and_flags() {
    let cli = Cli::try_parse_from(["lonk", "--qr", "https://a.example", "https://b.example"])
      .unwrap();
    assert!(cli.qr);
    assert_eq!(cli.urls, vec!["https://a.example", "https://b.example"]);
    assert!(cli.cmd.is_none());
  }

  #[test]
  fn parses_setup_subcommand_with_profile() {
    let cli = Cli::try_parse_from(["lonk", "setup", "--profile", "work", "https://s.example"])
      .unwrap();
    match cli.cmd {
      Some(Cmd::Setup { base_url }) => assert_eq!(base_url.as_deref(), Some("https://s.example")),
      other => panic!("expected setup, got {other:?}"),
    }
    assert_eq!(cli.profile.as_deref(), Some("work"));
  }

  #[test]
  fn qr_and_qr_svg_conflict() {
    assert!(Cli::try_parse_from(["lonk", "--qr", "--qr-svg", "https://a.example"]).is_err());
  }

  #[test]
  fn zero_args_is_an_error_that_shows_help() {
    let err = Cli::try_parse_from(["lonk"]).unwrap_err();
    assert_eq!(
      err.kind(),
      clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
    );
  }
}
```

- [ ] **Step 3: Write the main with `--valid` support**

Create `src/cli/main.rs`:

```rust
mod args;

use args::{Cli, Cmd};
use clap::Parser;

const EXIT_OK: i32 = 0;
const EXIT_USAGE: i32 = 1;
#[allow(dead_code)] // used from Task 8 on
const EXIT_NETWORK: i32 = 2;

fn main() {
  let cli = match Cli::try_parse() {
    Ok(cli) => cli,
    Err(e) => {
      let code = match e.kind() {
        clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion => EXIT_OK,
        _ => EXIT_USAGE,
      };
      let _ = e.print();
      std::process::exit(code);
    }
  };
  std::process::exit(run(cli));
}

fn run(cli: Cli) -> i32 {
  match cli.cmd {
    Some(Cmd::Setup { .. }) => {
      eprintln!("setup: not implemented yet");
      EXIT_USAGE
    }
    None if cli.valid => run_valid(&cli.urls),
    None => {
      eprintln!("shorten: not implemented yet");
      EXIT_USAGE
    }
  }
}

fn run_valid(urls: &[String]) -> i32 {
  let mut ok = true;
  for url in urls {
    match lonk_validate::validate_url(url) {
      Ok(_) => println!("valid"),
      Err(e) => {
        println!("invalid: {e}");
        ok = false;
      }
    }
  }
  if ok {
    EXIT_OK
  } else {
    EXIT_USAGE
  }
}
```

- [ ] **Step 4: Write integration tests**

Create `tests/cli.rs`:

```rust
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
    .args(["--valid", "https://ok.example/x", "ftp://bad.example", "also not a url"])
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
```

- [ ] **Step 5: Run tests**

Run: `cargo test --bin lonk` then `cargo test --test cli`
Expected: unit tests pass; integration tests pass (build produces the `lonk` binary automatically).

Run: `cargo test`
Expected: everything green.

- [ ] **Step 6: Commit**

```bash
cargo fmt && git add -A && git commit -m "feat: lonk CLI skeleton with --valid local validation"
```

---

### Task 6: CLI config module — profiles, load/save

**Files:**
- Create: `src/cli/config.rs`
- Modify: `src/cli/main.rs` (add `mod config;`)

**Interfaces:**
- Consumes: `lonk::paths::config_dir()`.
- Produces (used by Tasks 7–8):
  - `config::Config { profiles: BTreeMap<String, Profile> }`, `config::Profile { base_url: String }` (both serde + `PartialEq`, `Clone` on `Profile`)
  - `Config::load(path: &Path) -> Result<Config, String>` (missing file → `Ok(Config::default())`)
  - `Config::save(&self, path: &Path) -> Result<(), String>` (creates parent dirs)
  - `Config::base_url(&self, profile: &str) -> Option<&str>`
  - `config::config_path() -> PathBuf` = `lonk::paths::config_dir().join("config.toml")`

- [ ] **Step 1: Write the module with failing tests**

Create `src/cli/config.rs`:

```rust
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Default, Debug, PartialEq)]
pub struct Config {
  #[serde(default)]
  pub profiles: BTreeMap<String, Profile>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct Profile {
  pub base_url: String,
}

pub fn config_path() -> PathBuf {
  lonk::paths::config_dir().join("config.toml")
}

impl Config {
  pub fn load(path: &Path) -> Result<Self, String> {
    todo!()
  }

  pub fn save(&self, path: &Path) -> Result<(), String> {
    todo!()
  }

  pub fn base_url(&self, profile: &str) -> Option<&str> {
    todo!()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn missing_file_loads_empty_config() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = Config::load(&dir.path().join("config.toml")).unwrap();
    assert_eq!(cfg, Config::default());
  }

  #[test]
  fn save_then_load_roundtrip_with_profiles() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nested").join("config.toml");
    let mut cfg = Config::default();
    cfg.profiles.insert(
      "default".into(),
      Profile { base_url: "https://s.example.com".into() },
    );
    cfg.profiles.insert(
      "work".into(),
      Profile { base_url: "https://links.corp.example".into() },
    );
    cfg.save(&path).unwrap();
    let loaded = Config::load(&path).unwrap();
    assert_eq!(loaded, cfg);
    assert_eq!(loaded.base_url("work"), Some("https://links.corp.example"));
    assert_eq!(loaded.base_url("nope"), None);
  }

  #[test]
  fn malformed_toml_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "profiles = 42").unwrap();
    assert!(Config::load(&path).is_err());
  }
}
```

Add `mod config;` to `src/cli/main.rs` under `mod args;`. Because `config` is not referenced by `main` yet, silence the warning for one task with `#[allow(dead_code)] mod config;` — Task 7 removes the allow.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --bin lonk config`
Expected: FAIL with `not yet implemented`.

- [ ] **Step 3: Implement**

```rust
impl Config {
  pub fn load(path: &Path) -> Result<Self, String> {
    match std::fs::read_to_string(path) {
      Ok(s) => {
        toml::from_str(&s).map_err(|e| format!("malformed config {}: {e}", path.display()))
      }
      Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
      Err(e) => Err(format!("cannot read {}: {e}", path.display())),
    }
  }

  pub fn save(&self, path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
      std::fs::create_dir_all(parent)
        .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
    }
    let body = toml::to_string_pretty(self).map_err(|e| format!("cannot serialize config: {e}"))?;
    std::fs::write(path, body).map_err(|e| format!("cannot write {}: {e}", path.display()))
  }

  pub fn base_url(&self, profile: &str) -> Option<&str> {
    self.profiles.get(profile).map(|p| p.base_url.as_str())
  }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --bin lonk config`
Expected: 3 passed.

- [ ] **Step 5: Commit**

```bash
cargo fmt && git add -A && git commit -m "feat: CLI config file with named profiles"
```

---

### Task 7: `lonk setup` subcommand

**Files:**
- Modify: `src/cli/main.rs`
- Test: `tests/cli.rs`

**Interfaces:**
- Consumes: `args::Cmd::Setup`, `config::{Config, config_path}`, `lonk_validate::validate_url`.
- Produces: `run_setup(base_url: Option<String>, profile: &str) -> i32` — validates the base URL (http/https), trims trailing `/`, writes it under `profiles.<profile>` in the config file. With no arg: prompts on a TTY (`Base URL of your lonk server: `), errors with a hint when stdin is not a TTY. Also `prompt_base_url() -> Option<String>` (returns `None` when not a TTY). Task 8 reuses `prompt_base_url` for unconfigured shortening.

- [ ] **Step 1: Write failing integration tests**

Append to `tests/cli.rs`:

```rust
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
  assert_eq!(out.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&out.stderr));
  let cfg = read_config(dir.path());
  assert!(cfg.contains("[profiles.default]"), "{cfg}");
  // trailing slash trimmed
  assert!(cfg.contains("base_url = \"https://s.example.com\""), "{cfg}");
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
  assert!(cfg.contains("https://one.example") && cfg.contains("https://two.example"), "{cfg}");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test cli setup`
Expected: FAIL — exit code 1 with "setup: not implemented yet".

- [ ] **Step 3: Implement setup**

In `src/cli/main.rs`: change `#[allow(dead_code)] mod config;` to `mod config;`, replace the `Some(Cmd::Setup { .. })` arm in `run`:

```rust
    Some(Cmd::Setup { base_url }) => {
      let profile = cli.profile.as_deref().unwrap_or("default");
      run_setup(base_url, profile)
    }
```

and add:

```rust
use std::io::IsTerminal;
use std::io::Write;

fn run_setup(base_url: Option<String>, profile: &str) -> i32 {
  let raw = match base_url.or_else(prompt_base_url) {
    Some(url) => url,
    None => {
      eprintln!("no base url given and stdin is not a terminal");
      eprintln!("usage: lonk setup <base-url>   (e.g. lonk setup https://s.example.com)");
      return EXIT_USAGE;
    }
  };
  let parsed = match lonk_validate::validate_url(raw.trim()) {
    Ok(u) => u,
    Err(e) => {
      eprintln!("{e}");
      return EXIT_USAGE;
    }
  };
  let base = parsed.as_str().trim_end_matches('/').to_string();

  let path = config::config_path();
  let mut cfg = match config::Config::load(&path) {
    Ok(c) => c,
    Err(e) => {
      eprintln!("{e}");
      return EXIT_USAGE;
    }
  };
  cfg
    .profiles
    .insert(profile.to_string(), config::Profile { base_url: base.clone() });
  if let Err(e) = cfg.save(&path) {
    eprintln!("{e}");
    return EXIT_USAGE;
  }
  eprintln!("profile '{profile}' -> {base} saved to {}", path.display());
  EXIT_OK
}

/// Interactive base-url prompt. None when stdin is not a TTY or the line is empty.
fn prompt_base_url() -> Option<String> {
  if !std::io::stdin().is_terminal() {
    return None;
  }
  eprint!("Base URL of your lonk server: ");
  let _ = std::io::stderr().flush();
  let mut line = String::new();
  std::io::stdin().read_line(&mut line).ok()?;
  let line = line.trim().to_string();
  if line.is_empty() { None } else { Some(line) }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test cli`
Expected: all pass (including earlier Task 5 tests).

- [ ] **Step 5: Commit**

```bash
cargo fmt && git add -A && git commit -m "feat: lonk setup subcommand with profiles and tty prompt"
```

---

### Task 8: Shorten URLs against a live server

**Files:**
- Modify: `src/cli/main.rs`
- Test: `tests/cli_live.rs` (new — spawns a real `lonkd`)

**Interfaces:**
- Consumes: `config::Config::{load, base_url}`, `prompt_base_url` (Task 7), `lonk_validate`, `POST /api/links` returning `{id, url, short_url, qr_url}`.
- Produces: `run_shorten(cli: &Cli) -> i32` and `shorten_one(base: &str, url: &str) -> Result<String, (i32, String)>` where `Ok` is the **full short URL** (`{base}{short_url}`). Fail-fast: local validation of ALL args first (first failure → stderr + exit 1, nothing sent); then per-URL POSTs in input order, first network/HTTP failure → stderr + exit 2. Unknown `--profile` → exit 1. Missing `default` profile: TTY → prompt + save; non-TTY → exit 1 with `lonk setup` hint. Test harness `ServerGuard` (spawn `lonkd`, poll readiness, kill on drop) is reused by Task 9.

- [ ] **Step 1: Write failing integration tests with a live server**

Create `tests/cli_live.rs`:

```rust
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
  assert_eq!(out.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&out.stderr));
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test cli_live`
Expected: FAIL — "shorten: not implemented yet" paths exit 1 where 0/2 expected.

- [ ] **Step 3: Implement shorten**

In `src/cli/main.rs`, replace the `None => { ... not implemented ... }` arm of `run`:

```rust
    None => run_shorten(&cli),
```

and add:

```rust
fn run_shorten(cli: &Cli) -> i32 {
  // 1. local validation, fail fast before any network traffic
  for url in &cli.urls {
    if let Err(e) = lonk_validate::validate_url(url) {
      eprintln!("{url}: {e}");
      return EXIT_USAGE;
    }
  }
  if cli.qr_svg && cli.urls.len() != 1 {
    eprintln!("--qr-svg requires exactly one URL");
    return EXIT_USAGE;
  }

  // 2. resolve base url from profile
  let base = match resolve_base_url(cli.profile.as_deref()) {
    Ok(b) => b,
    Err(code) => return code,
  };

  // 3. shorten in input order, fail fast
  for (i, url) in cli.urls.iter().enumerate() {
    match shorten_one(&base, url) {
      Ok(short) => {
        if i > 0 && cli.qr {
          println!();
        }
        emit(&short, cli);
      }
      Err((code, msg)) => {
        eprintln!("{url}: {msg}");
        return code;
      }
    }
  }
  EXIT_OK
}

/// Resolve the profile's base url; Err carries the exit code (already reported).
fn resolve_base_url(profile: Option<&str>) -> Result<String, i32> {
  let path = config::config_path();
  let mut cfg = config::Config::load(&path).map_err(|e| {
    eprintln!("{e}");
    EXIT_USAGE
  })?;
  let name = profile.unwrap_or("default");
  if let Some(base) = cfg.base_url(name) {
    return Ok(base.to_string());
  }
  if profile.is_some() {
    eprintln!("profile '{name}' not found; run: lonk setup --profile {name} <base-url>");
    return Err(EXIT_USAGE);
  }
  // default profile missing: one-time interactive setup on a TTY
  match prompt_base_url().and_then(|raw| lonk_validate::validate_url(raw.trim()).ok()) {
    Some(parsed) => {
      let base = parsed.as_str().trim_end_matches('/').to_string();
      cfg
        .profiles
        .insert("default".into(), config::Profile { base_url: base.clone() });
      cfg.save(&path).map_err(|e| {
        eprintln!("{e}");
        EXIT_USAGE
      })?;
      Ok(base)
    }
    None => {
      eprintln!("no server configured; run: lonk setup <base-url>");
      Err(EXIT_USAGE)
    }
  }
}

/// POST one url; Ok(full short url), Err((exit code, message)).
fn shorten_one(base: &str, url: &str) -> Result<String, (i32, String)> {
  let resp = ureq::post(&format!("{base}/api/links"))
    .send_json(serde_json::json!({ "url": url }))
    .map_err(|e| match e {
      ureq::Error::Status(code, resp) => {
        let msg = resp
          .into_json::<serde_json::Value>()
          .ok()
          .and_then(|v| v["error"].as_str().map(String::from))
          .unwrap_or_else(|| format!("server returned {code}"));
        (EXIT_NETWORK, msg)
      }
      ureq::Error::Transport(t) => (EXIT_NETWORK, t.to_string()),
    })?;
  let body: serde_json::Value = resp
    .into_json()
    .map_err(|e| (EXIT_NETWORK, format!("bad response: {e}")))?;
  let short_path = body["short_url"]
    .as_str()
    .ok_or((EXIT_NETWORK, "response missing short_url".to_string()))?;
  Ok(format!("{base}{short_path}"))
}

/// Print one result. Task 9 extends this with --qr / --qr-svg rendering.
fn emit(short: &str, _cli: &Cli) {
  println!("{short}");
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test cli_live -- --test-threads=1`
Expected: 3 passed. (Serial threads keep the fixed port free; the other two tests don't bind it, but serial is deterministic.)

Run: `cargo test`
Expected: everything green.

- [ ] **Step 5: Commit**

```bash
cargo fmt && git add -A && git commit -m "feat: lonk shortens urls against the configured server"
```

---

### Task 9: `--qr` and `--qr-svg` output

**Files:**
- Modify: `Cargo.toml` (qrcode dep for the CLI path)
- Modify: `src/cli/main.rs` (`emit`)
- Test: `tests/cli_live.rs`

**Interfaces:**
- Consumes: `emit(short, cli)` call sites from Task 8; `qrcode::QrCode` (already a workspace dependency, `default-features = false, features = ["svg"]` — the `unicode` renderer is not feature-gated).
- Produces: `--qr` prints the short URL line then a unicode QR block; blank line between entries (handled in `run_shorten` from Task 8). `--qr-svg` prints ONLY the SVG document to stdout (pipeable); the short URL goes to stderr.

- [ ] **Step 1: Write failing tests**

Append to `tests/cli_live.rs` inside `shorten_end_to_end` (before the fail-fast section, while the server is up):

```rust
  // --qr: link line, then a unicode qr block
  let out = lonk_with_config(tmp.path())
    .args(["--qr", "https://example.com/qr"])
    .output()
    .unwrap();
  assert_eq!(out.status.code(), Some(0));
  let stdout = String::from_utf8_lossy(&out.stdout);
  assert!(stdout.lines().next().unwrap().starts_with(&format!("{}/", base())));
  assert!(stdout.contains('\u{2588}'), "no unicode blocks in: {stdout}"); // █

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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test cli_live -- --test-threads=1`
Expected: FAIL — no QR output yet (plain link printed; svg/unicode assertions fail). The two-url `--qr-svg` case already passes (guard added in Task 8).

- [ ] **Step 3: Implement rendering**

Replace `emit` in `src/cli/main.rs`:

```rust
fn emit(short: &str, cli: &Cli) {
  if cli.qr_svg {
    let svg = match qr_svg(short) {
      Ok(s) => s,
      Err(e) => {
        eprintln!("{e}");
        std::process::exit(EXIT_USAGE);
      }
    };
    eprintln!("{short}");
    println!("{svg}");
    return;
  }
  println!("{short}");
  if cli.qr {
    match qr_unicode(short) {
      Ok(q) => println!("{q}"),
      Err(e) => {
        eprintln!("{e}");
        std::process::exit(EXIT_USAGE);
      }
    }
  }
}

fn qr_unicode(content: &str) -> Result<String, String> {
  let code = qrcode::QrCode::new(content.as_bytes()).map_err(|e| format!("qr encoding failed: {e}"))?;
  Ok(
    code
      .render::<qrcode::render::unicode::Dense1x2>()
      .build(),
  )
}

fn qr_svg(content: &str) -> Result<String, String> {
  let code = qrcode::QrCode::new(content.as_bytes()).map_err(|e| format!("qr encoding failed: {e}"))?;
  Ok(
    code
      .render::<qrcode::render::svg::Color>()
      .min_dimensions(256, 256)
      .build(),
  )
}
```

`qrcode` is already in `[dependencies]`; no Cargo.toml change needed unless the unicode renderer turns out to be feature-gated (it is not in qrcode 0.14 — only `image`/`svg`/`pic` are features). If compilation fails on `render::unicode`, check `cargo doc -p qrcode` for the gate.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test cli_live -- --test-threads=1`
Expected: all pass.

Manual sanity check (server from Task 4 running or any configured instance):

```bash
cargo run --bin lonk -- --qr https://example.com/hello
```

Expected: short URL line, then a scannable QR block in the terminal.

- [ ] **Step 5: Commit**

```bash
cargo fmt && git add -A && git commit -m "feat: --qr unicode and --qr-svg output for lonk"
```

---

### Task 10: Playwright e2e project

**Files:**
- Create: `e2e/package.json`
- Create: `e2e/playwright.config.ts`
- Create: `e2e/tests/lonk.spec.ts`
- Create: `e2e/.gitignore`

**Interfaces:**
- Consumes: the running server (auto-spawned `cargo run --bin lonkd` on port 8808 with a temp `LONK_DB`, or an external `BASE_URL`).
- Produces: `cd e2e && npm test` runs the suite headless in Chromium. Tests never assume a clean DB.

- [ ] **Step 1: Scaffold the npm project**

`e2e/.gitignore`:

```
node_modules/
test-results/
playwright-report/
```

`e2e/package.json`:

```json
{
  "name": "lonk-e2e",
  "private": true,
  "scripts": {
    "test": "playwright test"
  },
  "devDependencies": {
    "@playwright/test": "^1.45.0"
  }
}
```

Run:

```bash
cd e2e && npm install && npx playwright install chromium
```

Expected: lockfile created, Chromium downloaded. Commit `package-lock.json` (do not gitignore it).

- [ ] **Step 2: Write the config**

`e2e/playwright.config.ts`:

```ts
import { defineConfig, devices } from '@playwright/test';
import * as os from 'node:os';
import * as path from 'node:path';

const PORT = 8808;
const baseURL = process.env.BASE_URL ?? `http://127.0.0.1:${PORT}`;
const tmpDb = path.join(os.tmpdir(), `lonk-e2e-${process.pid}.db`);

export default defineConfig({
  testDir: './tests',
  fullyParallel: true,
  use: { baseURL },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],
  // With BASE_URL set, target that deployed instance instead of spawning.
  webServer: process.env.BASE_URL
    ? undefined
    : {
        command: 'cargo run --bin lonkd',
        cwd: '..',
        url: baseURL,
        reuseExistingServer: false,
        timeout: 180_000,
        env: {
          ...process.env,
          LONK_DB: tmpDb,
          ROCKET_ADDRESS: '127.0.0.1',
          ROCKET_PORT: String(PORT),
        },
      },
});
```

- [ ] **Step 3: Write the specs**

`e2e/tests/lonk.spec.ts`:

```ts
import { test, expect } from '@playwright/test';

const SLUG = /[A-Za-z0-9]{7}$/;

test('create a short link through the form and follow it', async ({ page, request, baseURL }) => {
  await page.goto('/');
  await page.fill('#url', 'https://example.com/e2e/target');
  await page.click('button[type=submit]');

  const link = page.locator('#short');
  await expect(link).toBeVisible();
  await expect(link).toHaveText(new RegExp(`^${baseURL}/[A-Za-z0-9]{7}$`));

  const href = await link.getAttribute('href');
  expect(href).toMatch(SLUG);
  const res = await request.get(href!, { maxRedirects: 0 });
  expect(res.status()).toBe(303);
  expect(res.headers()['location']).toBe('https://example.com/e2e/target');
});

test('QR code renders for a created link', async ({ page, request }) => {
  await page.goto('/');
  await page.fill('#url', 'https://example.com/e2e/qr');
  await page.click('button[type=submit]');

  const qr = page.locator('#qr');
  await expect(qr).toBeVisible();
  const src = await qr.getAttribute('src');
  expect(src).toMatch(/\/qr$/);
  const res = await request.get(src!);
  expect(res.status()).toBe(200);
  expect(res.headers()['content-type']).toContain('image/svg+xml');
  expect(await res.text()).toContain('<svg');
});

test('rejected URL shows the error message in the UI', async ({ page }) => {
  await page.goto('/');
  // passes the browser's type=url check but fails the server's scheme rule
  await page.fill('#url', 'ftp://example.com/file');
  await page.click('button[type=submit]');

  const error = page.locator('#error');
  await expect(error).toBeVisible();
  await expect(error).toContainText('scheme');
});

test('unknown slug returns a JSON 404', async ({ request }) => {
  const res = await request.get('/zzzzzzz', { maxRedirects: 0 });
  expect(res.status()).toBe(404);
  const body = await res.json();
  expect(body.error).toBeTruthy();
});

test('POST /api/valid contract', async ({ request }) => {
  const good = await request.post('/api/valid', { data: { url: 'https://ok.example/x' } });
  expect(good.status()).toBe(200);
  expect(await good.json()).toEqual({ valid: true });

  const bad = await request.post('/api/valid', { data: { url: 'not a url' } });
  expect(bad.status()).toBe(200);
  const body = await bad.json();
  expect(body.valid).toBe(false);
  expect(body.error).toBeTruthy();
});
```

- [ ] **Step 4: Run the suite**

Run: `cd e2e && npm test`
Expected: 5 passed (Playwright builds + spawns `lonkd` itself; first run includes cargo compile time).

Also verify the override path: `cd e2e && BASE_URL=http://127.0.0.1:8808 npm test` while a `lonkd` you started manually is running — same 5 passed, no server spawned.

- [ ] **Step 5: Commit**

```bash
git add e2e && git commit -m "test: add playwright e2e suite driving the real UI"
```

---

### Task 11: README documentation

**Files:**
- Modify: `README.md`

**Interfaces:**
- Consumes: everything shipped in Tasks 1–10.
- Produces: user-facing docs.

- [ ] **Step 1: Write the README**

Replace `README.md` with:

````markdown
# lonk

Self-hosted link shortener with REST API + QR code generation.

## Server (`lonkd`)

```bash
cargo run --bin lonkd
```

By default the SQLite database lives next to the config at
`~/.config/lonk/lonk.db` (respects `$XDG_CONFIG_HOME` and `$LONK_CONFIG_DIR`);
set `LONK_DB` to override the path.

### API

| Endpoint          | Description                                               |
| ----------------- | --------------------------------------------------------- |
| `POST /api/links` | `{"url": "https://…"}` → `201` with `{id, url, short_url, qr_url}` |
| `POST /api/valid` | `{"url": "…"}` → `200` with `{valid: true}` or `{valid: false, error}` |
| `GET /<id>`       | `303` redirect to the original URL                         |
| `GET /<id>/qr`    | SVG QR code for the short link                             |

## CLI (`lonk`)

One-time setup, then shorten away:

```bash
lonk setup https://s.example.com     # writes ~/.config/lonk/config.toml
lonk https://example.com/some/very/long/url
lonk --qr https://example.com/a      # + scannable QR in the terminal
lonk --qr-svg https://example.com/a > code.svg
lonk --valid https://maybe.example   # local validation only, no network
```

Profiles let you target several servers:

```bash
lonk setup --profile work https://links.corp.example
lonk --profile work https://example.com/for-work
```

Multiple URLs print one short link per line, in input order; the first
invalid URL or failed request aborts the run (exit 1 for validation/usage
errors, exit 2 for network/server errors).

## Development

```bash
cargo test                                # unit + integration tests
cd e2e && npm install && npm test         # Playwright e2e (spawns lonkd)
cd e2e && BASE_URL=https://s.example.com npm test   # against a deployed instance
```
````

- [ ] **Step 2: Verify examples against reality**

Run each README command shape once (`cargo run --bin lonkd`, `cargo run --bin lonk -- --valid https://x.example`); confirm flags/paths match implementation.

- [ ] **Step 3: Commit**

```bash
git add README.md && git commit -m "docs: document lonkd server, lonk CLI, and e2e suite"
```
