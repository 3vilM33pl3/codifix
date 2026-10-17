#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CARGO_TARGET_ROOT="${CARGO_TARGET_DIR:-$ROOT_DIR/target}"
if [[ "$CARGO_TARGET_ROOT" != /* ]]; then
  CARGO_TARGET_ROOT="$PWD/$CARGO_TARGET_ROOT"
fi
INSTALL_ROOT="${INSTALL_ROOT:-$HOME/.local}"
BIN_DIR="$INSTALL_ROOT/bin"
if [[ "$(uname -s)" == "Darwin" ]]; then
  CONFIG_DIR="$HOME/Library/Application Support/codifix"
  SHARE_DIR="$CONFIG_DIR"
  SERVICE_STEP="$BIN_DIR/codifix service enable"
else
  CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/codifix"
  SHARE_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/codifix"
  SERVICE_STEP="$BIN_DIR/codifix service run --config $CONFIG_DIR/codifix.toml"
fi
ENV_FILE="$CONFIG_DIR/codifix.env"
SKILL_TEMPLATE_DIR="$SHARE_DIR/skill-template"
WEB_DIR="$SHARE_DIR/web"
COMPLETION_SHARE_DIR="${XDG_DATA_HOME:-$HOME/.local/share}"
BASH_COMPLETION_DIR="$COMPLETION_SHARE_DIR/bash-completion/completions"
ZSH_COMPLETION_DIR="$COMPLETION_SHARE_DIR/zsh/site-functions"
FISH_COMPLETION_DIR="$COMPLETION_SHARE_DIR/fish/vendor_completions.d"

mkdir -p \
  "$BIN_DIR" \
  "$CONFIG_DIR" \
  "$SKILL_TEMPLATE_DIR" \
  "$WEB_DIR" \
  "$BASH_COMPLETION_DIR" \
  "$ZSH_COMPLETION_DIR" \
  "$FISH_COMPLETION_DIR"

echo "Building release binaries..."
cargo build --release --manifest-path "$ROOT_DIR/Cargo.toml" --bin codifix
echo "Building web UI..."
npm --prefix "$ROOT_DIR/web" ci
npm --prefix "$ROOT_DIR/web" run build

install -m 0755 "$CARGO_TARGET_ROOT/release/codifix" "$BIN_DIR/codifix"
"$BIN_DIR/codifix" completion bash > "$BASH_COMPLETION_DIR/codifix"
"$BIN_DIR/codifix" completion zsh > "$ZSH_COMPLETION_DIR/_codifix"
"$BIN_DIR/codifix" completion fish > "$FISH_COMPLETION_DIR/codifix.fish"
rm -rf "$SKILL_TEMPLATE_DIR"
mkdir -p "$SKILL_TEMPLATE_DIR"
cp -R "$ROOT_DIR/.agents/skills/." "$SKILL_TEMPLATE_DIR/"
find "$SKILL_TEMPLATE_DIR" -type f -name '*.sh' -exec chmod 0755 {} +
rm -rf "$WEB_DIR"
mkdir -p "$WEB_DIR"
cp -R "$ROOT_DIR/web/dist/." "$WEB_DIR/"

if [[ ! -f "$CONFIG_DIR/codifix.toml" ]]; then
  install -m 0644 "$ROOT_DIR/codifix.toml.example" "$CONFIG_DIR/codifix.toml"
  if [[ "$(uname -s)" == "Darwin" ]]; then
    mkdir -p "$CONFIG_DIR/run"
    perl -0pi -e "s|capnp_unix_socket = \"/tmp/codifix\\.capnp\\.sock\"|capnp_unix_socket = \"$CONFIG_DIR/run/codifix.capnp.sock\"|" \
      "$CONFIG_DIR/codifix.toml"
  fi
  echo "Installed default config at $CONFIG_DIR/codifix.toml"
else
  echo "Keeping existing config at $CONFIG_DIR/codifix.toml"
fi

if [[ ! -f "$ENV_FILE" ]]; then
  cat > "$ENV_FILE" <<'EOF'
# Shared secrets and overrides for Codifix CLI and background services.
# The service API token is provisioned automatically during setup.
# Example:
# OPENAI_API_KEY=replace-me
EOF
  chmod 600 "$ENV_FILE"
  echo "Installed default environment file at $ENV_FILE"
else
  echo "Keeping existing environment file at $ENV_FILE"
fi

"$BIN_DIR/codifix" --config "$CONFIG_DIR/codifix.toml" service ensure-api-token --rotate-placeholder >/dev/null
echo "Ensured shared service API token at $ENV_FILE"

cat <<EOF

Installed:
  $BIN_DIR/codifix
  $SKILL_TEMPLATE_DIR
  $WEB_DIR
  $BASH_COMPLETION_DIR/codifix
  $ZSH_COMPLETION_DIR/_codifix
  $FISH_COMPLETION_DIR/codifix.fish

Next steps:
1. In each repo, run:
   $BIN_DIR/codifix wizard
2. Optional: if you want to configure shared/global defaults too, open:
   $CONFIG_DIR/codifix.toml
3. Optional: shared env file path:
   $ENV_FILE
4. Start the backend:
   $SERVICE_STEP
5. Optional: enable the automation watcher user service:
   $BIN_DIR/codifix watcher enable --project <slug>
6. Launch the TUI:
   $BIN_DIR/codifix tui
7. Open the web UI:
   http://127.0.0.1:4040/

Shell completion:
  bash and fish usually pick up the installed files in a new shell.
  For zsh, add this before compinit if needed:
    fpath=("$ZSH_COMPLETION_DIR" \$fpath)

EOF
