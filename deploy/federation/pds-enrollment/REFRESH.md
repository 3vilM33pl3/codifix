# Existing-account refresh-only maintenance

This is the opt-in operational successor to manually reviewed session refresh.
It **does not extend the enrollment operator's 24-hour approval**. A separate,
exactly reviewed standing policy permits only `refreshSession` and `getSession`
for one already enrolled active account. No password, bootstrap key, invitation,
administrator credential or enrollment SDK is required or read. There are no
create-account, activate, key-discovery, identity-change or login fallback paths.

## Approval and installation

1. Back up the current deployment and enrollment state first. Stop any manual
   enrollment invocation; the maintainer must not be adopted during an unresolved
   operator lock. Preserve the existing journal, session and DID document.
2. Prepare the following strict private policy (`0600`, executing UID, regular
   file, no symlink/hardlink). Populate the enrolled identity hash and signing key
   from the existing journal, and hash the exact existing `did.json` bytes.
   These are placeholders, **not deployable identities or credentials**:

```json
{
  "schema_version": 1,
  "purpose": "memory-private-pds-refresh-only-v1",
  "policy_id": "fdb89f7c-1bdb-4d70-9722-54fca59262f1",
  "image": "ghcr.io/bluesky-social/pds@sha256:d95725b24dbe53af9d91dc69750556931ebed6c396f2cfa42b221434db642f12",
  "enrollment_identity_sha256": "REPLACE_WITH_EXISTING_JOURNAL_IDENTITY_SHA256",
  "repository": "did:web:space.company.example",
  "handle": "space.company.example",
  "pds_origin": "https://pds.company.example",
  "operator_origin": "https://operator.company.example:8443",
  "service_did": "did:web:pds.company.example",
  "signing_key": "REPLACE_WITH_EXISTING_ENROLLED_PUBLIC_DID_KEY",
  "did_document_sha256": "REPLACE_WITH_EXISTING_DID_FILE_SHA256",
  "tls_ca_file": "/var/lib/memory-pds-refresh/ca.pem",
  "tls_ca_sha256": "REPLACE_WITH_EXACT_CA_FILE_SHA256",
  "state_directory": "/var/lib/memory-pds-refresh/enrollment",
  "output_directory": "/var/lib/memory-pds-access",
  "not_before": 1789826400
}
```

Both directories must already exist, be `0700`, and belong to the executing UID.
They must not overlap or contain one another. The CA file lives outside both;
it is public trust material, not a refresh credential. The enrollment directory
contains only the preserved session/journal/DID and maintenance metadata—not
passwords, bootstrap keys or invitations. Existing journal identity hashes remain
unchanged if the directory is relocated; this policy explicitly binds the new
location and original identity. Do not manufacture a fresh enrollment journal.

3. Run the **read-only** review using ordinary Node on Linux:

```sh
node deploy/federation/pds-enrollment/refresh-cli.mjs review /private/refresh-policy.json
```

The review reads public enrollment metadata, not session credentials. Review its
identity, pinned key, operations and policy digest. Confirm exactly that review:

```sh
node deploy/federation/pds-enrollment/refresh-cli.mjs authorize /private/refresh-policy.json EXACT_REVIEW_SHA256
```

Authorization writes `refresh-owner.json` under the enrollment operator's lock,
but makes **no network call**. Every subsequent legacy enrollment execution
refuses this owned directory. A changed policy cannot silently adopt it.

4. After approval, check once and then install the reviewed service template:

```sh
node deploy/federation/pds-enrollment/refresh-cli.mjs once /private/refresh-policy.json
node deploy/federation/pds-enrollment/refresh-cli.mjs run /private/refresh-policy.json
```

The [systemd template](memory-pds-refresh.service.example) needs concrete paths,
UID and operator-edge connectivity. It is not installed automatically. Node and
Linux `/usr/bin/flock` are required; no npm packages, pinned signer or provider
key are needed. The image field binds the reviewed PDS version, not an attestation
of a running container: independently verify that image before activation.

Use a dedicated OS identity and filesystem/network restrictions. Grant access
only to the state/output directories, policy and pinned CA. The maintainer may
reach the configured operator-only HTTPS edge; it has no participant routes,
Docker socket, model service, database or general network-discovery requirement.
Do not expose operator-only XRPC to participants to make connectivity easier.

## Gateway contract

The output directory contains only atomic `0600` access and metadata snapshots.
Mount the **directory** read-only into the gateway, not an individual bind-mounted
file: replacing a host file atomically does not replace an already-bound inode.
Never mount the enrollment directory into gateways or participants.

`session.json`, bounded below 8 KiB, has exactly:

```text
schema_version: 1
repository: enrolled DID
pds: pinned canonical publisher origin
generation: positive session generation
access_token: access JWT only
issued_at: installation/refresh observation time, Unix seconds
expires_at: actual access-token expiry, Unix seconds
```

`issued_at` is operational metadata, not a cryptographically verified JWT issue
time. JWT claim decoding schedules expiry and checks subject/audience/scope; it
does not verify JWT signatures. Session authority comes from the fixed,
authenticated TLS PDS response. Independent signed-repository verification is
still mandatory for publication acknowledgement.

`status.json`, bounded below 4 KiB, has exactly:

```text
schema_version, repository, pds, observed_at, last_success, retry_at,
state, reason, generation, tls_expires_at
```

All timestamps use Unix seconds. `last_success`, `retry_at`, `reason` and
`tls_expires_at` may be null. States are `healthy`, `refreshing`, `retry_wait` and
`intervention`. `last_success` means a refresh/reconciliation completed, not
publication or application health. An existing unexpired session can be installed
without a new refresh; it does not fabricate a successful-refresh timestamp.

`tls_expires_at` is the last observed **operator-edge leaf certificate** expiry
from authenticated HTTPS, not the CA issuer expiry or an assertion about other
gateway/participant certificates. Before an observed TLS response it is unknown.
The pinned CA is separately parsed and checked for expiry. No certificates are
renewed or trust roots changed automatically. Consumers must reject stale status
as evidence of health and show unknown certificate observations honestly.

Configure the gateway's opt-in `rotating_session_file` and
`maintenance_status_file` with these mounted paths instead of `token_file`.
The gateway must validate generation/identity/expiry, hold one token snapshot per
publication attempt, and retry only authentication-blocked work after a newer
valid generation. It must not reset integrity/conflict failures. Static-token
deployments retain their existing restart/manual-recovery behavior.

## Bounded execution and interruptions

- Normal polling: 60 seconds. Refresh starts 15 minutes before actual expiry.
- Five attempts per automatic recovery cycle. Delays start at 10 seconds and
  double, capped at 10 minutes; a fifth failure becomes durable intervention.
- Credentials, policy/identity mismatch, invalid responses, TLS verification and
  invalid requests block immediately. Rate limits, transient network/server
  errors and recoverable local-persistence interruptions use bounded retry.
- The kernel lock is held on the parent's open file description. Competing
  maintainers fail before accessing a session. A killed parent releases the
  lock automatically; **do not delete `refresh.lock`**. The lock inode remains
  persistent. Legacy `operator.lock` files require investigation, never automatic
  PID-based deletion.
- Before a potentially effective request, persist both its pending enrollment
  intent and its bounded attempt/deadline. A restart waits for the old request's
  30-second deadline plus backoff before reconciliation.
- Persist the returned session before advancing the enrollment journal. Saved
  next-generation credentials are reconciled without another rotation; where
  still valid, `getSession` confirms active identity. A missing saved response
  permits reuse of the retained token only inside the pinned PDS's two-hour
  grace window. Outside that window, stop. No password fallback is available.
- Failed access-file publication reuses the saved session rather than rotating
  again, and is itself bounded. No exception claims a remote effect rolled back.
- SIGTERM stops new polls and lets an in-flight bounded request finish; the unit
  allows 40 seconds. Abnormal process death may restart after 60 seconds.
  Deliberate intervention/configuration refusals are not auto-restarted.

Private `maintenance.json` retains the current bounded attempt state; the public
status contains fixed metadata only. The ordinary timer can never reset a
terminal failure or approve a new account/policy.

## Intervention recovery

Stop the maintainer and investigate the fixed failure code, unchanged enrollment
identity, PDS health, current pinned TLS trust, directory permissions and retained
session state. Do not print tokens or upstream response bodies into tickets.
Do not delete the enrollment/maintenance journals or reset an attempt count by
hand. Ambiguous expired-grace cases require separate account-recovery planning.

After correcting a transient cause, an operator may explicitly review and open
one further bounded recovery generation for the **same** policy:

```sh
node deploy/federation/pds-enrollment/refresh-cli.mjs recovery-review /private/refresh-policy.json
node deploy/federation/pds-enrollment/refresh-cli.mjs recover /private/refresh-policy.json EXACT_REVIEW_SHA256
```

This makes no network call, writes an immutable `recovery-N.json` metadata
receipt before reopening retries, preserves the failed reason/count and waits
one minute. It cannot alter identity, credentials, trust, inputs or the original
ambiguity/grace timestamp. Replaying the same completed approval cannot open
another generation. An expired or revoked refresh token still fails closed.
Then restart the reviewed service. Any need for a new account, key, password
login, trust replacement or policy change is outside this maintenance authority.

## Verification

```sh
node --test deploy/federation/pds-enrollment/operator.test.mjs deploy/federation/pds-enrollment/refresh.test.mjs
```

Tests use temporary private directories, a synthetic certificate and fake PDS
transport only. They exercise expiry, publication, concurrent/abnormally killed
maintainers, ambiguous refreshes, saved-session and output-write interruption,
bounded retries, explicit recovery, forged identities, unsafe files and secret-
free CLI failures. They do not prove deployment activation, real PDS refresh,
customer recovery, or gateway/application readiness.
