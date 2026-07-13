# Future: CLI polish

Status: future intent — small, independent fixes; could ship as one small PR
without a full spec cycle. Sources: deferred minors from the 2026-07-11/12
review waves (see git history and past specs in `plans/`).

## Items

1. **`lonk --valid` with zero URLs exits 0** (vacuous success).
   `run_valid` loops zero times and returns `EXIT_OK`
   (`crates/lonk-cli/src/main.rs`). Mirror the shorten guard: empty URL list
   → usage error, exit 1, stderr mentions URL. Covering test beside
   `qr_flag_without_urls_is_a_usage_error` in `crates/lonk-cli/tests/cli.rs`.

2. **`--version` is not wired.** `main`'s clap error mapping handles
   `ErrorKind::DisplayVersion`, but `#[command(...)]` in
   `crates/lonk-cli/src/args.rs` never sets `version`, so `lonk --version`
   is an "unexpected argument" error (exit 1). Add `version` to the command
   attrs (pulls from Cargo.toml); the existing exit-0 mapping then becomes
   reachable. Test: `--version` exits 0 and prints a version.

3. **`Config::save` is not atomic.** `crates/lonk-cli/src/config.rs` writes
   with `fs::write`; a crash mid-write can corrupt `config.toml`. Write to a
   temp file in the same directory, then rename. Low value until profiles
   grow — bundle it here rather than doing it alone.

4. **`shorten_one`'s HTTP-status error branch has no end-to-end test.**
   The `ureq::Error::Status` arm (server returns non-2xx to create) is
   covered only by inspection; the live tests exercise the transport arm.
   Cheapest coverage: a cli_live case creating a link with a header the
   server rejects — except the CLI pre-validates headers, so instead point
   the CLI at the lonkd `/api/valid`-style path... simplest honest option:
   a raw TcpListener fixture returning a canned 500 (same pattern as
   lonkd's `check_destination` 405 test) and assert exit 2 + the server's
   error message on stderr.

5. **`qr_unicode`/`qr_svg` duplicate `QrCode::new(...).map_err(...)`**
   (`crates/lonk-cli/src/main.rs`). Extract the shared construction.

## Port note

Any new server-spawning live test must take a fresh port; registry so far:
8907–8912 (tests), 8808 (Playwright), 8913 (manual checks only).
