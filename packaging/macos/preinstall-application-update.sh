#!/bin/bash
set -eu

# Separate receipt: this update never owns the existing runtime or launcher.
if [[ ${3:-} != / ]]; then
  echo 'Application update requires the current startup volume (/).' >&2
  exit 1
fi
for installed in /usr /usr/local /usr/local/bin /usr/local/share /usr/local/share/codifix; do
  if [[ -L $installed ]]; then
    echo 'Symlinked application installation path refused; existing files preserved.' >&2
    exit 1
  fi
done
if [[ ! -f /usr/local/bin/codifix || -L /usr/local/bin/codifix ]]; then
  echo 'Application update requires an existing native Codifix installation.' >&2
  exit 1
fi
