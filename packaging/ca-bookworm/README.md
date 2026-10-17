# Debian 12 candidate builds (off-CA only)

## Public release orchestration

For a frozen public release, build this directory's Dockerfile, capture its image
ID, and invoke from the Git checkout:

```sh
mkdir -m 700 /absolute/build-volume/linux-amd64
bash packaging/ca-bookworm/release.sh FULL_COMMIT /absolute/build-volume/linux-amd64 sha256:IMAGE_ID amd64
```

Use a separate private work directory for `arm64`. CI may restore only the
`memory-cargo`, `memory-target` and `npm-cache` subdirectories to reuse matching
inputs; source and output directories must be new. The helper archives the exact
Git object, fetches public dependencies in a separate container without host
credentials, then compiles offline. It emits Debian packages, source/builder
provenance, per-architecture checksums, complete static ELF ABI reports and
native AMD64 or explicit QEMU ARM64 version/help checks under `WORK/output`.
These checks do not install packages or activate services; disposable package
installation and application-level acceptance remain separate release checks.

```sh
python3 packaging/ca-bookworm/test_release_build.py
```

This directory targets the CA's observed Debian 12 / AArch64 / glibc 2.36
userspace. It never upgrades the CA, installs host libraries, grants Pi provider
access or makes an ARM64 emulator into CA sandbox evidence. The source boundary
and outstanding gates are in the [execution ledger](../../docs/reviews/four-area-network/ca-bookworm-release.md).

## Inputs and isolation

Build `Dockerfile` on Monolith, then select its **exact local image ID**, not the
tag. Both base images are digest pinned. APT uses the signed Debian Bookworm
repositories; preserve `dpkg-query -W`, compiler versions and the final image ID
with every build. Re-running APT later can select security updates: the recipe
does not promise byte-for-byte reproducibility. Retain the actual build image
for replay. No host package installation or binfmt registration is necessary.

Prepare a new mode-0700 directory on the build volume containing:

| Directory | Contents |
| --- | --- |
| `memory/` | `git archive` of the exact clean reviewed Memory commit |
| `pi/` | archive of `7c018475d54d4e09c907691e47a520f1f09e3288`, with only `no-tools-1.patch` applied |
| `memory-cargo/`, `pi-cargo/` | private copies of the matching public Cargo `registry/` and `git/` caches only |
| `npm-cache/` | npm cache populated from the Memory `web/package-lock.json` |
| `output/` | initially empty output directory |

Keep the two original source archives and their SHA-256s. Verify all tracked
source files against them, except Pi's exact reviewed patch. Its unchanged
lockfile and full license must match the [recorded hashes](../../crates/mem-supervisor-pi/patches/README.md).
The first candidate additionally uses the separately recorded Debian packaging
fix from commit `d8d390fa756cde35e9c2763ae5961ac8ce83801b`; preserve its exact
`packaging/build-deb.sh` diff and verify it as the only Memory source difference.
New builds should use a clean commit containing that fix rather than an
unrecorded local change.
Do not copy full Cargo homes, npm configuration, project configuration, provider
keys, user homes, offline stores or databases. Use a separately prepared pinned
Pi nightly toolchain, `nightly-2026-08-31`, mounted read-only. The Memory image
contains Rust 1.98.1 and the ARM64 standard library; Pi's toolchain must also
contain its ARM64 standard library.

Dependency fetching is a separate preparation step without credentials. For
example, an isolated container may run `npm --prefix web ci --ignore-scripts
--no-audit --no-fund` with `npm_config_cache=/work/npm-cache`; no model endpoint
is used. Source compilation, Web tests, packaging and runtime checks then run
with **network none**, a read-only container root, non-root UID, no capabilities,
no new privileges, bounded CPU/memory/PIDs and only private build inputs mounted.
Do not repair a missing cache by adding host credentials or network to compilation.

## Repeat the phases

Supply absolute non-symlink paths and the recorded image ID. The helper refuses
floating tags, shared work directories and implicit image pulls. It also checks
that the builder actually has glibc 2.36 and cross GCC 12. Each invocation creates
a fresh log; previous logs/artifacts are not overwritten. Build outputs remain
under the private work directory, never installed over a running executable.

```sh
bash packaging/ca-bookworm/build.sh memory "$CA_BUILD_WORK" "$CA_BUILD_IMAGE"
bash packaging/ca-bookworm/build.sh pi "$CA_BUILD_WORK" "$CA_BUILD_IMAGE" "$CA_PI_TOOLCHAIN"
bash packaging/ca-bookworm/build.sh web "$CA_BUILD_WORK" "$CA_BUILD_IMAGE"
bash packaging/ca-bookworm/build.sh package "$CA_BUILD_WORK" "$CA_BUILD_IMAGE"
```

For a coherent two-architecture Memory release, use the **same clean source
revision** and captured builder for both targets. `memory` and `package` accept
an optional fourth argument `amd64` or `arm64`; omission retains the original
ARM64 default. Pi's fourth argument is still its explicit nightly toolchain.

```sh
bash packaging/ca-bookworm/build.sh package "$CA_BUILD_WORK" "$CA_BUILD_IMAGE" arm64
bash packaging/ca-bookworm/build.sh package "$CA_BUILD_WORK" "$CA_BUILD_IMAGE" amd64
```

Run the two package phases serially: they share Web/node_modules and build caches.
Both packages get newly verified source-bound provenance, even when Cargo reuses
matching compiled inputs. Do not relabel the preserved older candidate, mix old
AMD64 artifacts into a new manifest or assume a completed build is a signature.

Run memory and Pi builds concurrently only with separate target/cache paths as
above. Wait for cache copying to finish before starting either build. The package
phase reuses ARM64 outputs and builds a host-native lean CLI for completions;
it never executes an ARM64 binary implicitly. Existing Debian output is refused.
The package's actual `dpkg-shlibdeps` dependencies must also fit Debian 12.
Bookworm does not support `--package`; the build supplies one binary-package
control stanza. If its cross C++ shlibs entry is unversioned, the exact GCC
`Built-Using` version supplies a conservative bound. Missing or non-exact source
metadata is an error, not permission to drop the version constraint.

## Verification and evidence

The static checker defaults to ARM64. For the native companion explicitly use
`abi.py --architecture amd64 --sysroot <captured-amd64-root> <executables...>`.
Both modes require an independent captured sysroot (never host `/`), check the
complete required-library/symbol/version closure and reject GLIBC newer than
2.36. Preserve the native `/lib64/ld-linux-x86-64.so.2` location; symlinks must
resolve within that captured root. Wrong-architecture libraries, escaping links
and ambiguous library resolutions remain errors. Record the verifier's own
revision/hash separately when it is newer than the frozen package source.

`GLIBC_ABI_DT_RELR` is a distinct, empty libc version node introduced with
[glibc 2.36](https://sourceware.org/pipermail/libc-alpha/2022-August/141193.html),
not a numeric symbol version. The checker recognizes only that exact feature,
requires its declared `libc.so.6` provider and matching version definition, and
reports its 2.36 floor separately from the highest numeric GLIBC requirement.
It does not waive strong-symbol resolution or accept other `GLIBC_ABI_*` names.
This matters for the native Bookworm `libm` dependency even when the application
itself uses only older numeric GLIBC symbols.

- Static audit every shipped ARM64 executable against the exact captured
  `/usr/aarch64-linux-gnu` sysroot, including transitive libraries and symbol
  versions. Checking just `file`, the filename or one GLIBC string is insufficient.
- Run explicit `qemu-aarch64 -L /usr/aarch64-linux-gnu` checks in a fresh
  network-none/non-root/read-only container; mount only the selected candidates,
  test tools and private scratch, not the build tree or production state.
- Memory smoke checks are `--version`, operator help and offline `schema-plan`.
  Do not run a service against any existing database to test binary launch.
- Pi checks use only synthetic text and an invalid token against loopback fake
  responses. No arbitrary model endpoint or production pin override is supplied.
- Preserve source/toolchain/patch/license/package/binary/Web hashes and the exact
  test outputs. Record native execution, CA path denials, systemd/cgroup resource
  enforcement, database compatibility and real deployment as **not tested** here.

The older rc.1 packages, existing Pi production hashes and live previews stay
unchanged. Internal compatibility artifacts are not a signed/published release.
Operator signing trust, CA-native tests and actual activation need separate review.

Focused command-construction tests (no Docker daemon or builds):

```sh
python3 packaging/ca-bookworm/test_build.py
```
