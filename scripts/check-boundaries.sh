#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
#
# Structural guards for the crate boundaries established by the pre-federation
# refactor. A cosmetic re-merge of the old codifix-api kitchen sink or a re-coupled
# read path fails CI here. The explicitly lean CLI build is guarded separately.
set -euo pipefail
cd "$(dirname "$0")/.."

fail=0
err() {
  echo "BOUNDARY VIOLATION: $*" >&2
  fail=1
}

# 1. codifix-api must not come back.
if [ -d crates/codifix-api ]; then
  err "crates/codifix-api exists again"
fi
if grep -rq "codifix_api::" crates/ --include='*.rs'; then
  err "code references codifix_api::"
fi

# 2. codifix-record purity: pure serde types, no runtime/config/db machinery.
record_tree=$(cargo tree -p codifix-record -e normal --prefix none | awk '{print $1}' | sort -u)
for dep in tokio config sqlx axum codifix-platform duckdb; do
  if echo "$record_tree" | grep -qx "$dep"; then
    err "codifix-record depends on $dep"
  fi
done

# 3. codifix-config purity: no async runtime, transport, or database.
config_tree=$(cargo tree -p codifix-config -e normal --prefix none | awk '{print $1}' | sort -u)
for dep in tokio sqlx axum duckdb; do
  if echo "$config_tree" | grep -qx "$dep"; then
    err "codifix-config depends on $dep"
  fi
done

# Narrow filesystem primitives must not pull the broad platform/service layer
# into protected import, federation or supervisor operators.
files_tree=$(cargo tree -p codifix-platform-files -e normal --prefix none | awk '{print $1}' | sort -u)
for dep in tokio sqlx axum sysinfo codifix-platform codifix-config reqwest; do
  if echo "$files_tree" | grep -qx "$dep"; then
    err "codifix-platform-files depends on $dep"
  fi
done

# 3a. codifix-federation purity: protocol-neutral domain and state only. Runtime,
# persistence, and concrete AT Protocol dependencies belong in adapters.
federation_tree=$(cargo tree -p codifix-federation -e normal --prefix none | awk '{print $1}' | sort -u)
for dep in tokio config sqlx axum codifix-platform duckdb atrium-api atrium-xrpc atrium-repo atrium-crypto codifix-atproto-verify; do
  if echo "$federation_tree" | grep -qx "$dep"; then
    err "codifix-federation depends on $dep"
  fi
done
if grep -RInE 'atrium(_|::)|atrium-' crates/codifix-federation/src --include='*.rs'; then
  err "codifix-federation source references Atrium"
fi

# Area planning and supervisor policy are independent of live service machinery.
for crate in codifix-area codifix-supervisor; do
  pure_tree=$(cargo tree -p "$crate" -e normal --prefix none | awk '{print $1}' | sort -u)
  for dep in tokio config sqlx axum duckdb reqwest atrium-api atrium-xrpc codifix-service codifix-platform; do
    if echo "$pure_tree" | grep -qx "$dep"; then
      err "$crate depends on $dep"
    fi
  done
done

# Offline target import must not acquire live configuration, network clients,
# application services, provider execution or filesystem snapshot discovery.
for crate in codifix-area-store codifix-area-ops; do
area_store_tree=$(cargo tree -p "$crate" -e normal --prefix none | awk '{print $1}' | sort -u)
for dep in reqwest axum codifix-config codifix-service codifix-platform codifix-supervisor-runtime codifix-supervisor-pi atrium-api; do
  if echo "$area_store_tree" | grep -qx "$dep"; then
    err "$crate depends on $dep"
  fi
done
done

# Supervisor persistence is not a provider client. Pi process control has no
# credentials, SQL or network transport. Only the narrow runtime may connect the
# two; neither adapter may reach ordinary knowledge/query/curation pipelines.
supervisor_store_tree=$(cargo tree -p codifix-supervisor-store -e normal --prefix none | awk '{print $1}' | sort -u)
for dep in reqwest axum codifix-config codifix-service codifix-platform codifix-supervisor-pi atrium-api; do
  if echo "$supervisor_store_tree" | grep -qx "$dep"; then
    err "codifix-supervisor-store depends on $dep"
  fi
done
supervisor_pi_tree=$(cargo tree -p codifix-supervisor-pi -e normal --prefix none | awk '{print $1}' | sort -u)
for dep in reqwest sqlx axum codifix-config codifix-service codifix-platform codifix-supervisor-store; do
  if echo "$supervisor_pi_tree" | grep -qx "$dep"; then
    err "codifix-supervisor-pi depends on $dep"
  fi
done
supervisor_runtime_tree=$(cargo tree -p codifix-supervisor-runtime -e normal --prefix none | awk '{print $1}' | sort -u)
for dep in codifix-service codifix-platform codifix-search codifix-curate codifix-config duckdb atrium-api; do
  if echo "$supervisor_runtime_tree" | grep -qx "$dep"; then
    err "codifix-supervisor-runtime depends on $dep"
  fi
done

# 3b. Transactional storage may use SQL/runtime, but must not become a network
# Local jobs/events obey the same boundary independently of federation.
store_tree=$(cargo tree -p codifix-store -e normal --prefix none | awk '{print $1}' | sort -u)
for dep in atrium-api atrium-xrpc atrium-repo axum reqwest codifix-config codifix-service codifix-federation-store; do
  if echo "$store_tree" | grep -qx "$dep"; then
    err "codifix-store depends on $dep"
  fi
done

# 3b. Transactional storage may use SQL/runtime, but must not become a network
# adapter or service router. Production PDS/auth integration is a separate edge.
federation_store_tree=$(cargo tree -p codifix-federation-store -e normal --prefix none | awk '{print $1}' | sort -u)
for dep in atrium-api atrium-xrpc atrium-repo atrium-crypto codifix-atproto-verify axum reqwest codifix-service; do
  if echo "$federation_store_tree" | grep -qx "$dep"; then
    err "codifix-federation-store depends on $dep"
  fi
done

# 3c. The offline wire adapter stays separate from repository trust/networking.
atproto_tree=$(cargo tree -p codifix-atproto -e normal --prefix none | awk '{print $1}' | sort -u)
for dep in atrium-crypto codifix-atproto-verify reqwest sqlx axum; do
  if echo "$atproto_tree" | grep -qx "$dep"; then
    err "codifix-atproto depends on $dep"
  fi
done
# Verification is a security adapter, not an identity resolver or service.
verify_tree=$(cargo tree -p codifix-atproto-verify -e normal --prefix none | awk '{print $1}' | sort -u)
for dep in reqwest sqlx axum codifix-service codifix-federation-store; do
  if echo "$verify_tree" | grep -qx "$dep"; then
    err "codifix-atproto-verify depends on $dep"
  fi
done

# 4. No monolith re-forms in the new crates.
# The dedicated gateway must not acquire general service routes, local capture,
# automatic retrieval/curation, or model-provider discovery through dependencies.
federation_runtime_tree=$(cargo tree -p codifix-federation-runtime -e normal --prefix none | awk '{print $1}' | sort -u)
for dep in codifix-service codifix-platform codifix-search codifix-curate codifix-reinforce codifix-models duckdb; do
  if echo "$federation_runtime_tree" | grep -qx "$dep"; then
    err "codifix-federation-runtime depends on $dep"
  fi
done
# Shared imports require the explicit scoped context/egress API, never the
# application's implicit search, consolidation, embedding or provider pipelines.
if rg -n 'federation_(inbox|replica_receipts)|codifix_federation_runtime' crates/codifix-search/src crates/codifix-curate/src crates/codifix-reinforce/src; then
  err "implicit local processing reads federation imports"
fi

while read -r lines file; do
  if [ "$lines" -gt 1200 ] && [ "$file" != "total" ]; then
    err "$file is $lines lines (>1200) - split it"
  fi
done < <(find crates/codifix-record/src crates/codifix-config/src -name '*.rs' ! -name legacy_tests.rs -exec wc -l {} + | awk '{print $1, $2}')

# 5. Read path stays free of the write path.
search_tree=$(cargo tree -p codifix-search -e normal --prefix none | awk '{print $1}' | sort -u)
for dep in codifix-reinforce codifix-curate; do
  if echo "$search_tree" | grep -qx "$dep"; then
    err "codifix-search depends on $dep"
  fi
done

# 6. The explicit TUI-only CLI build stays lean: no server, database, or DuckDB.
# The default source build is full so `cargo run --bin codifix -- service run`
# works without a feature flag.
cli_tree=$(cargo tree -p codifix-cli --no-default-features --features tui -e normal --prefix none | awk '{print $1}' | sort -u)
for dep in codifix-service axum duckdb openidconnect; do
  if echo "$cli_tree" | grep -qx "$dep"; then
    err "lean codifix-cli build depends on $dep"
  fi
done
if grep -q '^sqlx' crates/codifix-cli/Cargo.toml; then
  err "codifix-cli declares sqlx directly"
fi
postgres_cli_tree=$(cargo tree -p codifix-cli --no-default-features --features tui,embedded-service-postgres -e normal --prefix none | awk '{print $1}' | sort -u)
if echo "$postgres_cli_tree" | grep -qx 'duckdb'; then
  err "PostgreSQL-only service build depends on DuckDB"
fi
if ! echo "$postgres_cli_tree" | grep -qx 'codifix-service'; then
  err "PostgreSQL-only service build omits the service"
fi
default_cli_tree=$(cargo tree -p codifix-cli -e normal --prefix none | awk '{print $1}' | sort -u)
if ! echo "$default_cli_tree" | grep -qx 'duckdb'; then
  err "default full CLI build lost offline support"
fi

# 7. Dependents declare minimal needs.
for crate in codifix-curate codifix-graph codifix-eval codifix-loops; do
  if grep -q 'codifix-config' "crates/$crate/Cargo.toml"; then
    err "$crate depends on codifix-config (records only, please)"
  fi
done
if grep -q 'codifix-record' crates/codifix-skills/Cargo.toml; then
  err "codifix-skills depends on codifix-record (config only, please)"
fi

if [ "$fail" -ne 0 ]; then
  exit 1
fi
echo "boundaries OK"
