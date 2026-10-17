#!/bin/bash
# SPDX-License-Identifier: AGPL-3.0-or-later
set -euo pipefail
[[ $(uname -s) == Darwin ]] || { echo 'Requires native macOS.' >&2; exit 1; }
output=${1:?Usage: build-network-access.sh NEW_APP_PATH}
[[ "$output" == /* && "$output" == *.app && ! -e "$output" ]] || { echo 'Choose a new absolute .app path; existing apps are preserved.' >&2; exit 1; }
source_dir=$(cd -- "$(dirname -- "$0")" && pwd)
architecture=${2:-$(uname -m)}
case "$architecture" in aarch64|arm64) architecture=arm64;; x86_64) ;; *) echo 'Unsupported Mac architecture.' >&2; exit 1;; esac
mkdir -p "$output/Contents/MacOS"
xcrun clang -arch "$architecture" -fobjc-arc -O2 -Wall -Wextra -Wno-unused-parameter -framework AppKit -framework Foundation \
  "$source_dir/network-access.m" -o "$output/Contents/MacOS/codifix-network-access"
plist="$output/Contents/Info.plist"
plutil -create xml1 "$plist"
/usr/libexec/PlistBuddy -c 'Add :CFBundleIdentifier string dev.codifix.network-access' "$plist"
/usr/libexec/PlistBuddy -c 'Add :CFBundleName string Codifix Network Access' "$plist"
/usr/libexec/PlistBuddy -c 'Add :CFBundleExecutable string codifix-network-access' "$plist"
/usr/libexec/PlistBuddy -c 'Add :CFBundlePackageType string APPL' "$plist"
/usr/libexec/PlistBuddy -c 'Add :CFBundleVersion string 1' "$plist"
/usr/libexec/PlistBuddy -c 'Add :NSLocalNetworkUsageDescription string Synchronize the Codifix projects you selected with your company server.' "$plist"
codesign --force --sign - "$output"
codesign --verify --strict "$output"
