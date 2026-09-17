# Internal no-tools.1 experiment

This is the user-authorized narrow local patch, not an upstream Pi release or a
production activation. Build label: `0.5.1+memory.no-tools.1`. Pi's own package
version remains `0.5.1`; that version alone must never identify this artifact.
The original source/archive/binary pins in `lib.rs` remain unchanged.

- Upstream: `https://github.com/Dicklesworthstone/pi_agent_rust`
- Source: `7c018475d54d4e09c907691e47a520f1f09e3288`
- Patch: `no-tools-1.patch`, SHA-256
  `bec724b3d1cef1df4f4cddc50573ce6af1724c743496e7f5e900f4e4a6c815bc`
- Unchanged lockfile SHA-256:
  `1aea4e3fe13a86760317d9c3da80d7461bd5cd5c013508ab5446a391c7fa14e5`
- Complete upstream notice: [LICENSE.pi](LICENSE.pi), SHA-256
  `32a82e0a5754e72e51fae44b65a936c831c07376f21c90f5fb9e76897fcc3509`

The notice is MIT-derived **with a nonstandard OpenAI/Anthropic rider**. It is
retained verbatim, not replaced by the surrounding Memory repository licence.
Olivier confirmed continuation after inspection; that is not a claim of legal
clearance or maintainer permission. No publication/redistribution is part of
this experiment.

## Scope

Only two registration sites in `src/main.rs` change: `--no-tools` constructs an
empty registry rather than the constructor that implicitly adds `manage_skill`
and discovery; later startup does not add `submit_plan`. With tools enabled,
both paths retain their existing behavior. Plan-state initialization is intact.
No dependency, model, provider, authentication or licence edits are included.

This is specifically the classic/RPC startup used by Memory with extensions,
skills, context files and inherited configuration disabled. It does not certify
arbitrary MCP/extension configuration, SDK sessions or the interactive FTUI.
It does not replace a sandbox, disclosure review or external request admission.

## Build and verification

The source's pinned `nightly-2026-08-31` is Rust 1.100.0-nightly
(`90850177249efe0321573c569aec5d12b257f8d6`, LLVM 23.1.0). The internal AMD64
experiment uses its unmodified default features and release profile:

```sh
git apply --check /absolute/path/to/no-tools-1.patch
git apply /absolute/path/to/no-tools-1.patch
cargo build --release --bin pi --locked
```

Use private, separate source, Cargo, Rustup, target and temporary directories on
the build volume; do not install over the official artifact or inherit provider
credentials. Preserve the exact patch, toolchain and binary digest together.
Build output is not assumed reproducible: upstream embeds build/Git metadata.

Upstream's DSR release/quality infrastructure is unavailable here and is not
invoked. This user-authorized local experiment is neither upstream quality-gate
evidence nor an official release. Memory's test-only diagnostic separately pins
the resulting binary and runs under an independently enforced network-none,
read-only-root, non-root container. It uses only synthetic text and an invalid
test token, never a paid provider. Production provider execution remains closed.

### AMD64 evidence, 2026-09-17

The build completed in 11m 50s. Binary SHA-256:
`912366d4178bd7c12ccf4401b6241b1dcb6f8cdbd66302bc1764a1cec9dfe88c`.
The preserved local artifact is
`/data/tmp/memory-pi-no-tools-1-amd64.UfMVfCQ4/pi`, alongside the complete licence
and exact patch. The build log is
`/data/tmp/memory-pi-no-tools-1-amd64-build.log`. The binary requires glibc 2.43;
this patch does not improve CA runtime compatibility.

The ignored Memory diagnostic
`provider_probe::sandboxed_patched_fake_provider_has_no_tools` was explicitly
run and passed in 12.35s against that binary. It first proves the original
production pin rejects the patched artifact, then checks:

| Synthetic case | Advertised tools | Local provider requests |
| --- | --- | --- |
| Success, default tool settings | None | 1 |
| Success, explicit off tiers | None | 1 |
| HTTP 429, explicit off tiers | None | 1, no automatic retry |

Both success cases return exactly the supplied synthetic text; HTTP 429 reports
failure without invented content. Model, question, output limit, context,
retry/compaction controls, framing and child cleanup are still checked. The
unchanged official artifact's diagnostic also passes **by reproducing the
defect**, not by satisfying the tool-free gate. Ordinary tests: 13 passed, four
environment-bound diagnostics ignored by default; strict Memory-adapter Clippy,
formatting and repository boundaries pass. Patch and full licence hashes are
unit-tested. No upstream whole-project quality or deployed-security claim follows.

The container is pinned to
`ubuntu@sha256:cd21a4f68a617580279d4b091cb18e3af9fa8a87500665f0ae5f7f757d17d367`,
network-none/read-only-root, UID 1000, no capabilities/new privileges, 32-process,
512-MiB and one-CPU limits, private scratch tmpfs and only two read-only mounts
(the pinned candidate and Memory's diagnostic executable). No host credentials,
project contents or database were mounted.

The upstream build reports an existing duplicate example/bench target warning
and future-compatibility warnings in `nix 0.28.0` and `proc-macro-error2 2.0.1`;
they were not hidden or fixed by an unrelated dependency update. ARM64 compilation
is a separate pending experiment, not covered by the AMD64 result. Do not bundle
the patch/binary into a public release without an explicit licensing review.
