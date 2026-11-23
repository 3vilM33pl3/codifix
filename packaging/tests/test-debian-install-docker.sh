#!/usr/bin/env bash
# Native dpkg fresh/upgrade/repair/removal test inside an existing disposable image.
set -euo pipefail

if [[ ${1:-} == --container ]]; then
  package=$2
  if [[ $# != 2 || ! -f /.dockerenv || "$0" != /harness.sh || "$package" != /input/candidate.deb || ! -f "$package" || -L "$package" ]]; then
    echo 'Refusing lifecycle mutation outside the expected Docker container.' >&2
    exit 2
  fi
  work=$(mktemp -d /tmp/codifix-dpkg-test.XXXXXXXX)
  trap 'rm -rf -- "$work"' EXIT
  mkdir -p "$work/bin"
  cat > "$work/bin/systemctl" <<'SYSTEMCTL'
#!/bin/sh
printf '%s\n' "$*" >> "$CODIFIX_SERVICE_CALLS"
case "$*" in
  daemon-reload|"stop codifix-watch.service codifix.service") exit 0 ;;
  *) echo "Forbidden service action: $*" >&2; exit 99 ;;
esac
SYSTEMCTL
  chmod 0755 "$work/bin/systemctl"
  export CODIFIX_SERVICE_CALLS="$work/service-calls"
  export PATH="$work/bin:/usr/sbin:/usr/bin:/sbin:/bin"
  export DEBIAN_FRONTEND=noninteractive
  arch=$(dpkg-deb -f "$package" Architecture)
  version=$(dpkg-deb -f "$package" Version)
  [[ $(dpkg --print-architecture) == "$arch" ]]
  [[ $version =~ ^3\.[01]\.0(~rc\.[1-9][0-9]{0,5})?$ ]]
  [[ ! -e /usr/bin/codifix && ! -e /etc/codifix && ! -e /var/lib/codifix ]]
  application_version=${version//\~/-}
  check_binary() {
    [[ $(/usr/bin/codifix --version) == "codifix $application_version" ]]
    /usr/bin/codifix-area --help >/dev/null
    /usr/bin/codifix-area-import --help >/dev/null
    /usr/bin/codifix-agency --help >/dev/null
    /usr/bin/codifix-agency-openbao --help >/dev/null
    /usr/libexec/codifix/agency/release-rollout.py --help >/dev/null
    test -s /usr/libexec/codifix/agency/ssh-renew.py
    test -s /usr/share/doc/codifix/agency/ssh-host-renewal.md
    test -s /usr/share/doc/codifix/agency/openbao.md
    test -x /usr/libexec/codifix/agency/inspect-systemd.py
    test -s /usr/share/codifix/agency/incidents.json.example
    test -s /usr/share/doc/codifix/agency/incident-response.md
    test ! -e /lib/systemd/system/codifix-agency.service
    test -s /usr/share/codifix/agency/codifix-agency.service.example
  }

  dpkg -i "$package"
  check_binary
  test -s /usr/share/codifix/agency/config.json.example
  dpkg --purge codifix

  # Create a synthetic prior package from this candidate's maintainer hooks.
  mkdir -p "$work/prior/DEBIAN" "$work/prior/usr/bin" "$work/prior/etc/codifix"
  dpkg-deb -e "$package" "$work/control"
  printf 'Package: codifix\nVersion: 3.0.0\nArchitecture: %s\nMaintainer: Disposable fixture <nobody@example.invalid>\nDescription: synthetic prior package\n' "$arch" > "$work/prior/DEBIAN/control"
  cp "$work/control/postinst" "$work/control/prerm" "$work/prior/DEBIAN/"
  printf '/etc/codifix/codifix.toml\n' > "$work/prior/DEBIAN/conffiles"
  printf '#!/bin/sh\necho synthetic-prior-version\n' > "$work/prior/usr/bin/codifix"
  chmod 0755 "$work/prior/usr/bin/codifix"
  printf '# prior operator configuration\n[service]\nbind_addr = "127.0.0.1:18371"\n' > "$work/prior/etc/codifix/codifix.toml"
  dpkg-deb --root-owner-group --build "$work/prior" "$work/prior.deb" >/dev/null
  dpkg -i "$work/prior.deb"
  printf '\n# preserved operator setting\n' >> /etc/codifix/codifix.toml
  printf 'retain local runtime data\n' > /var/lib/codifix/acceptance-sentinel
  before=$(sha256sum /etc/codifix/codifix.toml /var/lib/codifix/acceptance-sentinel)

  dpkg --force-confold -i "$package"
  [[ $(dpkg-query -W -f='${Version}' codifix) == "$version" ]]
  [[ $(sha256sum /etc/codifix/codifix.toml /var/lib/codifix/acceptance-sentinel) == "$before" ]]
  check_binary
  dpkg --force-confold -i "$package"
  [[ $(sha256sum /etc/codifix/codifix.toml /var/lib/codifix/acceptance-sentinel) == "$before" ]]
  check_binary
  dpkg -r codifix
  [[ ! -e /usr/bin/codifix ]]
  [[ $(sha256sum /etc/codifix/codifix.toml /var/lib/codifix/acceptance-sentinel) == "$before" ]]
  if grep -Ev '^(daemon-reload|stop codifix-watch.service codifix.service)$' "$CODIFIX_SERVICE_CALLS"; then
    echo 'Unexpected service activation during package lifecycle' >&2
    exit 1
  fi
  grep -Fxq 'daemon-reload' "$CODIFIX_SERVICE_CALLS"
  grep -Fxq 'stop codifix-watch.service codifix.service' "$CODIFIX_SERVICE_CALLS"
  echo "PASS: container fresh install, 3.0.0-to-$version upgrade, repair and removal preserve operator config/data; no service started"
  exit 0
fi

if [[ $# != 2 || "$1" != sha256:* || ! "$2" == /* || ! -f "$2" || -L "$2" ]]; then
  echo 'Usage: test-debian-install-docker.sh sha256:LOCAL_IMAGE_ID /absolute/candidate.deb' >&2
  exit 2
fi
image=$1
package=$(realpath -e -- "$2")
[[ "$package" != *,* && "$package" != *$'\n'* ]] || { echo 'Invalid package path.' >&2; exit 2; }
[[ $(docker image inspect --format '{{.Id}}' "$image") == "$image" ]] || {
  echo 'The exact disposable image must already exist locally.' >&2
  exit 2
}
script=$(realpath -e -- "${BASH_SOURCE[0]}")
docker run --rm --pull never --network none --user 0:0 --cap-add CHOWN \
  --security-opt no-new-privileges --pids-limit 256 --memory 2g \
  --mount "type=bind,src=$package,dst=/input/candidate.deb,readonly" \
  --mount "type=bind,src=$script,dst=/harness.sh,readonly" \
  "$image" /bin/bash /harness.sh --container /input/candidate.deb
