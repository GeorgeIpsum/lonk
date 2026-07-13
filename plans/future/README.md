# Future plans

Pre-spec intent documents for work deliberately deferred, not forgotten.
Each captures enough context to start a full spec → plan → implementation
cycle later; none is designed in detail yet. Pick one up by expanding it
into a dated `plans/YYYY-MM-DD-<topic>-design.md` (per CLAUDE.md) when its
trigger fires.

- **[sqlite-connection-pool.md](sqlite-connection-pool.md)** — replace the
  single mutex-guarded connection with a WAL pool. Trigger: measurable
  contention or real multi-user traffic. The substantive one.
- **[cli-polish.md](cli-polish.md)** — small independent CLI fixes
  (`--valid` empty-arg exit code, `--version`, atomic config save, an
  untested error branch, a dup helper). Could ship as one small PR.
- **[frontend-and-tooling-hygiene.md](frontend-and-tooling-hygiene.md)** —
  lonk-client non-JSON guard, e2e temp-DB teardown, sync I/O note. Low
  priority.
- **[untrusted-multi-user-hardening.md](untrusted-multi-user-hardening.md)**
  — SSRF/open-redirect/rate-limit/auth policy. STRATEGIC; only if lonk ever
  serves untrusted link creators. Out of scope for self-hosted single-user.
