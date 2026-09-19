# Homelab / Workshop exchange increment

Status: independent local Homelab preview verified, 2026-09-19; exchange remains
in progress. This is an increment of the
[approved four-area contract](contract.md), not a completed network or customer
release. The approved-plan Memory checkpoint was renewed before edits.

## Current scope

- [x] Independently import a small reviewed Homelab observability collection,
  preserving original project/version identities and archive-reference provenance.
- [x] Verify encrypted backup copies and restore the new target before activation.
- [x] Demonstrate the independently authenticated local Homelab Web/CLI instance.
- [x] Add reviewed, digest-bound gateway membership enrollment tooling.
- [x] Add a private company topology without fixture accounts or bypass networks.
- [ ] Provision the private gateway/PDS with separately pinned receipt/repository
  keys and a separate space authority; retain interrupted-operation evidence.
- [ ] Review exact per-recipient collections and enable deterministic exchange.
- [ ] Demonstrate both directions and a denied non-member, with signed receipts.

The initial local Homelab selection contains two historical observability
records: SigNoz endpoint ownership and Docker blackbox-probe routing. This is a
delegated operator content review under the approved logical area split, not a
claim of separate human approval or current infrastructure validation. No raw
captures, sessions, source paths or unrelated project rows enter the new corpus.
The complete originals remain in the protected archive. This does not finish
the full dataset partition or fence original writers.

Homelab starts with model calls, Pi execution, federation and background learning
disabled. Workshop's existing time-bounded model grant is not extended to it.
The local preview uses a different loopback host (`127.0.0.2:4261`) so its browser
session cookies do not replace Workshop's cookies on `127.0.0.1:4260`.
Neither owner-bootstrap preview endpoint is a LAN service.

Trust/CA, General, DNS, existing homelab services, paid evaluation and customer
release gates remain unchanged. Existing Workshop remains independently usable.

## Verified local increment

Use the [Homelab demo guide](../../user/homelab-preview.md). The separate instance
is available at `http://127.0.0.2:4261/`; `memory-homelab` opens its TUI.
The original Homelab project ID is preserved, with persistent imported instance
ID `1573e46e-ee3a-4ed9-b5ac-4b2fbf8da196`. The health endpoint's process instance
ID is a different runtime identifier, not an enrollment identity.

The preview contains two complete, single-version canonical lineages and 14
archive-only provenance references, no raw captures and no other projects. The
114-table pre-activation logical database restore matched row counts and ordered
row-content digests. The restored test database was then closed to connections.
This is database recovery evidence, not a full host/key/offsite recovery claim.

At 09:14 UTC, both instances accepted only their own credentials (`200`) and
rejected the other's (`401`). Asking Homelab for the absent `memory` project
returned an empty result, not Workshop records. Its deterministic query cited
exactly the two selected Homelab versions. Web inspection and a real PTY TUI
session showed both records and source detail; CLI health and query succeeded.
The app and database have no published Docker ports. Their network is internal;
only the fixed UI and database relays listen on host loopback.

Runtime startup exposed an owner-privilege assumption in automatic migration.
The new explicit `database.migrations = "verify_only"` checks the full embedded
migration history and fails closed without attempting DDL. The runtime role has
neither schema-create nor migration-history-write privileges. Required-database
runtime tests (including sealed R3 behavior), focused config tests, PostgreSQL/
TUI CLI build and strict Clippy, formatting and boundaries passed. The unchanged
Web bundle was copied from Workshop; this is not a claim of a new full workspace,
Web or Lean regression run.

### Protected evidence

Receipts and configuration are under
`/home/olivier/.local/state/memory-homelab-preview-20260919`.
Encrypted, hash-verified NAS copies are under
`/media/bunnychow/backups/memory-four-area-20260917/homelab-preview-20260919`.
No credentials or source archive bytes are committed to the repository.

| Artifact | SHA-256 | Scope |
| --- | --- | --- |
| Reviewed manifest | `8b82d938eed242f8d93b6ab15303880e8f8867ca213d6f8e38bade4ef2ca45d8` | Two selected lineages; complete remaining source archive |
| Encrypted source/review archive | `2d0af863e67e5e53f6b8c930231f8efc6be7206dd6081a55001393a77c220518` | Protected full source and review inputs |
| Encrypted pre-activation dump | `023f6f6d96172fbebb5ce31729bb02512c2225a748a84161d2703cb9299589c2` | Logical contents independently restored before activation |
| Restored-table comparison | `d0bdce70e924605c0e5482de3fa31903aa3b8e1478e179ab165559ac5583e97f` | All 114 logical tables |
| Running application | `35a2416aae9aca5d094cde45819ece41976a58e314649650860cedf11de57ac8` | Prepared-database mode, lean PostgreSQL/TUI build |
| Runtime recovery bundle | `d77faf76e639ac7fd800c201a886fd8b78e77d492316919214f3953b46e4e6b9` | Fresh dump, exact binary/Web, own configuration/credentials and relay helpers; no provider key |

The runtime bundle and encrypted decryption/recovery path have **not** had a full
restore rehearsal. Do not equate a verified ciphertext copy with verified key
recovery. Do not replay the import into the now-active target.

## Gateway work remaining

The [membership operator](../../user/federation-enrollment.md) requires an
independent authority, exact review digest and pinned signed history. Its tests
include private-input file replacement rejection. The
[company topology](../../../deploy/federation/company-topology.md) and isolated
[bootstrap overlay](../../../deploy/federation/company-bootstrap-topology.md)
keep member, publisher, operator and DID-host paths separate.

The [durable PDS provisioning operator](../../../deploy/federation/pds-enrollment/README.md)
now retains creation, signing-key handoff and refresh intent instead of borrowing
a short-lived session from the disposable fixture probe. Its 16 injected tests
pass. Real pinned-image SDK, TLS/DID and signed-repository rehearsal remains a
gate before real company provisioning; fake tests do not establish readiness.
Gateway token installation/restart and blocked-publication retry remain explicit
operator operations. No real company PDS, membership or collection has yet been
activated by this increment.
