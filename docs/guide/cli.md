---
title: The lonk CLI
---

[← lonk guide](index.md)

# The `lonk` CLI

## Install

```bash
cargo install --path crates/lonk-cli
```

Installs the `lonk` binary (crate name `lonk-cli`, binary name `lonk`) to
your cargo bin directory.

## Setup

`lonk` talks to a server through a **profile** — a name mapped to a base
URL, stored in `config.toml` under the config directory (see the
environment variable table in [Running the server](server.md)):
`$LONK_CONFIG_DIR`, else `$XDG_CONFIG_HOME/lonk`, else `~/.config/lonk`.

```bash
lonk setup https://s.example.com
```

writes the `default` profile:

```
profile 'default' -> https://s.example.com saved to /home/you/.config/lonk/config.toml
```

The base URL is validated (must parse as `http://`/`https://`, and must
not carry a query string or fragment) and trimmed of a trailing slash
before saving.

**Interactive first run:** if you shorten a URL before ever running
`setup`, and no `default` profile exists, and stdin is a terminal, `lonk`
prompts once instead of failing:

```
Base URL of your lonk server: 
```

Whatever you type is validated and saved as the `default` profile, then
the run continues. If stdin isn't a terminal (e.g. in a script or CI) and
no profile exists, `lonk` exits with an error instead of hanging:

```
no server configured; run: lonk setup <base-url>
```

**`--profile`** targets a named profile instead of `default`, for setup
and for every other subcommand:

```bash
lonk setup --profile work https://links.corp.example
lonk --profile work https://example.com/for-work
lonk --profile work status Ab3dEf9
```

Shortening (or `status`) against a profile that was never set up fails
fast with the exact command to fix it:

```
profile 'work' not found; run: lonk setup --profile work <base-url>
```

## Shortening URLs

```bash
lonk https://example.com/some/very/long/url
```

prints the full short URL on stdout:

```
https://s.example.com/Ab3dEf9
```

Pass multiple URLs to shorten them all in one invocation; they're
processed **in input order** and each short link is printed on its own
line as soon as it's created:

```bash
lonk https://example.com/a https://example.com/b
```

```
https://s.example.com/Nbn3yAv
https://s.example.com/szE6G7s
```

This is **fail-fast**: local validation runs for every URL (and every
`-H` header) before any network request is made, so a bad URL anywhere in
the list aborts the whole run before anything is created; a request that
fails partway through (e.g. the server rejects the third of five URLs)
stops there — the short links already printed for the first two were
already created.

## Custom response headers (`-H`)

```bash
lonk -H "Set-Cookie: seen=1; Path=/" -H "Cache-Control: no-store" https://example.com/a
```

`-H`/`--header` takes `"Name: value"` and is repeatable, up to 16 pairs
(over the limit: `too many headers (max 16)`). Each pair is parsed and
validated locally — name 1–128 bytes of HTTP token characters, not one of
the six structural headers, value ≤1024 visible-ASCII bytes with no
CR/LF/NUL — **before** any network traffic; the full rules and worked
example are in [Custom response headers](headers.md).

## QR codes

```bash
lonk --qr https://example.com/a       # short link, then a scannable QR in the terminal
lonk --qr-svg https://example.com/a > code.svg   # short link to stderr, SVG to stdout
```

`--qr` and `--qr-svg` are mutually exclusive. `--qr-svg` requires exactly
one URL (`--qr-svg requires exactly one URL` otherwise) since the SVG
goes to stdout in place of the short-link text; `--qr` has no such
restriction and prints a QR block under each short link when shortening
several URLs at once.

## `--valid`

```bash
lonk --valid https://maybe.example
```

Runs the same local validation as shortening (scheme must be `http` or
`https`, URL must parse) but makes **no network request and needs no
profile or config file** — useful in scripts. Prints `valid` or `invalid:
<reason>` per URL and exits non-zero if any URL was invalid.

## `status`

```bash
lonk status Ab3dEf9
# or, equivalently:
lonk status https://s.example.com/Ab3dEf9
```

Accepts either a bare slug or a full short URL (the last non-empty path
segment is used as the slug). Calls the configured server's `GET
/<id>/status` (documented in [Running the server](server.md)) and prints
one of:

```
alive (200)
dead (404)
dead (<transport error>)
```

Unknown slugs print `no such link: <slug>` (the server's `404` translated
to a CLI message).

## Exit codes

| Code | Meaning |
| ---- | ------- |
| `0`  | Success: a URL was valid, a link was created, or `status` found the destination alive. |
| `1`  | Usage or validation error: bad arguments, an invalid URL or header, `--valid` found an invalid URL, unknown profile, or `status` found the destination dead/unknown (including an unknown slug). |
| `2`  | Network or server error: the request couldn't reach the server, or the server returned an unexpected error. |

`clap` argument-parsing failures (missing required value, unknown flag)
also exit `1`, except `--help`/`--version`, which print and exit `0`.
