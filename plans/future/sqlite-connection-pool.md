# Future: multi-connection SQLite (pool + WAL)

Status: future intent — expand into a dated spec + plan before implementing.

## Problem

`Db` is a single connection behind a mutex (`crates/lonkd/src/db.rs:5`,
`Db(Mutex<Connection>)`). Every query — reads included — serializes through
one lock, and Rocket's concurrent request handling funnels into it
single-file. This also forfeits SQLite's actual concurrency model: under WAL,
SQLite serves many readers alongside one writer, but only across multiple
connections, which we don't have.

Fine today: every operation is a sub-millisecond point query and traffic is
personal-scale. The lock is held for microseconds.

## Trigger for doing this

Measurable contention (slow redirects under concurrent load) or a deployment
with real multi-user traffic. Do not do it speculatively.

## Proposed approach

Swap `db.rs` internals for a connection pool; keep the public API
(`Db::open`, `insert`, `get_url`, `get_link`) byte-identical so `routes.rs`
does not change.

- Pool: `r2d2` + `r2d2_sqlite` — synchronous like rusqlite, minimal, no
  framework coupling. (Considered: `rocket_db_pools`/sqlx — heavier and ties
  the DB layer to Rocket; deadpool — async, mismatch with rusqlite.)
- Per-connection init (r2d2 `CustomizeConnection` or on-checkout):
  `PRAGMA journal_mode=WAL;` (persistent per DB file, cheap to re-issue),
  `PRAGMA busy_timeout=5000;`, `PRAGMA synchronous=NORMAL;` (the standard
  WAL pairing).
- Pool size: small fixed cap (e.g. 8); SQLite still allows only one writer
  at a time — busy_timeout absorbs writer collisions.

## The trap that must be in any spec: `:memory:` tests

`rusqlite` gives every `:memory:` connection its **own private database** —
a pool of `:memory:` connections is N empty databases and every existing
unit test (`Db::open(":memory:")` throughout `crates/lonkd/src/db.rs` tests)
silently breaks. Options, pick one explicitly:

1. Shared-cache URI: open `file:<unique-name>?mode=memory&cache=shared` with
   `OpenFlags::SQLITE_OPEN_URI`, unique name per `Db::open(":memory:")` call
   (translate the legacy string inside `open`). Keeps tests as-is.
2. Switch the affected tests to `tempfile` DB files (WAL then applies in
   tests too, closer to production).

Option 2 is more honest; option 1 is less churn.

## Acceptance

- Entire existing suite green with unchanged assertions (modulo the
  `:memory:` decision above).
- A concurrency smoke test: N threads hammering `get_url` while another
  inserts — no `SQLITE_BUSY` errors surface to callers, reads don't block
  on the writer (observable as wall-clock, not just correctness).
- `Db`'s public signatures unchanged; `routes.rs` diff is empty.
