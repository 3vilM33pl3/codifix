#!/usr/bin/env bash
# Real native dpkg lifecycle on a fresh disposable GitHub-hosted Linux runner.
# Never invoke on Monolith, the CA, a self-hosted runner or another installation.
set -euo pipefail
if [[ $# != 1 || "$1" != /* || ! -f "$1" || -L "$1" || ${GITHUB_ACTIONS:-} != true || ${RUNNER_OS:-} != Linux || ${RUNNER_ENVIRONMENT:-} != github-hosted || ${CODIFIX_HOSTED_INSTALL_TEST:-} != yes || $(id -u) != 0 ]]; then
  echo 'Requires an explicitly disposable GitHub-hosted Linux runner and root.' >&2
  echo 'Usage: sudo --preserve-env=GITHUB_ACTIONS,RUNNER_OS,RUNNER_ENVIRONMENT env CODIFIX_HOSTED_INSTALL_TEST=yes bash install-test.sh ABSOLUTE_CANDIDATE_DEB' >&2
  exit 2
fi
package=$1
arch=$(dpkg-deb -f "$package" Architecture)
version=$(dpkg-deb -f "$package" Version)
[[ $(dpkg --print-architecture) == "$arch" ]]
[[ $version =~ ^3\.0\.0(~rc\.[1-9][0-9]{0,5})?$ ]]
# Escape the pattern: an unquoted bare tilde expands to HOME in Bash.
application_version=${version/\~/-}
[[ ! -e /usr/bin/codifix && ! -e /etc/codifix && ! -e /var/lib/codifix ]]
[[ -z $(dpkg-query -W -f='${Version}' codifix 2>/dev/null || true) ]]
work=$(mktemp -d /tmp/codifix-debian-install.XXXXXXXX)
mkdir "$work/bin"
# Real dpkg and native package hooks run, but no service manager is contacted.
printf '#!/bin/sh\nprintf "%%s\\n" "$*" >> "$CODIFIX_SERVICE_CALLS"\ncase "$1" in daemon-reload|stop) exit 0;; *) exit 99;; esac\n' > "$work/bin/systemctl"
chmod 755 "$work/bin/systemctl"
export CODIFIX_SERVICE_CALLS="$work/systemctl-calls"
export PATH="$work/bin:/usr/sbin:/usr/bin:/sbin:/bin"
export DEBIAN_FRONTEND=noninteractive
check_binary() {
  [[ $(/usr/bin/codifix --version) == "codifix $application_version" ]]
  for executable in codifix codifix-area codifix-area-import; do
    "/usr/bin/$executable" --help > /dev/null
  done
  # Gateway has positional private-config arguments, not general --help flags.
  local gateway_status=0 gateway_output
  gateway_output=$(timeout 15s /usr/bin/codifix-federation-gateway 2>&1) || gateway_status=$?
  [[ $gateway_status == 1 && $gateway_output == 'Error: "expected private gateway configuration file"' ]]
  test -s /usr/share/codifix/web/index.html
  test -s /usr/libexec/codifix/releases/release.py
}
# Fresh candidate installation on the native runner, no dependency bypasses.
dpkg -i "$package"
check_binary
dpkg --purge codifix
mkdir -p "$work/prior/DEBIAN" "$work/prior/usr/bin" "$work/prior/etc/codifix"
dpkg-deb -e "$package" "$work/candidate-control"
printf 'Package: codifix\nVersion: 2.0.1\nArchitecture: %s\nMaintainer: Synthetic acceptance fixture <nobody@example.invalid>\nDescription: synthetic prior-version lifecycle fixture, not a historic released artifact\n' "$arch" > "$work/prior/DEBIAN/control"
cp "$work/candidate-control/postinst" "$work/candidate-control/prerm" "$work/prior/DEBIAN/"
printf '/etc/codifix/codifix.toml\n' > "$work/prior/DEBIAN/conffiles"
printf '#!/bin/sh\necho synthetic-prior-version\n' > "$work/prior/usr/bin/codifix"
chmod 755 "$work/prior/usr/bin/codifix"
printf '# preserved operator configuration\n[service]\nbind_addr = "127.0.0.1:18371"\n' > "$work/prior/etc/codifix/codifix.toml"
dpkg-deb --root-owner-group --build "$work/prior" "$work/prior.deb"
dpkg -i "$work/prior.deb"
printf '\n# operator changed this after installing the prior package\n' >> /etc/codifix/codifix.toml
printf 'retain local runtime data\n' > /var/lib/codifix/acceptance-sentinel
before=$(sha256sum /etc/codifix/codifix.toml /var/lib/codifix/acceptance-sentinel)
dpkg --force-confold -i "$package"
[[ $(dpkg-query -W -f='${Version}' codifix) == "$version" ]]
[[ $(sha256sum /etc/codifix/codifix.toml /var/lib/codifix/acceptance-sentinel) == "$before" ]]
check_binary
# Repair the exact installed version without replacing operator state.
dpkg --force-confold -i "$package"
[[ $(sha256sum /etc/codifix/codifix.toml /var/lib/codifix/acceptance-sentinel) == "$before" ]]
dpkg -r codifix
[[ ! -e /usr/bin/codifix ]]
[[ $(sha256sum /etc/codifix/codifix.toml /var/lib/codifix/acceptance-sentinel) == "$before" ]]
if grep -Ev '^(daemon-reload|stop codifix-watch.service codifix.service)$' "$CODIFIX_SERVICE_CALLS"; then
  echo 'Unexpected service activation during installation or repair' >&2
  exit 1
fi
printf 'PASS: native %s fresh install, synthetic-2.0.1-to-%s upgrade, same-version repair, configuration/data preservation, removal; no service started.\n' "$arch" "$version"
printf 'LIMIT: Ubuntu-hosted package lifecycle and native launch only; Debian 12 ABI is verified separately. No database provisioning or running service acceptance is claimed.\n'
