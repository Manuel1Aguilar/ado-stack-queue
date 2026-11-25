# C0008 r2 — POC

_Generated 2026-11-25T04:31:53+00:00 from decision.json (run ic-jhx). The verdict is a research outcome; execution state is tracked separately._

**Verdict: POC** (technical experiment, open-sourced MIT after the POC).

**Why.** ADO has no native queue or retarget [M-005]. Pro-Sharp (MIT) offers one-click retarget but "nothing runs in the background" [M-001][C-001]. wshm claims an ADO queue, but it is score-and-automerge, not speculative or batched, by README [M-003][C-002]. Rebase needs git, so a headless service is a distinct niche [X-005]. The core loop is cheap to prove with no credentials.

**The POC.** `adoq`, a Rust service and CLI. When a parent PR merges, the child is retargeted and rebased and gets a status. A queue tests PRs on a synthetic `queue/<n>` branch, batches builds, and bisects on red. It runs against a docs-faithful ADO twin and real local git repos.

**Top risks.** A green twin suite shows internal consistency, not ADO feasibility [X-001][X-004]. Retargeting may be off in real orgs. Builds on non-PR refs are unproven. The bypass-policies permission leaks the queue gate. wshm's code and the Marketplace were not checked [M-006][C-004]. Demand is unmeasured.

**After the POC.** Roadmap F1 reads wshm and Pro-Sharp and decides contribute vs build. F2 and F3 test a personal ADO org (retarget PATCH, queue-ref builds). Review on 2026-12-25: archived unless a roadmap feature is queued.

**Market viability (technical_experiment):** weak — Informational only; goal is technical_experiment so market evidence does not gate the POC. Buyer: ADO teams that stack PRs (unverified). Supported price: 0 (open source); willingness to pay unknown; break-even: Not applicable: technical experiment, no revenue goal.  
Gap: Background auto-retarget and rebase plus speculative/batched queue for ADO as a headless service.  
Incumbents cover it: partly (Pro-Sharp (manual retarget), wshm (scored queue))

**Recommendation:** Build the designed POC (adoq: headless ADO stack keeper + merge queue, tested on a docs-faithful ADO twin with real git) and publish it MIT after the POC. Nothing found covers the core job; the first roadmap steps read wshm's queue and try a personal ADO org.  
**Buyer / user:** Manu as experimenter; ADO teams that stack PRs or want a queue (open-source adopters, demand unmeasured)  
**Why now:** ADO has no native queue or retarget, the only ADO stacking extension cannot run in the background, and the core loop is cheap to prove on a twin with no credentials.  
**Confidence:** medium — Success = POC proves the retarget+rebase loop and a batched queue on the twin. Likely, since the loop is simple git plus a REST subset; real-ADO feasibility is the unproven part and is not part of the threshold.  
**Cost / effort:** 1–3 factory build runs (Rust workspace of core, twin-ado and CLI per design.md; Manu's cost is reviewing one delivery. No cash.)  
**Next action:** Factory builds the POC per spec.json/roadmap.json: stacking slice first (AC3-5), then serial and batched queue; F1 (read wshm and Pro-Sharp) recorded in the README. (owner factory, budget No cash; one build plus one fix round, by 2026-12-25)  
**Success threshold:** First POC build satisfies >= 80% of holdout.json: parent merge retargets and rebases the child idempotently, restart recovery works, queue merges each PR once with fewer builds than serial.  
**Failure threshold:** Holdout satisfaction < 60% after the fix round, or F1 shows wshm or Pro-Sharp already ship background restack plus a speculative queue.  
**Challenger's independent verdict:** POC

## Validation
**Result:** mixed — The gap is plausible: no ADO-native queue or retarget, one thin MIT stacking extension, one shallow queue claim in wshm. Demand is unmeasured and real-ADO behaviour untested, but for a technical experiment the twin POC is cheap and informative.

Optimistic assumptions (and what happens if wrong):
- wshm's queue is shallow (README only; code unread) → if wrong: Project becomes a contribution to wshm; F1 catches this first.
- Retargeting is enabled in typical ADO orgs → if wrong: Stacking slice infeasible via API; only the queue survives.
- Build validation can run on a queue/<n> ref → if wrong: Only a serial queue is possible; the efficiency claim fails.
- Adopters accept a bot with force-push and PR-update rights → if wrong: Few installs; still a valid experiment.

## Optional real-world checks (never blocking)
- [2h] Spin up a free personal ADO org, enable PR retargeting and try the retarget PATCH on a stacked PR — would change: Confirms or kills the stacking slice on real ADO (X-001)
- [30min] Search the Visual Studio Marketplace for 'merge queue' and 'stacked PR' extensions — would change: Closes the open inventory gap (M-006); a hit could turn this into a contribution

## After the POC (roadmap)
- F1 [S] Read wshm's ADO queue and Pro-Sharp code; decide contribute vs build — Avoid duplicating an existing queue [M-003]
- F2 [M] Real ADO adapter against a personal ADO org — Settles retarget flag, bot permissions, rate limits the twin cannot [A-006]
- F3 [M] Speculative builds on queue refs via ADO pipelines API — Core efficiency claim; unproven on real ADO
- F4 [S] Service hooks (webhook) receiver with polling fallback — Lower latency and fewer API calls
- F5 [S] Packaging: Docker-free static binary release, config docs, MIT repo, README demo — Open-source adoption experiment (stars/issues in 4 weeks)
- F6 [M] Policy-aware mode: required-status check setup, bypass detection, reviewer-vote reset warnings — Makes the queue gate trustworthy [X-003][X-004]
- F7 [S] Queue priority, per-PR batch opt-out, and pause/resume — Maintainer control

## Strongest support
- [M-001] Pro-Sharp ADO Stacked PRs extension detects 'parent merged' and offers one-click retarget; 'parent ahead' only flags a needed rebase; merge order via optional status + branch policy; nothing runs in the background. So automatic retarget/rebase and a queue are NOT covered. — vendor_claim; https://raw.githubusercontent.com/Pro-Sharp/ado-stacked-pull-requests-extension/main/README.md (retrieved 2026-11-25)
- [C-001] Pro-Sharp README re-fetched: 'Parent merged ... One click retargets the child'; 'Parent ahead ... needs a rebase'; 'Nothing runs in the background.' Confirms M-001. — observed; https://raw.githubusercontent.com/Pro-Sharp/ado-stacked-pull-requests-extension/main/README.md (retrieved 2026-11-25)
- [C-002] wshm README re-fetched: merge queue is a scored auto-promote of green PRs, ADO listed as a forge; no speculative/batched builds or stacking mentioned. Not a speculative-build queue. — observed; https://raw.githubusercontent.com/wshm-dev/wshm/main/README.md (retrieved 2026-11-25)
- [X-002] PR Statuses API lets a service post a status with context {genre,name}, state and iterationId on a PR; this is the hook for a queue-owned check. — observed; https://learn.microsoft.com/en-us/rest/api/azure/devops/git/pull-request-statuses/create?view=azure-devops-rest-7.1 (retrieved 2026-11-25)

## Strongest contrary evidence
- [M-003] wshm (Apache-2.0) claims Azure DevOps support and a 'scored merge queue' that promotes green PRs above a threshold; it is an AI agent bundle, with no stacked PRs or speculative merge-ref builds mentioned in the README lines found. Closest claimed overlap for the queue; depth unverified. — vendor_claim; https://raw.githubusercontent.com/wshm-dev/wshm/main/README.md (retrieved 2026-11-25)
- [X-004] Bypass policies when completing pull requests is a separate permission; users/groups with it can complete PRs that fail policy. Implies: a queue cannot block human completion by policy alone unless nobody else holds bypass, and the required queue status must be enforced as a Required status policy. — inference; https://learn.microsoft.com/en-us/azure/devops/repos/git/branch-policies?view=azure-devops (retrieved 2026-11-25)
- [M-006] Visual Studio Marketplace search page for 'merge queue' is JS-rendered and returned no readable results via curl; Marketplace queue extensions are therefore NOT ruled out. — unknown; https://marketplace.visualstudio.com/search?term=merge%20queue&target=AzureDevOps (retrieved 2026-11-25)
- [C-004] Attempt to search GitHub for other ADO merge-queue/stacking repos failed: API rate limited (unauthenticated). Marketplace/OSS inventory remains incomplete. — unknown; https://api.github.com/search/repositories (retrieved 2026-11-25)

## Key unknowns
- Does wshm's queue code already do more than its README says (scored automerge only)?
- Is the PR retargeting feature on by default in real ADO orgs (X-001)?
- Can build validation run on a non-PR queue/<n> ref, or does the queue degrade to serial?
- Marketplace queue extensions never checked (M-006, C-004)

## Objections and responses
- O1 (accepted_risk): Agreed: a green twin suite proves internal consistency, not ADO feasibility. Design.md and this verdict say so explicitly; the verdict is 'loop works under docs-faithful twin'. Roadmap F2 (personal ADO org) and F3 settle retarget flag and queue-ref builds. Real-org checks are not a precondition for a lab POC.
- O2 (resolved): Contribute-vs-build is roadmap F1, the first feature: read wshm queue and Pro-Sharp and record the decision in the README. Challenger found wshm's queue is score-and-automerge and Pro-Sharp does nothing in the background [C-001][C-002], so building a headless service is justified now; Marketplace remains unchecked.
- O3 (accepted_risk): AC7 (<=2 builds for 4 PRs) and AC12 (<=0.3 per PR) disagree. Frozen files cannot change; the stricter AC12 is the target and the builder may treat AC7 as satisfied by it.
- O4 (accepted_risk): Scope is wide but frozen; build order is stacking first, so a partial delivery still proves the first claim.
- O5 (accepted_risk): Minor: tokio only inside twin-ado via axum; core stays blocking. Builder should state this in the README.

## Reopen if
- wshm or Pro-Sharp ships auto-restack or a speculative ADO queue (then contribute instead)
- A real ADO org shows retargeting disabled or build validation impossible on queue refs

Detail: triage.json · market.md · competitors.json · tech.md · econ.md · cost-model.csv · challenge.json · experiment-plan.md · evidence-register.json · design.md · spec.json · roadmap.json
