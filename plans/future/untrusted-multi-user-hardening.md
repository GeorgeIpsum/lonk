# Future: hardening for untrusted / multi-user deployments

Status: future intent — STRATEGIC, not a cleanup. Only relevant if lonk ever
serves users you don't fully trust (public instance, shared multi-tenant).
On a single-user self-hosted box this is explicitly out of scope and was
accepted as such in the headers/status spec.

## Why this is deferred, not ignored

The current trust model is "anyone who can create a link is trusted" — which
is correct for a personal self-hosted shortener. Two surfaces only become
problems under a different trust model:

1. **`/status` probe oracle + SSRF** (`check_destination` in
   `crates/lonkd/src/routes.rs`). The server issues HEAD/GET to any stored
   URL and reports reachability. An untrusted submitter can use it to probe
   internal hosts (`http://169.254.169.254/…`, `http://10.x/…`) and read
   liveness. Flagged by the automated security scanner; accepted in the spec
   with a README disclosure.

2. **Open redirect.** `GET /<id>` 303s to any stored http(s) URL. Fine when
   link creators are trusted; a vector for phishing/redirect laundering when
   they are not.

## Proposed direction (when triggered)

`lonk-core` is the designed home for a URL allow/deny policy — a single
`validate_url` extension both `create_link` and `check_destination` consult:

- Reject/allow by host: block RFC1918, loopback, link-local, ULA,
  `169.254.0.0/16`, `::1`, `fc00::/7` by default when a "public mode" flag
  is on; optional explicit allowlist.
- Resolve-then-check for `check_destination` to defeat DNS-rebinding
  (validate the resolved IP, not just the hostname), and pin redirects to
  the same policy at each hop.
- A `lonkd` config/env switch (e.g. `LONK_PUBLIC=1`) that turns the strict
  policy on; default off preserves today's self-hosted behavior.

## Also in scope if this is ever built

- Rate limiting on `POST /api/links` and `/status`.
- Auth on link creation (the current API is unauthenticated by design).

## Trigger

Any plan to expose lonk to link creators you don't personally trust. Until
then, the README note is the correct treatment.
