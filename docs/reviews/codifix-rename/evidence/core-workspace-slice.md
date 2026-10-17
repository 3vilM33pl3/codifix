# Core workspace identity slice

Source baseline: `6af8cc36`. Implementation: `7f3eeabc`.

The metadata-derived [target map](core-target-map.json) records all 32 packages
and all 12 binary targets: six explicitly declared and six Cargo auto-discovered
targets. Each `mem-*` workspace package/directory is now `codifix-*`; Rust crate
imports and aliases use `codifix_*`. The executable target mapping is:

| Previous target | Current target |
| --- | --- |
| memory | codifix |
| memory-area | codifix-area |
| memory-area-import | codifix-area-import |
| memory-gateway-schema | codifix-gateway-schema |
| memory-windows-evaluation-helper | codifix-windows-evaluation-helper |
| memory-windows-isolation-probe | codifix-windows-isolation-probe |
| memory-windows-service | codifix-windows-service |
| memory-federation-enroll | codifix-federation-enroll |
| memory-federation-gateway | codifix-federation-gateway |
| memory-pds-pipe | codifix-pds-pipe |
| memory-pi-sandbox | codifix-pi-sandbox |
| memory-workshop-worker | codifix-workshop-worker |

The one-shot [transformation](../../../../scripts/codifix-core-rename.py) has a
default dry run, parsed TOML equivalence checks, a fixed-baseline requirement,
clean tracked-file checks and destination-path guards. The reviewed
[dry run](core-dry-run.json) was regenerated immediately before applying it.
Validation subsequently found two relative test-support `#[path]` references
outside the initial `crates/` prefix and one stale Windows build-target test
expectation; those were corrected along with the transformation. Rustfmt
reflowed expressions affected by the longer crate identifiers.

The built CLI smoke test exposed an additional custom `--version`/`-V` fast path
outside clap which still printed the old name. Its executable label was changed
without changing the version, and a real binary regression test now covers both
version flags and help output.

Only local package names changed in Cargo.lock. A parsed comparison against the
baseline, applying only the 32-name map to local package/dependency names,
confirmed exact equality: no third-party versions, checksums or resolution
changes. No Cargo update was run.

Initial check gates passed:

- `cargo metadata --no-deps --format-version 1 --locked`
- `cargo fmt --all --check`
- `cargo check --workspace --all-targets --locked`
- `bash scripts/check-boundaries.sh`
- `git diff --exit-code b56292a1 -- migrations/`
- `git diff --check`
- Focused Windows script contracts: `test_build_tools.py`,
  `test_native_eval_bootstrap.py`, `test_core_install.py`, and
  `test_installed_core_check.py`: 40 passed, three PowerShell/native-only checks
  skipped on Linux.
- `cargo build -p codifix-cli --bin codifix --locked`
- `target/debug/codifix --help` reports `Usage: codifix ...`.
- `target/debug/codifix --version` reports `codifix 2.1.0-dev`.
- Focused CLI unit checks: root command metadata and shell completions (two
  passed).
- `cargo test -p codifix-cli --test identity --locked` (one passed).
- `cargo test -p codifix-area --test planning --locked cli_` (two passed,
  exercising the renamed offline executable).
- `cargo test -p codifix-area-ops --test gateway_schema --locked
  default_review_is_offline_exact_and_never_discloses_private_values` (one
  passed, exercising the renamed offline gateway-schema executable).

Before compilation the filesystem had 218 GiB free and `target/` occupied 71 GiB.
One build owner reused the existing target cache; no caches or recovery binaries
were pruned.

Runtime environment/configuration/state paths, UUID derivation, credentials,
protocol identities, cookies, plugin registration, version changes and live
deployment are reserved for later slices. Existing migrations and historical
evidence are unchanged. Packaging and `scripts/install-local.sh` are explicitly
deferred in full; remaining package/binary references in those scripts are
pending the installation slice. CLI help examples and generated bootstrap
content still need the later branding pass. Current source executable discovery,
completion generation and directly coupled Windows executable filenames use
the new targets while service/account/state names remain unchanged.
