# Integration review and provenance exceptions

Astra reviewed core/runtime, native/web, packaging and the one-release macOS
transfer on 17 October 2026. Findings addressed in source:

- Candidate Debian filename, package control version, release inventory and
  install selector consistently use `3.0.0~rc.1`; source version is `3.0.0-rc.1`.
- RC publication is prerelease and never latest; stable publication is separate.
- Windows MSI allocation is RC build 1, stable build 2; unallocated versions fail.
- Active UI guidance uses the `codifix` command and default project slug.
- Windows service/task IDs, web asset roots and current skill discovery use the
  same names as the packages.
- Broken historical source links point to their exact pre-rename Git object;
  historic assertions and original signed bytes remain unchanged.

The pinned third-party Pi adapter is an explicit provenance exception:
its `memory-windows-pipe` helper, reviewed upstream patches/lock hashes,
`MEMORY_BROKER_HANDLE` and `memory-broker.invalid` protocol literals retain their
original bytes. Renaming one side would break the pinned integration. The
Codifix-owned wrapper paths and source-digest manifest are refreshed; this is
not an old CLI alias or dual federation namespace.

Integration validation: release asset tests 5 passed, including RC ordering and
source/digest rejection; Windows source-contract tests 71 passed with 3 native
platform skips; Windows Pi tests 3 passed; Debian lifecycle and isolated package
staging passed. Native installation, complete federation review, full Rust
integration and live cutover remain separate gates. `macos-state --inspect`
does not verify secret readability or destination credential equality.
