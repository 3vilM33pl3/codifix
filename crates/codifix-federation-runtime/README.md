# Private gateway credential maintenance

The gateway can keep its original `token_file` configuration, or opt into
`rotating_session_file` plus optional `maintenance_status_file`. They are mutually
exclusive. Static token expiry remains **unknown**, not reported as healthy.

The rotating operator writes a strict access-only JSON bundle:

```json
{
  "schema_version": 1,
  "repository": "did:web:private.example.test",
  "pds": "https://private.example.test",
  "generation": 1,
  "access_token": "operator-issued-access-JWT",
  "issued_at": 1800000000,
  "expires_at": 1800007200
}
```

Times are Unix **seconds**. The endpoint and repository must exactly match the
gateway's enrolled configuration. The bundle contains no refresh token, password
or bootstrap credential. JWT decoding checks bindings and expiry; it does not
verify cryptography. The fixed authenticated PDS response and trusted refresh
operator establish credential authority.

Mount the **containing directory**, not an individual bind-mounted file, so an
atomic rename becomes visible. Files must be regular, single-link `0600`, owned
by the gateway UID, in an owned `0700` directory. Symlink paths and unsafe
ancestors are rejected. Share this access-only directory, never the enrollment
state directory. See the [operator](../../deploy/federation/pds-enrollment/README.md).

Every publication pins one immutable credential snapshot before claiming work.
Missing, malformed or expired credentials prevent claims. HTTP 401 creates a
durable `authentication:<generation>` failure. Only a usable, strictly newer
generation automatically resumes it. HTTP 403 is `authorization_denied` and
requires intervention. Conflict, invalid proof and exhausted transient retries
are never reset by credential maintenance. Historic `invalid_receipt` failures
remain blocked even if an old gateway might have misclassified authentication.
No applied migration is changed by this feature.

`GET /v1/status?nonce=<uuid>` requires an enrolled instance token and current
`Pull` membership. Replies use the enrolled receipt key, a distinct signature
purpose, audience/space/authority binding and a 30-second expiry. Use
`GatewayClient::status(instance)` or `InstanceSync::gateway_status()` to validate
them. This endpoint is not a public health check and returns no content, file
paths, tokens or provider errors. Local project authorization remains mandatory.

The optional operator status sidecar supplies last success, retry, intervention
and observed TLS-leaf expiry. More than 180 seconds without an observation becomes
unknown. `tls_expires_at` means the **last observed PDS operator-edge leaf**, not
the issuer or gateway certificate. `publication_usable` only means credentials
are locally eligible for an attempt; it is not a liveness or publication-success
claim. The operator does not renew certificates or extend sharing grants.

Tests in `credential_recovery.rs` use fake PDS responses and signed synthetic CAR
fixtures; their database test requires the normal disposable PostgreSQL binding.
