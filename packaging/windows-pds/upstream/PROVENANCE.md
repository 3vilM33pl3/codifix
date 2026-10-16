# Pinned PDS dependency inputs

`package.json` and `pnpm-lock.yaml` are copied byte-for-byte from `/app` in
`ghcr.io/bluesky-social/pds@sha256:d95725b24dbe53af9d91dc69750556931ebed6c396f2cfa42b221434db642f12`.
The image records distro 0.4.5027 and source
`374cf1d4ba782d4391bbb73e4e2d3f320d4846d6`; its PDS package is 0.5.27.
This is the existing baseline, not a protocol upgrade or a Windows binary.

SHA-256:

- package.json: `c0e7740808e0d6bc26a9fd773162f780a45665b0cff2137a256157f3cff77fd5`
- pnpm-lock.yaml: `23a8c97e01dff561266a6dcf67a4b74b25e98ebc695e6fa17c60ddf964160f73`

The upstream package declares MIT. Dependencies retain their individual package
licences; native bundling must include their notices. The lock includes exact
package integrity values. Native proof uses the same lock with a hoisted layout
(no runtime junction dependency), skips lifecycle scripts globally, then builds
only the upstream-approved better-sqlite3 native component. Its observed DLL
and complete runtime tree hashes are recorded separately after the Windows build.
Neither vendoring these files nor Linux adapter tests proves Windows execution.
