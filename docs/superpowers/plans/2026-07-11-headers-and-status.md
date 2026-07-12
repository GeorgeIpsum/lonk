# 3-crate restructure + response headers + /status Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restructure lonk into three crates (core/server/CLI), add per-link custom response headers on redirects, and add a live `GET /<id>/status` dead-link check — per `docs/superpowers/specs/2026-07-11-headers-and-status-design.md`.

**Architecture:** Phase 0 (Tasks 1–4) converts the repo to a virtual workspace: `crates/lonk-core` (validation, wire types, paths — pure, no I/O), `crates/lonkd` (Rocket server + SQLite + static UI), `crates/lonk-cli` (binary named `lonk`). Phase 1 (Tasks 5–9) adds headers end to end: core validation → DB column with migration → API + custom responder → CLI `-H` → web UI. Phase 2 (Tasks 10–12) adds the status check: server endpoint with outbound HEAD/GET probe → CLI `status` subcommand → e2e. Task 13 documents everything.

**Tech Stack:** Rust (Rocket 0.5, rusqlite, clap 4, ureq 2, serde, toml, qrcode), Playwright.

## Global Constraints

- Branch: `feat/headers-and-status` (stacked on `feat/cli-and-e2e`). 2-space indentation; run `cargo fmt` before every commit. TDD unless a task says it's a pure move/refactor guarded by existing tests.
- Test command is `cargo test --workspace` (plain `cargo test` misses workspace members). cli_live-style suites run serially: `cargo test --test <file> -- --test-threads=1` when a file has multiple server-spawning tests.
- **Port registry** — every test that spawns a server owns a unique port: 8907 (`cli_live::shorten_end_to_end`), 8908 (`cli_live::shorten_with_trailing_slash_config_base_url`), 8909 (`cli_live` headers test, Task 8), 8910 (`cli_live` status test, Task 11), 8911 (`lonkd/tests/status_live.rs`, Task 10), 8808 (Playwright).
- Header validation (single source of truth `lonk_core::validate_header`): name is an RFC 7230 token (ASCII alphanumerics or ``!#$%&'*+-.^_`|~``), 1..=128 bytes; value 0..=1024 bytes of visible ASCII/space/tab (no CR/LF/NUL); denylist case-insensitive: `location`, `content-length`, `transfer-encoding`, `connection`, `content-type`, `content-encoding`; max 16 pairs per link (`lonk_core::MAX_HEADERS_PER_LINK`), enforced by API and CLI.
- Headers wire shape: JSON array of `[name, value]` pairs = `Vec<(String, String)>` in Rust. Repeats allowed, order preserved.
- Status semantics: HEAD, ≤5 redirects, 5s timeout, retry once with GET on 405/501; **alive = final 2xx**. Known slug → HTTP 200 with `StatusResp` (deadness is data); unknown slug → 404 JSON error.
- CLI exit codes: 0 success/alive; 1 validation/usage/config/dead/unknown-slug; 2 network/server errors reaching the lonk server.
- Tests never touch the real `~/.config/lonk` (always set `LONK_CONFIG_DIR`) and never assume a clean DB in e2e.

---

### Task 1: Create `lonk-core` (rename lonk-validate, absorb `paths`)

**Files:**
- Rename: `crates/lonk-validate/` → `crates/lonk-core/` (git mv)
- Move: `src/paths.rs` → `crates/lonk-core/src/paths.rs` (git mv)
- Modify: `crates/lonk-core/Cargo.toml`, `crates/lonk-core/src/lib.rs`, root `Cargo.toml`, `src/lib.rs`, `src/routes.rs`, `src/main.rs`, `src/cli/main.rs`, `src/cli/config.rs`

**Interfaces:**
- Consumes: existing `lonk_validate::{validate_url, ValidateError}`, `lonk::paths::config_dir`.
- Produces: crate `lonk-core` exposing `lonk_core::validate_url`, `lonk_core::ValidateError`, `lonk_core::paths::config_dir() -> PathBuf`. All later tasks reference `lonk_core::…`.

This is a mechanical rename guarded by the existing suite — no new tests.

- [ ] **Step 1: Move the files**

```bash
git mv crates/lonk-validate crates/lonk-core
git mv src/paths.rs crates/lonk-core/src/paths.rs
```

- [ ] **Step 2: Rename the crate and export paths**

`crates/lonk-core/Cargo.toml`: change `name = "lonk-validate"` to `name = "lonk-core"`.

`crates/lonk-core/src/lib.rs`: add at the top (below any existing `use` lines is fine):

```rust
pub mod paths;
```

`crates/lonk-core/src/paths.rs` is unchanged (it only uses `std`).

- [ ] **Step 3: Rewire the root package**

Root `Cargo.toml`:
- `[workspace] members = ["crates/lonk-validate"]` → `members = ["crates/lonk-core"]`
- dependency `lonk-validate = { path = "crates/lonk-validate" }` → `lonk-core = { path = "crates/lonk-core" }`

`src/lib.rs`: delete the `pub mod paths;` line.

Replace identifiers across the root package:
- `src/routes.rs`: `lonk_validate::` → `lonk_core::` (two call sites: `create_link`, `valid_url`)
- `src/cli/main.rs`: every `lonk_validate::` → `lonk_core::`
- `src/main.rs`: `lonk::paths::config_dir()` → `lonk_core::paths::config_dir()`
- `src/cli/config.rs`: `lonk::paths::config_dir()` → `lonk_core::paths::config_dir()`

- [ ] **Step 4: Verify nothing references the old names**

Run: `grep -rn "lonk_validate\|lonk-validate\|lonk::paths" src/ crates/ tests/ Cargo.toml`
Expected: no matches.

- [ ] **Step 5: Full suite**

Run: `cargo test --workspace`
Expected: same counts as before (5 lib + 7 CLI unit + 10 api + 9 cli + 4 cli_live + 4 core = 39). Note the lib suite drops to 4 and core rises to 5 (the `paths::resolution_order` test moved crates) — total stays 39.

- [ ] **Step 6: Commit**

```bash
cargo fmt && git add -A && git commit -m "refactor: rename lonk-validate to lonk-core and absorb paths"
```

---

### Task 2: Shared wire types in `lonk-core`

**Files:**
- Create: `crates/lonk-core/src/types.rs`
- Modify: `crates/lonk-core/Cargo.toml`, `crates/lonk-core/src/lib.rs`, `src/routes.rs`, `src/cli/main.rs`

**Interfaces:**
- Consumes: serde.
- Produces (used by every later task): `lonk_core::types::{CreateLinkReq, LinkResp, ValidResp}` with these exact shapes (headers fields arrive in Task 7; `StatusResp` in Task 10):

```rust
pub struct CreateLinkReq { pub url: String }
pub struct LinkResp { pub id: String, pub url: String, pub short_url: String, pub qr_url: String }
pub struct ValidResp { pub valid: bool, pub error: Option<String> }
```

- [ ] **Step 1: Add serde to lonk-core and write the types with a failing test**

`crates/lonk-core/Cargo.toml`:

```toml
[dependencies]
url = "2"
serde = { version = "1", features = ["derive"] }

[dev-dependencies]
serde_json = "1"
```

`crates/lonk-core/src/types.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct CreateLinkReq {
  pub url: String,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct LinkResp {
  pub id: String,
  pub url: String,
  pub short_url: String,
  pub qr_url: String,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct ValidResp {
  pub valid: bool,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub error: Option<String>,
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn valid_resp_omits_error_key_when_none() {
    let json = serde_json::to_string(&ValidResp {
      valid: true,
      error: None,
    })
    .unwrap();
    assert_eq!(json, r#"{"valid":true}"#);
  }

  #[test]
  fn link_resp_roundtrip() {
    let resp = LinkResp {
      id: "Ab3dEf9".into(),
      url: "https://example.com/x".into(),
      short_url: "/Ab3dEf9".into(),
      qr_url: "/Ab3dEf9/qr".into(),
    };
    let json = serde_json::to_string(&resp).unwrap();
    assert_eq!(serde_json::from_str::<LinkResp>(&json).unwrap(), resp);
  }
}
```

`crates/lonk-core/src/lib.rs`: add `pub mod types;`.

- [ ] **Step 2: Run the core tests**

Run: `cargo test -p lonk-core`
Expected: PASS (7 tests: 4 validate + 1 paths + 2 types).

- [ ] **Step 3: Server consumes the shared types**

In `src/routes.rs`:
- Delete the local `CreateReq`, `LinkResp`, and `ValidResp` definitions (keep `ErrorBody` — it is server-internal).
- Add `use lonk_core::types::{CreateLinkReq, LinkResp, ValidResp};`
- `create_link` signature: `body: Json<CreateLinkReq>` (field access unchanged: `body.url`). Struct literal now uses the imported `LinkResp` (all fields are `pub`).
- `valid_url` signature: `body: Json<CreateLinkReq>` and returns the imported `ValidResp`.
- Remove the now-unused `use rocket::serde::{Deserialize, Serialize};` import parts if only `Serialize` remains for `ErrorBody` — keep exactly what compiles: `use rocket::serde::Serialize;` plus `#[derive(Serialize)] #[serde(crate = "rocket::serde")]` on `ErrorBody` as today.

- [ ] **Step 4: CLI consumes the shared types**

In `src/cli/main.rs`, `shorten_one`: replace the `serde_json::Value` parsing:

```rust
  let body: lonk_core::types::LinkResp = resp
    .into_json()
    .map_err(|e| (EXIT_NETWORK, format!("bad response: {e}")))?;
  Ok(format!("{base}{}", body.short_url))
```

(The `short_url`-missing error path disappears — deserialization fails instead, caught by the same `bad response` arm.)

- [ ] **Step 5: Full suite proves wire compatibility**

Run: `cargo test --workspace`
Expected: all green (41 = 39 + 2 new types tests). The unchanged `tests/api.rs` assertions prove the JSON on the wire is identical.

- [ ] **Step 6: Commit**

```bash
cargo fmt && git add -A && git commit -m "refactor: shared wire types in lonk-core"
```

---

### Task 3: Split the server into `crates/lonkd`

**Files:**
- Move (git mv): `src/main.rs`, `src/lib.rs`, `src/db.rs`, `src/routes.rs` → `crates/lonkd/src/`; `static/` → `crates/lonkd/static/`; `tests/api.rs` → `crates/lonkd/tests/api.rs`
- Create: `crates/lonkd/Cargo.toml`
- Modify: root `Cargo.toml`, `tests/cli_live.rs`, `e2e/playwright.config.ts`

**Interfaces:**
- Consumes: `lonk_core` (Tasks 1–2).
- Produces: package `lonkd` (lib `lonkd::rocket_app`, binary `lonkd`). `tests/cli_live.rs` gains `fn lonkd_bin() -> PathBuf` (current_exe-based locator) that Task 4 carries along and Task 10's status_live does NOT need (same-package `CARGO_BIN_EXE_lonkd` works there).

- [ ] **Step 1: Move the files**

```bash
mkdir -p crates/lonkd/src crates/lonkd/tests
git mv src/main.rs src/lib.rs src/db.rs src/routes.rs crates/lonkd/src/
git mv static crates/lonkd/static
git mv tests/api.rs crates/lonkd/tests/api.rs
```

- [ ] **Step 2: Write the lonkd manifest**

`crates/lonkd/Cargo.toml`:

```toml
[package]
name = "lonkd"
version = "0.1.0"
edition = "2021"

[dependencies]
rocket = { version = "0.5.1", features = ["json"] }
rusqlite = { version = "0.31", features = ["bundled"] }
qrcode = { version = "0.14", default-features = false, features = ["svg"] }
rand = "0.8"
serde_json = "1"
lonk-core = { path = "../lonk-core" }

[dev-dependencies]
serde_json = "1"
```

(`serde_json` appears under `[dependencies]` because Task 6 needs it at runtime; harmless now. The package's default binary is `lonkd` — named after the package — from `src/main.rs`; the lib target is `lonkd` from `src/lib.rs`. No `[[bin]]` section needed.)

`crates/lonkd/tests/api.rs`: replace both occurrences of `lonk::rocket_app` with `lonkd::rocket_app` (the `client()` helper).

- [ ] **Step 3: Shrink the root package to CLI-only**

Root `Cargo.toml` becomes:

```toml
[package]
name = "lonk"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "lonk"
path = "src/cli/main.rs"

[dependencies]
clap = { version = "4", features = ["derive"] }
ureq = { version = "2", features = ["json"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
qrcode = { version = "0.14", default-features = false, features = ["svg"] }
lonk-core = { path = "crates/lonk-core" }

[dev-dependencies]
tempfile = "3"

[workspace]
members = ["crates/lonk-core", "crates/lonkd"]
```

- [ ] **Step 4: Fix the cross-package binary lookup in cli_live**

`CARGO_BIN_EXE_lonkd` no longer exists for the root package. In `tests/cli_live.rs`, add:

```rust
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
```

and in `start_server`, replace `Command::new(env!("CARGO_BIN_EXE_lonkd"))` with `Command::new(lonkd_bin())`.

- [ ] **Step 5: Point Playwright at the moved package**

`e2e/playwright.config.ts`: change the webServer `command` from `'cargo run --bin lonkd'` to `'cargo run -p lonkd --bin lonkd'` (explicit `-p` works regardless of which package is the workspace root).

- [ ] **Step 6: Full suite**

Run: `cargo build -p lonkd && cargo test --workspace`
Expected: all 41 tests green (same tests, new homes: api suite now under the lonkd package).

- [ ] **Step 7: Commit**

```bash
cargo fmt && git add -A && git commit -m "refactor: split server into crates/lonkd"
```

---

### Task 4: Split the CLI into `crates/lonk-cli`; root goes virtual

**Files:**
- Move (git mv): `src/cli/main.rs` → `crates/lonk-cli/src/main.rs`; `src/cli/args.rs` → `crates/lonk-cli/src/args.rs`; `src/cli/config.rs` → `crates/lonk-cli/src/config.rs`; `tests/cli.rs`, `tests/cli_live.rs` → `crates/lonk-cli/tests/`
- Create: `crates/lonk-cli/Cargo.toml`
- Modify: root `Cargo.toml` (virtual manifest); delete empty `src/`, `tests/`

**Interfaces:**
- Consumes: `lonk_core` and the `lonkd_bin()` locator from Task 3.
- Produces: package `lonk-cli` whose `[[bin]]` is named `lonk`. `CARGO_BIN_EXE_lonk` keeps working in its tests (same package). Root is a virtual workspace: `members = ["crates/lonk-core", "crates/lonkd", "crates/lonk-cli"]`.

- [ ] **Step 1: Move the files**

```bash
mkdir -p crates/lonk-cli/src crates/lonk-cli/tests
git mv src/cli/main.rs crates/lonk-cli/src/main.rs
git mv src/cli/args.rs crates/lonk-cli/src/args.rs
git mv src/cli/config.rs crates/lonk-cli/src/config.rs
git mv tests/cli.rs tests/cli_live.rs crates/lonk-cli/tests/
rmdir src/cli src tests
```

(`mod args; mod config;` declarations in main.rs resolve the same way at the new paths — no edits needed.)

- [ ] **Step 2: Write the lonk-cli manifest and make the root virtual**

`crates/lonk-cli/Cargo.toml`:

```toml
[package]
name = "lonk-cli"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "lonk"
path = "src/main.rs"

[dependencies]
clap = { version = "4", features = ["derive"] }
ureq = { version = "2", features = ["json"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
qrcode = { version = "0.14", default-features = false, features = ["svg"] }
lonk-core = { path = "../lonk-core" }

[dev-dependencies]
tempfile = "3"
```

Root `Cargo.toml` becomes exactly:

```toml
[workspace]
resolver = "2"
members = ["crates/lonk-core", "crates/lonkd", "crates/lonk-cli"]
```

- [ ] **Step 3: Full suite + dependency-isolation acceptance checks**

Run: `cargo test --workspace`
Expected: 41 tests green (cli/cli_live suites now under lonk-cli).

Run: `cargo tree -p lonkd -e normal | grep -E "^.*(clap|toml v)" ; echo "lonkd clean: $?"`
Expected: no matches, `lonkd clean: 1`.

Run: `cargo tree -p lonk-cli -e normal | grep -E "rocket|rusqlite" ; echo "cli clean: $?"`
Expected: no matches, `cli clean: 1`.

- [ ] **Step 4: Playwright still works against the restructured tree**

Run: `cd e2e && npm test`
Expected: 5 passed (webServer builds `-p lonkd`).

- [ ] **Step 5: Commit**

```bash
cargo fmt && git add -A && git commit -m "refactor: split CLI into crates/lonk-cli, virtual workspace root"
```

---

### Task 5: `validate_header` in lonk-core

**Files:**
- Modify: `crates/lonk-core/src/lib.rs`

**Interfaces:**
- Consumes: nothing new.
- Produces: `lonk_core::validate_header(name: &str, value: &str) -> Result<(), ValidateError>`; `ValidateError::Header(String)` variant (Display prints the inner message verbatim); `pub const MAX_HEADERS_PER_LINK: usize = 16;`.

- [ ] **Step 1: Write the failing tests**

Append to `crates/lonk-core/src/lib.rs` (extend the existing `tests` module):

```rust
  #[test]
  fn header_accepts_common_cases() {
    assert!(validate_header("X-Custom-Header", "hello world").is_ok());
    assert!(validate_header("Set-Cookie", "a=1; Path=/; HttpOnly").is_ok());
    assert!(validate_header("Cache-Control", "no-store").is_ok());
    assert!(validate_header("X-Empty", "").is_ok());
    assert!(validate_header("X-Tab", "a\tb").is_ok());
  }

  #[test]
  fn header_rejects_bad_names() {
    for name in ["", "Bad Name", "Bad:Name", "Bad\nName", "Béader"] {
      assert!(validate_header(name, "v").is_err(), "accepted name {name:?}");
    }
    assert!(validate_header(&"a".repeat(129), "v").is_err());
    assert!(validate_header(&"a".repeat(128), "v").is_ok());
  }

  #[test]
  fn header_rejects_injection_in_value() {
    for value in ["a\r\nSet-Cookie: evil=1", "a\rb", "a\nb", "a\0b", "café"] {
      assert!(validate_header("X-Test", value).is_err(), "accepted value {value:?}");
    }
    assert!(validate_header("X-Test", &"v".repeat(1025)).is_err());
    assert!(validate_header("X-Test", &"v".repeat(1024)).is_ok());
  }

  #[test]
  fn header_denylist_is_case_insensitive() {
    for name in ["Location", "location", "CONTENT-LENGTH", "Transfer-Encoding", "connection", "Content-Type", "content-encoding"] {
      assert!(validate_header(name, "v").is_err(), "accepted denylisted {name}");
    }
  }
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p lonk-core header`
Expected: FAIL to compile — `validate_header` not defined.

- [ ] **Step 3: Implement**

Add to `crates/lonk-core/src/lib.rs`:

```rust
/// Maximum header pairs a single link may carry (enforced by API and CLI).
pub const MAX_HEADERS_PER_LINK: usize = 16;

const HEADER_DENYLIST: &[&str] = &[
  "location",
  "content-length",
  "transfer-encoding",
  "connection",
  "content-type",
  "content-encoding",
];

fn is_token_byte(b: u8) -> bool {
  b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b)
}

/// Validate one custom response-header pair for a link.
pub fn validate_header(name: &str, value: &str) -> Result<(), ValidateError> {
  if name.is_empty() || name.len() > 128 {
    return Err(ValidateError::Header(format!(
      "header name must be 1-128 bytes (got {})",
      name.len()
    )));
  }
  if !name.bytes().all(is_token_byte) {
    return Err(ValidateError::Header(format!(
      "invalid header name {name:?}"
    )));
  }
  if HEADER_DENYLIST.contains(&name.to_ascii_lowercase().as_str()) {
    return Err(ValidateError::Header(format!("header not allowed: {name}")));
  }
  if value.len() > 1024 {
    return Err(ValidateError::Header(format!(
      "header value too long ({} bytes, max 1024)",
      value.len()
    )));
  }
  if !value.bytes().all(|b| b == b'\t' || (b' '..=b'~').contains(&b)) {
    return Err(ValidateError::Header(
      "header value contains control or non-ASCII bytes".to_string(),
    ));
  }
  Ok(())
}
```

Extend `ValidateError` and its `Display`:

```rust
pub enum ValidateError {
  Parse(String),
  Scheme(String),
  Header(String),
}
```

```rust
      ValidateError::Header(msg) => write!(f, "{msg}"),
```

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p lonk-core`
Expected: all pass (11 = 7 prior + 4 new).

- [ ] **Step 5: Commit**

```bash
cargo fmt && git add -A && git commit -m "feat: header validation in lonk-core"
```

---

### Task 6: DB — headers column, migration, `get_link`

**Files:**
- Modify: `crates/lonkd/src/db.rs`, `crates/lonkd/src/routes.rs` (insert call site only), `crates/lonkd/Cargo.toml` (dev-dependency tempfile)

**Interfaces:**
- Consumes: nothing new.
- Produces: `Db::insert(&self, id: &str, url: &str, headers_json: &str) -> rusqlite::Result<bool>`; `Db::get_link(&self, id: &str) -> rusqlite::Result<Option<(String, Vec<(String, String)>)>>`. `Db::get_url` unchanged (still used by `/qr` now and `/status` later).

- [ ] **Step 1: Write the failing tests**

Add `tempfile = "3"` to `[dev-dependencies]` in `crates/lonkd/Cargo.toml`.

Extend the `tests` module in `crates/lonkd/src/db.rs` (also update the three existing `db.insert(...)` calls in that module to the 3-arg form with `"[]"`):

```rust
  #[test]
  fn get_link_returns_url_and_headers() {
    let db = Db::open(":memory:").unwrap();
    let headers = r#"[["Set-Cookie","a=1"],["Set-Cookie","b=2"],["X-Demo","x"]]"#;
    assert!(db.insert("abc1234", "https://example.com/", headers).unwrap());
    let (url, parsed) = db.get_link("abc1234").unwrap().unwrap();
    assert_eq!(url, "https://example.com/");
    assert_eq!(
      parsed,
      vec![
        ("Set-Cookie".to_string(), "a=1".to_string()),
        ("Set-Cookie".to_string(), "b=2".to_string()),
        ("X-Demo".to_string(), "x".to_string()),
      ]
    );
  }

  #[test]
  fn get_link_unknown_id_is_none() {
    let db = Db::open(":memory:").unwrap();
    assert_eq!(db.get_link("nope").unwrap(), None);
  }

  #[test]
  fn open_migrates_pre_headers_schema() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("old.db");
    {
      let conn = rusqlite::Connection::open(&path).unwrap();
      conn
        .execute(
          "CREATE TABLE links (id TEXT PRIMARY KEY, url TEXT NOT NULL, created_at TEXT NOT NULL)",
          [],
        )
        .unwrap();
      conn
        .execute(
          "INSERT INTO links VALUES ('old1234', 'https://example.com/old', datetime('now'))",
          [],
        )
        .unwrap();
    }
    let db = Db::open(path.to_str().unwrap()).unwrap();
    assert_eq!(
      db.get_link("old1234").unwrap().unwrap(),
      ("https://example.com/old".to_string(), vec![])
    );
    assert!(db.insert("new1234", "https://example.com/new", "[]").unwrap());
  }
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p lonkd --lib`
Expected: compile FAIL — `insert` arity and missing `get_link`.

- [ ] **Step 3: Implement**

In `crates/lonkd/src/db.rs`:

```rust
  pub fn open(path: &str) -> rusqlite::Result<Self> {
    let conn = Connection::open(path)?;
    conn.execute(
      "CREATE TABLE IF NOT EXISTS links (
         id         TEXT PRIMARY KEY,
         url        TEXT NOT NULL,
         created_at TEXT NOT NULL,
         headers    TEXT NOT NULL DEFAULT '[]'
       )",
      [],
    )?;
    // Databases created before the headers feature lack the column; add it.
    let has_headers: bool = conn
      .prepare("SELECT 1 FROM pragma_table_info('links') WHERE name = 'headers'")?
      .exists([])?;
    if !has_headers {
      conn.execute(
        "ALTER TABLE links ADD COLUMN headers TEXT NOT NULL DEFAULT '[]'",
        [],
      )?;
    }
    Ok(Db(Mutex::new(conn)))
  }

  /// Returns Ok(false) if `id` already exists. `headers_json` is a JSON array
  /// of [name, value] pairs (already validated by the caller).
  pub fn insert(&self, id: &str, url: &str, headers_json: &str) -> rusqlite::Result<bool> {
    let conn = self.0.lock().unwrap();
    let n = conn.execute(
      "INSERT OR IGNORE INTO links (id, url, created_at, headers)
       VALUES (?1, ?2, datetime('now'), ?3)",
      [id, url, headers_json],
    )?;
    Ok(n == 1)
  }

  pub fn get_link(&self, id: &str) -> rusqlite::Result<Option<(String, Vec<(String, String)>)>> {
    let conn = self.0.lock().unwrap();
    let row = conn
      .query_row(
        "SELECT url, headers FROM links WHERE id = ?1",
        [id],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
      )
      .optional()?;
    Ok(row.map(|(url, headers_json)| {
      (url, serde_json::from_str(&headers_json).unwrap_or_default())
    }))
  }
```

In `crates/lonkd/src/routes.rs`, `create_link`: change the insert call to `db.insert(&id, parsed.as_str(), "[]")` (real headers arrive in Task 7).

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p lonkd`
Expected: 7 lib tests pass (`insert_then_get_roundtrip`, `get_unknown_id_is_none`, `insert_duplicate_id_returns_false`, `gen_slug_has_len_and_charset` + the 3 new) and the api suite still passes.

- [ ] **Step 5: Commit**

```bash
cargo fmt && git add -A && git commit -m "feat: headers column with in-place migration and get_link"
```

---

### Task 7: API — accept headers, echo them, attach them to the redirect

**Files:**
- Modify: `crates/lonk-core/src/types.rs`, `crates/lonkd/src/routes.rs`
- Test: `crates/lonkd/tests/api.rs`, `crates/lonk-core/src/types.rs` (tests module)

**Interfaces:**
- Consumes: `validate_header`, `MAX_HEADERS_PER_LINK`, `Db::{insert, get_link}` (Tasks 5–6).
- Produces: `CreateLinkReq { url, #[serde(default)] headers: Vec<(String, String)> }`; `LinkResp { …, headers: Vec<(String, String)> }`; responder `RedirectWithHeaders { location: String, headers: Vec<(String, String)> }` in routes.rs. Task 8's CLI serializes `CreateLinkReq` directly.

- [ ] **Step 1: Extend the wire types with a failing test**

In `crates/lonk-core/src/types.rs`:

```rust
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct CreateLinkReq {
  pub url: String,
  #[serde(default)]
  pub headers: Vec<(String, String)>,
}
```

Add `pub headers: Vec<(String, String)>,` to `LinkResp` too. Update the existing `link_resp_roundtrip` test (add `headers: vec![("X-A".into(), "1".into())]`). Add:

```rust
  #[test]
  fn create_req_headers_default_to_empty() {
    let req: CreateLinkReq = serde_json::from_str(r#"{"url":"https://example.com/x"}"#).unwrap();
    assert_eq!(req.headers, vec![]);
    let req: CreateLinkReq =
      serde_json::from_str(r#"{"url":"https://e.com/x","headers":[["Set-Cookie","a=1"]]}"#)
        .unwrap();
    assert_eq!(req.headers, vec![("Set-Cookie".to_string(), "a=1".to_string())]);
  }
```

Run: `cargo test -p lonk-core` → the new test passes; **the workspace now fails to compile** (routes.rs `LinkResp` literal lacks `headers`) — that's the RED for the server side.

- [ ] **Step 2: Write the failing API tests**

Append to `crates/lonkd/tests/api.rs`:

```rust
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
```

- [ ] **Step 3: Implement**

In `crates/lonkd/src/routes.rs`:

Add the responder (near the top, after `ApiError`):

```rust
/// 303 redirect that also carries a link's stored custom response headers.
pub struct RedirectWithHeaders {
  location: String,
  headers: Vec<(String, String)>,
}

impl<'r> rocket::response::Responder<'r, 'static> for RedirectWithHeaders {
  fn respond_to(self, _req: &'r rocket::Request<'_>) -> rocket::response::Result<'static> {
    let mut builder = rocket::Response::build();
    builder
      .status(Status::SeeOther)
      .raw_header("Location", self.location);
    for (name, value) in self.headers {
      builder.header_adjoin(rocket::http::Header::new(name, value));
    }
    Ok(builder.finalize())
  }
}
```

`create_link` — validate and store headers (after URL validation, before the slug loop):

```rust
  if body.headers.len() > lonk_core::MAX_HEADERS_PER_LINK {
    return Err(api_error(
      Status::BadRequest,
      &format!("too many headers (max {})", lonk_core::MAX_HEADERS_PER_LINK),
    ));
  }
  for (name, value) in &body.headers {
    lonk_core::validate_header(name, value)
      .map_err(|e| api_error(Status::BadRequest, &e.to_string()))?;
  }
  let headers_json = serde_json::to_string(&body.headers)
    .map_err(|_| api_error(Status::InternalServerError, "header encoding failed"))?;
```

change the insert to `db.insert(&id, parsed.as_str(), &headers_json)` and the response literal to include `headers: body.headers.clone(),` (add `let body = body.into_inner();` at the top of the function if field moves fight the borrow checker — then use `body.url`/`body.headers` directly).

`follow_link` — use the stored headers:

```rust
#[rocket::get("/<id>")]
pub fn follow_link(db: &State<Db>, id: &str) -> Result<RedirectWithHeaders, ApiError> {
  match db.get_link(id).map_err(db_error)? {
    Some((url, headers)) => Ok(RedirectWithHeaders {
      location: url,
      headers,
    }),
    None => Err(api_error(Status::NotFound, "no such link")),
  }
}
```

(`Redirect` import becomes unused — remove it.)

- [ ] **Step 4: Run to verify everything passes**

Run: `cargo test --workspace`
Expected: all green, including the 4 new api tests and existing redirect tests (plain links unchanged).

- [ ] **Step 5: Commit**

```bash
cargo fmt && git add -A && git commit -m "feat: per-link response headers on create and redirect"
```

---

### Task 8: CLI `-H/--header`

**Files:**
- Modify: `crates/lonk-cli/src/args.rs`, `crates/lonk-cli/src/main.rs`
- Test: `crates/lonk-cli/tests/cli.rs`, `crates/lonk-cli/tests/cli_live.rs`

**Interfaces:**
- Consumes: `lonk_core::{validate_header, MAX_HEADERS_PER_LINK}`, `lonk_core::types::CreateLinkReq` (Task 7 shape), existing `run_shorten`/`shorten_one`.
- Produces: `args::Cli.headers: Vec<String>` (raw `"Name: value"` strings); `fn parse_headers(raw: &[String]) -> Result<Vec<(String, String)>, String>` in main.rs.

- [ ] **Step 1: Failing tests**

`crates/lonk-cli/src/args.rs` tests module:

```rust
  #[test]
  fn parses_repeated_headers() {
    let cli = Cli::try_parse_from([
      "lonk", "-H", "X-A: 1", "--header", "X-B: 2", "https://a.example",
    ])
    .unwrap();
    assert_eq!(cli.headers, vec!["X-A: 1", "X-B: 2"]);
  }
```

`crates/lonk-cli/tests/cli.rs`:

```rust
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
    .args(["-H", "Location: https://evil.example", "https://example.com/x"])
    .output()
    .unwrap();
  assert_eq!(out.status.code(), Some(1));
  assert!(String::from_utf8_lossy(&out.stderr).contains("not allowed"));
  // config dir untouched proves fail-fast happened before setup/prompt logic
  assert!(!dir.path().join("config.toml").exists());
}
```

`crates/lonk-cli/tests/cli_live.rs` — new test, **own port 8909** per the registry:

```rust
#[test]
fn shorten_with_headers_end_to_end() {
  const PORT_HEADERS: u16 = 8909;
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
      "-H", "X-Demo: 1",
      "-H", "Set-Cookie: a=1",
      "-H", "Set-Cookie: b=2",
      "https://example.com/with-headers",
    ])
    .output()
    .unwrap();
  assert_eq!(out.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&out.stderr));
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
```

(Uses the `start_server(dbdir, port)` / `base(port)` helpers that exist since the port-race fix.)

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p lonk-cli`
Expected: compile FAIL (`cli.headers` missing).

- [ ] **Step 3: Implement**

`crates/lonk-cli/src/args.rs` — add to `Cli`:

```rust
  /// Custom response header for the created link(s), "Name: value" (repeatable)
  #[arg(short = 'H', long = "header", value_name = "NAME: VALUE")]
  pub headers: Vec<String>,
```

`crates/lonk-cli/src/main.rs`:

```rust
/// Parse and validate repeatable -H "Name: value" flags.
fn parse_headers(raw: &[String]) -> Result<Vec<(String, String)>, String> {
  if raw.len() > lonk_core::MAX_HEADERS_PER_LINK {
    return Err(format!(
      "too many headers (max {})",
      lonk_core::MAX_HEADERS_PER_LINK
    ));
  }
  raw
    .iter()
    .map(|h| {
      let (name, value) = h
        .split_once(':')
        .ok_or_else(|| format!("invalid header {h:?}: expected \"Name: value\""))?;
      let (name, value) = (name.trim().to_string(), value.trim().to_string());
      lonk_core::validate_header(&name, &value).map_err(|e| e.to_string())?;
      Ok((name, value))
    })
    .collect()
}
```

In `run_shorten`, before URL validation:

```rust
  let headers = match parse_headers(&cli.headers) {
    Ok(h) => h,
    Err(e) => {
      eprintln!("{e}");
      return EXIT_USAGE;
    }
  };
```

and thread them through: `shorten_one(&base, url, &headers)` with the updated signature/body:

```rust
fn shorten_one(
  base: &str,
  url: &str,
  headers: &[(String, String)],
) -> Result<String, (i32, String)> {
  let resp = ureq::post(&format!("{base}/api/links"))
    .send_json(&lonk_core::types::CreateLinkReq {
      url: url.to_string(),
      headers: headers.to_vec(),
    })
    .map_err(|e| match e {
      // ... unchanged error mapping ...
```

(only the request body construction changes; the error mapping and response handling stay as they are).

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p lonk-cli -- --test-threads=1` then `cargo test --workspace`
Expected: all green including the three new tests.

- [ ] **Step 5: Commit**

```bash
cargo fmt && git add -A && git commit -m "feat: -H/--header flag attaches response headers via the CLI"
```

---

### Task 9: Web UI advanced headers + Playwright spec

**Files:**
- Modify: `crates/lonkd/static/index.html`
- Test: `e2e/tests/lonk.spec.ts`

**Interfaces:**
- Consumes: the API from Task 7.
- Produces: DOM: `<details id="advanced">` containing `#headers` (rows of `.h-name`/`.h-value` inputs) and `#add-header` button.

- [ ] **Step 1: Write the failing Playwright spec**

Append to `e2e/tests/lonk.spec.ts`:

```ts
test('custom response header set via the advanced section', async ({ page, request }) => {
  await page.goto('/');
  await page.click('#advanced summary');
  await page.click('#add-header');
  await page.fill('.h-name', 'X-E2E-Header');
  await page.fill('.h-value', 'hello');
  await page.fill('#url', 'https://example.com/e2e/headers');
  await page.click('button[type=submit]');

  const link = page.locator('#short');
  await expect(link).toBeVisible();
  const href = await link.getAttribute('href');
  const res = await request.get(href!, { maxRedirects: 0 });
  expect(res.status()).toBe(303);
  expect(res.headers()['x-e2e-header']).toBe('hello');
});
```

Run: `cd e2e && npm test`
Expected: the new spec FAILS (`#advanced` not found); the original 5 pass.

- [ ] **Step 2: Implement the UI**

In `crates/lonkd/static/index.html`, after the `<form id="form">…</form>` block, add:

```html
  <details id="advanced">
    <summary>Advanced: response headers</summary>
    <p class="hint">Sent with the 303 redirect response (e.g. Set-Cookie, Cache-Control, Referrer-Policy) — not to the destination.</p>
    <div id="headers"></div>
    <button id="add-header" type="button">Add header</button>
  </details>
```

Add to the `<style>` block:

```css
    #advanced { margin-top: 1rem; }
    #advanced .hint { font-size: 0.85rem; color: #666; }
    .header-row { display: flex; gap: 0.5rem; margin-bottom: 0.5rem; }
    .header-row input { flex: 1; padding: 0.35rem; }
```

In the `<script>`, add before the submit handler:

```js
    const headersBox = document.getElementById('headers');
    document.getElementById('add-header').addEventListener('click', () => {
      const row = document.createElement('div');
      row.className = 'header-row';
      row.innerHTML =
        '<input class="h-name" placeholder="Header-Name">' +
        '<input class="h-value" placeholder="value">';
      headersBox.appendChild(row);
    });

    function collectHeaders() {
      return [...headersBox.querySelectorAll('.header-row')]
        .map((row) => [
          row.querySelector('.h-name').value.trim(),
          row.querySelector('.h-value').value.trim(),
        ])
        .filter(([name]) => name !== '');
    }
```

and change the fetch body to:

```js
          body: JSON.stringify({
            url: document.getElementById('url').value,
            headers: collectHeaders(),
          }),
```

- [ ] **Step 3: Run the suites**

Run: `cd e2e && npm test`
Expected: 6 passed.

Run: `cargo test -p lonkd --test api index_serves_create_form`
Expected: PASS (the form and `#url` are untouched).

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "feat: advanced response-headers section in the web UI"
```

---

### Task 10: `GET /<id>/status` endpoint

**Files:**
- Modify: `crates/lonk-core/src/types.rs`, `crates/lonkd/Cargo.toml`, `crates/lonkd/src/routes.rs`, `crates/lonkd/src/lib.rs`
- Test: `crates/lonkd/tests/api.rs` (404 case), Create: `crates/lonkd/tests/status_live.rs`

**Interfaces:**
- Consumes: `Db::get_url`, `lonk_core::types`.
- Produces: `StatusResp { id: String, url: String, alive: bool, http_status: Option<u16>, error: Option<String> }` in lonk-core (both Options `skip_serializing_if`); route `link_status`; `fn check_destination(url: &str) -> (bool, Option<u16>, Option<String>)` in routes.rs. Task 11's CLI deserializes `StatusResp`.

- [ ] **Step 1: Add StatusResp with a failing serialization test**

`crates/lonk-core/src/types.rs`:

```rust
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct StatusResp {
  pub id: String,
  pub url: String,
  pub alive: bool,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub http_status: Option<u16>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub error: Option<String>,
}
```

Test in the same file:

```rust
  #[test]
  fn status_resp_omits_absent_fields() {
    let json = serde_json::to_string(&StatusResp {
      id: "a".into(),
      url: "https://e.com/".into(),
      alive: false,
      http_status: None,
      error: Some("connect timeout".into()),
    })
    .unwrap();
    assert!(!json.contains("http_status"));
    assert!(json.contains("connect timeout"));
  }
```

Run: `cargo test -p lonk-core status` → PASS (pure addition).

- [ ] **Step 2: Failing endpoint tests**

Append to `crates/lonkd/tests/api.rs`:

```rust
#[test]
fn status_unknown_id_is_404() {
  let client = client();
  let res = client.get("/zzzzzzz/status").dispatch();
  assert_eq!(res.status(), Status::NotFound);
}
```

Create `crates/lonkd/tests/status_live.rs` (this package still has `CARGO_BIN_EXE_lonkd`; **port 8911**; one test fn so one server/port):

```rust
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
```

Add one line to `[dependencies]` in `crates/lonkd/Cargo.toml` (serde_json and the tempfile dev-dependency already exist from earlier tasks):

```toml
ureq = { version = "2", features = ["json"] }
```

(`ureq` is a regular dependency — the endpoint needs it; the live test reuses it.)

- [ ] **Step 3: Run to verify they fail**

Run: `cargo test -p lonkd --test api status` and `cargo test -p lonkd --test status_live`
Expected: FAIL — `/zzzzzzz/status` currently matches no route (404 from the catcher... note: the api test may pass trivially since an unrouted path 404s; the status_live test is the real RED — `/{id}/status` returns 404 for existing links too before implementation).

- [ ] **Step 4: Implement**

In `crates/lonkd/src/routes.rs`:

```rust
use lonk_core::types::StatusResp;

/// Live destination check: (alive, final http status if any, transport error if any).
/// HEAD with a one-shot GET retry on 405/501; <=5 redirects; 5s timeout; alive = final 2xx.
fn check_destination(url: &str) -> (bool, Option<u16>, Option<String>) {
  let agent = ureq::AgentBuilder::new()
    .redirects(5)
    .timeout(std::time::Duration::from_secs(5))
    .build();
  let result = match agent.head(url).call() {
    Err(ureq::Error::Status(405 | 501, _)) => agent.get(url).call(),
    other => other,
  };
  match result {
    Ok(resp) => (resp.status() / 100 == 2, Some(resp.status()), None),
    Err(ureq::Error::Status(code, _)) => (false, Some(code), None),
    Err(ureq::Error::Transport(t)) => (false, None, Some(t.to_string())),
  }
}

#[rocket::get("/<id>/status")]
pub async fn link_status(db: &State<Db>, id: &str) -> Result<Json<StatusResp>, ApiError> {
  let url = match db.get_url(id).map_err(db_error)? {
    Some(url) => url,
    None => return Err(api_error(Status::NotFound, "no such link")),
  };
  let check_url = url.clone();
  // ureq is blocking; keep the 5s worst case off Rocket's async workers.
  let (alive, http_status, error) =
    rocket::tokio::task::spawn_blocking(move || check_destination(&check_url))
      .await
      .map_err(|_| api_error(Status::InternalServerError, "status check failed"))?;
  Ok(Json(StatusResp {
    id: id.to_string(),
    url,
    alive,
    http_status,
    error,
  }))
}
```

Mount it in `crates/lonkd/src/lib.rs`:

```rust
      rocket::routes![
        routes::create_link,
        routes::follow_link,
        routes::qr_svg,
        routes::valid_url,
        routes::link_status
      ],
```

- [ ] **Step 5: Run to verify they pass**

Run: `cargo test -p lonkd` then `cargo test --workspace`
Expected: all green (status_live takes a few seconds: real HTTP + one refused connection).

- [ ] **Step 6: Commit**

```bash
cargo fmt && git add -A && git commit -m "feat: GET /<id>/status live dead-link check"
```

---

### Task 11: CLI `lonk status`

**Files:**
- Modify: `crates/lonk-cli/src/args.rs`, `crates/lonk-cli/src/main.rs`
- Test: `crates/lonk-cli/tests/cli_live.rs`

**Interfaces:**
- Consumes: `GET /<id>/status` (Task 10), `StatusResp`, `resolve_base_url`, `lonk_core::validate_url`.
- Produces: `Cmd::Status { target: String }`; `fn extract_slug(target: &str) -> Result<String, String>`; `fn run_status(target: &str, profile: Option<&str>) -> i32`.

- [ ] **Step 1: Failing tests**

`crates/lonk-cli/src/args.rs` tests:

```rust
  #[test]
  fn parses_status_subcommand() {
    let cli = Cli::try_parse_from(["lonk", "status", "Ab3dEf9"]).unwrap();
    match cli.cmd {
      Some(Cmd::Status { target }) => assert_eq!(target, "Ab3dEf9"),
      other => panic!("expected status, got {other:?}"),
    }
  }
```

`crates/lonk-cli/tests/cli_live.rs` — new test, **own port 8910**:

```rust
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
  assert_eq!(out.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&out.stderr));
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
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p lonk-cli`
Expected: compile FAIL (`Cmd::Status` missing).

- [ ] **Step 3: Implement**

`crates/lonk-cli/src/args.rs` — extend `Cmd`:

```rust
  /// Check whether a short link's destination is alive
  Status {
    /// Slug or full short URL (e.g. Ab3dEf9 or https://s.example.com/Ab3dEf9)
    target: String,
  },
```

`crates/lonk-cli/src/main.rs` — dispatch arm in `run`:

```rust
    Some(Cmd::Status { ref target }) => run_status(target, cli.profile.as_deref()),
```

(adjust the existing `match cli.cmd` to match on `&cli.cmd`/`ref` patterns as needed so `cli` stays usable — simplest is `match &cli.cmd` with `Some(Cmd::Setup { base_url }) => run_setup(base_url.clone(), …)`.)

New functions:

```rust
/// A bare slug passes through; a full short URL contributes its last path segment.
fn extract_slug(target: &str) -> Result<String, String> {
  if !target.starts_with("http://") && !target.starts_with("https://") {
    return Ok(target.to_string());
  }
  let parsed = lonk_core::validate_url(target).map_err(|e| e.to_string())?;
  parsed
    .path_segments()
    .and_then(|segments| segments.filter(|s| !s.is_empty()).last())
    .map(String::from)
    .ok_or_else(|| format!("no slug in url {target:?}"))
}

fn run_status(target: &str, profile: Option<&str>) -> i32 {
  let slug = match extract_slug(target) {
    Ok(slug) => slug,
    Err(e) => {
      eprintln!("{e}");
      return EXIT_USAGE;
    }
  };
  let base = match resolve_base_url(profile) {
    Ok(base) => base,
    Err(code) => return code,
  };
  match ureq::get(&format!("{base}/{slug}/status")).call() {
    Ok(resp) => {
      let body: lonk_core::types::StatusResp = match resp.into_json() {
        Ok(body) => body,
        Err(e) => {
          eprintln!("bad response: {e}");
          return EXIT_NETWORK;
        }
      };
      if body.alive {
        println!("alive ({})", body.http_status.unwrap_or(0));
        EXIT_OK
      } else {
        match (body.http_status, body.error) {
          (Some(code), _) => println!("dead ({code})"),
          (None, Some(err)) => println!("dead ({err})"),
          (None, None) => println!("dead"),
        }
        EXIT_USAGE
      }
    }
    Err(ureq::Error::Status(404, _)) => {
      eprintln!("no such link: {slug}");
      EXIT_USAGE
    }
    Err(ureq::Error::Status(code, _)) => {
      eprintln!("server returned {code}");
      EXIT_NETWORK
    }
    Err(ureq::Error::Transport(t)) => {
      eprintln!("{t}");
      EXIT_NETWORK
    }
  }
}
```

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p lonk-cli -- --test-threads=1` then `cargo test --workspace`
Expected: all green.

- [ ] **Step 5: Commit**

```bash
cargo fmt && git add -A && git commit -m "feat: lonk status subcommand"
```

---

### Task 12: Playwright /status contract spec

**Files:**
- Test: `e2e/tests/lonk.spec.ts`

**Interfaces:**
- Consumes: `GET /<id>/status` (Task 10), `POST /api/links`.

- [ ] **Step 1: Write and run the spec**

Append to `e2e/tests/lonk.spec.ts`:

```ts
test('link status reports alive and dead destinations', async ({ request, baseURL }) => {
  const alive = await request.post('/api/links', { data: { url: `${baseURL}/` } });
  const aliveId = (await alive.json()).id;
  const res1 = await request.get(`/${aliveId}/status`);
  expect(res1.status()).toBe(200);
  const body1 = await res1.json();
  expect(body1.alive).toBe(true);
  expect(body1.http_status).toBe(200);

  const dead = await request.post('/api/links', { data: { url: `${baseURL}/zzzzzzz` } });
  const deadId = (await dead.json()).id;
  const body2 = await (await request.get(`/${deadId}/status`)).json();
  expect(body2.alive).toBe(false);
  expect(body2.http_status).toBe(404);

  const missing = await request.get('/zzzzzzz/status');
  expect(missing.status()).toBe(404);
});
```

Run: `cd e2e && npm test`
Expected: 7 passed.

- [ ] **Step 2: Commit**

```bash
git add -A && git commit -m "test: e2e coverage for /<id>/status"
```

---

### Task 13: README

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Update the README**

Apply these changes to the existing `README.md`:

1. Under the Server section's install/run text, note the workspace layout and update the API table:

```markdown
| Endpoint            | Description                                               |
| ------------------- | --------------------------------------------------------- |
| `POST /api/links`   | `{"url": "https://…", "headers": [["Name","value"], …]}` → `201` with `{id, url, short_url, qr_url, headers}` (`headers` optional, max 16 pairs) |
| `POST /api/valid`   | `{"url": "…"}` → `200` with `{valid: true}` or `{valid: false, error}` |
| `GET /<id>`         | `303` redirect to the original URL, carrying the link's custom response headers |
| `GET /<id>/qr`      | SVG QR code for the short link                             |
| `GET /<id>/status`  | Live dead-link check → `200` with `{id, url, alive, http_status?, error?}`; `404` if the slug is unknown |
```

2. Add after the API table:

```markdown
### Custom response headers

Headers stored on a link are attached to the **303 redirect response** —
they are not (and cannot be) injected into the request your browser then
makes to the destination. Useful values: `Set-Cookie` (cookie on the lonk
domain), `Cache-Control` (redirect cacheability), `Referrer-Policy`
(e.g. `no-referrer` strips the Referer sent onward), `X-*` for
proxies/middleware. Structural headers (`Location`, `Content-Length`,
`Transfer-Encoding`, `Connection`, `Content-Type`, `Content-Encoding`)
are rejected.

### Dead-link checks

`GET /<id>/status` makes the server probe the stored destination (HEAD,
falling back to GET on 405/501, up to 5 redirects, 5s timeout); the
destination is alive when the final response is 2xx. Note: this means
anyone who can reach your lonkd can make it issue requests to stored
URLs and see whether they answered — on a self-hosted instance this is
the same trust level as creating links.
```

3. In the CLI section, extend the examples:

```markdown
lonk -H "Set-Cookie: seen=1; Path=/" -H "Cache-Control: no-store" https://example.com/a
lonk status Ab3dEf9                        # or: lonk status https://s.example.com/Ab3dEf9
                                           # prints "alive (200)" / "dead (404)" / "dead (<error>)"
                                           # exit 0 alive · 1 dead/unknown · 2 server unreachable
```

4. In the Development section, update the install hint and add the layout:

```markdown
The workspace has three crates: `crates/lonk-core` (validation + wire
types), `crates/lonkd` (the server), `crates/lonk-cli` (the `lonk`
binary — install with `cargo install --path crates/lonk-cli`).
```

- [ ] **Step 2: Verify the documented commands**

Run: `cargo run -p lonkd --bin lonkd` briefly with `LONK_CONFIG_DIR` set to a temp dir, plus `cargo run -p lonk-cli --bin lonk -- --valid https://x.example` — confirm every README command/flag exists as documented. Kill the server.

- [ ] **Step 3: Commit**

```bash
git add README.md && git commit -m "docs: document headers, status checks, and the crate layout"
```
