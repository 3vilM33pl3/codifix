# Optional company PDS bootstrap boundary

`company-bootstrap-topology.mjs` extends the
[company blueprint](company-topology.md) with **operator prerequisites only**.
It neither starts containers nor provisions an account, invite, identity, key,
membership or collection. The ordinary generator and fixture remain unchanged.
This is not authorization to activate the four-area deployment.

The overlay adds two non-root services and three internal, unpublished networks:

- `pds_operator_tls` joins only `pds_operator_ingress` and
  `pds_operator_upstream`. The latter contains the PDS, not members or databases.
- `repository_did` joins only `repository_identity` and the operator ingress.
  The PDS joins `repository_identity` to resolve the explicitly pinned DID.
- An independently approved maintenance runner may join only the operator ingress.
  It can reach the two TLS services, not raw PDS, member apps, gateway or databases.
  The generator does not create this runner or give it credentials.

Neither new service joins a participant, publisher-client, gateway or database
network. Existing participant and gateway allowlists are not broadened. All image
digests, non-root users, no-new-privileges, read-only roots and PID limits come from
the reviewed company settings. No public port, model/provider/Pi egress, host
Docker socket, general proxy or enrollment authority is introduced.

## Exact operator routes

The explicit operator hostname serves HTTPS on container port 8443. Its only
upstream is `pds:2583`. All other paths or methods receive 404 before forwarding.

| Method | XRPC path suffix under `/xrpc/` |
| --- | --- |
| POST | `com.atproto.server.createInviteCode` |
| POST | `com.atproto.server.createAccount` |
| GET | `com.atproto.identity.getRecommendedDidCredentials` |
| POST | `com.atproto.server.activateAccount` |
| POST | `com.atproto.server.createSession` |
| GET | `com.atproto.server.getSession` |
| POST | `com.atproto.server.refreshSession` |
| GET | `com.atproto.sync.getRepo` |

Route availability is not permission: normal upstream authentication still applies.
Invite creation is reserved for a separately authenticated administrator. The
bounded enrollment operator consumes a pre-issued single-use invite rather than
holding the administrator password. `getRepo` permits independent operator
verification; it is not exposed to members. Upload, deletion, repository import,
delegation, arbitrary XRPC and firehose routes are absent.

## Port 443 is deliberate

The inspected pinned `@atproto/pds` **0.5.27** derives its public service URL as
`https://${PDS_HOSTNAME}` outside localhost. There is no `PDS_PUBLIC_URL` override
in that configuration path. Consequently this optional overlay changes the
publisher TLS listener from 8443 to **443** and returns the publisher origin
`https://pds.<company-domain>`. It retains the same three publisher routes,
networks, image and credential scope. The base generator still returns 8443.

Source evidence: the official [0.5.27 package metadata](https://registry.npmjs.org/@atproto/pds/0.5.27)
and [package archive](https://registry.npmjs.org/@atproto/pds/-/pds-0.5.27.tgz),
`dist/config/config.js` (`envToCfg`) and
`dist/api/com/atproto/identity/getRecommendedDidCredentials.js`. The inspected
archive integrity is
`sha512-gCk3vVCMDF62Q8Sh4NG7CAt/QhaWWJT2HiTjdqRfF/kjruKLJaqAcMg4D8kaenAQT8oqqLaGC2dbAFmjKU18qw==`.

Configure the gateway and enrollment operator using the overlay's returned
publisher origin. The operator's separate origin remains
`https://<operator-hostname>:8443`. The DID service endpoint must name the publisher
origin, not the privileged operator origin. Reverify these assumptions for every
changed PDS image digest; do not patch upstream or disable validation on mismatch.

Both the publisher TLS proxy and DID host explicitly set the container-network
namespace sysctl `net.ipv4.ip_unprivileged_port_start=443`. This allows their
configured non-root UID to bind 443 without root or `NET_BIND_SERVICE`. No host
sysctl or port publication is changed. If the target engine prohibits that
namespace setting, stop and review the image/engine contract; do not silently
run either process as root. IPv6 remains disabled.

## Inputs and generated artifacts

```js
import { companyBootstrapTopology } from './deploy/federation/company-bootstrap-topology.mjs';

const result = companyBootstrapTopology(reviewedCompanySettings, {
  schemaVersion: 1,
  operatorHostname: 'operator.memory.company.internal',
  repositoryHostname: 'space.memory.company.internal',
  repositoryDid: 'did:web:space.memory.company.internal',
  accountHandle: 'space.memory.company.internal',
  didDocument: reviewedPublicDocumentBytes,
  didDocumentSha256: reviewedPublicDocumentDigest,
  caCertificateSha256: reviewedCaFileDigest,
});
```

The example names are not enrolled identities. Supply separately reviewed real
values. The account handle must equal the repository hostname and be beneath
the configured company domain; the overlay sets `PDS_SERVICE_HANDLE_DOMAINS` to
that company suffix. Repository, operator, member and gateway origins cannot
collide. `repositoryDid` must explicitly equal `did:web:<repositoryHostname>`.
The PDS also applies its own reserved-handle policy: for pinned 0.5.27,
`repository` is reserved, while the disposable rehearsal uses `space`. The
generator does not replace the upstream account-policy validation.

`didDocument` is the exact UTF-8 JSON string, at most 16 KiB, with its matching
SHA-256. Compact JSON, pretty JSON and a trailing newline retain their exact
bytes. The accepted public shape is the existing private BYOD shape: the W3C DID
context, matching `id` and `alsoKnownAs`, one `Multikey` verification method with
matching controller and `#atproto`, and one `AtprotoPersonalDataServer` service.
Unknown/private-key fields, duplicate keys, string-escape ambiguity and Caddy
injection are rejected. This checks document shape, not key possession or account
ownership; those remain the operator's cryptographic/authentication gates.

The DID host serves only `GET /.well-known/did.json` for its explicit hostname,
over HTTPS443, as `application/did+json` with `Cache-Control: no-store`. The reviewed
bytes are embedded as a static response, not read from a mutable file server.
Backtick token quoting preserves bytes; opening braces are escaped against Caddy
placeholder expansion. See the official [quoting and placeholder rules](https://caddyserver.com/docs/caddyfile/concepts)
and [static response directive](https://caddyserver.com/docs/caddyfile/directives/respond).
Other paths and methods return 404. No access-log body capture is configured.

The new Caddyfiles appear under `proxyFiles` at:

- `bootstrap/operator/config/Caddyfile`
- `bootstrap/repository/config/Caddyfile`

Each service gets only its own generated-config directory mounted read-only at
`/config`, plus its own certificate/key mounted at `/tls`. The dedicated directory
must contain only generated public configuration, never private operator state,
bootstrap signing keys, account credentials or arbitrary host files. TLS private
keys remain 0600, scoped to their respective TLS process. Container/host
administrators remain trusted.

The PDS receives only the public `tls/ca.pem` at
`/run/trust/repository-ca.pem`, selected by `NODE_EXTRA_CA_CERTS`. The required-file
inventory records the supplied SHA-256 pin. The pure generator cannot inspect
the host file: verify its bytes against that pin before installation. Node's
additional CA setting extends its trust roots; it is not exclusive certificate
pinning or an SSRF bypass. The private networks, fixed hostnames and unchanged
`PDS_DEV_MODE=false` / `PDS_DISABLE_SSRF_PROTECTION=false` remain essential.

## Explicit bootstrap-to-handoff procedure

1. Review the operator's public bootstrap document and digest. Preserve the
   operator's protected private state separately; no private state directory is
   mounted into either TLS service. Render the overlay with those exact public
   bytes, approved image digests and CA hash. Review the output and TLS hostnames.
2. During a separately approved disposable rehearsal, install only the generated
   configuration into the dedicated config directories and provision each TLS
   service's certificate/key. Validate the rendered Caddyfiles with the pinned
   image, verify file ownership, and demonstrate the namespace port binding.
   Nothing in this module performs those steps automatically.
3. Verify the served bootstrap document's exact bytes/hash over correctly
   validated TLS before authorizing account creation. The operator must reject
   stale documents, changed identities, endpoint mismatches and ambiguous retries.
4. After obtaining the PDS-recommended public key, review the operator's handoff
   document and new digest. Regenerate with those exact bytes. Atomically replace
   the Caddyfile **inside the same dedicated host config directory**, then
   explicitly recreate the DID-host process to load it. A directory bind avoids
   the stale-inode problem of replacing an individually bind-mounted file; Caddy
   does not automatically reload it because its administration API is disabled.
   Replacing the host directory itself requires recreating the container too.
5. Read the live DID endpoint and verify the new exact bytes/hash before account
   activation. Recheck the independently expected recommended signing key and
   publisher endpoint. Do not activate on the assumption that a filesystem write
   changed the running response. Preserve the reviewed previous artifact for
   controlled recovery, not automatic key rollback after activation.
6. Keep the DID host and its persistent, backed-up public configuration available
   after bootstrap. It is an identity prerequisite, not a temporary provisioning
   fixture. TLS/CA rotation requires an explicit reviewed update and recreation
   of affected processes; the PDS reads `NODE_EXTRA_CA_CERTS` at startup. Never
   replace certificate verification with a broad bypass to make recovery work.

The initial static document and its later handoff are separate reviewable
revisions. The module generates neither; it validates and embeds the supplied
one. Account provisioning, signed repository verification, application enrollment,
knowledge review, release readiness and live gateway activation remain separate
work with their own gates.

## Offline checks and remaining evidence

```sh
node --test deploy/federation/company-topology.test.mjs deploy/federation/company-bootstrap-topology.test.mjs
```

Tests cover unchanged default output, exact routes/methods, network non-adjacency,
scoped credentials, 443 sysctls, CA inventory pinning, public-document byte/hash
round-trip and rejection of malicious/ambiguous input. These are graph and
generation tests. The separate [disposable runtime rehearsal](../../tests/company-topology/README.md)
checks the actual pinned parser's served bytes, TLS rejection, IPv4 firewall
denials, non-root startup and optional real PDS bootstrap/handoff/activation.
Re-run that gate for the reviewed target engine/images before deployment; it
does not prove Memory application authorization or activate existing services.
