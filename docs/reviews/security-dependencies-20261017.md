# Dependency security remediation — 17 October 2026

## Scope and result

Repository fixes only; no active Monolith, CA, Mac, VM, Linux OCI image or prepared
Windows runtime was upgraded. No paid calls, customer data or R3 changes.
Application version remains `2.1.0-rc.5`; these commits are not a new release.

The GitHub default-branch inventory at `881b55f1` contained **96 open alerts**
covering 72 unique GHSAs: 4 critical, 32 high, 46 medium and 14 low. The earlier
push summary's 19 was not the complete current inventory.

| Manifest | Initial GitHub alerts | Verified current local audit |
| --- | ---: | --- |
| `web/package-lock.json` | 1 | npm: zero vulnerabilities |
| `docs-site/package-lock.json` | 12 | npm: zero vulnerabilities |
| `packaging/windows-pds/upstream/pnpm-lock.yaml` | 77 | pnpm: zero vulnerabilities |
| `Cargo.lock` | 6 | Those dependencies updated; one additional unpatched RSA advisory remains |

Alert instances, unique advisories and npm's affected dependency paths are
different counts. GitHub must re-evaluate the pushed default branch before its
alerts can be described as closed. No alerts were dismissed and no audit
exclusions were added. This is dependency remediation, not proof that all source
code or installed software is free of vulnerabilities. GitHub code scanning
reported no analysis; secret-scanning inventory was inaccessible with the
current token's scopes. No permissions were expanded.

## Changes

- Web: `source-map-js 1.2.2`.
- Docs: Next `16.4.2`, Mermaid `12.2.0` and its compatible patched KaTeX,
  DOMPurify, sharp and source-map dependencies. Mermaid was upgraded rather than
  forcing an incompatible KaTeX version into its older major.
- Rust: rmcp `2.2.0`, rustls `0.23.46`, quinn-proto `0.11.19`, h2 `0.4.20`,
  anyhow `1.0.104`, event-listener `5.4.2` and chacha20 `0.10.2`.
  The MCP adapter uses the SDK's renamed content/resource models; the existing
  per-request authorization remains in place.
- ZIP `9.0.4` replaces the vulnerable extraction implementations. DuckDB and
  libduckdb-sys `1.10506.1` remove their older build-time ZIP dependency and the
  former optional rkyv graph. Archive name decoding now propagates errors.
- A small, licensed [Atrium common overlay](../../vendor/atrium-common/SECURITY-PATCH.md)
  updates its WASM-only LRU dependency to `0.18.2`. Upstream source is unchanged;
  native builds still use Moka. Exact Atrium API/XRPC/repository/crypto pins and
  federation formats are preserved.
- The [PDS security overlay](../../packaging/windows-pds/upstream/PROVENANCE.md)
  retains PDS `0.5.27` and records dependency overrides and two hashed source
  patches. File-type's ESM API is adapted. Key-encoder's vulnerable Elliptic
  dependency is removed, preserving key formats while using Node/OpenSSL with
  explicit secp256k1 validation. Windows preparation and packaging verify the
  changed inputs and test the actual installed runtime before issuing a receipt.
- Dependabot now covers docs and the native PDS tree. Nightly reports audit all
  three JavaScript manifests separately, including low severity, and Rust;
  failure in one report no longer prevents the others from running. Existing
  non-blocking nightly-report policy is retained, including the visible RSA
  failure.

## Unpatched dependency — not hidden or declared fixed

[`RUSTSEC-2023-0071`](https://rustsec.org/advisories/RUSTSEC-2023-0071.html)
affects RSA private-key operations; no patched release is published. The remaining
`rsa 0.9.10` dependency comes through `openidconnect 4.0.1`.

The inspected Memory OIDC path (`crates/mem-service/src/auth/oidc.rs`) performs
authorization-code/PKCE exchange and verifies ID tokens with public keys.
OpenID Connect's `verify_rsa_signature` calls `RsaPublicKey::verify`. No Memory
RSA private-key signing or decryption use was found. This is a reachability
assessment, not a fix to RSA or a cryptographic proof. `cargo audit` continues
to return a failure for the advisory; it is not suppressed. Reassess if private
RSA operations are introduced or the upstream library publishes a fix.

The lockfile also retains yanked `spin 0.9.8` through transitive dependencies;
the audit reports it as a warning, not a separate vulnerability. No fake version
or unsupported cryptographic backport was introduced to make the report green.

## Observed verification

- Web: 293 tests and production build pass; npm audit zero.
- Docs: content, links, assets and production build pass. Chromium rendered
  Mermaid SVGs on all four diagram pages without page errors; npm audit zero.
- Native PDS overlay: clean frozen production install and SQLite rebuild;
  five tests pass, including real PDS database initialization/private HTTP,
  denied writes, signed JWT verification, malformed keys, MIME detection and
  email templates. pnpm audit zero at every severity. Build-tool tests: 24
  pass; existing transport/path tests: 9 pass, 3 opt-in cases skipped.
- Lean CLI with TUI/project-sync: 387 tests pass. This includes archive
  traversal/alias protections and pinned-skill setup tests.
- Required-database federation/adapter/MCP/skills tests pass on a disposable
  loopback PostgreSQL/pgvector container. The HTTP MCP scoped-token test passes
  with a required database; it exercises allowed/denied projects, expiry and
  revocation under the upgraded SDK. Four DuckDB offline/replay tests pass.
- The patched Atrium common crate compiles for `wasm32-unknown-unknown` in an
  isolated wrapper with exact `lru 0.18.2` and Tokio `1.50.0`. A workspace-wide
  WASM check first failed because unrelated workspace Tokio networking features
  activate Mio, which does not support that target. This is not WASM product
  acceptance.
- Rust formatting, diff whitespace and boundary checks pass. Production
  library/binary Clippy passes for the affected MCP/skills/ATProto/federation
  crates and lean CLI. All-targets Clippy remains blocked by existing unrelated
  federation test-layout/unwrap/useless-as-ref findings; it is not reported as a
  pass. No full workspace or new native Windows acceptance is claimed.

## Release implications

Existing installed binaries, the pinned Linux PDS OCI image and the old Windows
runtime still contain their original dependency versions. Rebuild and test the
candidate before upgrading them. Keep old Windows runtime inputs and receipts
together; preparation rejects an old receipt paired with the new security
overlay. Native Windows acceptance must include a full blob upload and the
existing signed-repository/interoperability tests. Repository audit results do
not certify unchanged deployed artifacts.
