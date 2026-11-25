# Design — C0008 r2: `adoq`, an open-source merge queue + stack keeper for Azure DevOps

## Summary
`adoq` is a headless, always-on service (and CLI) that watches an ADO repo and (1) auto-retargets and rebases child PRs when their parent PR merges, (2) runs a merge queue that tests PRs on a synthetic merge branch and completes them in order, batching builds when nothing goes wrong. The gap: Pro-Sharp gives one-click retarget but "nothing runs in the background"; ADO has no queue [M-001][M-005]; wshm claims a queue but is unread [M-003]. Rebase is a git operation, not a REST call [X-005]. The POC proves the core loop against a **digital twin** of the ADO REST subset plus real local bare git repos, because this box has no ADO org or credentials. Contribute-vs-build stays open until wshm's queue code is read (roadmap F1).

## Users & core loop
Users: ADO developers who stack PRs, and the repo maintainer who wants a queue. Core loop per tick: `poll PRs → reconcile stacks → advance queue → post status`.
- Stack: child PR targets parent's source branch. Parent completes → bot PATCHes child `targetRefName` to the parent's target, `git rebase --onto <new target> <old parent tip> <child>`, force-push-with-lease, posts status `adoq/stack`.
- Queue: PR tagged `queue` (label) enters FIFO. Bot builds `queue/<n>` (target tip + batch of PRs merged), runs the build; green → complete each PR; red → bisect the batch, evict the culprit with a failed status, rebuild the rest.

## POC scope
In: reconcile engine, git ops (system `git`), SQLite state, ADO client over HTTP, ADO twin (PRs, statuses, refs, builds), CLI (`adoq ls` / `adoq do`), telemetry, bench. Out: real ADO, webhooks (poll only), UI, multi-repo, auth beyond PAT, policy emulation.

## Architecture
```
 adoq-cli ──► core::Engine ◄── Clock port
              │   (pure state machine: Action -> State, plans effects)
   ┌──────────┼───────────┬──────────────┐
   ▼          ▼           ▼              ▼
 AdoApi     Git port    Builds port    Store port
 (ureq)    (system git) (ADO builds)   (SQLite)
   │          │           │
   └──► twin/ado (axum, 127.0.0.1) + bare repos in $DATA_DIR  [tests]
```
Reflex applied in part (see Patterns): the engine is UI-free; all edges are ports with in-memory/recording fakes.

## Stack
- Rust 1.100 workspace, crates `core`, `adoq` (bin), `twin-ado` (bin+lib). Why: fast native single binary, easy for an agent to keep correct with types; always-on process must be small.
- axum 0.8.9 for the twin only [A-001]. ureq 3.5.2 blocking client for the real/twin ADO API [A-003]. rusqlite 0.43.0 with `bundled` for state [A-002]. serde/serde_json, clap. No async in the core: one reconcile loop, rate-limit friendly.
- git via the system binary (2.43.0 here) incl. `merge-tree --write-tree` [A-004]: no libgit2 to keep rebase semantics identical to what adopters' runners do.

## Data model
SQLite `$DATA_DIR/adoq.db`: `pr(id, repo, source, target, state, parent_id NULL, queued_at NULL, last_source_sha)`; `queue_batch(id, ref, base_sha, pr_ids_json, build_id, state)`; `event(ts, kind, pr_id, detail_json)` (audit). Illegal states avoided: a PR is in at most one active batch (UNIQUE on active membership table `batch_member(pr_id UNIQUE WHERE active)`); `parent_id` is derived each tick, never trusted from cache.

## Interfaces
CLI: `adoq --config adoq.toml run [--once]`, `adoq ls` (JSON: stacks, queue, batches), `adoq do <Action> [json]` with Actions `Tick`, `Enqueue{pr}`, `Dequeue{pr}`, `Pause`. Refusals list valid actions. NDJSON on stdio.
ADO calls used (REST 7.1): list PRs, update PR (`targetRefName`, `status`, `completionOptions`) [X-001], create PR status [X-002], create/get build, get refs.
Twin extra control endpoints (non-ADO, `/_twin/*`): create PR, complete-as-human, set build outcome by ref, set flags (`retargeting=off`, `rate_limit=N`).

## Fakes for external services
`twins/ado/` (also the crate `twin-ado`): follows Microsoft Learn Update-a-pull-request and PR-Statuses pages [A-005][X-001][X-002]: Basic PAT auth (401 without), `api-version` required (400), 404 unknown PR/repo, `$top/$skip` pagination, `targetRefName` silently ignored when the retargeting flag is off, 409 on completing with stale `lastMergeSourceCommit`, 429 with `Retry-After` after a configurable request budget, builds queued→inProgress→completed with scripted outcome per branch. Merges done with real git in bare repos. Not faithful: policies, permissions, reviewer-vote reset, build triggers on non-PR refs [A-006]. Real ADO integration = roadmap F2.

## Performance targets
Reconcile tick on 50 PRs / 10 stacks: p50 < 50 ms excluding git; idle RSS < 30 MB; one rebase+push of a 200-commit repo < 1 s. Efficiency number: builds per merged PR in a batch of 4 green PRs ≤ 0.3 (naive serial = 1.0).

## Security & data
No secrets in repo; PAT read from env `ADOQ_PAT`, never logged (test greps logs). Twin listens only on 127.0.0.1. Bot pushes only `queue/*` temp branches and force-pushes (with lease) only branches of PRs it is stacking; never touches default branches except via PR completion API. This box holds no work repos (constraint): only generated fixture repos.

## Success metrics
1. Telemetry `$DATA_DIR/telemetry.jsonl` (no personal data, PR ids only): `tick{ms,prs}`, `retarget{pr,ok}`, `rebase{pr,conflict,ms}`, `batch{size,result,builds}`, `merge{pr,wait_s}`. The one number: **median seconds from parent merge to child retargeted+rebased** (target < 1 tick interval).
2. Bench: `deploy.json` bench `cargo run -q --release -p adoq -- bench`, last line JSON `{"tick_p50_ms":..,"rss_mb":..,"builds_per_merged_pr":..}`.
3. Factory per-delivery ledger (PRs, LOC, comments, tests, review defects, tokens, $ per task) applies via `ilf metrics`.

## Patterns
- reflex: applied in part. No UI, but the interactive-state logic is a UI-free core with Action/State, ports for ADO/git/builds/store/time, and `adoq ls` / `adoq do` driving it; behaviour tests via CLI with the twin as fixtures. No ViewModel tree/Elm views (no screens).
- bombadil: not applied: no browser UI; spec.json has no UI invariants.
- codex-cross-review: rejected pattern, not applied.

## Risks
Retargeting disabled in adopter orgs → stacking infeasible [X-001][A-006]. Bypass-policies leaks the queue gate [X-004]. Force-push resets votes [X-003]. Speculative builds on non-PR refs unproven. Rate limits unknown. wshm may already cover the queue [M-003]. Demand weak/unquantified [M].

## What the POC proves / does not prove
Proves (feasibility): the loop parent-merge → retarget → real git rebase → status, and a batched queue with bisection and fewer builds, all correct under a faithful-to-docs REST twin, including conflicts, retargeting-off, 429 and restart recovery. Does not prove: real ADO behaviour (flags, permissions, build triggers, policies), demand, adopter acceptance of a bot with force-push rights, or that wshm/Pro-Sharp don't already suffice.
