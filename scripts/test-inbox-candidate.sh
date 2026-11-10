#!/usr/bin/env bash
# Build and run the native desktop -> service -> worker -> local Linear test.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
: "${CODIFIX_TEST_DATABASE_URL:?Set this to a disposable pgvector PostgreSQL database with CREATEDB access}"
export CODIFIX_TEST_REQUIRE_DB=1
export CODIFIX_INBOX_CANDIDATE_LINEAR_TOKEN=synthetic-inbox-candidate-linear-token

build_messages=$(mktemp)
trap 'rm -f "$build_messages"' EXIT
cargo test --manifest-path desktop/src-tauri/Cargo.toml --locked --bin codifix-desktop \
    --no-run --message-format=json >"$build_messages"
CODIFIX_INBOX_NATIVE_TEST=$(python3 - "$build_messages" <<'PY'
import json
import sys

with open(sys.argv[1]) as messages:
    executables = [
        message['executable']
        for line in messages
        if (message := json.loads(line))['reason'] == 'compiler-artifact'
        and message['target']['name'] == 'codifix-desktop'
        and message['profile']['test']
        and message['executable']
    ]
if len(executables) != 1:
    sys.exit('Expected one native desktop test executable')
print(executables[0])
PY
)
export CODIFIX_INBOX_NATIVE_TEST
cargo test --locked -p codifix-service --no-default-features --lib \
    network::candidate::native_inbox_linear_workflow -- --ignored --exact --nocapture | tee "$build_messages"
rg -qx 'INBOX_CANDIDATE_VERIFIED' "$build_messages"
