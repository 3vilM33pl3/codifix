#!/usr/bin/env bash
# Native package acceptance. Run only on disposable GitHub macOS runners.
set -euo pipefail
[[ ${GITHUB_ACTIONS:-} == true && ${RUNNER_OS:-} == macOS ]] || {
  echo 'Refusing package installation outside a disposable GitHub macOS runner.' >&2; exit 1;
}
[[ $# == 4 ]] || { echo 'Usage: check-release.sh ARCH VERSION STANDARD.pkg UPDATE.pkg' >&2; exit 1; }
arch=$1; version=$2; standard=$3; update=$4
case "$arch" in aarch64) native_arch=arm64 ;; x86_64) native_arch=x86_64 ;; *) exit 1 ;; esac
stage=$(mktemp -d "${TMPDIR%/}/memory-release-check.XXXXXXXX")
trap 'rm -rf -- "$stage"' EXIT
pkgutil --expand-full "$standard" "$stage/standard"
pkgutil --expand-full "$update" "$stage/update"
for variant in standard update; do
  binary="$stage/$variant/Payload/usr/local/bin/memory"
  [[ -x "$binary" && -f "$stage/$variant/Payload/usr/local/share/memory-layer/web/index.html" ]]
  lipo -verify_arch "$native_arch" "$binary"
  [[ $("$binary" --version) == "memory $version" ]]
  "$binary" --help >/dev/null
  # Standalone package executables must not resolve libraries from build hosts.
  otool -L "$binary" > "$stage/$variant-libraries.txt"
  if grep -E '/(opt/homebrew|usr/local/opt|Users/runner|target)/' "$stage/$variant-libraries.txt"; then
    echo 'Unbundled build-host dependency detected.' >&2; exit 1
  fi
done
cmp "$stage/standard/Payload/usr/local/bin/memory" "$stage/update/Payload/usr/local/bin/memory"
diff -r "$stage/standard/Payload/usr/local/share/memory-layer/web" "$stage/update/Payload/usr/local/share/memory-layer/web"
sudo installer -pkg "$standard" -target /
[[ $(/usr/local/bin/memory --version) == "memory $version" ]]
# Sentinel-only runtime preservation check, not a separately receipted runtime
# installation: updates must preserve these bytes and the existing launcher.
# No dependency is executed and this is not full runtime-upgrade acceptance.
runtime=/usr/local/share/memory-layer/runtime/release-check
sudo mkdir -p "$runtime"
printf 'preserve independently owned runtime\n' | sudo tee "$runtime/sentinel" >/dev/null
printf '#!/bin/sh\nexit 0\n' | sudo tee /usr/local/bin/memory-onboard >/dev/null
sudo chmod 755 /usr/local/bin/memory-onboard
sudo installer -pkg "$update" -target /
[[ $(cat "$runtime/sentinel") == 'preserve independently owned runtime' ]]
[[ -x /usr/local/bin/memory-onboard ]]
[[ $(/usr/local/bin/memory --version) == "memory $version" ]]
/usr/local/bin/memory federation --help >/dev/null
echo 'Native macOS standard installation and runtime-preserving update passed.'
