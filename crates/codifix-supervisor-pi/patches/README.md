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

## Distribution boundary

This directory retains the corresponding experimental patch and complete upstream
notice already present in the public source. No upstream Pi executable is bundled
in this ARM64 Linux package. The notice contains a nonstandard licence rider;
retention of this source is not a claim of upstream licence clearance or permission
to activate a provider. Production provider execution remains closed.
