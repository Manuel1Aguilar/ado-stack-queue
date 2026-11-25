# Telemetry

`adoq` appends one JSON object per line to `$DATA_DIR/telemetry.jsonl` and prints the same line on stdout. Lines carry PR ids and numbers only: no names, emails, branch names or tokens. Every line has `kind` and `ts` (Unix epoch milliseconds).

| kind | fields | meaning |
|---|---|---|
| `tick` | `ms`, `prs` | One reconcile pass: its duration in milliseconds and how many PRs it read. |
| `retarget` | `pr`, `ok` | The bot changed a child PR's target after its parent completed. `ok` is false when a re-read shows the target did not change. |
| `rebase` | `pr`, `conflict`, `ms` | A child branch was rebased and force-pushed with lease. `conflict` is true when the rebase was abandoned and nothing was pushed. |
| `batch` | `size`, `result`, `builds` | A queue build for `size` PRs resolved. `result` is `green`, `red`, `conflict` (a PR did not merge into the queue branch) or `stale` (a member changed or left the queue). `builds` counts builds started for the batch so far. |
| `merge` | `pr`, `wait_s` | The bot completed a PR. `wait_s` is the whole seconds since the queue first saw it. |

The `event` table in `$DATA_DIR/adoq.db` holds the same lines for audit (`ts`, `kind`, `pr_id`, `detail_json`).
