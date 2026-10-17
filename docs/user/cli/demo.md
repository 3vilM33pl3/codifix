# `codifix demo`

Load a showcase project so query, resume, and the graph show something real on a fresh install.

```bash
codifix demo
codifix demo --project playground
```

Seeds the chosen project (default `demo`) with an embedded, privacy-safe corpus of memories about Codifix itself, then curates it. Works keyless — no LLM or embedding provider required. Requires a running service (`docker compose up` or `codifix service run`). Idempotent: rerunning re-observes the same facts rather than duplicating them.

After loading, try:

```bash
codifix query --project demo --question "How does reinforcement work?"
codifix tour   # guided walkthrough of the three core commands
codifix tui    # browse everything, including the codifix graph
```

Do not seed demo data into a real project slug.

## Related Docs

- [Tour Command](tour.md)
- [Query Command](query.md)
- [Wizard Command](wizard.md)
