# Future: frontend + tooling hygiene

Status: future intent — low-priority cleanups from the 2026-07-12 review
waves. Independent; batch as convenient.

## Items

1. **`lonk-client` resolves `undefined` on a 2xx non-JSON body.**
   `packages/lonk-client/src/index.ts` — `request()` does
   `res.json().catch(() => undefined)` and, on `res.ok`, returns that as `T`.
   Against real lonkd every 2xx is JSON, so this is latent; a captive
   portal / misbehaving proxy returning 200 + HTML would yield
   `undefined as LinkResponse`. Fix: on the ok path, if the parsed body is
   `undefined`, throw `LonkError('invalid response body')`.

2. **Playwright temp DBs accumulate.** `e2e/playwright.config.ts` points
   `LONK_DB` at `os.tmpdir()/lonk-e2e-<pid>.db` with no teardown, so each
   run leaves a file behind. Add a `globalTeardown` that unlinks it, or use
   a `test-results/`-relative path that the existing gitignore/cleanup
   already sweeps.

3. **Lockfile nests Playwright under `e2e/node_modules`** rather than
   hoisting to the workspace root. Benign (smoke-tested; regenerates on
   install), noted only so a future "why isn't this hoisted" question has
   an answer: npm chose it, not us. No action unless it causes a real
   resolution problem.

4. **`web_assets` dir branch does sync `fs::read`/`canonicalize` on Rocket
   worker threads** (`crates/lonkd/src/web_assets.rs`, the `WebSource::Dir`
   path). Fine for small UI assets; if a bespoke `LONK_WEB_DIR` ever holds
   large files, wrap the read in `rocket::tokio::task::spawn_blocking`
   (same pattern as `link_status`). Add a code comment noting the
   assumption in the meantime.

## Not worth a doc on their own

Cosmetic doc-example nits (e.g. bundler-vs-raw framing in
`docs/guide/customizing.md`) were already fixed in the final review wave;
nothing outstanding there.
