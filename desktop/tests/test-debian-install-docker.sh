#!/usr/bin/env bash
# Native dpkg lifecycle acceptance in a disposable Ubuntu image; never install on host.
set -euo pipefail

if [[ ${1:-} == --container ]]; then
  package=${2:-}
  if [[ $# != 2 || ! -f /.dockerenv || "$0" != /harness.sh || "$package" != /input/candidate.deb || ! -f "$package" || -L "$package" ]]; then
    echo 'Refusing desktop package lifecycle mutation outside the expected Docker container.' >&2
    exit 2
  fi
  work=$(mktemp -d /tmp/codifix-desktop-dpkg.XXXXXXXX)
  trap 'rm -rf -- "$work"' EXIT
  mkdir -p "$work/bin"
  for command_name in systemctl service start-stop-daemon; do
    cat > "$work/bin/$command_name" <<'STUB'
#!/bin/sh
printf '%s %s\n' "$(basename "$0")" "$*" >> "$CODIFIX_FORBIDDEN_CALLS"
echo "Forbidden service/process action: $(basename "$0") $*" >&2
exit 99
STUB
    chmod 0755 "$work/bin/$command_name"
  done
  export CODIFIX_FORBIDDEN_CALLS="$work/forbidden-calls"
  export PATH="$work/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"
  export DEBIAN_FRONTEND=noninteractive

  arch=$(dpkg-deb -f "$package" Architecture)
  version=$(dpkg-deb -f "$package" Version)
  package_name=$(dpkg-deb -f "$package" Package)
  [[ $package_name == codifix-desktop && $arch == amd64 && $version == 0.2.0~rc.1 ]]
  dpkg --compare-versions "$version" gt 0.1.4
  dpkg --compare-versions "$version" lt 0.2.0
  [[ ! -e /usr/bin/codifix-desktop ]]
  [[ -z $(dpkg-query -W -f='${Version}' codifix-desktop 2>/dev/null || true) ]]
  mkdir "$work/control" "$work/candidate"
  dpkg-deb -e "$package" "$work/control"
  # These packages ship no lifecycle hooks; package installation cannot start a GUI or service.
  [[ -z $(find "$work/control" -mindepth 1 -maxdepth 1 ! -name control ! -name md5sums -print -quit) ]]
  dpkg-deb -x "$package" "$work/candidate"
  [[ -x "$work/candidate/usr/bin/codifix-desktop" ]]
  [[ -s "$work/candidate/usr/share/applications/Codifix Desktop.desktop" ]]
  [[ -s "$work/candidate/usr/share/icons/hicolor/128x128/apps/codifix-desktop.png" ]]
  ldd -r "$work/candidate/usr/bin/codifix-desktop" > "$work/ldd.txt" 2>&1
  if grep -Eq 'not found|undefined symbol' "$work/ldd.txt"; then
    cat "$work/ldd.txt" >&2
    echo 'Desktop binary has unresolved runtime libraries or symbols in the target image.' >&2
    exit 1
  fi

  dpkg -i "$package"
  [[ $(dpkg-query -W -f='${Version}' codifix-desktop) == "$version" ]]
  cmp "$work/candidate/usr/bin/codifix-desktop" /usr/bin/codifix-desktop
  [[ -s /usr/share/applications/Codifix\ Desktop.desktop ]]
  [[ -s /usr/share/icons/hicolor/128x128/apps/codifix-desktop.png ]]
  dpkg --purge codifix-desktop

  mkdir -p "$work/prior/DEBIAN" "$work/prior/usr/bin"
  printf 'Package: codifix-desktop\nVersion: 0.1.4\nArchitecture: amd64\nMaintainer: Disposable fixture <nobody@example.invalid>\nDescription: synthetic previous desktop package\n' > "$work/prior/DEBIAN/control"
  printf '#!/bin/sh\nexit 0\n' > "$work/prior/usr/bin/codifix-desktop"
  chmod 0755 "$work/prior/usr/bin/codifix-desktop"
  dpkg-deb --root-owner-group --build "$work/prior" "$work/prior.deb" >/dev/null
  dpkg -i "$work/prior.deb"
  useradd --create-home --shell /usr/sbin/nologin codifix-acceptance
  user_home=/home/codifix-acceptance
  mkdir -p "$user_home/.config/com.codifix.desktop" "$user_home/.local/share/com.codifix.desktop"
  printf 'endpoint = "http://127.0.0.1:4040"\n' > "$user_home/.config/com.codifix.desktop/config.toml"
  printf 'keep this local desktop data\n' > "$user_home/.local/share/com.codifix.desktop/state.json"
  chown -R codifix-acceptance:codifix-acceptance "$user_home/.config" "$user_home/.local"
  before=$(sha256sum "$user_home/.config/com.codifix.desktop/config.toml" "$user_home/.local/share/com.codifix.desktop/state.json")

  dpkg --force-confold -i "$package"
  [[ $(dpkg-query -W -f='${Version}' codifix-desktop) == "$version" ]]
  cmp "$work/candidate/usr/bin/codifix-desktop" /usr/bin/codifix-desktop
  [[ $(sha256sum "$user_home/.config/com.codifix.desktop/config.toml" "$user_home/.local/share/com.codifix.desktop/state.json") == "$before" ]]
  dpkg --force-confold -i "$package"
  [[ $(sha256sum "$user_home/.config/com.codifix.desktop/config.toml" "$user_home/.local/share/com.codifix.desktop/state.json") == "$before" ]]
  dpkg -r codifix-desktop
  [[ ! -e /usr/bin/codifix-desktop ]]
  [[ $(sha256sum "$user_home/.config/com.codifix.desktop/config.toml" "$user_home/.local/share/com.codifix.desktop/state.json") == "$before" ]]
  [[ ! -s "$CODIFIX_FORBIDDEN_CALLS" ]]
  echo 'PASS: isolated fresh desktop install, synthetic 0.1.4 upgrade, repair and removal preserve test-user XDG config/data; package hooks/services/GUI were not started.'
  exit 0
fi

if [[ $# != 2 || "$1" != sha256:* || "$2" != /* || ! -f "$2" || -L "$2" ]]; then
  echo 'Usage: test-debian-install-docker.sh sha256:LOCAL_IMAGE_ID /absolute/codifix-desktop_0.2.0~rc.1_amd64.deb' >&2
  exit 2
fi
image=$1
package=$(realpath -e -- "$2")
[[ "$package" != *,* && "$package" != *$'\n'* ]] || { echo 'Invalid package path.' >&2; exit 2; }
[[ $(docker image inspect --format '{{.Id}}' "$image") == "$image" ]] || {
  echo 'The exact desktop lifecycle image must already exist locally.' >&2
  exit 2
}
script=$(realpath -e -- "${BASH_SOURCE[0]}")
docker run --rm --pull never --network none --user 0:0 --cap-add CHOWN \
  --security-opt no-new-privileges --pids-limit 256 --memory 3g \
  --mount "type=bind,src=$package,dst=/input/candidate.deb,readonly" \
  --mount "type=bind,src=$script,dst=/harness.sh,readonly" \
  "$image" /bin/bash /harness.sh --container /input/candidate.deb
