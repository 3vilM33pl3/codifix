# Private PDS enrollment operator — explicit provisioning, not deployment

This bounded operator replaces the **account-provisioning portion** of the
disposable PDS probe with durable local state. It creates no Docker resources,
DNS, TLS certificates, keys, passwords, invitations, memberships or model calls.
It never starts a gateway. No invocation reports `gateway_ready: true` or
`repository_verified: true`.

All verification in this slice uses temporary files and injected fake PDS
responses. The pinned-image SDK adapter and a real company PDS must still pass
their deployment rehearsal. Do not describe the fake tests as a live enrollment.

## Boundaries and prerequisites

- Use the existing image pin in [upstream-lock.json](../../../fixtures/federation/upstream-lock.json):
  `ghcr.io/bluesky-social/pds@sha256:d95725b24dbe53af9d91dc69750556931ebed6c396f2cfa42b221434db642f12`.
  Execution loads only that image's existing `@atproto/pds` 0.5.27 dependencies:
  `@atproto/crypto` and `@atproto/xrpc-server`. No npm installation or latest-version
  fallback exists. Selecting/verifying the actual OCI image is the deployment
  operator's responsibility; a JSON image field is not runtime attestation.
- Supply an existing, separately reviewed K256 bootstrap private key and its
  public `did:key`, account password, and **pre-issued single-use invite** in
  separate `0600` files. This tool intentionally receives no administrator
  password. Invite issuance is a separate administrator action; an ambiguous
  invite response must not trigger automatic creation of more invitations.
- Supply an existing private `0700` state directory. It holds the journal,
  account sessions and prepared public DID document. Back it up securely alongside
  the PDS data and required credentials. Never mount the whole directory into the
  gateway or participants; it contains much more than a publication token.
- Use distinct fixed origins: the repository's `did:web` hostname on HTTPS443,
  the normal private PDS publisher origin, and a separately isolated operator-only
  XRPC origin. No member may reach operator routes or raw PDS contents. Use the
  [company bootstrap topology](../company-bootstrap-topology.md), not the
  disposable fixture network.
- The account handle is exactly the repository hostname. The PDS must be able to
  resolve its HTTPS DID document and trust its certificate. Configure the proper
  service handle domain. Keep development mode and SSRF bypass disabled.
- Keep the PDS repository signing key, initial bootstrap key and gateway receipt
  key as different trust responsibilities. This operator neither obtains the
  PDS-owned private signing key nor generates a gateway receipt key.

The upstream PDS computes its public service URL as `https://${PDS_HOSTNAME}`
outside localhost. Consequently, the recommended DID endpoint uses HTTPS443,
not the original blueprint's publisher port 8443. The operator-only edge may use
8443. There is no assumed `PDS_PUBLIC_URL` override.

## Private configuration

Use an absolute, regular, non-symlink JSON file with mode `0600`. Unknown fields,
duplicate keys, credential-bearing URLs, HTTP, URL paths, input/output aliases,
malformed identities and review windows longer than 24 hours are rejected.
These sample values are **not usable credentials or deployment identities**:

```json
{
  "schema_version": 1,
  "image": "ghcr.io/bluesky-social/pds@sha256:d95725b24dbe53af9d91dc69750556931ebed6c396f2cfa42b221434db642f12",
  "operation_id": "fdb89f7c-1bdb-4d70-9722-54fca59262f1",
  "repository": "did:web:space.company.example",
  "handle": "space.company.example",
  "email": "operator@company.example",
  "pds_origin": "https://pds.company.example",
  "operator_origin": "https://pds-operator.company.example:8443",
  "service_did": "did:web:pds.company.example",
  "tls_ca_file": "/private/company/ca.pem",
  "tls_ca_sha256": "REPLACE_WITH_EXACT_CA_FILE_SHA256",
  "bootstrap_public_key": "REPLACE_WITH_EXISTING_BOOTSTRAP_DID_KEY",
  "bootstrap_key_file": "/private/company/bootstrap-key.hex",
  "password_file": "/private/company/account-password",
  "invite_file": "/private/company/single-use-invite",
  "state_directory": "/private/company/enrollment-state",
  "review_not_before": "2026-09-19T15:00:00Z",
  "review_expires_at": "2026-09-19T16:00:00Z"
}
```

The bootstrap key file contains exactly 64 lowercase hexadecimal characters.
The password is at least 32 characters; invitation codes at least eight.
The CA file is SHA-256 pinned and must be a regular file. Its public certificate
may be mode `0644`; secret files must be `0600`.

The identity/config binding excludes only the two review timestamps. This lets an
operator renew an expired **review window**, with a new digest, without changing
the operation, identity, paths or endpoint. It does not renew credentials or
silently extend a previous authorization. Preserve all other fields unchanged.

## Commands and phases

An ordinary local Node invocation can produce the offline review:

```sh
node deploy/federation/pds-enrollment/cli.mjs /private/company/config.json
node deploy/federation/pds-enrollment/cli.mjs /private/company/config.json bootstrap
```

No phase argument selects the next available phase. The default only reads config
and metadata; it does not read secret values, load the SDK, write files or call a
network. Its output contains exact public DID data, intended operations and
`review_sha256`, but no password, token, email, local path or provider body.

Execute only inside the reviewed one-shot operator environment using the pinned
PDS image and its embedded SDK, with the module mounted read-only and only the
explicit private inputs/state mounted. Run as the owner of those files. A typical
**in-container command**, not a container creation command, is:

```sh
node /operator/cli.mjs execute /private/company/config.json bootstrap EXACT_REVIEW_SHA256
```

Repeat preview and explicit digest confirmation for each phase:

| Phase | Effect and required next step |
| --- | --- |
| `bootstrap` | Verify the supplied private/public key pair; persist `did.json` and initial journal. No network. |
| `create` | Verify the served bootstrap DID document; persist create intent; sign the fixed account-creation service JWT; call `createAccount`; persist session before advancing the journal. |
| `discover-key` | Verify current session identity; fetch recommended public key, handle and PDS endpoint. Persist candidate key for a separate review. |
| `handoff` | After reviewing the candidate in the preview, replace local `did.json` with that exact public key. No network or remote DID-host change. |
| `activate` | Verify the newly served document, authenticate the exact account, record intent, activate, then confirm active status. |
| `refresh` | Record intent; rotate through `refreshSession`; atomically store both returned tokens before advancing the journal. No gateway restart. |
| `reconcile` | Resolve a pending create/activate/refresh without manufacturing a new identity or blindly repeating account creation. |

After `bootstrap` and again after `handoff`, separately install the exact reviewed
`did.json` bytes into the DID host's pinned configuration and recreate only that
service. The review/result's `did_document_sha256` hashes the actual canonical
JSON plus trailing newline. The topology generator also accepts those bytes.
This operator never changes DNS, TLS or the serving configuration automatically.
It refuses create/activation until the served document matches the exact reviewed
fields; imported endpoint URLs never become transport destinations.

Account creation uses the service JWT issuer equal to the repository DID,
audience equal to the configured PDS service DID and method equal to
`com.atproto.server.createAccount`. The PDS-generated recommended key must differ
from the bootstrap key and be a valid K256 key. Every returned account/session is
checked against the original DID and handle; token claims must contain the exact
audience, subject and expected access/refresh scope. JWT claim decoding is used
for expiry scheduling, **not** claimed to verify a JWT signature: these session
values are received through the fixed authenticated TLS PDS connection.

## Interruptions, reconciliation and operational limits

The journal and session are separate atomic, fsynced files. The output-first,
journal-second protocol explicitly repairs a crash between those writes; it is
not a claim of a filesystem transaction or rollback of remote effects. A lock
prevents two local operators from running simultaneously. A process kill can
leave `operator.lock`; verify the recorded PID is no longer the owning process
and that no operator is active before an administrator removes **that exact
lock file**. Never delete the journal to force a retry.

- A lost `createAccount` reply leaves `create_pending`. The next reviewed
  `reconcile` calls `createSession` with the exact DID and original password.
  Changed original key/password/invitation inputs are rejected. It never tries a
  second DID, new key or new account. If the account was not created or login
  cannot establish its identity, stop for operator investigation; the tool does
  not assume that a failed login proves account absence.
- A lost activation reply is resolved by current session status. If the account
  is still inactive, the journal returns to `handoff` for a newly reviewed
  activation; reconciliation itself never repeats activation.
- If refreshed credentials were already durably saved, reconciliation completes
  the journal without refreshing again. Otherwise it explicitly retries the
  retained refresh token only within the pinned PDS's two-hour grace window.
  Older ambiguous attempts stop for manual recovery, even if the JWT's original
  expiry is later.
- Expired access credentials cannot perform authenticated read/activation work.
  Use an independently reviewed `refresh` while the refresh credential remains
  valid. Expired or revoked refresh credentials require separately reviewed
  account-login recovery; this bounded version does not add an automatic
  password-login fallback. During pending activation, explicit refresh is allowed
  and returns to activation reconciliation afterward.
- `session.json` stores both credentials privately, with their expiry times.
  Deployment must separately install only the selected access token into the
  gateway's existing private token file, using an atomic operator-controlled
  replacement. Do not print it, expose the refresh token to the gateway, or mount
  the entire enrollment state. The gateway currently reads its token only at
  startup: **gateway restart and blocked-publication retry remain manual**.
- Exact completed phase retries are refused as phase mismatches; inspect the
  next offline review to see committed state. A new state/config binding cannot
  silently reuse the old directory. The tool does not erase unowned prior output.
- A network request has a 30-second deadline and a 128-KiB response limit. Errors
  are fixed codes, not upstream bodies or stack traces. A timeout can leave a real
  remote effect; no response says it was rolled back.
- `active` means account activation was confirmed, not repository publication,
  gateway access controls, disaster recovery, knowledge consent or readiness for
  participants. Independent signed-CAR verification and the reviewed
  [authority enrollment](../../../docs/user/federation-enrollment.md), local
  grants and collections remain separate gates.

## Verification and pinned source evidence

Run without Docker, credentials, paid providers or network services:

```sh
node --test deploy/federation/pds-enrollment/operator.test.mjs
```

The suite injects transport, signer, clock and write interruptions. It covers
strict/dry inputs, exact confirmation, changed retry, wrong DID/audience/endpoint,
key-handoff gating, interrupted creation/activation, partial credential writes,
refresh reuse/rotation, expiry and grace windows, locks, secret-free CLI refusal
and preservation of unowned outputs. It does not test actual PDS cryptography or
live provisioning; existing PDS regression gates still apply.

The published [PDS 0.5.27 package metadata](https://registry.npmjs.org/@atproto/pds/0.5.27)
identifies the [versioned source archive](https://registry.npmjs.org/@atproto/pds/-/pds-0.5.27.tgz).
Its bytes were checked against SHA-512 integrity
`gCk3vVCMDF62Q8Sh4NG7CAt/QhaWWJT2HiTjdqRfF/kjruKLJaqAcMg4D8kaenAQT8oqqLaGC2dbAFmjKU18qw==`.
Relevant files in that archive:

- `dist/config/config.js`: fixed HTTPS public origin and default service DID.
- `dist/api/com/atproto/server/createAccount.js`: BYOD service authentication,
  PDS-owned signing key and initially deactivated account.
- `dist/api/com/atproto/identity/getRecommendedDidCredentials.js`: public key,
  handle and configured endpoint handoff.
- `dist/account-manager/helpers/auth.js`: 120-minute access and 90-day refresh
  default lifetimes.
- `dist/account-manager/account-manager.js`: refresh rotation, reuse identity
  and two-hour previous-token grace period.

These defaults do not extend a particular token: the operator respects the
returned token's actual expiry. Dependency package inspection is not evidence
that a running deployment has the correct image, clock, credential custody or TLS.
