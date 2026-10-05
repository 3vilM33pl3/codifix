#!/usr/bin/env bash
# Invoked as the target user. Only create missing Memory-owned paths. Do not
# chmod/chown existing data, follow relocated directories, or overwrite files.
set -eu
umask 077

USER_HOME="${1:?Expected user home}"
SHARE="${2:?Expected packaged share directory}"
if [[ "$USER_HOME" != /* || "$USER_HOME" == / || "$USER_HOME" == /var/root ]]; then exit 1; fi
for parent in "$USER_HOME" "$USER_HOME/Library" "$USER_HOME/Library/Application Support"; do
  if [[ ! -d "$parent" || -L "$parent" ]]; then
    echo "User Application Support is unavailable or relocated; skipping defaults."
    exit 0
  fi
done
APP_SUPPORT="$USER_HOME/Library/Application Support/memory-layer"
if [[ -e "$APP_SUPPORT" || -L "$APP_SUPPORT" ]]; then
  if [[ ! -d "$APP_SUPPORT" || -L "$APP_SUPPORT" || ! -w "$APP_SUPPORT" ]]; then
    echo "Existing Memory Application Support is not writable; skipping defaults."
    exit 0
  fi
else
  mkdir "$APP_SUPPORT" || exit 0
fi
for directory in log run; do
  path="$APP_SUPPORT/$directory"
  if [[ ! -e "$path" && ! -L "$path" ]]; then mkdir "$path" || true; fi
done

CONFIG="$APP_SUPPORT/memory-layer.toml"
if [[ ! -e "$CONFIG" && ! -L "$CONFIG" && -f "$SHARE/memory-layer.toml.example" ]]; then
  # noclobber also protects an existing file/symlink appearing after the check.
  (set -C; cat "$SHARE/memory-layer.toml.example" > "$CONFIG") || true
fi
ENV_FILE="$APP_SUPPORT/memory-layer.env"
if [[ ! -e "$ENV_FILE" && ! -L "$ENV_FILE" ]]; then
  (set -C; printf '%s\n' \
    '# Shared secrets and overrides for Memory Layer CLI and background services.' \
    '# Optional legacy setup provisions the service token; installation does not.' \
    '# Example: OPENAI_API_KEY=replace-me' > "$ENV_FILE") || true
fi
