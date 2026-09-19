# Disposable company topology rehearsal

This gate exercises the actual pinned Caddy binary and the generated company
network graph, without using any existing deployment. It creates a random,
marked `memory-company-rehearsal-<hex>` Docker Compose project, ephemeral TLS
certificates and protected temporary files. It never publishes a host port.

## Run

Prerequisites: Linux, a non-root user with permission to access a **local Unix
Docker socket**, Docker Compose, Node.js and OpenSSL. Both reviewed images must
already be cached; the runner does not pull images or install dependencies:

- Caddy: `docker.io/library/caddy@sha256:226d1f059b75399fe19182893c7184591c07b97afc8dfcf44eeb80c9a77a530f`
- PDS / Node fixture runtime: `ghcr.io/bluesky-social/pds@sha256:d95725b24dbe53af9d91dc69750556931ebed6c396f2cfa42b221434db642f12`

From the repository root:

```sh
node --test deploy/federation/company-topology.test.mjs deploy/federation/company-bootstrap-topology.test.mjs
node tests/company-topology/run.mjs
node tests/company-topology/run.mjs --with-pds
```

The first Docker command exercises TLS, routing and isolation. The optional
`--with-pds` adds the actual pinned PDS provisioning/key-handoff/session/signed-CAR
rehearsal. It is synthetic account creation on a disposable PDS, **not** gateway
enrollment or installation on the homelab. There are no paid calls, model/Pi
execution, public DNS changes, customer data or existing credentials.

## What is tested

- Both the unchanged default PDS Caddyfile and every bootstrap-overlay Caddyfile
  are parsed by the pinned Caddy binary. TLS processes run as the current
  non-root UID with read-only roots and all capabilities dropped. HTTPS443 uses
  only the explicit container-namespace sysctl, never a host sysctl.
- Member, publisher, operator and DID routes are probed through real TLS.
  Unknown routes and forbidden operator methods return 404. An empty trusted
  root set and a wrong TLS hostname must fail; no TLS bypass is used.
- The DID response matches the reviewed UTF-8 bytes and SHA-256 exactly,
  including pretty-printing, braces and a trailing newline. A second compact
  revision is installed by atomic replacement inside the dedicated config
  directory and explicit recreation of only the DID process. The new served
  bytes are checked independently, preventing a stale bind-mount inode claim.
- Probes run inside the actual member, gateway and operator network namespaces.
  Forbidden DNS names and every inspected IPv4 address are unreachable; own
  database or TLS destinations provide positive reachability controls.
  Members cannot reach the other member, raw PDS, publisher/operator TLS,
  gateway backend/database or DID host. The gateway cannot reach the raw PDS,
  privileged operator/DID host or member databases. The operator cannot reach
  apps, databases, gateway or publisher TLS.
- Live Docker inspection checks no published or host-network listeners, internal
  IPv4-only networks, read-only roots, non-root users and capability limits.
  This does not constrain trusted host/Docker administrators.

The ordinary routing phase deliberately uses small synthetic HTTP marker servers
in place of Memory apps, databases, the gateway and PDS. Database positive
controls are TCP reachability checks, not PostgreSQL transactions. Consequently
this gate does **not** prove application authorization, prepared database roles,
federated knowledge exchange, restart recovery or customer readiness. Those
require their separate service/database integration gates. With `--with-pds`,
only the PDS marker is replaced with the real pinned service; other upstreams
remain markers.

## Fixture hardening and cleanup

The official cached Caddy binary has a file capability. The existing
`deploy/federation/Proxy.Dockerfile` removes that xattr in a uniquely named,
fixture-only derived image so it can run with `cap_drop: ALL`. The build uses
`--network none --pull=false`, has only that Dockerfile in its build context and
does not modify the binary's content. This temporary image is not a reviewed
production release image.

Every TLS process receives only its own private key. The root signing key remains
in the protected temporary directory, never mounted in a container. The public
CA is explicitly chmod 0644: OpenSSL honors a group-writable host umask, which
the real enrollment operator correctly rejects. Private enrollment inputs remain
0600 and private state 0700. Operator/publisher runners are explicitly temporary
maintenance roles, not new production participant network grants.

The fixed synthetic handle is `space.company.example.com`. The pinned PDS
reserves the label `repository` and rejects it with `HandleNotAvailable`; the
rehearsal uses a new fixture identity rather than weakening that policy or
changing an existing operator journal after account-creation intent.

SIGINT/SIGTERM abort active commands and enter cleanup. The runner verifies its
protected project marker before removing only its exact Compose resources and
unique fixture image, then verifies no labeled containers/networks remain before
removing its protected temporary directory. No global prune, unrelated container
stop, existing volume or service mutation is used. On incomplete cleanup, the
exact random project and retained temporary root are reported for operator
inspection. Do not publish that directory: it contains synthetic private keys.
SIGKILL or a host crash cannot guarantee cleanup; use the printed project name
and its matching protected marker to inspect and remove only those resources.

Runtime failures print fixed stage/error classifications, not arbitrary response
bodies, expanded Compose secrets or container logs. Historical trial failures
are not passing evidence. See the [2026-09-19 evidence](evidence-2026-09-19.md)
for the definitive run and its limits.
