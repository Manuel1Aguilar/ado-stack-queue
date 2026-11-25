# ado-stack-queue — agent notes

Merge queue and stacked PRs for Azure DevOps

Born from idea-lab-factory case C0008 r2 (verdict POC). The design in docs/design.md is the source of truth for architecture and stack; docs/roadmap.json lists the features after the POC; docs/spec.json is the POC's acceptance criteria.

## Build and test

```sh
cargo test --workspace --locked
```

Before publishing: `bash scripts/check-publishable.sh` and `python3 -I scripts/copy-lint.py README.md` (both also run by `crates/adoq/tests/publish.rs`). Release bench (last stdout line is JSON): `cargo run -q --release -p adoq -- bench`. The first release build compiles SQLite and takes several minutes.

## Layout

- `crates/core`: engine, ports, queue and stack logic. No HTTP or process imports.
- `crates/adoq`: CLI, ADO HTTP client, system git adapter, SQLite store, bench. Tests spawn the binary against the twin.
- `crates/twin-ado`: ADO REST twin (`twins/ado/README.md`). Tests start it in-process on 127.0.0.1:0.
- Settings: flags or env (`ADOQ_URL`, `ADOQ_REMOTE`, `DATA_DIR`, `ADOQ_PAT`, `ADOQ_BATCH_MAX`); tests set `ADOQ_LOOPBACK_ONLY=1`.

## Conventions

- Performance and fit over ease of coding; keep the stack in docs/design.md.
- Every behaviour change comes with an automated test; keep test commands green.
- No credentials in the repo. External services sit behind interfaces with local fakes.
- Factory deliveries land on `main` only after an independent review accepted the exact commit. Manual work: use a branch, keep main green.
