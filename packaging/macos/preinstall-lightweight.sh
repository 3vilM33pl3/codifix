#!/bin/bash
set -eu

# Installer can remove receipt-owned files omitted by a same-ID replacement.
# A lightweight payload must not replace a bundled runtime installation.
if [[ ${3:-} != / ]]; then
  echo 'Lightweight package requires the current startup volume (/).' >&2
  exit 1
fi
for installed in /usr/local/share/codifix/runtime /usr/local/bin/codifix-onboard; do
  if [[ -e $installed || -L $installed ]]; then
    echo 'Lightweight package cannot replace a bundled-runtime installation; use a reviewed runtime-inclusive package. Existing files preserved.' >&2
    exit 1
  fi
done
