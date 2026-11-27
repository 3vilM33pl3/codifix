#!/usr/bin/env bash
# Exercise first install and re-enrolment after container replacement using disposable state.
set -euo pipefail
umask 077
here=$(dirname "$(realpath "$0")")
state=$(mktemp -d)
compose() {
  (set -a; source "$state/pds.env"; set +a
   export PRIVATE_AT_UID=$(id -u) PRIVATE_AT_GID=$(id -g) PRIVATE_AT_STATE="$state"
   docker compose --project-name "$PRIVATE_AT_PROJECT" -f "$here/compose.yaml" "$@")
}
cleanup() {
  if [[ -e $state/pds.env ]]; then compose down --volumes --timeout 1 >/dev/null; fi
  rm -rf -- "$state"
}
trap cleanup EXIT
accounts=$("$here/install.sh" "$state")
cp "$accounts" "$state/before.json"
compose down --timeout 1 >/dev/null
[[ $("$here/install.sh" "$state") == "$accounts" ]]
python3 - "$accounts" "$state/before.json" <<'PY'
import base64
import json
import stat
import sys
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

output, before = map(Path, sys.argv[1:])
after = json.loads(output.read_text())
prior = json.loads(before.read_text())
endpoint = after["pds_url"]
assert stat.S_IMODE(output.stat().st_mode) == 0o600
assert endpoint == prior["pds_url"] and urllib.parse.urlsplit(endpoint).hostname == "127.0.0.1"
for role, peer in (("homelab", "linear"), ("linear", "homelab")):
    account = after["accounts"][role]
    old = prior["accounts"][role]
    assert all(account[field] == old[field] for field in ("did", "handle", "signing_key", "password"))
    assert account["refresh_jwt"] != old["refresh_jwt"]
    audience = after["accounts"][peer]["did"] + "#codifix_agency"
    method = "dev.codifix.agency.sendMessage"
    params = urllib.parse.urlencode(dict(aud=audience, lxm=method))
    request = urllib.request.Request(endpoint + "/xrpc/com.atproto.server.getServiceAuth?" + params,
                                     headers={"Authorization": "Bearer " + account["access_jwt"]})
    with urllib.request.urlopen(request, timeout=5) as response:
        token = json.load(response)["token"]
    part = token.split(".")[1]
    claims = json.loads(base64.urlsafe_b64decode(part + "=" * (-len(part) % 4)))
    assert (claims["iss"], claims["aud"], claims["lxm"]) == (account["did"], audience, method)
try:
    urllib.request.urlopen(endpoint + "/xrpc/com.atproto.server.createInviteCode", timeout=5)
except urllib.error.HTTPError as error:
    assert error.code == 404
else:
    raise AssertionError("Administrative endpoint reachable from host")
print("PASS: protected accounts, same DIDs and keys, fresh sessions, private companion tokens after recreation")
PY
