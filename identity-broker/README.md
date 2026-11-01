# Codifix identity broker

This optional Linux sidecar verifies personal AT Protocol identities and reads
public Codifix capability records. Private Inbox conversations do not require it.
It uses `@atproto/oauth-client-node` **0.5.13** and only the `atproto` identity scope.
No Bluesky posting, messaging, repository writes or task grants are requested.

## Install and configure

Use Node 22 or newer, a dedicated unprivileged `codifix-identity` account, and a
shared group which the home Codifix service can access. Install from the lockfile:

```sh
npm ci --ignore-scripts --prefix identity-broker
```

Provide these environment variables to the broker:

```text
CODIFIX_IDENTITY_PUBLIC_ORIGIN=https://your-codifix-service.example.org
CODIFIX_IDENTITY_SOCKET=/run/codifix-identity/broker.sock
```

The socket directory must already exist, belong to the broker user, and deny
group/world writes (for example `0750`). The socket is created as `0660`. Run:

```sh
node identity-broker/src/main.mjs
```

The home service connects through `codifix_identity::BrokerClient`; it should
forward only these two public HTTPS routes to the matching socket methods:

- `GET /v1/network/oauth/client-metadata.json` -> `metadata()`.
- `GET /v1/network/oauth/callback?...` -> `callback(raw_query)`.

The metadata must be reachable by the authorization server. The callback must be
reachable in the user's system browser. Keep the `/internal/*` socket API off
public HTTP routing. Do not log callback query strings at the service, proxy or
broker. The process logs neither request URLs nor OAuth credentials.

The service creates a link attempt before `begin`, binding its ID, enabled human,
home instance and SHA-256 Codifix session binding. It checks that binding again
on every poll, commits a unique local DID link, then calls `acknowledge`. Completed
results are idempotent until acknowledged. Acknowledgement clears the result;
identity unlinking is handled by the home service and preserves conversations.

## Limits and failure behavior

- Attempts, SDK state and temporary sessions expire after ten minutes. They live
  only in memory; a broker restart requires restarting the linking flow. Results
  contain identity metadata and cleanup status, never access/refresh tokens.
- Successful verification attempts remote token revocation, clears SDK caches and
  discards all local credentials. `discarded_revocation_failed` reports an
  unconfirmed remote revocation rather than claiming success.
- All SDK and discovery HTTP traffic uses HTTPS, rejects redirects, resolves all
  addresses before sending, denies private/reserved addresses and pins that DNS
  answer to the connection. No request is made to advertised agent-card URLs.
- There are at most 32 pending attempts, eight concurrent HTTP operations,
  256 KiB remote bodies, bounded JSON depth/collections and fixed network timeouts.
  Discovery reads at most 100 capability records per requested account; it does
  not enumerate a public feed or imply enrollment.
- Public-only transport intentionally excludes private PDS deployments. Private
  enrolled specialist conversations use the separate agency transport.

## Validation and dependency finding

Run `npm test --prefix identity-broker` and `cargo test -p codifix-identity`.
Fixtures exercise the actual pinned SDK's PAR, PKCE and DPoP exchange, issuer/PDS
mismatch rejection, callback replay, bindings, cleanup, DNS pinning, disclosure
provenance and the protected Unix listener. No real account login occurs.

At implementation, `npm audit` reports Zod advisory
[GHSA-mrhx-c7xc-m3w5](https://github.com/advisories/GHSA-mrhx-c7xc-m3w5) through
16 dependency entries. The pinned official SDK uses Zod 3; blindly overriding it
with Zod 4 changes its API contract. The broker bounds response bytes and JSON
collection/depth/total-node counts before SDK validation, plus request concurrency,
to constrain the reported collection-memory exhaustion behavior. This is a local
mitigation, not a claim that the upstream audit finding is resolved. Keep the
sidecar isolated and re-evaluate the pinned SDK before production activation.
