# CA Trust: watched-agent expertise rc.4 handoff

Instruction ID: `monolith-ca-20261004-watched-agent-rc4-v24`.

**This is a candidate procedure, not an active CA instruction.** Execute only
when the CURRENT ACTION page explicitly promotes this instruction ID and records
Olivier's approval for this rc.4 unsigned internal release. Until then, retain
the working rc.3 installation. Report directly in the RESULT INBOX page body;
Olivier should not relay routine commands or logs.

Release source: `71cbfc9c2030e5f9fed1320c88b80b34dd36c68c`. The package
uses the same private ATProto/PDS and Pi pins as rc.3. It adds addressed
question/answer records and schema migrations 0062–0065; it does not grant
model disclosure consent. The normal signed-release verifier cannot
authenticate this **unsigned** candidate. Use only the narrowly pinned
existing CA unsigned-candidate path if the current instruction authorizes the
exception for rc.4. Do not disable signature checking generally.

## Obtain and verify

Use the existing private LAN transfer server and an isolated CA-local directory.
Retrieve `START-HERE.md`, `SHA256SUMS`, `manifest.json`,
`memory-layer_2.1.0-rc.4_arm64.deb`,
`memory-layer_2.1.0-rc.4_arm64.tree.tar` and
`memory-layer_2.1.0-rc.4_source.tar` from the `rc4-watched-agents/` directory.
Verify every byte against `SHA256SUMS` and independently compare at least these
anchors from this page:

| File | SHA-256 |
| --- | --- |
| `manifest.json` | `9402dd6c0ea6d0bc74aca57243893a556148797586ae25cea202676dab4c5f6d` |
| ARM64 Debian package | `add42967641576f578872021f7102f1042cee1a09c155c5c3999f801e9756c9e` |
| ARM64 tree | `2e5789c058ae6f2abda12e5ce90a1a1529baf154c29c6a2ed816625f966d1277` |
| Source archive | `6b44997b4c69c579e5bc411304a62301650ac00e12975c183e729581e359d3cd` |

The package requires ARM64 Debian 12 glibc 2.36. Its `/usr/bin/memory` hash is
`477d1669e5f5fbaac81ac5be06ec2e64e10c7444ccade71b161b2d8a2d5c117e`.
The same binary reported `2.1.0-rc.4` under network-disabled QEMU on Monolith;
that is not CA-native verification. Never install the AMD64 package on CA.

## Goal and operating limits

Upgrade only the active Trust Memory service, normal CLI/TUI, watcher manager,
watched-agent skill, applicable MCP path and Web assets to rc.4. Preserve Trust's
current database, authoritative identity, receipts, collection, policy, budget
journal, routing, PostgreSQL 17/main and the existing sandbox drop-in. Do not
re-import or reset knowledge. Keep Step CA, OpenBao, PDS/Atrium pins, Pi pin and
unrelated services unchanged. Do not start the legacy singleton writer.

First record the live schema, release, accepted sequence, active paths and
services. Take and verify a fresh stopped-writer CA-local database and config
backup. Then use the exact source archive's append-only migrations 0062–0065
through the existing schema-owner maintenance route; check SQLx history and
head 65 before starting an rc.4 non-owner service. Old rc.3 executables must
not write a migrated database. Preserve the owner/runtime separation. The
bundled release tree and package must match the manifest and current action.

Restart Memory-owned components in dependency order, leaving other CA services
running. Verify normal CLI/TUI routes to Trust, local search/history/citations,
watcher and skill version, package and process hashes, federation sequence and
zero unexpected queue or sync errors. Check an existing deterministic
Workshop→Trust cited answer and exact same-ID replay; no paid model call is
needed. A denied unbound caller must remain denied. Do not enable addressed
fleet participation until Monolith's gateway and other participants are at the
compatible schema/feature level and the joint test is scheduled. Joining the
fleet does not itself approve Codex/Claude collection access.

If a step fails after a migration or accepted publication, preserve the
database and accepted work; do not restore an older snapshot over it. Keep the
last compatible service stopped as needed, diagnose and report the exact state.
Do not ask Olivier to copy logs or commands.

## Report directly in RESULT INBOX

Give the instruction ID, release/package/binary hashes and explicit unsigned
exception; backup and restore-check results; Trust database/schema/instance ID;
pre/post publication sequence; CLI/TUI/watcher/skill/MCP routing; local
search/history/citation and replay results; denied-access probe; current
fleet/policy status; any incomplete step or retained old process. Label each
finding observed, not tested, or blocked. No source text, credentials, raw
captures or private answer bodies belong in Notion.
