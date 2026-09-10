# R1 operator workflow evidence — 2026-09-10

Scope: synthetic records, separately created PostgreSQL database, loopback
service, disabled watcher/cluster/MCP, and an in-process fake-compatible provider.
No customer records, real API credentials or paid provider calls were used.
This evidence is not a declaration that the entire R1 contract is complete.

## Observed end-to-end path

The built `memory` binary ingested one structured capture and curated it through
the real HTTP service. Curation returned one memory version and four durable
stage IDs immediately. Database inspection confirmed those stages and the
mutation timeline. The fake provider returned HTTP 401 with a private marker in
its body: embedding became `blocked/credentials` after one attempt; independent
validation became `skipped/disabled`; dependent stages remained queued.

The browser loaded the production Web build and authenticated through the local
session flow. **More → jobs → Inspect → Review retry → checkbox → Confirm retry**
opened generation 2. Because the fake credential condition was unchanged, that
generation blocked again. No raw provider marker appeared in the inspection or
attempts view. The confirmation named the project, stage and stable request ID.

CLI inspection showed the same stage IDs, dependencies and sanitized history.
Two identical `jobs retry --request-id … --yes` invocations both returned
generation 3, not two generations. The unchanged failure condition blocked that
generation too.

An actual PTY opened the TUI Jobs tab, refreshed, inspected the blocked stage and
used `R` then `y` to confirm a new retry. After the fake provider was made healthy,
generation 4 succeeded. The browser and CLI then showed embedding and semantic
dedup succeeded, consolidation and validation explicitly skipped. The already
completed validation stage remained at generation 1, attempt 1.

PostgreSQL inspection showed seven total attempt rows: four embedding attempts,
one validation, one semantic dedup and one consolidation. There was one embedded
chunk. The fake provider counted four requests. All three failed generations
remained in browser history; successful stages had retry disabled.

## Bugs found and corrected

- Oldest-ready age included dependency-waiting jobs. The query now excludes
  stages whose prerequisites have not succeeded or skipped; the real HTTP
  regression asserts a null ready age when only blocked dependencies remain.
- At 80×24, the TUI list hid its attempt details. Inspection now replaces the
  list, history precedes sibling stages, and Escape returns to the list. A
  standard-terminal rendering test checks visible attempts and confirmation IDs.
  A rebuilt-binary PTY inspection displayed generations 4, 3 and 2 without
  resizing the terminal.
- The Jobs `r` handler intercepted the recovery screen's health retry. When
  health is unavailable, it now also schedules the normal backend refresh.

## Additional database fault evidence

Thirteen background-worker tests pass with required disposable PostgreSQL.
They include partial embedding success, lease recovery, revoked authority during
provider execution, stale worker health during database failure, source changes
during synthesis, quota enforcement after explicit retry, real HTTP project
denial/CSRF checks and bounded sanitized operator output.

Consolidation now caches theme extraction separately from synthesis. A fake
second-step failure produces three calls across two attempts (themes, failed
synthesis, successful synthesis), one proposal receipt and two private cached
outputs. Restarting the supervisor does not repeat the completed provider or
proposal. This preserves the existing synthesis prompts and citation guard; it
does not alter clustering or retrieval algorithms.

## Real process crashes and competing services

`node scripts/r1-process-smoke.mjs` passed against the built binary. The script
creates its own pinned pgvector container, fake provider and independent service
processes, then removes its exact container and stops its owned children.

- Killing the process after curation commits but before any claim preserves the
  queue. A fresh process completes the embedding stage on its first attempt.
- A second service does not duplicate a live owner's provider request. Killing
  that owner during the request leads to lease recovery, an uncertain
  `lease_lost` attempt and one new request. This deliberately demonstrates the
  repeat-billing window rather than claiming exactly-once provider execution.
- A database trigger delays only the final job acknowledgement. After the
  worker's applied-version marker and vector commit, killing its process and
  starting another produces one chunk/vector and no second provider request.

The harness waits for the committed applied-version marker, not text-chunk
existence: curation itself creates unembedded chunks. It also waits for TCP
PostgreSQL readiness, avoiding the image's temporary initialization server.
CI's required-database bucket now runs the entire workspace and this process
smoke. No remote CI run is claimed by the local result.

Still required by the full gate: remaining mutation/external-operation inventory,
final validation and the coordinated writer-stop rollout procedure.
