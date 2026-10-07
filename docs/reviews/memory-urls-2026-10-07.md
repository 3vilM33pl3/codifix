# `memory urls` implementation evidence

The read-only configuration inventory is implemented in
`crates/mem-cli/src/commands/urls.rs`. Dispatch occurs before normal client,
database and area-resolution setup. No live availability or authorization claim
is made.

## Verification

- All 13 focused lean CLI tests pass and cover ordering, JSON, redaction, public addresses,
  wildcard IPv4/IPv6, disabled endpoints, development overlays, area filtering,
  missing configurations, remote-only areas and gateway inventory.
- Lean and full CLI builds and formatting pass. Targeted Clippy passes with
  `-D clippy::all -A dead_code -A unused_imports`; the allowances are for existing
  feature-disabled code, not new URL code.
- Monolith configuration inspection lists Workshop, Homelab and General Web
  roots, installed/repository configurations and repository development. Trust
  remains a remote-only notice. Workshop filtering excludes other local areas;
  Trust filtering emits no invented local address. Duplicate installed/repository
  origins retain both labels. Explicit development configuration is also included.
- Federation gateways are read only from the explicitly supplied participant
  configuration or `MEMORY_LAYER_FEDERATION_CONFIG`. No PDS discovery or secret
  loading is performed.

The broader runtime test subset is not green: existing failures concern missing
agent help for `auth login`, Debian restart script expectations and a machine-local
link in `mac-onboarding.md`. The root documentation test, once this command's page
was added, reaches another pre-existing missing page, `cli/projects.md`. These are
outside this command's scope; this work does not claim a passing workspace suite.

The source-built command does not upgrade installed named services or immutable
release packages. Running services, configuration, permissions and databases were
left unchanged. The previous full dev service executable was preserved while
building the lean CLI; the full source executable is built separately.

The dev Memory `add-memory-urls` completion checkpoint verified all six approved
items and recorded the implementation. Run the built command from the checkout:

```bash
./target/debug/memory urls
./target/debug/memory urls --json
./target/debug/memory urls --area workshop
```
