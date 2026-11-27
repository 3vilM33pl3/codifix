#!/usr/bin/env bash
# Provision a dedicated PDS and emit only the path to its private session file.
set -euo pipefail
umask 077
[[ $# == 1 || $# == 3 ]] || { echo "usage: $0 STATE_DIR [FIRST_ROLE SECOND_ROLE]" >&2; exit 2; }
mkdir -p -- "$1"
state=$(realpath -- "$1")
chmod 700 "$state"
exec 9>"$state/install.lock"
flock 9
here=$(dirname "$(realpath -- "$0")")
if [[ ! -f "$state/pds.env" ]]; then
  first=${2:-homelab} second=${3:-linear}
  for role in "$first" "$second"; do
    [[ $role =~ ^[a-z]([a-z0-9-]{0,30}[a-z0-9])?$ && $role != pds ]] || { echo 'Invalid companion role' >&2; exit 2; }
  done
  [[ $first != "$second" ]] || { echo 'Choose distinct companion roles' >&2; exit 2; }
  id=$(openssl rand -hex 8)
  cat > "$state/pds.env.new" <<ENV
PRIVATE_AT_PORT=$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')
PRIVATE_AT_DOMAIN=c$id.codifix.example.com
PRIVATE_AT_FIRST=$first
PRIVATE_AT_SECOND=$second
PRIVATE_AT_PROJECT=codifix-pds-$id
PRIVATE_AT_JWT_SECRET=$(openssl rand -hex 32)
PRIVATE_AT_ADMIN_PASSWORD=$(openssl rand -hex 32)
PRIVATE_AT_ROTATION_KEY=$(openssl rand -hex 32)
ENV
  mv "$state/pds.env.new" "$state/pds.env"
fi
set -a
source "$state/pds.env"
set +a
if [[ $# == 3 && ( $2 != "$PRIVATE_AT_FIRST" || $3 != "$PRIVATE_AT_SECOND" ) ]]; then
  echo 'The requested roles differ from this PDS installation' >&2; exit 2
fi
export PRIVATE_AT_UID=$(id -u) PRIVATE_AT_GID=$(id -g) PRIVATE_AT_STATE="$state"
mkdir -p "$state/tls" "$state/state"
if [[ ! -s "$state/tls/cert.pem" || ! -s "$state/tls/key.pem" ]]; then
  openssl req -x509 -newkey rsa:2048 -nodes -days 3650 -subj '/CN=Codifix private DID directory' \
    -addext "subjectAltName=DNS:*.$PRIVATE_AT_DOMAIN" -out "$state/tls/cert.pem" -keyout "$state/tls/key.pem" 2>/dev/null
fi
compose=(docker compose --project-name "$PRIVATE_AT_PROJECT" --file "$here/compose.yaml")
"${compose[@]}" up -d --wait --wait-timeout 90 >&2
"${compose[@]}" exec -T directory node /app-setup/directory.cjs enroll >&2
port=$("${compose[@]}" port directory 2584)
[[ $port == 127.0.0.1:* ]] || { echo 'Expected a loopback-only PDS port' >&2; exit 1; }
python3 - "$state" "$port" "$PRIVATE_AT_FIRST" "$PRIVATE_AT_SECOND" <<'PY'
import json, os, sys, urllib.request
from pathlib import Path
state = Path(sys.argv[1]); endpoint = 'http://' + sys.argv[2]
with urllib.request.urlopen(endpoint + '/xrpc/_health', timeout=5) as response:
    if response.status != 200:
        raise RuntimeError('PDS health check failed')
accounts = {}
for role in sys.argv[3:]:
    source = json.loads((state / 'state' / (role + '.json')).read_text())
    accounts[role] = {key: source[col] for key, col in [('did', 'did'), ('handle', 'handle'), ('signing_key', 'signingKey'), ('access_jwt', 'accessJwt'), ('refresh_jwt', 'refreshJwt'), ('password', 'password')]}
out = state / 'accounts.json'; tmp = out.with_suffix('.new')
tmp.write_text(json.dumps({'pds_url': endpoint, 'accounts': accounts}, indent=2) + '\n')
os.replace(tmp, out)
print(out)
PY
