# Codifix rename: CA console inventory and remaining checks

CA is console-administered only. SSH is deliberately disabled: do not enable it,
request SSH certificates or renew a login. Preserve Step CA, OpenBao, issuance,
trust, credentials, firewall and the existing host security boundary.
Ask Olivier before any CA installation, service/configuration change, migration,
retirement or recovery mutation. Read-only follow-ups are coordinated through
Notion's CURRENT ACTION and RESULT INBOX page bodies.

## Received inventory, 17 October 2026

The CA-local agent returned the pinned helper report in
[RESULT INBOX](https://app.notion.com/p/3e78990715f481cd935ff0a897040e4e),
captured at `2026-10-17T21:17:38.259836+00:00`. Monolith independently matched
the posted JSON SHA-256:
`b5c7e46b37a999af9d4a2100bf3a6a3cffd47d06fdce7beeb5f703ca62ff357b`.
The exact JSON is retained privately, rather than republishing host metadata.

- Debian 12/aarch64, glibc 2.36; legacy `memory 2.1.0-rc.4`, no `codifix` binary.
- Named Trust service active; generic service and SSH masked/inactive; no port-22
  listener observed. Step CA and OpenBao processes active.
- Local PostgreSQL port 5433 has `memory_trust`, schema 65, zero failed migrations,
  508 canonical histories and 557 memory-entry versions.
- Inventory does **not** bind that database to the running application. Effective
  process configuration/socket correlation and recovery/offline coverage remain
  pending. Missing sandbox capabilities are not permission to install tooling;
  the separate Pi artifact's glibc 2.43 requirement is not a rename prerequisite.

The next bounded console request must inspect the named process, its process-view
credential and local PostgreSQL connection metadata without printing secrets.
Existing recovery receipts need exact operator-supplied paths and a reviewed
schema; do not recursively search arbitrary backup/private directories. An old
installation receipt is not proof of a current matched recovery bundle.

## Current Notion follow-ups

[CURRENT ACTION](https://app.notion.com/p/3e78990715f48170a918f3f2c70277c9)
contains two read-only requests, with prior installation instructions marked
historical:

- `monolith-ca-20261017-recovery-evidence-paths-v2`: metadata for already-known
  recovery receipts/manifests, exact paths and field names for scoped review.
- `monolith-ca-20261017-binding-observation-v3`: complete
  [binding helper source](../../../deploy/areas/ca-binding-check.py) from commit
  `f6791fa4830040b06de821e4d0fc603482c36a0f`, 12,963 bytes, SHA-256
  `8a98b258264306e98ebe2bd79c1c0905427c030daae3e3458881cf10c9ddf5a3`.
  All 22 synthetic tests passed; Astra's final source review found no blocker
  within the stated observational scope. Notion source was fetched again and
  matched byte-for-byte, including its final newline.

The helper checks pinned executable identity, current process-view credential
target and stable socket/PostgreSQL metadata correlation separately. It emits
fixed failure-stage/status enums and permission limits. Correlation does not
prove startup credential bytes, Unix-socket/TCP server identity, exclusive writer
ownership or recovery. See its [scope and limitations](../../../deploy/areas/ca-binding-check.md).
Both CA-local follow-up results are pending; no native execution is claimed.

## Completed inventory procedure (historical reference)

Transfer `deploy/areas/ca-preflight.sh` from commit `4eca6946` through the existing
approved local transfer mechanism. No network installer or package is needed.
At the existing CA console, in the directory containing that standalone file:

```bash
export PATH=/usr/sbin:/usr/bin:/sbin:/bin
printf '%s  %s\n' '65cb4aca0de64079ac8e94d9bad5b053c147474a6985f41b5192aadcdcb08887' 'ca-preflight.sh' | sha256sum --check --strict - &&
  bash ca-preflight.sh
```

Run the second command only after the checksum passes. Use the existing root
console for local peer-authenticated PostgreSQL metadata; otherwise the script
reports permission limits. Do not install a missing dependency or change access
policy to make a check pass. The helper has a 60-second budget and prints one
sanitized JSON report. Return that report, including partial/failed checks.

It reads legacy and candidate binary versions, service states, listening-port
scopes, OS/libc/disk metadata, local PostgreSQL candidates, migration heads,
fixed aggregate counts and anonymous client counts. It does not print application
configuration, environment, credentials, private keys, record contents, process
arguments or raw command failures. Database queries force read-only transactions.
It never starts, stops, installs, migrates or reconfigures anything.

The report does **not** establish which database the active application owns or
prove that offline data is covered. Those checks remain pending. Once the console
report identifies the actual installation, provide exact scoped follow-up commands
for service/database binding and a matched recovery capture. No mutation command
is authorized by this inventory handoff itself.

## Remote health evidence, 17 October 2026

- At 21:25 UTC, `https://ca.metatao.net/health`: verified HTTPS, HTTP 200,
  `status: ok`.
- At 21:25 UTC, canonical `https://secrets.metatao.net/v1/sys/health`: verified
  HTTPS, HTTP 200, initialized, unsealed, active (not standby), version 2.5.1.
  Earlier TLS failure on `openbao.metatao.net` was a different hostname; no TLS
  bypass or configuration change was needed to check the canonical endpoint.

HTTPS endpoint health does not establish local service/database ownership.
Host inventory is received; binding, recovery coverage and restore validation
remain pending. Retirement and installation have not been authorized or performed.
No CA SSH connection or host mutation was made for this follow-up.
