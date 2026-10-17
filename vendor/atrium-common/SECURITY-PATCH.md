# Atrium common security overlay

Source: `atrium-common 0.1.4`, crates.io archive SHA-256
`98466259d2e6189ddf3439b96432d5303668b4d51c2a78996934d981065550f3`.
Upstream repository: <https://github.com/atrium-rs/atrium>, commit
`1762e5bcf00fb2a038fa4a94777acdd9cb6d4b70`, directory `atrium-common`.
The MIT license is retained from that commit. Source files are unchanged.

The sole manifest modification raises the `wasm32`-only `lru` dependency from
`0.16.3` to `0.18.2`, fixing
[RUSTSEC-2026-0253](https://rustsec.org/advisories/RUSTSEC-2026-0253.html).
Native targets continue using Moka. This does not change the exact Atrium API,
XRPC, repository or crypto pins, wire formats, repository commitments or CIDs.

Vendored files deliberately omit upstream lockfiles and development history.
Review this overlay when upgrading Atrium; remove it when the upstream
dependency requirement includes the fixed release.
