# Map

- Start every session with `./init.sh` (builds and runs the tests). Fix red before new work.
- `features.json`: what exists. Work on one entry at a time; set `passes: true` only when a
  test proves it, never on a hunch.
- `progress.md`: append what you did, what is next and anything surprising. Newest last.
- Commit after each feature, with its id in the message.
- `docs/design.md` is the architecture, `docs/spec.json` the POC acceptance criteria,
  `CLAUDE.md` the full agent notes.
- Error messages from checks you add say how to fix the problem, not only what failed.
- Third-party APIs: tests and default config use a local fake in `twins/<service>/`, built
  from the provider's public docs. No real credentials, no paid calls.
- Web app or service: keep `deploy.json` (`build`, `start` serving on 127.0.0.1:$PORT with
  data under $DATA_DIR, `health`). Every integrated commit is deployed as the tailnet preview.
