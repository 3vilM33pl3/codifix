#!/usr/bin/env bash
# Real native dpkg lifecycle on a fresh disposable GitHub-hosted Linux runner.
# Never invoke on Monolith, the CA, a self-hosted runner or another installation.
set -euo pipefail
if [[ $# != 1 || "$1" != /* || ! -f "$1" || -L "$1" || ${GITHUB_ACTIONS:-} != true || ${RUNNER_OS:-} != Linux || ${RUNNER_ENVIRONMENT:-} != github-hosted || ${MEMORY_HOSTED_INSTALL_TEST:-} != yes || $(id -u) != 0 ]]; then
  echo 'Requires an explicitly disposable GitHub-hosted Linux runner and root.' >&2
  echo 'Usage: sudo --preserve-env=GITHUB_ACTIONS,RUNNER_OS,RUNNER_ENVIRONMENT env MEMORY_HOSTED_INSTALL_TEST=yes bash install-test.sh ABSOLUTE_CANDIDATE_DEB' >&2
  exit 2
fi
package=$1
arch=$(dpkg-deb -f "$package" Architecture)
version=$(dpkg-deb -f "$package" Version)
[[ $(dpkg --print-architecture) == "$arch" ]]
[[ $version == 2.1.0 ]]
[[ ! -e /usr/bin/memory && ! -e /etc/memory-layer && ! -e /var/lib/memory-layer ]]
[[ -z $(dpkg-query -W -f='${Version}' memory-layer 2>/dev/null || true) ]]
work=$(mktemp -d /tmp/memory-debian-install.XXXXXXXX)
mkdir "$work/bin"
# Real dpkg and native package hooks run, but no service manager is contacted.
printf '#!/bin/sh\nprintf "%%s\\n" "$*" >> "$MEMORY_SERVICE_CALLS"\ncase "$1" in daemon-reload|stop) exit 0;; *) exit 99;; esac\n' > "$work/bin/systemctl"
chmod 755 "$work/bin/systemctl"
export MEMORY_SERVICE_CALLS="$work/systemctl-calls"
export PATH="$work/bin:/usr/sbin:/usr/bin:/sbin:/bin"
export DEBIAN_FRONTEND=noninteractive
check_binary() {
  [[ $(/usr/bin/memory --version) == 'memory 2.1.0' ]]
  for executable in memory memory-area memory-area-import memory-federation-gateway; do
    "/usr/bin/$executable" --help > /dev/null
  done
  test -s /usr/share/memory-layer/web/index.html
  test -s /usr/libexec/memory-layer/releases/release.py
}
# Fresh candidate installation on the native runner, no dependency bypasses.
dpkg -i "$package"
check_binary
dpkg --purge memory-layer
mkdir -p "$work/prior/DEBIAN" "$work/prior/usr/bin" "$work/prior/etc/memory-layer"
dpkg-deb -e "$package" "$work/candidate-control"
printf 'Package: memory-layer\nVersion: 2.0.1\nArchitecture: %s\nMaintainer: Synthetic acceptance fixture <nobody@example.invalid>\nDescription: synthetic prior-version lifecycle fixture, not a historic released artifact\n' "$arch" > "$work/prior/DEBIAN/control"
cp "$work/candidate-control/postinst" "$work/candidate-control/prerm" "$work/prior/DEBIAN/"
printf '/etc/memory-layer/memory-layer.toml\n' > "$work/prior/DEBIAN/conffiles"
printf '#!/bin/sh\necho synthetic-prior-version\n' > "$work/prior/usr/bin/memory"
chmod 755 "$work/prior/usr/bin/memory"
printf '# preserved operator configuration\n[service]\nbind_addr = "127.0.0.1:18371"\n' > "$work/prior/etc/memory-layer/memory-layer.toml"
dpkg-deb --root-owner-group --build "$work/prior" "$work/prior.deb"
dpkg -i "$work/prior.deb"
printf '\n# operator changed this after installing the prior package\n' >> /etc/memory-layer/memory-layer.toml
printf 'retain local runtime data\n' > /var/lib/memory-layer/acceptance-sentinel
before=$(sha256sum /etc/memory-layer/memory-layer.toml /var/lib/memory-layer/acceptance-sentinel)
dpkg --force-confold -i "$package"
[[ $(dpkg-query -W -f='${Version}' memory-layer) == "$version" ]]
[[ $(sha256sum /etc/memory-layer/memory-layer.toml /var/lib/memory-layer/acceptance-sentinel) == "$before" ]]
check_binary
# Repair the exact installed version without replacing operator state.
dpkg --force-confold -i "$package"
[[ $(sha256sum /etc/memory-layer/memory-layer.toml /var/lib/memory-layer/acceptance-sentinel) == "$before" ]]
dpkg -r memory-layer
[[ ! -e /usr/bin/memory ]]
[[ $(sha256sum /etc/memory-layer/memory-layer.toml /var/lib/memory-layer/acceptance-sentinel) == "$before" ]]
if grep -Ev '^(daemon-reload|stop memory-watch.service memory-layer.service)$' "$MEMORY_SERVICE_CALLS"; then
  echo 'Unexpected service activation during installation or repair' >&2
  exit 1
fi
printf 'PASS: native %s fresh install, synthetic-2.0.1-to-%s upgrade, same-version repair, configuration/data preservation, removal; no service started.\n' "$arch" "$version"
printf 'LIMIT: Ubuntu-hosted package lifecycle and native launch only; Debian 12 ABI is verified separately. No database provisioning or running service acceptance is claimed.\n'
