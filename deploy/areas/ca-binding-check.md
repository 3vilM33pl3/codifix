# Trust service binding follow-up

`ca-binding-check.py` is a narrowly scoped read-only console check. It is source
for review, not an installed command. It requires Python 3.11+, root procfs access,
local `systemctl`, and passwordless PostgreSQL peer access through `runuser`.
Run only when the operator authorizes this observation:

```bash
python3 -I -S deploy/areas/ca-binding-check.py
```

It inspects only `memory-layer@trust.service`, its current process-view credential,
and PostgreSQL `memory_trust` on the fixed local socket port 5433. It never uses
application credentials to connect. The pinned rc4 opened-image SHA256 is
`477d1669e5f5fbaac81ac5be06ec2e64e10c7444ccade71b161b2d8a2d5c117e`;
the configuration-loader reference is source revision `71cbfc9c`.

The output separates executable identity, the current credential's configured
target, and an observed connection. The latter requires stable process socket
inodes/client ports surrounding a bounded `pg_stat_activity` metadata query,
a private role comparison, and unchanged service PID, process start time,
namespaces, and opened-image hash. The helper and process must share a network
namespace. IPv4 and IPv6 loopback literals are supported, and the observed TCP destination
must equal the configured literal. The PostgreSQL Unix socket and TCP listener
server identity are not independently bound, so the result is correlation only.
Fixed failure-stage enums and a permission-limited flag distinguish partial checks
without exposing exception text. Unsupported arguments,
profiles, URL options, environment overrides, sibling `memory-layer.env`, symlinked
credential components, permission errors, malformed metadata and socket churn
fail closed. Outer credential directories must be root-owned; only the unit
credential directory and regular credential file may instead belong to the exact
process UID. Group/world writable components are rejected. It emits a fixed allowlist without values from parser errors,
configuration, environment, addresses, roles, paths, commands, or records.

This is a point-in-time correlation. Current credential contents do not establish
what was read at startup, and matching the pinned hash does not establish artifact
provenance independently. Absence of a matching connection stays unknown. This
check does not establish dataset completeness, isolation, migration readiness,
offline coverage or recoverability. Recovery remains pending operator-supplied
exact receipt paths and a reviewed schema; no directory search is performed.
No installation, activation, configuration, auth, or service changes are made.

Synthetic checks (no CA access):

```bash
python3 packaging/tests/test-ca-binding-check.py
```
