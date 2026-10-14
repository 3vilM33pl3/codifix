# Mac project replication execution

Approved 2026-10-14; continues the existing onboarding work on
`codex/mac-project-onboarding`. The device downloads all curated knowledge for
the selected Homelab and Workshop/Memory projects. Only locally curated knowledge
uploads; raw captures and session transcripts remain on the collecting machine.
Git clone uses user credentials; commit/push remains the user's normal workflow.

The dev checkpoint succeeded before implementation: capture
`ffc3768d-06e7-4610-9faa-b16be4f61356`, curated plan
`c3b4ac9a-63ce-447c-b384-973270144f21`, thread
`mac-project-sync-20261014`. The isolated onboarding database and dev service at
loopback port 4250 were restarted. Production services were not changed.

## Delivery checklist

- [ ] Versioned full curated-project device download, separate from shared
  publication consent; histories/state/tombstones/relations and bounded provenance.
- [ ] Transactional curated contribution queue and current-authority owner
  submission/receipts; corrections/conflicts preserve owner history.
- [ ] Startup/reconnect/five-minute and post-curation synchronization, local
  writable workflow, normal retrieval/history/agent access and watcher routing.
- [ ] Homelab company configuration, identity/project assignments and manifests.
- [ ] Native package with dependencies, guided resumable setup, Keychain and
  LaunchAgents without changing unrelated repositories/settings/services.
- [ ] Observed MacBook acceptance: login/select/clone/offline search, both sync
  directions, restart/retry/conflict/revocation and no raw/session transfer.

## Compatibility and acceptance

The existing publication-only feed and private ATProto schemas remain compatible.
Full-project device replication does not grant other participants access to the
project. Downloaded knowledge retains origin attribution and cannot be echoed
as a new local publication. Acceptance requires actual native macOS execution;
synthetic/Linux tests are intermediate evidence only. Trust, Windows, R3,
new paid providers and additional plugin formats remain separate.
