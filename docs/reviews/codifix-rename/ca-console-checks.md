# Codifix rename: CA console checks pending

CA is console-administered only. SSH is deliberately disabled: do not enable it,
request SSH certificates or renew a login. Preserve Step CA, OpenBao, issuance,
trust, credentials, firewall and the existing host security boundary.

## Exact read-only console commands

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

- `https://ca.metatao.net/health`: verified HTTPS, HTTP 200, `status: ok`.
- `https://openbao.metatao.net/v1/sys/health`: TLS negotiation returned
  `TLSV1_ALERT_INTERNAL_ERROR`; no HTTP health result. OpenBao health is unverified.
  Do not bypass TLS verification or alter its configuration to complete this check.

HTTPS endpoint health does not establish local service/database ownership.
All CA host-local inventory, restore, retirement and install checks remain pending
until console results are supplied. No CA SSH connection or host mutation was made
for this follow-up.
