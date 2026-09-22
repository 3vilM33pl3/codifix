# Trust inactive preparation — proposal for the CA agent

22 September 2026. **Draft; do not execute host changes from this document.**
This supplements the original runbook, leaving the signed USB release unchanged.
The local transfer mechanism is tested; the CA receiver is not yet implemented
or approved for its exact PostgreSQL/extension profile.

## What Monolith has completed

The full shared source has a new encrypted snapshot and independent row/sequence
restore proof. A Trust-only candidate from the previously approved September19
partition has been restored and checked locally, preserving its logical identity
and import receipts without transferring other areas or executing source DDL.
Its encrypted local bundle is 301,633 bytes. It has **not** been sent to the CA.

Trust candidate: 19 histories, 22 versions, 65 relations, 185 reference-only source
records and 375 total imported rows. Reference-only records are not original
evidence bytes. All auth/runtime/job/federation/supervisor authority remains inert.

Fresh source comparison is **not ready for cutover**: 17 of 19 Trust histories have
dependency changes, although their memory-version/source rows are unchanged.
Across the full source, 262 decisions carry forward, 2,945 require renewed review
and 63 are new. Nothing was automatically approved. Old source writers stay up.

## CA agent: return these observations in one sanitized reply

Use existing authorized local inspection only; do not install packages or change
access to obtain them. Never post credentials, configurations, payloads or keys.

1. PostgreSQL server/client majors and installed/available `vector`, `pgcrypto`
   and `pg_trgm` versions. The tested local profile is PostgreSQL 17.11,
   pgvector 0.8.6, `pgcrypto` 1.3, `pg_trgm` 1.6; host `pg_restore` 17.9. Another major
   needs an explicit compatible implementation/rehearsal, not a forced restore
   or unapproved PostgreSQL upgrade.
2. Whether an **existing approved encryption recipient** can be decrypted on the
   CA (fingerprint/availability only). Release signing has no encryption subkey.
   Do not copy or create private keys based on this request.
3. Existing Memory units/processes, observed shared-source configuration binding
   (report match/mismatch, no URL), and existence/size of offline stores and local
   evidence directories. Unreadable or undiscovered is unknown, not empty.
4. Exact candidate **unused** CA-local database/role, loopback service port,
   resource allowance and protected backup destination. These remain proposals,
   not newly provisioned resources. No shared Monolith database credential.
5. Confirm the named-instance install paths are unused and record conflicts
   without moving or deleting anything. Do not touch the singleton's paths.

## Grouped inactive host-change proposal (complete locally before approval)

CA owns exact local paths/DB/role/network/security and interruption handling.
Monolith owns the minimized transfer, content bindings and source/delta review.
Ask Olivier once for the fully specified inactive change, not piecemeal commands.
Do not ask for blanket approval while required fields remain unknown.

| Resource | Proposed action and boundary |
| --- | --- |
| Signed release 2.1.0-rc.1 | Reuse authenticated native ARM64 artifacts already staged; preserve the USB. No new release signature or Pi artifact. |
| Root-owned tools | Exact authenticated `console.py`, `instance.py`, `release.py` at `/usr/libexec/memory-layer/releases/`; confirm hashes from original handoff. |
| Instance files | New `/opt/memory-layer/instances/trust/release/`, `/etc/memory-layer-instances/trust/`, `/var/lib/memory-layer-installations/trust/`; no overwrite/upgrade of existing state. |
| Database and role | CA-local only; name/owner/credential generation and least-privilege grants filled by CA agent. Fresh, isolated, never copied auth or service authority. |
| Data delivery | Separately authenticated, approved-recipient encrypted Trust-only artifacts over an explicit transport. No full archive or unrelated area. Receiver tooling/version pair must be tested first. |
| Restore | Fresh target, trusted pre-data + allowlisted data + trusted post-data in one transaction; independent receipt/hash/sequence validation. Preserve target UUID/import UUID. Never `--clean`, disable triggers or edit immutable receipts. |
| Configuration | Exact digest reviewed locally; read-only application mode and all background/model/watch/federation/offline replay/MCP paths off. Secrets stay in protected files. |
| Systemd | Install inactive under the existing console contract; no start/enable or activation marker. Native sandbox/CA-secret-path/egress denial tests are later explicit checks, not assumed from package compatibility. |
| Recovery | Preserve inputs, encrypted backup and install journal. Use inspected journal recovery for interrupted file installation. Failed fresh restore must roll back; ambiguous success requires inspection, never automatic drop/recreate. |

Keep Step CA, OpenBao, DNS, OS and the existing singleton unchanged. Installing
inactive is not permission to launch the service: startup can bootstrap/migrate
a database even when application requests are read-only.

## Separate activation gate

Before stopping or moving any original writer: complete CA-only/offline
reconciliation, renewed source/dependency review, exact receiver validation,
native runtime and sandbox checks, and a final coordinated writer-fenced capture.
Both the original staging copy and the relocated logical Trust instance must not
be active simultaneously. After any accepted target writes, recovery is not a
blind rollback to the old source.

Pi, federation enrollment, model destinations/budgets, R3 and customer readiness
remain separate. The [coordination page](https://app.notion.com/p/3e38990715f48117b9e8d8f3a95637d1)
is for sanitized agent messages, not authorization or an automatic worker.
