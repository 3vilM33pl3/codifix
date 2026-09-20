# CA console installation: inactive first-install slice

Continuation authorized by Olivier on 2026-09-20. Dev Memory task:
`dd4d6431-97e9-4399-ba85-e787dd0203b2`, memory
`ebd87bbe-d411-4158-9401-c7ee7a5cd04f`, thread
`ca-console-installer-and-recovery`, starting at `32763df4`.

## Scope

Build and test an **inactive first-install** console tool for one new named
Memory instance. This is not an upgrade of the CA's running singleton. No
operator key, real credentials, CA service, original database or existing
preview may be changed by development or verification. Tests use disposable
signing keys, synthetic configuration, private directories and isolated containers.

Required behavior:

- Independently authenticate a signed release/preparation and bind the exact
  operator-reviewed configuration hash, instance, architecture and schema claim.
- Use fixed per-instance installation and recovery paths; no CLI override of the
  live filesystem root, no package hooks, arbitrary commands or model execution.
- Refuse existing instance paths, active/pending units, activation markers,
  unsafe permissions, links, changed bytes and unsuitable staging configuration.
- Journal intent before publishing each filesystem effect. Recovery verifies
  actual bytes before reconciling an ambiguous rename; never overwrite or erase
  an unexpected path. Preserve interrupted stages and retained installations.
- Install matching executable/Web/configuration/digest files without enabling,
  starting, stopping or restarting services. No activation marker is produced.
- Allow restart recovery after a committed transaction plan, and retention of
  this inactive first install. No database rollback, schema observation, writer
  fencing, upgrade rollback or live credential rotation is claimed.

The unsigned Debian 12 candidate is still **not** an authenticated release.
Its identity remains in `ca-bookworm-release.md`. The existing release format
requires both architectures from a coherent release; do not invent a second
artifact or sign with a disposable fixture key to bypass that requirement.

## Delivery checklist

- [x] Checkpoint scope and inspect existing release, launcher and sandbox contracts.
- [ ] Implement the inactive installer, protected journal and recovery commands.
- [ ] Prove signature/config/path refusals, concurrent exclusion and interrupted
      publication recovery using signed synthetic fixtures.
- [ ] Wire offline CI checks and document exact console preparation/recovery.
- [ ] Record verification evidence and remaining real-release/CA activation gates.

## Remaining independent gates

Complete release assembly and operator signing, CA-native ABI/sandbox checks,
offline-data reconciliation, distinct database/credentials, final backup/restore
and delta review, original writer fencing and explicit cutover approval. Pi stays
separate. The first-install tool must not claim those gates merely because its
filesystem transaction completed.
