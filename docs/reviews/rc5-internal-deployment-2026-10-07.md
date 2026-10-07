# Memory 2.1.0-rc.5 internal deployment

Olivier approved Monolith installation and the Trust upgrade instruction on
7 October 2026. This is an unsigned, checksum-verified internal Linux candidate,
not a public/stable release or macOS/Windows acceptance result. R3 remains paused.

## Frozen inputs

- Source: `b8bd43119f76c606ad6a59d969043050fe96a930`.
- Captured Debian-12 builder:
  `sha256:ed8f07b19272db7d8759355733e5564a2745a4059f44b7065b130abc97668fac`.
- AMD64 package: `7863fc1cd7322c3f938e906ce2053c6c55c092a75e1190ca71301b64975243a1`.
- AMD64 executable: `0a31df45dded21a46d865729216ee5ce67512e130db54f4fd6e58e51da360ddc`.
- ARM64 package: `282bd85a04b6bdf4ffe3c5894ff049fdc5a8ecd3916a4acc816262f5232754b2`.
- ARM64 executable: `f83fb49c7f55e1dedac2bd27bcd8e890c9ddccc334eb62ecad62052c2f4cfa4e`.
- Manifest: `302ec14514358ce0d02d0f8e880b6ec67e0a5f874d7d602d1f4d01bbf09e9d25`.
- Source archive: `53093f9b838df4127fdae9c90bb5ae031bc266b41d8ee07130532976f6992431`.

Artifacts, isolated build logs, one-off finite-target deployment scripts,
encrypted backups, previous binaries/skills and rollout journal are retained in
`/data/tmp/memory-rc5.u7DOhAtV`. Existing rc.4 release directories remain available.
Backups contain private material; only public source/package artifacts are on
the existing private LAN transfer server.

## Observed Monolith state

- Workshop, Homelab and General services report rc.5 with healthy databases.
- Gateway and all three participant databases migrated from SQLx head 65 to 76.
- Normal `~/.local/bin/memory` routes through the immutable
  `~/.local/state/memory-rc5-client/release` tree. `memory urls` works there.
- The watcher manager and its two active watchers use that rc.5 executable.
- Global Memory-owned skills were replaced from the exact bundled templates;
  earlier directories are preserved under the private deployment backup.
- The isolated company-administration development preview at port 4250 is now
  `memory-company-admin-dev.service`, running rc.5-dev with existing disabled
  federation/model/background-work settings. The browser reports Web and
  service rc.5-dev. This does not activate Authentik company management or finish
  native Mac onboarding.
- All three federation streams retain sequence 23 with zero synchronization
  failures and no queued/blocked publication intents. Existing credential
  maintenance renewed successfully after the restart.

The system-wide Debian package remains rc.4 **pending refreshed sudo**.
The user-owned deployment is not a claim that `/usr/bin/memory` changed.
Already-open rc.4 TUI/MCP processes also retain their original executable until
a normal restart/reconnection; agent chats were not forcibly terminated.

## Recovery and corrections

Four encrypted PostgreSQL backups and an encrypted stopped-writer runtime
archive were taken. The preflight General dump restored without network access:
1,179 memory rows, four sequences, and successful additive migration to head 76
(146 public tables). This is a scoped restore test, not customer disaster
recovery certification or a claimed restore of every final dump.

Before rollout, the gateway TLS container had failed to start because its
reserved API-network address `172.31.0.2` had been allocated dynamically to the
gateway. Pinning the gateway to free `172.31.0.3` and starting the existing TLS
front end restored authenticated status and synchronization. The explicit IPAM
binding persists across gateway restarts; certificates and trust keys did not
change.

The initial one-off migration operator mistakenly applied participant onboarding
table grants to the gateway role too. The gateway's startup restriction rejected
that role before serving requests. Those grants were revoked and the operator
corrected: gateway access is limited to the four new federation-project tables
and its existing federation tables. The runtime then started successfully.
No owner, login, role-membership, schema-creation or cross-database grant was added.

Migrations and accepted writes require forward recovery. Do not replace the
current authoritative databases with stale rc.4 snapshots or restart rc.4 service
writers against head 76.

## Trust handoff

CURRENT ACTION v25 (`monolith-ca-20261007-rc5-v25`) is published and re-read with
status `RC5_PACKAGE_READY`. The complete package and procedure are at
`http://10.22.6.42:8444/rc5-internal/START-HERE.md`.
All five payload files were downloaded through that server and passed the
delivered `SHA256SUMS`. Its SHA-256 is
`bfdf32d69c6863d4615c0b95adfa559e25d9255ad346d2676b96010907770874`.

The instruction tells the CA agent to own its backup, existing-target software/
schema upgrade, routing/skill refresh, local checks and existing-consent
deterministic federation check, reporting directly in RESULT INBOX. No CA-native
rc.5 result has been received yet; Trust installation is not claimed complete.
ARM64 Memory version and area-import help ran under network-disabled QEMU,
which does not substitute for native CA verification.

## Validation

- 13 focused lean CLI URL tests pass.
- 271 Web tests pass; both architecture package Web builds pass.
- Seven build-helper tests pass; ABI-helper tests: 65 passed, two skipped.
- Rust formatting and boundary checks pass.
- Both full release packages built from the same frozen source, preserving the
  existing PDS/Atrium/Pi pins.
- Live area health, new CLI URL output, watcher executable paths, browser
  version/status, migrated schema and federation synchronization were observed.

Existing broader CLI help/documentation failures remain documented in
[URL-command evidence](memory-urls-2026-10-07.md). A fresh full workspace,
customer release or native Mac/Windows acceptance pass is not claimed.
