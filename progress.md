# Progress

Append-only log, newest last. One short entry per session.

- 2026-11-25 scaffold: repo created by idea-lab-factory; nothing built yet.
- 2026-11-25 POC build (D0008): workspace `adoq-core` (engine, ports, queue, stack), `adoq` (CLI, ureq client, system git, SQLite, bench), `twin-ado` (axum ADO twin with real bare repos). All 13 acceptance criteria have tests; bench gives builds_per_merged_pr 0.25, tick p50 about 145 ms on 50 PRs (twin spawns git per PR, so mostly twin cost), rss 8.6 MB. Prose check: files checked README.md, twins/ado/README.md, docs/telemetry.md, progress.md; no edits. Next: F1 (read wshm's queue) and F2 (real ADO org). Known gaps: all PRs (including completed) are listed each tick; a PR whose retarget succeeded but rebase hit a git error is marked failed and not retried; config is flags and env, no TOML file.
- 2026-11-25 OSS release (D0008, US1): README rewritten for outside readers with real `adoq ls` and `adoq bench` output captured from the release binary against the twin; added `scripts/copy-lint.py` (copy of the factory check), `scripts/check-publishable.sh` and `crates/adoq/tests/publish.rs` (AC1-AC3 plus a leak-detection test). No engine, CLI or twin changes. Next: F1, F2.
