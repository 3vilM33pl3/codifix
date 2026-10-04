# Watched-agent expertise release candidate — 2026-10-04

Status: **built and locally verified, not installed**. The live four-area
deployment remains on `2.1.0-rc.3`. This `2.1.0-rc.4` candidate is built from
source revision `71cbfc9c2030e5f9fed1320c88b80b34dd36c68c` on
`codex/federation-v1`. It changes the database schema from the installed
application head 61 (gateway head 60) to head 65, and adds an opt-in addressed
agent-expertise policy. The release manifest declares an approval-required
migration and permission change. It is **unsigned**; no installation may be
described as authenticated by the normal signing verifier.

| Candidate artifact | SHA-256 | Evidence |
| --- | --- | --- |
| AMD64 Debian package | `7438f2940da45641c3976241c17e974f79fda1d26033aa246472ab7dedd0b849` | Monolith build; `memory --version` says rc.4 |
| Debian 12 ARM64 package | `add42967641576f578872021f7102f1042cee1a09c155c5c3999f801e9756c9e` | Pinned Debian 12 builder `sha256:ed8f07b19272db7d8759355733e5564a2745a4059f44b7065b130abc97668fac`; glibc dependency 2.36 |
| ARM64 Memory/Web tree | `2e5789c058ae6f2abda12e5ce90a1a1529baf154c29c6a2ed816625f966d1277` | Extracted from the ARM64 package for the existing CA upgrade path |
| Unsigned manifest | `9402dd6c0ea6d0bc74aca57243893a556148797586ae25cea202676dab4c5f6d` | Schema interval 60–65; source and Pi pins recorded |

The packaged AMD64 Memory and gateway hashes are
`29c3850eee210802f893d40fc28832af7f0ca7a1b33c8066a3f83f6d82bf1b22`
and `1ac5a9e91749d3d4f81e2eace982f3c3d03bcec978415f2f0915a6d6cdf82ade`.
The ARM64 Memory and gateway hashes are
`477d1669e5f5fbaac81ac5be06ec2e64e10c7444ccade71b161b2d8a2d5c117e`
and `2220bf59f90d02f241e2d13b5455832bcc59efba1168433b462174adc90b37e8`.
All four ARM64 executables passed the repository's static Debian 12 ABI
closure check in the pinned image. The packaged ARM64 Memory executable
reported rc.4 under QEMU with network disabled. That does not establish native
CA execution or service health.

The inert CA candidate is staged in the existing private LAN drop at
`/home/olivier/.local/state/memory-ca-federation-drop-20260928/rc4-watched-agents/`.
Its `START-HERE.md` SHA-256 is
`900de44a38b54a7d38a5d9076ac200cc517c8c89390c7313da24789bca7133a6`;
`SHA256SUMS` is
`c6d8bca4c2f1b581b4d4dd174739d4a61d33799ed58d7db4db4ad2457c736cea`.
The full frozen-source archive is
`6b44997b4c69c579e5bc411304a62301650ac00e12975c183e729581e359d3cd`.
The LAN server returned HTTP 200 for the handoff. The handoff itself states
that it is **not an active CA instruction** until CURRENT ACTION promotes its
instruction ID. It contains no credentials or source data.

Before any live change, fresh public-key-encrypted backups were captured under
`/data/tmp/memory-agent-rc4.GM1GOp/backups/`: custom PostgreSQL dumps for
Workshop, Homelab, General and gateway; a consistent SQLite backup of the
private PDS and its repository key; and selected configuration, identity and
receipt material. All ciphertext hashes and decryptability were checked, and
the four PDS SQLite copies passed `quick_check`. These are **live-writer
snapshots, not a coordinated cutover point**. A stopped-writer backup and a
restore check are still needed at installation. No CA-local backup is included.

All four encrypted Monolith PostgreSQL dumps were restored into separate
disposable PostgreSQL 17 databases, entirely separate from production. Pinned
`sqlx-cli 0.8.6` applied pending migrations to each: gateway 61–65 and each
participant 62–65. Every clone then reported migration head 65 with 65
successful history rows. The gateway clone retained 22 accepted publications
and had zero new addressed intents. This verifies those four snapshot upgrade
paths; it does not verify a stopped-writer cutover, runtime grants, CA's own
database, or upgraded live services.

The full Rust workspace, Lean build/test, Web tests/build, scoped JavaScript
tests, formatting, Clippy and boundary checks passed before packaging. A
disposable four-enrolled-area gateway test exercises Codex/Claude destination
bindings and recipient isolation; separate fake-PDS tests exercise signed CAR
inclusion/absence and addressed deletion. A true four-service-plus-PDS
rehearsal and live agent-type cited answers have **not** yet been demonstrated.

The rc.4 candidate does not automatically grant Codex or Claude onward
disclosure. Existing collections remain `local_human` only unless an operator
reviews exact content and explicitly changes each collection and caller policy.
Neither the package nor this record authorizes paid synthesis, Pi expansion,
public access or ordinary-query automatic remote requests.

The ARM64 package contains a byte-identical `memory-federation-expertise`
skill template from the frozen source revision. A post-build Web run passed
32 test files and 150 tests. The CA coordination page now carries only a
`PREPARED_NOT_ACTIVE` v24 note; v23 rc.3 remains its active completed instruction.
