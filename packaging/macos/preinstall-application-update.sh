#!/bin/bash
set -eu

# Separate receipt: this update never owns the existing runtime or launcher.
if [[ ${3:-} != / ]]; then
  echo 'Application update requires the current startup volume (/).' >&2
  exit 1
fi
for installed in /usr /usr/local /usr/local/bin /usr/local/share /usr/local/share/memory-layer; do
  if [[ -L $installed ]]; then
    echo 'Symlinked application installation path refused; existing files preserved.' >&2
    exit 1
  fi
done
if [[ ! -f /usr/local/bin/memory || -L /usr/local/bin/memory ]]; then
  echo 'Application update requires an existing native Memory installation.' >&2
  exit 1
fi
