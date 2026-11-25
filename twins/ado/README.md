# Azure DevOps twin

The `twin-ado` crate (`crates/twin-ado`) is a local behavioural fake of the Azure DevOps Services REST API 7.1 subset that `adoq` calls. It listens on 127.0.0.1 only and keeps its pull requests in memory. Branches live in a real bare git repository, so merges, rebases and pushes are real git operations.

Run it by hand: `DATA_DIR=/some/dir PORT=8099 cargo run -p twin-ado`.

## Microsoft Learn pages followed

- Pull Requests, Update: https://learn.microsoft.com/en-us/rest/api/azure/devops/git/pull-requests/update?view=azure-devops-rest-7.1 (`targetRefName`, `status`, `lastMergeSourceCommit`, `completionOptions`)
- Pull Requests, Get and Get Pull Requests: https://learn.microsoft.com/en-us/rest/api/azure/devops/git/pull-requests/get-pull-requests?view=azure-devops-rest-7.1 (`searchCriteria.status`, `$top`, `$skip`)
- Pull Request Statuses, Create: https://learn.microsoft.com/en-us/rest/api/azure/devops/git/pull-request-statuses/create?view=azure-devops-rest-7.1
- Pull Request Labels, Create and Delete: https://learn.microsoft.com/en-us/rest/api/azure/devops/git/pull-request-labels?view=azure-devops-rest-7.1
- Refs, List: https://learn.microsoft.com/en-us/rest/api/azure/devops/git/refs/list?view=azure-devops-rest-7.1
- Builds, Queue and Get: https://learn.microsoft.com/en-us/rest/api/azure/devops/build/builds?view=azure-devops-rest-7.1
- Rate and usage limits (429 and `Retry-After`): https://learn.microsoft.com/en-us/azure/devops/integrate/concepts/rate-limits
- Personal access tokens (Basic auth with an empty user name): https://learn.microsoft.com/en-us/azure/devops/organizations/accounts/use-personal-access-tokens-to-authenticate

## Behaviour

| Rule | Twin response |
|---|---|
| No `Authorization: Basic` header with a non-empty token | 401 |
| No `api-version` query parameter | 400 |
| Unknown repository or pull request | 404 |
| `$top` and `$skip` on the PR list | page of that size, default `$top` 100 |
| More requests than the `rate_limit` budget | 429 with `Retry-After: 1`, budget resets |
| Completing with a `lastMergeSourceCommit` that is not the source tip | 409 |
| Completing when the merge conflicts | 409 |
| `targetRefName` PATCH while the `retargeting` flag is off | 200, field silently unchanged |
| Build created, then polled | `notStarted`, then `inProgress`, then `completed` |

A build fails when the tested commit contains a file named `BUILD_FAILS`, otherwise it succeeds.
Completion creates a merge commit named `Merged PR <id>: <title>` on the target branch.

## Control endpoints (not part of ADO)

- `POST /_twin/prs` with `{"source","target","title"}` creates a PR and returns `{"id"}`.
- `POST /_twin/prs/{id}/complete` completes a PR as a human would.
- `POST /_twin/flags` with any of `retargeting` (bool), `rate_limit` (number or null), `fail_complete_after` (number or null, makes the next API completion fail with 500 once).
- `GET /_twin/log` returns the ordered list of `build_created`, `build_completed` and `pr_completed` entries.
- `GET /_twin/stats` returns `{"mutations","builds"}`; `mutations` counts non-GET calls under `/_apis`.

## Not faithful

- Branch policies, required reviewers, permissions and the reviewer-vote reset on push.
- Build triggers on branches that are not PR refs: the twin builds any branch it is asked to build. Real ADO may not.
- The default value of PR retargeting in a real organization.
- Completion options other than a no-fast-forward merge; no squash, rebase or delete-source-branch.
- Merge status fields, iterations, threads, webhooks.
