# Design: 3-crate restructure, per-link response headers, link status endpoint

Date: 2026-07-11
Status: approved (pending spec review)
Branch: `feat/headers-and-status`, stacked on `feat/cli-and-e2e` (PR #2). If PR #2
merges first, rebase onto main.

## Goal

Three pieces of work, in order:

0. Restructure the workspace into three crates — core, server, CLI — so the
   binaries stop sharing one dependency tree and the API contract lives in
   one place.
1. Per-link custom HTTP headers, attached to the redirect **response**.
2. `GET /<id>/status`: a live check reporting whether a short link's
   destination is dead or alive.

The restructure goes first because both features touch exactly the seams it
formalizes (shared validation, shared wire types, per-binary dependencies —
e.g. the server gains `ureq` for status checks, which must not bloat the CLI
and vice versa).

## Phase 0: 3-crate restructure

### Layout

```
Cargo.toml                 # virtual workspace manifest, members = ["crates/*"]
crates/lonk-core/          # renamed/absorbed lonk-validate + wire types + paths
crates/lonkd/              # server: rocket, rusqlite; owns static/ and tests/api.rs
crates/lonk-cli/           # CLI: clap, ureq, toml, qrcode; binary named `lonk`
e2e/                       # unchanged location
```

### `lonk-core` (rename of `lonk-validate`)

Pure library, no I/O. Dependencies: `url`, `serde` (derive).

- `validate_url(input: &str) -> Result<Url, ValidateError>` — unchanged.
- `validate_header(name: &str, value: &str) -> Result<(), ValidateError>` — new
  (Phase 1, listed here because it lives in this crate).
- `paths::config_dir() -> PathBuf` — moves here from the old root lib
  (resolution: `$LONK_CONFIG_DIR` → `$XDG_CONFIG_HOME/lonk` → `~/.config/lonk`).
- `types` module: the wire contract, plain serde derives
  (`Serialize`, `Deserialize`, `Debug`, `PartialEq`):
  - `CreateLinkReq { url: String, #[serde(default)] headers: Vec<(String, String)> }`
  - `LinkResp { id, url, short_url, qr_url: String, headers: Vec<(String, String)> }`
  - `ValidResp { valid: bool, #[serde(skip_serializing_if = "Option::is_none")] error: Option<String> }`
  - `StatusResp { id: String, url: String, alive: bool,`
    `#[serde(skip_serializing_if = "Option::is_none")] http_status: Option<u16>,`
    `#[serde(skip_serializing_if = "Option::is_none")] error: Option<String> }`

  A `Vec<(String, String)>` serializes as a JSON array of `[name, value]`
  pairs — the chosen header wire shape (order-preserving, repeats allowed).
  The server consumes these types through `rocket::serde::json::Json<T>`
  (works with plain serde derives since serde is a direct dependency of
  lonk-core); the CLI deserializes responses into them instead of poking
  `serde_json::Value`.

### `lonkd` (server package)

Package `lonkd`, binary `lonkd`. Dependencies: `rocket`, `rusqlite`,
`qrcode` (svg), `lonk-core`, and (Phase 2) `ureq`.

- `src/main.rs`, `src/lib.rs` (`rocket_app`), `src/db.rs`, `src/routes.rs`
  move here from the root package.
- `static/` moves to `crates/lonkd/static/` — `FileServer::from(relative!("static"))`
  resolves against `CARGO_MANIFEST_DIR`, so it moves with the crate.
- `tests/api.rs` moves here.
- The old root-lib `Db`/`routes` API is otherwise unchanged in this phase.

### `lonk-cli` (CLI package)

Package `lonk-cli`, `[[bin]] name = "lonk"`. Dependencies: `clap` (derive),
`ureq` (json), `serde`, `serde_json`, `toml`, `qrcode`, `lonk-core`;
dev-dependency `tempfile`.

- `src/cli/{main,args,config}.rs` move to `crates/lonk-cli/src/`.
- `tests/cli.rs` and `tests/cli_live.rs` move here.
- **Cross-package binary in tests:** `CARGO_BIN_EXE_lonkd` is unavailable
  outside the lonkd package. `cli_live.rs` locates the server binary from the
  test executable's own path: `current_exe()` → `target/debug/deps/…` →
  `parent().parent().join("lonkd")`, panicking with the message
  `lonkd binary not found — run: cargo test --workspace (or cargo build -p lonkd)`
  if missing. `cargo test --workspace` builds all workspace binaries, so the
  documented dev command keeps working.

### Root and docs

- Root `Cargo.toml` becomes a virtual manifest: `[workspace] members = ["crates/*"]`,
  `resolver = "2"`. Root `src/` and `tests/` are removed after the moves.
- `e2e/playwright.config.ts`: `command` stays `cargo run --bin lonkd` (from a
  virtual workspace root cargo resolves the unique `lonkd` bin target).
- README: install becomes `cargo install --path crates/lonk-cli`; server run
  command unchanged; paths updated.
- Acceptance for Phase 0: `cargo test --workspace` green with the same test
  count as before the move; `cd e2e && npm test` 5/5; `lonkd` no longer
  compiles clap/toml; `lonk` no longer compiles rocket/rusqlite.

## Phase 1: per-link response headers

### Semantics (what this is and is not)

Stored headers are attached to the **303 redirect response** lonk serves for
`GET /<id>`. They are NOT injected into the request the client then makes to
the destination — HTTP redirects cannot do that. Useful with `Set-Cookie`
(cookie on the lonk domain), `Cache-Control` (redirect cacheability),
`Referrer-Policy` (e.g. `no-referrer` strips the Referer the browser sends
onward), and `X-*` headers for proxies/middleware.

### Validation (`lonk_core::validate_header`)

- Name: RFC 7230 token — ASCII alphanumerics or ``!#$%&'*+-.^_`|~`` — 1..=128
  bytes.
- Value: 0..=1024 bytes; every byte must be visible ASCII, space, or tab
  (no CR, LF, NUL — header-injection guard).
- Denylist (case-insensitive): `location`, `content-length`,
  `transfer-encoding`, `connection`, `content-type`, `content-encoding`.
- Per-link limit: at most 16 header pairs (enforced at the API/CLI boundary,
  message `too many headers (max 16)`).
- Errors are a new `ValidateError::Header(String)` variant carrying the
  human-readable reason.

### Storage

`links` gains `headers TEXT NOT NULL DEFAULT '[]'` — the JSON array of
`[name, value]` pairs, serialized with serde_json from `Vec<(String, String)>`.
`Db::open` runs a guarded migration after `CREATE TABLE IF NOT EXISTS`:
check `PRAGMA table_info(links)` for the column and `ALTER TABLE links ADD
COLUMN headers TEXT NOT NULL DEFAULT '[]'` when absent. Existing DBs upgrade
in place; their links behave as before (no headers). `Db::insert` takes the
headers JSON string; a new `Db::get_link(id) -> Option<(String, Vec<(String, String)>)>`
returns url + parsed headers (replacing `get_url` where the redirect needs
both; `get_url` remains for `/qr` and `/status`).

### API

- `POST /api/links` body is now `CreateLinkReq { url, headers }` (`headers`
  defaults to empty — existing clients unaffected). Each pair is validated;
  the first invalid one → 400 `{ "error": "<why>" }`. Response is `LinkResp`
  including the echoed `headers`.
- `GET /<id>`: the handler returns a custom responder `RedirectWithHeaders`
  (303, `Location`, then each stored header appended via
  `rocket::http::Header`). Repeats like double `Set-Cookie` are appended, not
  merged.

### CLI

- New repeatable flag `-H, --header "Name: value"` on the shorten grammar.
  Parsed at the first `:`; both sides trimmed; parse failure or validation
  failure (via `lonk_core::validate_header`, checked locally before any
  network call, fail-fast) → exit 1.
- The same header set applies to every URL in the invocation (curl
  semantics).
- Like the existing top-level flags (`--qr` etc.), `-H` is defined on the
  root grammar and has no effect when a subcommand (`setup`, `status`) is
  given — same silent-ignore behavior, no new special-casing.

### Web UI

`static/index.html` gains a collapsed `<details>` "Advanced: response
headers" section: rows of name/value inputs plus an "add header" button
(plain DOM, no framework — matches the existing page). Non-empty rows are
sent as the `headers` array. Server-side validation errors render in the
existing `#error` element.

## Phase 2: `GET /<id>/status`

### Behavior

- Unknown slug → 404 JSON error (existing catcher shape).
- Known slug → the server performs a live check of the stored destination
  and returns **200** with `StatusResp` — deadness is data, mirroring
  `/api/valid`:
  - `{"id", "url", "alive": true, "http_status": 200}`
  - `{"id", "url", "alive": false, "http_status": 404}`
  - `{"id", "url", "alive": false, "error": "<transport error>"}`
- Check semantics: `HEAD`, follow at most 5 redirects, 5-second timeout;
  if the destination answers 405 or 501 to HEAD, retry once with `GET`.
  **Alive = final status 2xx.** Anything else (4xx/5xx final status,
  transport failure, timeout, redirect loop) is dead, with `http_status` or
  `error` saying why.
- Implementation: `ureq` in the lonkd crate. The handler is `async` and
  wraps the blocking call in `rocket::tokio::task::spawn_blocking` so a slow
  destination cannot pin a Rocket worker thread.
- Accepted risk (documented in README): `/status` causes the server to issue
  requests to stored URLs and reveal whether they answered — a limited probe
  oracle. On a self-hosted instance this is the same trust level as link
  creation itself. No allow/deny policy in this iteration; `lonk-core` is
  the natural home for one later.

### CLI

`lonk status <id-or-short-url>` (exactly one argument; `--profile` honored):

- Accepts a bare slug or a full short URL (if the argument parses as an
  http(s) URL, the slug is its last path segment).
- Calls `GET {base}/{id}/status`, deserializes `StatusResp`.
- Output: `alive (200)` / `dead (404)` / `dead (connect timeout)` to stdout.
- Exit codes: 0 alive; 1 dead or unknown slug (message names the slug);
  2 cannot reach the lonk server itself.

No web UI surface for status.

## Testing

- `lonk-core` unit tests: `validate_header` (valid tokens, bad name chars,
  CRLF/NUL injection attempts, denylist case-insensitivity, length limits).
  The 16-pair limit is enforced by callers (API handler, CLI), tested at
  those boundaries.
- `crates/lonkd/tests/api.rs`:
  - create with headers → response echoes them; `GET /<id>` carries them
    (including two `Set-Cookie` pairs both present);
  - invalid header name/value/denylisted → 400 with message;
  - link without headers behaves exactly as today;
  - migration: build a DB file with the pre-headers schema, `Db::open` it,
    insert+redirect works.
- Live tests (`crates/lonk-cli/tests/cli_live.rs`, new test with its own
  port per the one-port-per-server-spawning-test invariant): spawned lonkd
  is its own status target — link to its `/` → alive (200); link to its
  `/zzzzzzz` → dead with `http_status` 404; link to `http://127.0.0.1:1` →
  dead with transport `error`. Same file covers `lonk status` (slug and
  full-URL forms) and `-H` end-to-end (create with header via CLI, assert
  the redirect response carries it).
- Playwright (`e2e/tests/lonk.spec.ts`):
  - UI: open the advanced section, add a header, submit, assert the created
    short link's redirect response carries the header (request fixture,
    `maxRedirects: 0`);
  - `/status` contract: link to `baseURL` itself → alive; link to
    `baseURL + '/zzzzzzz'` → dead 404.

## Documentation

README: headers section (semantics + what response headers can and cannot
do), `-H` examples, `lonk status` examples, `/api/links` request shape and
`/<id>/status` rows in the API table, the probe-oracle note, and updated
install/run commands from Phase 0.
