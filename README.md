# adoq

`adoq` retargets and rebases stacked pull requests in Azure DevOps when their parent merges, and runs a batched merge queue, as a small background program.

## Why it exists

Azure DevOps has no built-in merge queue and no way to keep a stack of pull requests current. When the bottom PR of a stack completes, each child still points at a branch that is gone, and someone has to retarget and rebase it by hand. Merging many green PRs one at a time also means one full build per PR.

`adoq` does both jobs from outside Azure DevOps. It polls the REST API, uses your system `git` for rebases and merge checks, and keeps its state in a local SQLite file, so a restart picks up where it stopped.

## What this POC tests

- Tested (feasibility): the loop "parent PR completes, child is retargeted, rebased with real git, status posted" plus a batched queue with bisection and restart recovery can be built correctly as a poll-based service on Azure DevOps' documented REST API and plain git. The tests run it against a local fake of that API (`twins/ado/`) with real bare repositories. The claim would be refuted by non-linear rebased stacks, a double completion after restart, or batching that needs more builds than serial (`builds_per_merged_pr` above 0.3).
- Not tested (desirability): whether teams on Azure DevOps want a self-hosted bot for stacking and queues and will give it force-push and PR-update rights. No demand evidence was gathered.
- It has not been run against real Azure DevOps. Retargeting defaults, build triggers on queue branches, permissions and policies are unproven. See `twins/ado/README.md` for what the fake does not model.

## Install

You need Rust (stable, with `cargo`) and `git` 2.38 or newer. Clone this repository, then:

```sh
cargo build --release --locked -p adoq
```

The first release build compiles SQLite and takes a few minutes. The binary is `target/release/adoq`.

## Quick start

This runs `adoq` against the bundled fake Azure DevOps API on your own machine. It needs no account and no real token.

```sh
cargo build --release --locked -p adoq -p twin-ado
DATA_DIR=/tmp/twin PORT=8099 target/release/twin-ado &

export ADOQ_URL=http://127.0.0.1:8099 ADOQ_REMOTE=/tmp/twin/repo.git
export DATA_DIR=/tmp/adoq-data ADOQ_PAT=any-value-works-here
target/release/adoq ls
```

Output:

```text
{"paused":false,"stacks":[],"queue":[],"batches":[]}
```

No pull requests exist yet, so the stacks, queue and batches are empty. To see it work on four green PRs, run the built-in benchmark:

```sh
target/release/adoq bench
```

Output from one run (a laptop-class Linux machine, numbers vary):

```text
idle tick p50 over 20 ticks: 68.50 ms
batch of 4 green PRs: 1 build(s), 4 merged
{"builds_per_merged_pr":0.25,"rss_mb":8.71484375,"tick_p50_ms":68.502618}
```

Four PRs merged with one queue build. `rss_mb` is the memory of the benchmark process, which also hosts the fake API, so a real `adoq` process uses less.

Other commands:

```sh
adoq run                # poll forever, one tick every 30 s (--interval-secs)
adoq run --once         # one tick
adoq do Enqueue '{"pr":12}'
adoq do Tick            # also Dequeue, Pause, Resume
```

Queue a PR by adding the `queue` label in Azure DevOps or by running `adoq do Enqueue`.

## Configuration

Every setting is a flag or an environment variable. The token comes from `ADOQ_PAT` only, never from a flag.

| Variable | Flag | Default | Meaning |
|---|---|---|---|
| `ADOQ_URL` | `--ado-url` | required | Base URL of the Azure DevOps API |
| `ADOQ_REMOTE` | `--remote` | required | Git remote that `adoq` clones, rebases and pushes to |
| `DATA_DIR` | `--data-dir` | required | Where the SQLite file, the git work copy and `telemetry.jsonl` live |
| `ADOQ_PAT` | none | required | Personal access token |
| `ADOQ_ORG` | `--org` | `org` | Organization name |
| `ADOQ_PROJECT` | `--project` | `proj` | Project name |
| `ADOQ_REPO` | `--repo` | `repo` | Repository name |
| `ADOQ_BATCH_MAX` | `--batch-max` | `8` | Most PRs in one queue build |
| `ADOQ_LOOPBACK_ONLY` | none | off | Set to `1` to refuse any Azure DevOps address that is not loopback |

Telemetry is one JSON line per event in `$DATA_DIR/telemetry.jsonl`. The fields are in [docs/telemetry.md](docs/telemetry.md).

## How it compares

Both tools below were compared from their README files only. Their code was not read, so a feature missing from this table may exist in the code.

| Tool | Licence | What its README says | Gap `adoq` targets |
|---|---|---|---|
| [Pro-Sharp Azure DevOps Stacked Pull Requests](https://github.com/Pro-Sharp/ado-stacked-pull-requests-extension) | MIT | Detects that a parent merged and offers a one-click retarget of the child. Flags a child that needs a rebase. "Nothing runs in the background." | `adoq` retargets and rebases without a click |
| [wshm](https://github.com/wshm-dev/wshm) | Apache-2.0 | Lists Azure DevOps as a forge. Has a scored merge queue that promotes green PRs. No stacking or batched queue builds mentioned. | `adoq` builds a batch on a temporary `queue/<n>` branch and bisects on failure |

Azure DevOps itself has neither a merge queue nor a retarget API call that does this for you. The Visual Studio Marketplace was not searched, so other extensions may exist.

## Status and limits

- Tested only against the local fake API. Real Azure DevOps may behave differently: PR retargeting may be switched off in your organization, and builds may not run on a `queue/<n>` branch.
- A queue gate only holds if nobody else can complete PRs around it. Users with the "bypass policies when completing pull requests" permission can.
- Every tick lists all PRs, including completed ones.
- A PR whose retarget worked but whose rebase hit a git error is marked failed and not retried.
- Configuration is flags and environment variables. There is no config file.

## Develop and test

```sh
cargo test --workspace --locked
bash scripts/check-publishable.sh
python3 -I scripts/copy-lint.py README.md
```

The tests start the fake API in-process on 127.0.0.1 and spawn the real `adoq` binary. `check-publishable.sh` scans tracked files for keys, tokens, private addresses and personal emails.

More: [design](docs/design.md), [decision record](docs/decision.md), [POC spec](docs/spec.json), [roadmap](docs/roadmap.json).

## License

MIT. See [LICENSE](LICENSE).
