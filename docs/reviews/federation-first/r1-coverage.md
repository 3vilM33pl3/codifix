# R1 mutation coverage inventory

Status: implementation in progress. An unchecked row is an explicit gap, not a
claim of transactional coverage. Each completed row requires tests and concrete
write-path references before the R1 gate can close.

| Operation family | Existing ownership | Required durable boundary | Verified |
| --- | --- | --- | --- |
| Capture and curation | mem-curate repository | mutation + events + enrichment intent | [ ] |
| Replacement/validation versions and relations | mem-curate, mem-reinforce | revision/state/edges + event | [ ] |
| Archive/delete/retention | service memory repository | state/deletion + surviving subject event | [ ] |
| Provenance checks | service provenance repository | check transition + event | [ ] |
| Bundle import/export | service bundle repository | DB apply + event; external intent/outcome | [ ] |
| Graph/index/embedding batches | mem-graph, service embedding repository | batch result + operation event | [ ] |
| Reinforcement and procedural learning | mem-reinforce, service loops | state + dedicated journal/event | [ ] |
| Project/commit/checkpoint/plan activity | service project/activity repositories | mutation + event | [ ] |
| Agent workspaces | service agents repository | lifecycle/heartbeat + journal/event | [ ] |
| Loop settings/runs/proposals | mem-loops, service loops | lifecycle/state + audit/event | [ ] |
| Auth administration/session/token state | service auth store | state + protected audit (not project feed) | [ ] |
| Offline captures/activity | service offline DuckDB/replay | local atomic intent; PostgreSQL atomic replay | [ ] |
| Filesystem/external effects | settings/export/watch/provider boundaries | durable intent + reconciled outcome | [ ] |
| Post-curation stages | service curation worker | fenced output + attempts + dependent intent | [ ] |
| Federation | existing federation store/runtime | retain accepted/outbox/inbox/audit boundaries; regression proof | [ ] |

Operational data (leases, attempts, retry receipts, access/cache bookkeeping)
must map to its own journal or an explicitly documented operation-level event.
They must not recursively enqueue jobs or copy private inputs into activity.
