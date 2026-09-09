# Stage E completion contract

Approved roadmap continuation, 2026-09-09, from `15f3ff1`. This is implementation
and controlled deployment evidence, not customer release or commercial approval.

- [x] Finish discoverable local space, grant, subscription, review, member, stream and proposal views with exact-project permissions and bounded reads.
- [x] Implement idempotent authority-owned remote membership actions, origin-directed proposals and explicit approve/reject transactions; preserve source/shared counters and reviewed provenance.
- [x] Add explicit consent pause, retry and full-resync operations without bypassing current local/gateway policy or recalling retained copies.
- [x] Deliver Web, CLI and TUI controls for enrollment/membership, review/publication, correction/approval and synchronization status through the same authenticated API.
- [x] Exercise the two-instance workflow with separate databases and credentials, actual gateway/PDS publication, attributed retrieval, correction, disconnect/catch-up/duplicates and revocation; verify browser and terminal workflows.
- [x] Run Rust/Lean conformance, required database tests, browser/component tests, formatting, Clippy, boundaries and existing relevant isolation checks; document measured evidence and remaining release limitations.

Remote credentials/keys/endpoints remain explicitly administrator-enrolled, not
client-supplied request fields. No PDS fork, security bypass, alpha Spaces,
customer data/deployment, model forwarding, Notion or startup-document edits.

Measured results and scope qualifications: [E completion evidence](2026-09-09-e-completion.md).
717 Rust tests, 66 Web tests, 14 Lean traces, real-PDS process/browser/CLI/PTY gate
and TLS/SQL/network isolation passed. The full master roadmap remains incomplete.
