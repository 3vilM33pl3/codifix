# Watched-agent expertise rc.4 deployment — 2026-10-04

Status: **Monolith installed; CA Trust upgrade in progress**. This
`2.1.0-rc.4` release is built from
source revision `71cbfc9c2030e5f9fed1320c88b80b34dd36c68c` on
`codex/federation-v1`. It changes the database schema from the prior
application head 61 (gateway head 60) to head 65, and adds an opt-in addressed
agent-expertise policy. The release manifest declares an approval-required
migration and permission change. Olivier approved the rc.4 **unsigned internal
release exception** after the established signing key remained unavailable.
Checksums and the existing private channel bind this installation; it is not
authenticated by the normal signing verifier or customer release evidence.

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

The CA bundle is staged in the existing private LAN drop at
`/home/olivier/.local/state/memory-ca-federation-drop-20260928/rc4-watched-agents/`.
Its `START-HERE.md` SHA-256 is
`e195cc4a5b5cecfaed64b2fc4035a3a53dd4308dd39f26273b13fed16ef40806`;
`SHA256SUMS` is
`e46cd10fcc607b11785e41d6bd1f4b30cdece4db787cb78c2853b3d7e47147f5`.
The full frozen-source archive is
`6b44997b4c69c579e5bc411304a62301650ac00e12975c183e729581e359d3cd`.
The LAN server returned HTTP 200 for the handoff. The
[CURRENT ACTION](https://app.notion.com/p/3e78990715f48170a918f3f2c70277c9)
page has promoted instruction `monolith-ca-20261004-watched-agent-rc4-v24` to
`CA_RC4_INSTALL_ACTIVE`, and asks the CA agent to report directly in
[RESULT INBOX](https://app.notion.com/p/3e78990715f481cd935ff0a897040e4e).
The bundle contains no credentials or source data.

Before any live change, fresh public-key-encrypted backups were captured under
`/data/tmp/memory-agent-rc4.GM1GOp/backups/`: custom PostgreSQL dumps for
Workshop, Homelab, General and gateway; a consistent SQLite backup of the
private PDS and its repository key; and selected configuration, identity and
receipt material. All ciphertext hashes and decryptability were checked, and
the four PDS SQLite copies passed `quick_check`. These are **live-writer
snapshots, not a coordinated cutover point**. A stopped-writer backup was
subsequently taken; its independent restore remains untested. No CA-local
backup is included.

All four encrypted Monolith PostgreSQL dumps were restored into separate
disposable PostgreSQL 17 databases, entirely separate from production. Pinned
`sqlx-cli 0.8.6` applied pending migrations to each: gateway 61–65 and each
participant 62–65. Every clone then reported migration head 65 with 65
successful history rows. The gateway clone retained 22 accepted publications
and had zero new addressed intents. This verifies those four snapshot upgrade
paths; it does not verify a stopped-writer cutover, runtime grants, CA's own
database, or upgraded live services by itself.

For the Monolith cutover, the watcher manager and three active watcher units,
three app containers and gateway were stopped. All four target databases had
zero other clients and exactly the expected old schema heads. A fresh set of
public-key-encrypted stopped-writer custom dumps was taken under
`/data/tmp/memory-agent-rc4-cutover.5N6TB3/`; every ciphertext decrypted and
passed `pg_restore --list`. The ciphertext hashes are Workshop
`b6daa25177345d7d8ecc4a21f9d538c13a8f56bd1a6f44d7aec924449baa817c`,
Homelab
`cb01496db6da795750ad26fe972c034661ce51a13e16ebc1bb20ee57f8001888`,
General
`073eb2ce158da64e23b76bbe60aa73a5bb7a96702a631af88b591d8bc37bd383`,
and gateway
`39dea631fa6c3f59b070737bac862a487c7b43f995a98e618f8d112b8384044a`.
They are mode 0600 under a private directory. They were **not** independently
restored; the earlier live-writer versions were restored and migrated in
disposable databases.

Pinned SQLx applied 61–65 on the live gateway and 62–65 on each live area.
The gateway runtime received only the reviewed new table grants; each app role
received `SELECT, INSERT` on agent intents and `SELECT, INSERT, UPDATE` on the
daily budget table. The gateway and three apps were restarted on the packaged
AMD64 hashes above. Workshop, Homelab and General Web entry points returned
HTTP 200. Workshop, Homelab and General/sctp federation status retained
sequence 22 with no queued, blocked or synchronization-error intents; existing
credential health remained usable. The Debian package upgraded to
`2.1.0~rc.4` without starting services. The area launcher, watcher manager,
three active watcher processes and nine Memory-owned global skills use rc.4;
the new watched-agent skill appeared with the packaged hash in the Homelab,
sctp and dish2-tools repositories. Already-running desktop MCP processes may
still map the old rc.3 inode until their client reconnects.

The full Rust workspace, Lean build/test, Web tests/build, scoped JavaScript
tests, formatting, Clippy and boundary checks passed before packaging. A
disposable four-enrolled-area gateway test exercises Codex/Claude destination
bindings and recipient isolation; separate fake-PDS tests exercise signed CAR
inclusion/absence and addressed deletion. A true four-service-plus-PDS
rehearsal and live agent-type cited answers have **not** yet been demonstrated.

The rc.4 software does not automatically grant Codex or Claude onward
disclosure. Existing collections remain `local_human` only unless an operator
reviews exact content and explicitly changes each collection and caller policy.
Neither the package nor this record authorizes paid synthesis, Pi expansion,
public access or ordinary-query automatic remote requests.

The ARM64 package contains a byte-identical `memory-federation-expertise`
skill template from the frozen source revision. A post-build Web run passed
32 test files and 150 tests. CA-native installation, addressed-fleet enablement
and live agent-type cited answers remain unverified.
