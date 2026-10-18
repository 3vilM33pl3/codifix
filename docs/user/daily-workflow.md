# Daily workflow

Most sessions need the same sequence: verify the destination, recall context,
capture useful work, then inspect anything still pending. These examples follow
current source. See [release compatibility](https://codifix.dev/docs/help/known-limitations)
for the published candidate and platform boundaries.

## 1. Check where you are working

From your initialized repository:

```bash
codifix urls
codifix status --project example --json
```

Replace `example` with your project's slug. URLs come from configuration; their
presence does not prove a service is reachable. Status identifies the selected
area/instance when area routing is configured.

A project belongs to an installation; its slug alone does not choose the
installation. If an operator configured an `engineering` area, select it
explicitly when working outside a routed repository:

```bash
codifix --area engineering status --project example --json
```

Use that same destination for subsequent reads and writes. An unavailable area
is an error, not permission to redirect work into General. See
[Areas and projects](https://codifix.dev/docs/areas).

## 2. Recall before changing code

```bash
codifix query --project example --question "What should I know before changing deployment?"
codifix resume --project example
```

Query answers a specific question; resume provides recent context after an
interruption. Inspect citations and uncertainty. Insufficient evidence means
you need to investigate, not that a missing answer is safe to invent.

Use `codifix tui --project example` or the Web URL from `codifix urls` to inspect
memories, history, proposals and activity. The [TUI](https://codifix.dev/docs/tui)
and [Web UI](https://codifix.dev/docs/web-ui) guides describe their controls.

## 3. Capture useful results

```bash
codifix remember --project example \
  --title "Deployment recovery" \
  --summary "Verified recovery after a failed application restart." \
  --note "The selected instance must use its pinned executable and matching configuration."
```

Record verified decisions, fixes, constraints and handoffs. Include the
relevant evidence or changed files when available; avoid credentials and
transient session chatter. Review replacement proposals before accepting
changes to existing knowledge.

Curation success means the canonical mutations and their background-work
intents committed. Embedding or consolidation may still be pending:

```bash
codifix jobs --project example
codifix jobs --project example list --status blocked --limit 20
```

Inspect a failed stage before reviewing a retry. Preserve the retry request ID
after an uncertain reply; repeating provider work can incur another charge.
See [Background jobs](https://codifix.dev/docs/reference/cli/jobs).

## 4. Check synchronized work when you use it

For an operator-configured laptop profile:

```bash
codifix projects sync-status --profile /absolute/private/profiles/example.json
codifix projects offline-search --profile /absolute/private/profiles/example.json --question "How is deployment recovered?"
```

An offline search reads the selected downloaded projection. It does not query
the remote owner or prove current remote access. Last observed sync is not a
guarantee that your copy is current; pending uploads are not accepted owner
versions. Use [Team setup](https://codifix.dev/docs/team-setup) for enrollment
and [Offline and synchronization](https://codifix.dev/docs/offline) for
subscription, freshness, contribution and recovery detail.

Private sharing and expertise are optional, separate workflows. An ordinary
query does not ask another area. Use
[Federation and expertise](https://codifix.dev/docs/federation) to review the
destination and exact disclosure before submitting a question.

## 5. Recover from a concrete symptom

```bash
codifix doctor
codifix health
codifix status --project example
```

Check the selected area's service and configuration before changing anything.
For stalled enrichment inspect Jobs; for stale downloaded knowledge inspect
sync status; for permission failures inspect the relevant identity and grants.
Preserve unsent work and original request IDs.

[Help](https://codifix.dev/docs/help) starts from symptoms.
[Operations](https://codifix.dev/docs/operations) covers backups, selected-instance
maintenance and upgrade recovery. A blanket restart is not an instance upgrade.
