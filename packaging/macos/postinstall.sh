#!/usr/bin/env bash
# Installer runs as root. Seed only as a verified non-root user, never start or
# restart services and never rewrite an existing user's configuration/ownership.
set -u

CONSOLE_USER="$(stat -f %Su /dev/console 2>/dev/null || true)"
case "$CONSOLE_USER" in
  ""|root|loginwindow|_mbsetupuser) CONSOLE_USER="${SUDO_USER:-}" ;;
esac
case "$CONSOLE_USER" in
  ""|root|loginwindow|_mbsetupuser)
    echo "No logged-in user found; per-user Codifix setup was not changed."
    exit 0
    ;;
esac
CONSOLE_UID="$(id -u "$CONSOLE_USER" 2>/dev/null || true)"
CONSOLE_HOME="$(dscl . -read "/Users/$CONSOLE_USER" NFSHomeDirectory 2>/dev/null | sed -n 's/^NFSHomeDirectory: //p')"
if [[ ! "$CONSOLE_UID" =~ ^[0-9]+$ || "$CONSOLE_UID" == 0 || "$CONSOLE_HOME" != /* || \
  "$CONSOLE_HOME" == / || "$CONSOLE_HOME" == /var/root || ! -d "$CONSOLE_HOME" ]]; then
  echo "Could not verify the console user's home; per-user Codifix setup was not changed."
  exit 0
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if ! sudo -u "$CONSOLE_USER" -- /bin/bash "$SCRIPT_DIR/seed-user.sh" \
  "$CONSOLE_HOME" /usr/local/share/codifix; then
  echo "Per-user defaults could not be seeded; existing settings were preserved."
fi
echo "Codifix installed; no services were started or restarted."
echo "For optional legacy local setup, run: codifix wizard --global"
echo "Company device login and project setup require separate explicit approval."
