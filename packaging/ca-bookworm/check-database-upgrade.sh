#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# Exercise real packaged service builds against the same disposable PostgreSQL DB.
set -euo pipefail

if [[ $# != 1 || "$1" != /* || ! -f "$1" || -L "$1" ||
      ${GITHUB_ACTIONS:-} != true || ${RUNNER_OS:-} != Linux ||
      ${RUNNER_ENVIRONMENT:-} != github-hosted || $(id -u) == 0 ]]; then
  echo "Usage on an unprivileged GitHub-hosted Linux runner: $0 ABSOLUTE_CANDIDATE_DEB" >&2
  exit 2
fi

candidate=$1
arch=$(dpkg --print-architecture)
[[ $arch == amd64 || $arch == arm64 ]]
[[ $(dpkg-deb -f "$candidate" Package) == codifix ]]
[[ $(dpkg-deb -f "$candidate" Architecture) == "$arch" ]]
[[ $(dpkg-deb -f "$candidate" Version) == 3.1.0 ]]

work=$(mktemp -d "${RUNNER_TEMP:?}/codifix-db-upgrade.XXXXXXXX")
mkdir -p "$work/home"
db_admin='postgresql://codifix_release:disposable-only@127.0.0.1:5432/postgres'
export PGCONNECT_TIMEOUT=5
database=''
service_pid=''
stop_on_exit() {
  if [[ -n $service_pid ]]; then
    kill -TERM "$service_pid" 2>/dev/null || true
    for ((attempt=0; attempt<20; attempt++)); do
      kill -0 "$service_pid" 2>/dev/null || break
      sleep 0.25
    done
    kill -KILL "$service_pid" 2>/dev/null || true
    wait "$service_pid" 2>/dev/null || true
  fi
}
cleanup() {
  local status=$?
  stop_on_exit
  if [[ -n $database ]]; then
    dropdb --maintenance-db="$db_admin" "$database" || status=1
  fi
  if (( status == 0 )); then
    rm -rf -- "$work"
  else
    echo "PostgreSQL upgrade acceptance failed. Service logs follow." >&2
    for log in "$work"/*.log; do
      [[ -f $log ]] && tail -n 100 "$log" >&2
    done
  fi
  exit "$status"
}
trap cleanup EXIT

verify_digest() {
  local package=$1 expected=$2
  if [[ ! $expected =~ ^[a-f0-9]{64}$ ]] ||
    ! printf '%s  %s\n' "$expected" "$package" | sha256sum --check --status; then
    echo "Invalid checksum for ${package##*/}" >&2
    return 1
  fi
}

verify_candidate() {
  local receipt="${candidate%/*}/build-$arch.json" expected
  if [[ ! -f $receipt || -L $receipt ]] ||
    ! expected=$(jq -er --arg arch "$arch" --arg revision "${GITHUB_SHA:?}" \
      --arg name "${candidate##*/}" '
      select(.format == "codifix-linux-build-v1" and .architecture == $arch and
        .application_version == "3.1.0" and .source_commit == $revision) |
      .artifacts[$name]
    ' "$receipt"); then
    echo "Missing matching build receipt for ${candidate##*/}" >&2
    return 1
  fi
  verify_digest "$candidate" "$expected"
}

request() {
  local path=$1
  shift
  if ! curl --fail-with-body --silent --show-error --max-time 15 --noproxy '*' \
    --header "x-api-token: $token" "$@" "$origin$path"; then
    echo "Upgrade request failed at $path" >&2
    return 1
  fi
}

start_service() {
  local root=$1 version=$2 stage=$3 binary="$1/usr/bin/codifix"
  local status
  [[ $(env -i HOME="$work/home" PATH="$PATH" CODIFIX_PROFILE=prod "$binary" --version) == "codifix $version" ]]
  cat > "$work/service.toml" <<CONFIG
[service]
bind_addr = '127.0.0.1:$port'
api_token = '$token'
web_root = '$root/usr/share/codifix/web'
[database]
url = '$db_url'
[features]
llm_curation = false
[embeddings]
enabled = false
create_enabled = false
[background_work]
enabled = false
[cluster]
enabled = false
[automation]
enabled = false
[offline]
enabled = false
[reinforcement]
enabled = false
[provenance]
reverify_enabled = false
[consolidation]
enabled = false
[procedural]
enabled = false
[mcp]
enabled = false
http_enabled = false
[telemetry]
enabled = false
CONFIG
  env -i HOME="$work/home" PATH="$PATH" CODIFIX_PROFILE=prod "$binary" \
    --config "$work/service.toml" service run > "$work/$version-$stage.log" 2>&1 &
  service_pid=$!
  for ((attempt=0; attempt<120; attempt++)); do
    kill -0 "$service_pid" 2>/dev/null || break
    if status=$(request /healthz 2>/dev/null) &&
      jq -e --arg version "$version" \
        '.status == "ok" and .database == "up" and .version == $version' <<<"$status" >/dev/null; then
      return
    fi
    sleep 0.5
  done
  echo "$version service failed healthy startup ($stage)" >&2
  return 1
}

stop_service() {
  request /v1/admin/shutdown --request POST | jq -e '.accepted' >/dev/null
  for ((attempt=0; attempt<80; attempt++)); do
    if ! kill -0 "$service_pid" 2>/dev/null; then
      wait "$service_pid"
      service_pid=''
      return
    fi
    sleep 0.25
  done
  echo 'Service did not shut down cleanly' >&2
  return 1
}

schema_head() {
  psql -X -v ON_ERROR_STOP=1 -At "$db_url" \
    -c 'SELECT max(version) FROM _sqlx_migrations HAVING bool_and(success)'
}

schema_history() {
  psql -X -v ON_ERROR_STOP=1 -At "$db_url" \
    -c "SELECT version, encode(checksum, 'hex') FROM _sqlx_migrations ORDER BY version"
}

memory_id() {
  request /v1/projects/release-upgrade/memories |
    jq -er '[.items[] | select(.preview | contains("43117"))] |
      select(length == 1) | .[0].id'
}

seed_memory() {
  local capture_id
  capture_id=$(request /v1/capture/task --json '{
    "project": "release-upgrade",
    "task_title": "Synthetic release capture",
    "user_prompt": "Remember the synthetic backup port",
    "writer_id": "release-test",
    "agent_summary": "The synthetic backup listener uses port 43117.",
    "idempotency_key": "synthetic-release-upgrade",
    "structured_candidates": [{
      "canonical_text": "The synthetic backup listener uses port 43117.",
      "summary": "Synthetic backup port",
      "memory_type": "implementation",
      "confidence": 0.95,
      "importance": 4,
      "tags": ["synthetic"],
      "sources": [{"source_kind": "note", "excerpt": "The synthetic backup listener uses port 43117."}]
    }]
  }' | jq -er '.raw_capture_id')
  request /v1/curate --json "$(jq -nc --arg id "$capture_id" \
    '{project: "release-upgrade", raw_capture_id: $id}')" >/dev/null
}

check_upgrade() {
  local tag=$1 version=$2 prior_head=$3
  local repo='https://github.com/3vilM33pl3/codifix/releases/download'
  local package="$work/codifix_${version}_${arch}.deb"
  local root="$work/prior-$version" before after expected
  local asset
  for asset in "${package##*/}" "${package##*/}.sha256"; do
    curl --fail --silent --show-error --location --retry 3 --max-time 180 \
      --proto '=https' --proto-redir '=https' --output "$work/$asset" "$repo/$tag/$asset"
  done
  [[ -f "$package.sha256" && ! -L "$package.sha256" ]]
  read -r expected _ < "$package.sha256"
  verify_digest "$package" "$expected"
  [[ $(dpkg-deb -f "$package" Package) == codifix ]]
  [[ $(dpkg-deb -f "$package" Architecture) == "$arch" ]]
  [[ $(dpkg-deb -f "$package" Version) == "${version/-/\~}" ]]
  dpkg-deb -x "$package" "$root"
  dpkg --compare-versions "$(dpkg-deb -f "$package" Version)" lt 3.1.0

  database="codifix_release_$(tr -d '-' < /proc/sys/kernel/random/uuid)"
  db_url="${db_admin%/*}/$database"
  createdb --maintenance-db="$db_admin" "$database"

  start_service "$root" "$version" prior
  [[ $(schema_head) == "$prior_head" ]]
  schema_history > "$work/prior-history"
  seed_memory
  before=$(memory_id)
  stop_service

  start_service "$work/candidate" 3.1.0 upgrade
  [[ $(schema_head) == 105 ]]
  schema_history > "$work/candidate-history"
  diff -u "$work/prior-history" <(head -n "$(wc -l < "$work/prior-history")" "$work/candidate-history")
  [[ $(memory_id) == "$before" ]]
  stop_service

  start_service "$work/candidate" 3.1.0 restart
  [[ $(schema_head) == 105 ]]
  after=$(memory_id)
  [[ $after == "$before" ]]
  request /v1/query --json \
    '{"project":"release-upgrade","query":"synthetic backup port","retrieval_mode":"lexical","answer_mode":"deterministic"}' |
    jq -e --arg id "$before" 'any(.results[]; .memory_id == $id and (.snippet | contains("43117")))' >/dev/null
  stop_service

  dropdb --maintenance-db="$db_admin" "$database"
  database=''
  echo "PASS: native $arch packaged $version to 3.1.0 database upgrade ($prior_head to 105), migration history and application data preserved through restart"
}

verify_candidate
dpkg-deb -x "$candidate" "$work/candidate"
port=$(python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')
origin="http://127.0.0.1:$port"
token=$(tr -d '-' < /proc/sys/kernel/random/uuid)
check_upgrade v3.0.0 3.0.0 85
if [[ $arch == arm64 ]]; then
  check_upgrade arm64-v3.1.0-rc.2 3.1.0-rc.2 105
fi
