# Company topology: Workshop and Homelab

`company-topology.mjs` is a **production-intended, default-deny blueprint**, not a
deployed or runtime-verified installation. It generates a Compose document,
fixed-upstream Caddyfiles, a required-file inventory and private endpoint names.
It performs no I/O, starts nothing and creates no credentials, identities,
memberships, publication reviews or collection consent. The existing
`topology.mjs` isolation fixture is unchanged; its passing tests do not establish
that this new installation has passed a deployment rehearsal.

The initial members are `workshop` (Memory development) and `homelab`. Each has a
distinct application, database, data volume and TLS edge. The gateway has its own
database and reaches the PDS only through a separate fixed-upstream TLS proxy.
No participant shares another participant's application or database network.

The separate [PDS bootstrap overlay](company-bootstrap-topology.md) adds isolated
operator routes and reviewed static DID hosting without changing these defaults.
Its pinned-PDS compatibility contract uses publisher HTTPS443 rather than 8443.

| Process | Private networks it can directly reach |
| --- | --- |
| Each member app | Its own UI edge, own database, gateway TLS edge |
| Each member UI edge | Its own app and its own unpublished ingress network |
| Gateway TLS edge | Separate member-to-gateway networks and gateway API network |
| Gateway | Gateway API, gateway database, PDS TLS-client network |
| PDS TLS proxy | PDS TLS-client network and raw PDS repository network |
| Raw PDS | Repository network only |

All networks are internal, IPv6 is disabled, and no host ports are published.
The member apps cannot directly reach raw PDS, the gateway process, its database,
or another member's database. The TLS gateway edge allows only `/v1/accept`,
`/v1/pull`, `/v1/retention` and `/v1/status`; `/v1/expertise` needs an explicit option. Status is authenticated, current-membership checked and signed, not a public health endpoint. It never
forwards arbitrary URLs, raw repository requests, CAR downloads or event streams.
The PDS proxy allows only the four XRPC operations used by the publisher:
`createRecord`, `getRecord`, `deleteRecord` and `getRepo`. The delete route is
for expired, exact-CID addressed expertise records; shared publication records
remain append-only and are never deleted by this worker.

The host, Docker administrators and the scoped proxy/gateway processes remain
trusted. A network diagram is not a firewall or confidentiality proof. Negative
access tests must be rerun against the installed engine, host firewall and any
subsequently added ingress. Keep private network separation intact.

## Generate a review artifact

Import `companyTopology` from Node 22 or newer and pass a plain object:

```js
import { companyTopology } from './deploy/federation/company-topology.mjs';

const result = companyTopology({
  schemaVersion: 1,
  name: 'memory-company',
  root: '/srv/memory-company',
  domain: 'memory.company.internal',
  // Supply actual reviewed fully qualified image@sha256:<64 hex> references.
  images: reviewedImageDigests, // exact keys: app, gateway, postgres, pds, proxy
  // Explicit image-compatible non-root "uid:gid" values for the same five keys.
  users: reviewedContainerUsers,
  members: ['workshop', 'homelab'],
  federationEnabled: false,
  expertiseRoutes: false,
});

console.log(JSON.stringify(result, null, 2));
```

The `reviewedImageDigests` and `reviewedContainerUsers` objects above are inputs to
be supplied by the operator, not shipped release identities. Tags alone, missing
digests, unknown options, root users, path traversal and unsafe names are rejected.
The `compose` field is a Compose JSON document; `proxyFiles` maps relative paths
under the dedicated root to Caddyfile contents. `requiredFiles` lists every bind
and its intended ownership scope and mode. Rendering does not write those files
or check their existence, contents, ownership or certificate validity. Bind mounts
use `create_host_path: false`; missing files must not become empty directories.

To prepare a future game member, add `'game'` to `members`; no source change is
needed. The bounded generator requires Workshop and Homelab and allows up to six
additional names. Adding a member creates only its topology and required-file
entries, never a space membership or access to another area's collection.

## Installation prerequisites — separate approved work

1. Back up and verify recovery of the current installation before changing its
   deployment. Select a new dedicated root and fresh named volumes. Do not point
   this topology at existing databases or copy a live volume without a reviewed
   migration procedure.
2. Review and pin all five image digests. The app image must contain a compatible
   full service build at `/usr/local/bin/memory` and Web assets at
   `/opt/memory/web`; the gateway binary is
   `/usr/local/bin/memory-federation-gateway`. The proxy image must contain
   `/usr/bin/caddy`. PostgreSQL must support the current migrations and pgvector.
   This module does not build, download, scan or attest these images.
3. Verify each image works with its explicit non-root UID/GID, read-only root,
   dropped capabilities and 128-process limit. Pre-provision writable volume
   ownership before starting it, including each app's `/state`, PostgreSQL's
   data directory and the PDS `/pds` directory; a fresh root-owned volume will not
   become writable automatically. In particular, verify the selected PostgreSQL image's
   non-root initialization contract. Do not restore root privileges to bypass a
   failed image prerequisite.
4. Provision the three independent databases and run migrations separately as
   their schema owner. Use distinct runtime logins; never put the schema-owner
   password in app or gateway configuration. The database container alone gets
   its own `owner-password` file. Apply the reviewed
   [gateway grants](gateway-grants.sql) and follow the existing
   [gateway installation contract](README.md). The gateway cannot migrate its
   own database. Apps are explicitly locked to `database.migrations="verify_only"`:
   they verify the prepared schema, refuse missing/dirty/mismatched migrations and
   never fall back to running DDL or an offline capture store. Offline mode is
   separately disabled. Follow the [prepared database contract](../../docs/user/prepared-database.md)
   for compatible release artifacts and owner/runtime-role separation.
5. Provision independently enrolled identities, repository signing/public receipt
   keys and instance credentials through the controlled enrollment workflow.
   Populate the separately reviewed
   [runtime configuration](../../docs/user/federation-runtime.md). Gateway config
   listens at `0.0.0.0:4055`, points to `https://pds.<domain>:8443` with
   `pds_ca_file: /run/trust/ca.pem`, and references its individual secret mounts.
   Instance configs point to `https://gateway.<domain>:8443`,
   `token_file: /run/secrets/gateway-token` and
   `gateway_ca_file: /run/trust/ca.pem`. Keep `disposable_test_mode` false or absent.
   No fixture identity, test-mode flag or consent is emitted by this module.
6. Issue and verify TLS certificates with the exact generated hostnames. Put each
   edge's private key only in its own scoped directory; the shared CA file is
   public trust material, not a CA private key. Secret files must be private 0600
   and readable by that container's UID; check ownership, symlinks and all parent
   directories separately. No automatic certificate issuance or renewal is
   installed.
7. Review member `app.toml` files individually: own database only, proper local
   authentication/authorization, `/state` for writable state, no provider secrets,
   supervisor manifests, watcher paths, external model destinations or shared
   host repositories. Service settings are overridden to listen internally on
   port 4040; `HOME` and all writable XDG directories point into the app's own
   `/state` volume, not `/root`. Ordinary models, embeddings, background jobs, consolidation,
   validation automation, cluster replication and MCP are disabled. No Pi,
   Docker socket, model credentials or broad host directory is mounted.
8. Prepare the upstream PDS environment only in `pds/pds.env` (0600). Unlike the
   other processes, the unmodified PDS consumes environment-based credentials;
   these are visible to Docker administrators. They are not passed to other
   services. The generator forces development/SSRF-bypass flags off, invitations
   on and crawlers empty. Reverify pinned PDS private-operation, provisioning,
   credential-expiry and recovery prerequisites; no general internet access or
   administrative XRPC route is opened to make a failing prerequisite pass.
9. Rehearse the installation with disposable data, checking startup, TLS, separate
   auth/database boundaries, negative raw-repository access, restart and recovery.
   Connect human access only through a separately approved private ingress/VPN on
   that member's named `*_ingress` network. Do not publish backend, database or
   PDS ports or attach the ingress to their networks. Exposing the app over a new
   origin additionally requires correct browser session/Origin policy.

`federationEnabled: true` explicitly mounts each member's own federation config
and gateway token; it does not enroll anyone. `expertiseRoutes: true` additionally
opens only the gateway expertise route and requires federation to be enabled.
Compatible `expertise_fleet`, current memberships, local permissions, exact
collection reviews and recipient/model consent remain independent requirements.
Leave provider and agent execution disabled during this integration rehearsal.
Once configured and started, gateway publication and enabled federation workers
run normally; “agents off” does not mean the gateway has no recovery worker.

Only after those checks should a separately approved release activate real data
and federation. This generator does not complete the four-area migration, install
CA or General, enable live Pi, establish customer readiness or authorize paid
calls. Revocation cannot recall bytes already delivered to another area.

## Offline validation

```sh
node --test deploy/federation/company-topology.test.mjs
```

These tests verify deterministic generation, scoped credential mounts, explicit
opt-ins, pinned images, invalid-input rejection and direct-network adjacency.
They do **not** run Docker, create secrets, deploy services or prove live firewall,
DNS, TLS, PostgreSQL/PDS image compatibility or federation behavior.
