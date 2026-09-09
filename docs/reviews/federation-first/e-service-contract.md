# Stage E first increment: authenticated local workflow API

Execution scope for “Start working on E”, 2026-09-09. Continue the approved
roadmap from Stage D at `e27c475`; this is not completion of E1–E4.

- [x] Register the explicitly configured instance runtime with the local service;
  expose no gateway/PDS credentials, endpoints or client-selected trust roots.
- [x] Add centrally authorized, exact-project/space HTTP operations for local
  grants, synchronization status, publication preview/review and retained shared
  context. Local grants never enroll remote membership or imply space authority.
- [x] Bind publication consent to the authenticated reviewer and exact current
  source/evidence bytes; make standing future-update consent explicit. Report
  queueing independently of gateway acceptance and replication.
- [x] Preserve attribution, signed provenance and sharing restrictions in bounded
  retained context; keep it out of implicit search, curation and model execution.
- [x] Exercise positive and negative HTTP/database workflows, update API/operator
  documentation, run relevant Rust checks and commit the verified increment.

No production deployment, namespace publication, alpha Spaces, credential
rotation, Notion/startup-document edits or external model call. Remote enrollment,
correction proposals/approval, Web/CLI/TUI clients and the complete independently
administered two-party journey remain subsequent E work. Existing offline-copy
and revocation limits continue to apply.

Completed at the bounded application-API scope in `fdcffab`; see the
[implementation evidence](2026-09-09-e-service.md). The master roadmap remains
15/39 complete, with all four full Stage E items still unchecked.
