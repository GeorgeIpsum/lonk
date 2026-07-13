# Design: `lonk` CLI client + Playwright e2e tests

Date: 2026-07-11
Status: approved (pending spec review)

## Goal

Two additions to the lonk link shortener:

1. Browser-driven e2e tests with Playwright covering the web UI and API.
2. A CLI client, `lonk`, with one-time setup pointing at a hosted lonk instance.

## Binary naming (breaking change)

The CLI takes the name `lonk`. The server binary is therefore renamed to
**`lonkd`**. Anything that launches the server by binary name (`cargo run
--bin`, deploy scripts, systemd units) must use `lonkd` after this change.

## Workspace layout

Convert the repo to a Cargo workspace with minimal churn:

```
Cargo.toml            # workspace root + lonk package (server, unchanged layout)
src/main.rs           # server entry point -> [[bin]] name = "lonkd"
src/cli/main.rs       # CLI entry point   -> [[bin]] name = "lonk"
src/cli/              # CLI modules (args, config, output)
crates/lonk-validate/ # shared URL-validation crate
e2e/                  # Playwright project (npm, isolated from cargo)
```

The `lonk` package depends on `lonk-validate` by path. The CLI is a second
binary target in the same package (user decision), which means the package
gains the CLI's dependencies (`clap`, `ureq`). Acceptable; can be split into
its own crate later if compile times become a concern.

## `lonk-validate` crate

One function, one error type:

```rust
pub fn validate_url(input: &str) -> Result<Url, ValidateError>
```

Rules (exactly what `create_link` enforces today): input must parse with
`url::Url`, and the scheme must be `http` or `https`. `ValidateError` carries
a human-readable message. Server and CLI both call this function, so local
pre-validation in the CLI can never disagree with the server.

## Server changes

- Default database location: when `LONK_DB` is not set, `lonkd` stores the
  SQLite database alongside the settings, at `<config-dir>/lonk.db` where
  `<config-dir>` is the same directory the CLI config resolves to
  (`$LONK_CONFIG_DIR`, else `$XDG_CONFIG_HOME/lonk`, else `~/.config/lonk`).
  The directory is created on startup if missing. `LONK_DB` still overrides
  (tests and e2e rely on this).
- `routes::create_link` refactors to call `lonk_validate::validate_url`;
  behavior and response shapes are unchanged.
- New endpoint `POST /api/valid`, body `{"url": "<string>"}`:
  - `200 {"valid": true}` when the string is a valid shortenable URL
  - `200 {"valid": false, "error": "<why>"}` when it is not
  - Invalidity is data, not an HTTP error; malformed JSON still 400s via the
    existing catcher.

## CLI: `lonk`

### Grammar

```
lonk                          # zero args -> print help
lonk <url>...                 # shorten each URL, one short URL per line, input order
lonk setup [<base-url>]       # write config; prompts for the URL if omitted

Flags:
  --qr              after each short link, print a unicode QR code
  --qr-svg          print the QR as SVG text to stdout (exactly one URL required)
  --valid           validate-only mode: no network, no config needed
  --profile <name>  select/configure a named profile (default: "default")
  -h, --help        help
```

`--qr` and `--qr-svg` are mutually exclusive. Positional URLs are always
`http(s)://...`, so the `setup` subcommand cannot collide with a URL argument.

### Config and profiles

Config lives at `~/.config/lonk/config.toml` (respects `$XDG_CONFIG_HOME`;
`LONK_CONFIG_DIR` env var overrides the directory, used by tests):

```toml
[profiles.default]
base_url = "https://s.example.com"

[profiles.work]
base_url = "https://links.corp.example"
```

- `--profile <name>` on a shorten command selects that profile and **errors
  if it does not exist**. No flag means the `default` profile.
- `lonk setup --profile <name> [<base-url>]` configures that profile;
  without the flag it configures `default`. `setup` serves both first-time
  configuration and reconfiguration.
- Shortening when the `default` profile is missing (no `--profile` given):
  if stdin is a TTY, prompt interactively for the base URL and save it as the
  `default` profile; if not a TTY (piped/CI), exit with an error and a
  `lonk setup <url>` hint. An explicit `--profile <name>` never prompts — a
  missing named profile is always an error (see above).

### Behavior

1. URL arguments are sent as-is; no percent-decoding.
2. Before any network call, all URL arguments are validated locally with
   `lonk-validate`. The first invalid one prints an error to stderr and the
   process exits 1 with nothing sent (fail fast). If all are valid,
   shortening proceeds, still failing fast on the first server/network error.
3. `--valid`: prints one line per input (`valid` or `invalid: <reason>`);
   exit 0 only if all inputs are valid. No config or network required.
4. Success output: the full short URL (`<base_url>/<id>`) per line, in input
   order, to stdout.
5. `--qr`: after each short link, the QR code rendered as unicode blocks
   (scannable in-terminal), with a blank line between entries when multiple
   URLs are given. QR content is the full short URL — the same content the
   server's `/qr` endpoint encodes — generated locally with the `qrcode`
   crate already in the dependency tree (no extra HTTP round trip).
6. `--qr-svg`: requires exactly one URL (usage error otherwise). Prints the
   SVG text to stdout so it can be piped (`> code.svg`). Multi-URL SVG output
   was considered (auto-writing `<id>.svg` files) and rejected: implicit file
   creation is surprising, stdout stays composable.
7. HTTP client: `ureq` (small, blocking). Argument parsing: `clap` (derive).

### Exit codes

- `0` success
- `1` validation, usage, or configuration errors
- `2` network or server errors

## Playwright e2e (`e2e/`)

Self-contained npm project with `@playwright/test` and TypeScript.

- Playwright's `webServer` builds and spawns the real server
  (`cargo run --bin lonkd`) with `LONK_DB` pointed at a temp file, so
  `npx playwright test` is a single command and CI-friendly.
- A `BASE_URL` env var overrides the target to smoke-test a deployed
  instance. Tests do not assume a clean DB — each creates its own links.

Specs (Chromium, driving the real UI):

- Create a link through the form: the short URL appears, is absolute, and
  following it redirects to the target.
- The QR image for a created link renders (`/<id>/qr` returns SVG).
- Submitting an invalid URL shows the error message in the UI.
- An unknown slug returns a 404 JSON error.
- `POST /api/valid` contract test via Playwright's `request` fixture
  (valid and invalid cases).

## CLI testing

- Unit tests: config parsing and profile selection, argument grammar,
  output formatting.
- Integration test (Rust): spawn the compiled server (`CARGO_BIN_EXE_lonkd`)
  on a test port with a temp DB, then run the compiled CLI
  (`CARGO_BIN_EXE_lonk`) with `LONK_CONFIG_DIR` set to a temp dir. Covers
  setup -> shorten -> `--qr-svg` -> `--valid` end-to-end over real HTTP.

## Documentation

README gains:

- a CLI section: install, `lonk setup`, profiles, flags, examples;
- an e2e section: `cd e2e && npm ci && npx playwright test`;
- a note that the server binary is now `lonkd`.
