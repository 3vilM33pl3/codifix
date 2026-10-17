# Pinned PDS dependency inputs

The original `package.json` and `pnpm-lock.yaml` were copied byte-for-byte from `/app` in
`ghcr.io/bluesky-social/pds@sha256:d95725b24dbe53af9d91dc69750556931ebed6c396f2cfa42b221434db642f12`.
The image records distro 0.4.5027 and source
`374cf1d4ba782d4391bbb73e4e2d3f320d4846d6`; its PDS package is 0.5.27.
This is the existing baseline, not a protocol upgrade or a Windows binary.

Original, pre-security-overlay SHA-256 (provenance, not current build inputs):

- package.json: `c0e7740808e0d6bc26a9fd773162f780a45665b0cff2137a256157f3cff77fd5`
- pnpm-lock.yaml: `23a8c97e01dff561266a6dcf67a4b74b25e98ebc695e6fa17c60ddf964160f73`

## Security overlay 2026-10-17

The current inputs deliberately differ from the original image. PDS remains
**0.5.27**, with the same ATProto source/protocol baseline; this does not upgrade
the deployed Linux OCI image or any existing prepared Windows runtime. Build a
new immutable runtime and repeat native acceptance before deploying it. Old
prepared receipts fail the new input hashes instead of being relabeled secure.

Pinned pnpm 10.34.1 overrides update vulnerable HTTP, SMTP, MIME, telemetry and
template dependencies. `file-type` changes from the old CommonJS `fromStream`
export to the supported ESM `fileTypeFromStream` API through the small PDS patch.
Nodemailer 10.0.9 retains the PDS transport/template API, tested with an in-memory
mail transport. OpenTelemetry's auto-instrumentation bundle is updated coherently
so unused SQL instrumentations do not retain the username-disclosure advisory.
Undici aliases are pinned individually as well as its ordinary dependencies.

Elliptic has no patched release for GHSA-848j-6mx2-7j84. It is **removed**, not
ignored: `key-encoder` 2.0.3's only curve operation is public-key derivation from a
raw private scalar. A pnpm patch uses Node/OpenSSL `createECDH('secp256k1')` for
that operation and validates raw public points with `ECDH.convertKey` before
ASN.1 encoding. Existing ASN.1 formats and compressed/uncompressed public-key
representations remain unchanged. Malformed scalars/points fail closed, avoiding
invalid points reaching JOSE/native crypto. PDS JWT verification and AWS KMS
DER-to-raw conversion retain the existing `key-encoder` API. Repository signing
algorithms and wire formats are not changed.

Current build inputs:

- package.json: `02d55b497d78fcbc17516c377a243bce7f703c2fc4f6fe2d642be4b6923a986d`
- pnpm-lock.yaml: `c1e11d5d91e43aebe3e52e61aea198dbc2b3009b474a89ad9715f03a36de2621`
- patches/@atproto__pds@0.5.27.patch: `a7822b2ede85720e3591115354d3448a89c44e9acb6d740eaf3470779bc8c1b4`
- patches/key-encoder@2.0.3.patch: `fde72177f9e69d1f45ecef4600b856f3734727966c8e89663947b8cf7100bf71`

Observed on Linux with a clean frozen production dependency installation:

```sh
cd packaging/windows-pds/upstream
npx --yes pnpm@10.34.1 install --prod --frozen-lockfile --ignore-scripts --config.node-linker=hoisted
npx --yes pnpm@10.34.1 rebuild better-sqlite3 --config.node-linker=hoisted
npx --yes pnpm@10.34.1 audit --audit-level=low
cd ../../..
node --test packaging/windows-pds/security.test.mjs
```

Audit: zero vulnerabilities (original lock reported 74 affected paths).
Five tests passed: actual elliptic removal/pin, public/private byte round trips,
compressed/uncompressed signed/JWT verification and malformed-key rejection,
MIME stream parsing, PDS email templates, and real disposable PDS database
initialization with private HTTP describeServer/unauthorized-write requests.
`python3 -m unittest scripts/windows/test_build_tools.py`: 24 passed.
These are dependency/API checks, **not** a new native Windows full-fleet or
signed-repository acceptance pass. No existing installation was changed.

The upstream package declares MIT. Dependencies retain their individual package
licences; native bundling must include their notices. The lock includes exact
package integrity values. Native proof uses the overlaid lock and patches with a hoisted layout
(no runtime junction dependency), skips lifecycle scripts globally, then builds
only the upstream-approved better-sqlite3 native component. Its observed DLL
and complete runtime tree hashes are recorded separately after the Windows build.
Neither vendoring these files nor Linux adapter tests proves Windows execution.
