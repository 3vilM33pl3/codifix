# Stage C completion contract

This is the implementation breakdown of the approved C1–C3 scope, not a new
protocol decision or a change to the release gates. Continue from `58c3457`.

- [ ] Persist explicit local sharing grants, reviewed evidence and selected-stream subscriptions. Capture later source content/state changes as frozen intent inside the source mutation transaction, with no default enrollment or automatic evidence expansion.
- [ ] Add schema-v2 Atrium encoding with unknown-field retention, deterministic record identity and separate neutral digest/transport CID. Keep the old spike fixtures unchanged and the namespace unpublished.
- [ ] Implement immutable PDS create/equal-readback, bounded HTTP behavior and CID verification. Test real publication and uncertain replies against the pinned private PDS without a fork or weakened security settings.
- [ ] Implement authenticated gateway acceptance/published-prefix pages and source/receiver scheduling with durable failure/retry state. Catch-up and full paginated replay must preserve the already trusted prefix.
- [ ] Exercise source mutation, queue, acceptance, real PDS publication and receiver projection across separate databases; cover disconnection, duplicate/reordered delivery, revocation, corruption and retry recovery.
- [ ] Run real database, Rust/Atrium/Lean conformance, workspace, Clippy and boundary checks; update C1–C3 only on evidence and retain all D/E/R1 and release exclusions.

Review/enrollment APIs may be trusted in-process service interfaces at this stage;
Web/CLI/TUI parity remains E. Production deployment and external namespace
publication remain unauthorized. The live tests use disposable accounts/data.
R1's advisory post-curation jobs and timeline log are not used as federation order
and are not silently marked complete by this work.
