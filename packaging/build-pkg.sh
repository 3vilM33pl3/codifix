#!/usr/bin/env bash
set -euo pipefail

# Build a macOS .pkg installer for Memory Layer.
# Usage:
#   ./packaging/build-pkg.sh
#   ./packaging/build-pkg.sh --arch x86_64
#   ./packaging/build-pkg.sh --arch aarch64
#   ./packaging/build-pkg.sh --runtime-bundle /ABS/REVIEWED/TREE \
#     --runtime-manifest /ABS/REVIEWED/manifest.json
#   ./packaging/build-pkg.sh \
#     --sign-app "Developer ID Application: ..." \
#     --sign-pkg "Developer ID Installer: ..."
#   ./packaging/build-pkg.sh \
#     --sign-app "Developer ID Application: ..." \
#     --sign-pkg "Developer ID Installer: ..." \
#     --notarize --notary-profile "<profile>"
#
# The resulting .pkg installs:
#   /usr/local/bin/memory          (+ mem-cli and memory-layer symlinks)
#   /usr/local/share/memory-layer/ (web UI, skill templates, example config)
#   /usr/local/share/*             (bash, zsh, and fish completion scripts)

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CARGO_TARGET_ROOT="${CARGO_TARGET_DIR:-$ROOT_DIR/target}"
if [[ "$CARGO_TARGET_ROOT" != /* ]]; then
  CARGO_TARGET_ROOT="$PWD/$CARGO_TARGET_ROOT"
fi
VERSION="$(awk -F '"' '/^version = / { print $2; exit }' "$ROOT_DIR/Cargo.toml" 2>/dev/null || true)"
if [[ -z "${VERSION:-}" ]]; then
  VERSION="0.1.0"
fi

APP_SIGN_IDENTITY=""
PKG_SIGN_IDENTITY=""
NOTARIZE=0
NOTARY_PROFILE=""
APPLE_ID=""
TEAM_ID=""
APPLE_PASSWORD=""
PKG_ARCH=""
RUST_TARGET=""
RUNTIME_BUNDLE=""
RUNTIME_MANIFEST=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --arch|--target|--sign|--sign-app|--sign-pkg|--notary-profile|--apple-id|--team-id|--apple-password|--runtime-bundle|--runtime-manifest)
      if [[ $# -lt 2 || -z "$2" || "$2" == --* ]]; then
        echo "Option $1 requires a value." >&2
        exit 1
      fi
      ;;
  esac
  case "$1" in
    --arch)
      PKG_ARCH="$2"
      shift 2
      ;;
    --target)
      RUST_TARGET="$2"
      shift 2
      ;;
    --runtime-bundle)
      RUNTIME_BUNDLE="$2"
      shift 2
      ;;
    --runtime-manifest)
      RUNTIME_MANIFEST="$2"
      shift 2
      ;;
    --sign)
      PKG_SIGN_IDENTITY="$2"
      shift 2
      ;;
    --sign-app)
      APP_SIGN_IDENTITY="$2"
      shift 2
      ;;
    --sign-pkg)
      PKG_SIGN_IDENTITY="$2"
      shift 2
      ;;
    --notarize)
      NOTARIZE=1
      shift
      ;;
    --notary-profile)
      NOTARY_PROFILE="$2"
      shift 2
      ;;
    --apple-id)
      APPLE_ID="$2"
      shift 2
      ;;
    --team-id)
      TEAM_ID="$2"
      shift 2
      ;;
    --apple-password)
      APPLE_PASSWORD="$2"
      shift 2
      ;;
    *) echo "Unknown option: $1" >&2; exit 1 ;;
  esac
done

if [[ -n "$RUNTIME_BUNDLE" && -z "$RUNTIME_MANIFEST" || -z "$RUNTIME_BUNDLE" && -n "$RUNTIME_MANIFEST" ]]; then
  echo "Optional runtime requires both --runtime-bundle and --runtime-manifest." >&2
  exit 1
fi

if [[ -z "$PKG_ARCH" ]]; then
  case "$(uname -m)" in
    x86_64)
      PKG_ARCH="x86_64"
      ;;
    arm64|aarch64)
      PKG_ARCH="aarch64"
      ;;
    *)
      echo "Unsupported macOS architecture: $(uname -m)" >&2
      exit 1
      ;;
  esac
fi

case "$PKG_ARCH" in
  x86_64)
    EXPECTED_TARGET="x86_64-apple-darwin"
    ;;
  arm64|aarch64)
    PKG_ARCH="aarch64"
    EXPECTED_TARGET="aarch64-apple-darwin"
    ;;
  *)
    echo "Unsupported macOS package architecture: $PKG_ARCH" >&2
    echo "Supported values: x86_64, aarch64" >&2
    exit 1
    ;;
esac
RUST_TARGET="${RUST_TARGET:-$EXPECTED_TARGET}"
if [[ "$RUST_TARGET" != "$EXPECTED_TARGET" ]]; then
  echo "Package architecture $PKG_ARCH requires --target $EXPECTED_TARGET; got $RUST_TARGET." >&2
  exit 1
fi

if [[ "$NOTARIZE" -eq 1 && -z "$PKG_SIGN_IDENTITY" ]]; then
  echo "Notarization requires a signed installer. Pass --sign-pkg." >&2
  exit 1
fi

if [[ "$NOTARIZE" -eq 1 && -z "$NOTARY_PROFILE" ]]; then
  if [[ -z "$APPLE_ID" || -z "$TEAM_ID" || -z "$APPLE_PASSWORD" ]]; then
    echo "Notarization requires either --notary-profile or all of --apple-id, --team-id, and --apple-password." >&2
    exit 1
  fi
fi

# Fail before staging or builds. This script is not a Linux package cross-builder.
if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "Building a macOS .pkg requires Darwin and Apple's packaging tools." >&2
  exit 1
fi
REQUIRED_TOOLS=(cargo npm pkgbuild mktemp install cp ln find chmod mkdir rm awk du cut)
if [[ -n "$RUNTIME_BUNDLE" ]]; then REQUIRED_TOOLS+=(python3); fi
if [[ -n "$APP_SIGN_IDENTITY" ]]; then REQUIRED_TOOLS+=(codesign); fi
if [[ "$NOTARIZE" -eq 1 ]]; then REQUIRED_TOOLS+=(xcrun); fi
for tool in "${REQUIRED_TOOLS[@]}"; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "Required macOS package build tool unavailable: $tool" >&2
    exit 1
  fi
done

# Offline manifest validation only: no dependency binaries or hooks execute.
# The lightweight package remains independent of build-host Python.
if [[ -n "$RUNTIME_BUNDLE" ]]; then
  if [[ ! -f "$ROOT_DIR/packaging/macos/runtime_bundle.py" ]]; then
    echo "Required runtime bundle validator unavailable." >&2
    exit 1
  fi
  RUNTIME_MANIFEST_SHA256="$(python3 -I -S -B "$ROOT_DIR/packaging/macos/runtime_bundle.py" validate \
    --bundle "$RUNTIME_BUNDLE" --manifest "$RUNTIME_MANIFEST" --arch "$PKG_ARCH" --print-manifest-sha256)"
  if [[ ! "$RUNTIME_MANIFEST_SHA256" =~ ^[a-f0-9]{64}$ ]]; then
    echo "Runtime validator did not produce an exact manifest SHA-256." >&2
    exit 1
  fi
fi
if [[ "$NOTARIZE" -eq 1 ]]; then
  for apple_tool in notarytool stapler; do
    if ! xcrun --find "$apple_tool" >/dev/null 2>&1; then
      echo "Required Apple package tool unavailable: $apple_tool" >&2
      exit 1
    fi
  done
fi
for source in "$ROOT_DIR/.codex/config.toml" "$ROOT_DIR/.codex/agents/astra_adviser.toml" \
  "$ROOT_DIR/packaging/macos/postinstall.sh" "$ROOT_DIR/packaging/macos/seed-user.sh" \
  "$ROOT_DIR/packaging/macos/onboard.py" "$ROOT_DIR/packaging/macos/onboarding_files.py" \
  "$ROOT_DIR/packaging/macos/onboarding_native.py" "$ROOT_DIR/packaging/macos/onboarding_journey.py" \
  "$ROOT_DIR/packaging/macos/onboarding_agents.py" \
  "$ROOT_DIR/packaging/macos/onboarding_first_run.py" "$ROOT_DIR/packaging/macos/runtime_bundle.py" \
  "$ROOT_DIR/packaging/macos/operator-README.md"; do
  if [[ ! -f "$source" ]]; then
    echo "Required package input unavailable: $source" >&2
    exit 1
  fi
done

umask 022
STAGE_PARENT="${TMPDIR:-/tmp}"
if [[ "$STAGE_PARENT" != /* || ! -d "$STAGE_PARENT" ]]; then
  echo "Temporary staging parent must be an existing absolute directory." >&2
  exit 1
fi
# macOS commonly supplies /var/folders through TMPDIR, while /var aliases
# /private/var. Canonicalize only this build-owned temporary parent before
# creating a new stage; installed/user data path validation stays unchanged.
STAGE_PARENT="$(cd "$STAGE_PARENT" && pwd -P)"
STAGE_DIR="$(mktemp -d "${STAGE_PARENT%/}/memory-layer-macos-pkg.XXXXXXXX")"
# Only clean the private directory returned by mktemp, never a workspace stage.
case "$STAGE_DIR" in
  /*/memory-layer-macos-pkg.????????) ;;
  *) echo "Unexpected temporary staging path; refusing cleanup." >&2; exit 1 ;;
esac
cleanup() {
  if [[ -d "$STAGE_DIR" && ! -L "$STAGE_DIR" ]]; then
    # Optional runtime directories are intentionally read-only in the payload.
    # Make only directories inside this private mktemp stage removable; find
    # does not follow symlinks. Never alter a reviewed runtime input tree.
    find "$STAGE_DIR" -type d -exec chmod u+w {} +
    rm -rf -- "$STAGE_DIR"
  fi
}
trap cleanup EXIT
PAYLOAD="$STAGE_DIR/payload"
SCRIPTS="$STAGE_DIR/scripts"
PKG_PATH="$ROOT_DIR/target/memory-layer-${VERSION}-macos-${PKG_ARCH}.pkg"
BINARY_PATH="$CARGO_TARGET_ROOT/$RUST_TARGET/release/memory"

mkdir -p \
  "$ROOT_DIR/target" \
  "$PAYLOAD/usr/local/bin" \
  "$PAYLOAD/usr/local/share/memory-layer/skill-template" \
  "$PAYLOAD/usr/local/share/memory-layer/web" \
  "$PAYLOAD/usr/local/share/memory-layer/onboarding/.codex/agents" \
  "$PAYLOAD/usr/local/share/memory-layer/onboarding/operator" \
  "$PAYLOAD/usr/local/share/bash-completion/completions" \
  "$PAYLOAD/usr/local/share/zsh/site-functions" \
  "$PAYLOAD/usr/local/share/fish/vendor_completions.d" \
  "$SCRIPTS"

# --- Build -----------------------------------------------------------------
echo "Building release binary for $PKG_ARCH ($RUST_TARGET)..."
cargo build --locked --release --features full --manifest-path "$ROOT_DIR/Cargo.toml" --bin memory --target "$RUST_TARGET"

echo "Building web UI..."
npm --prefix "$ROOT_DIR/web" ci
npm --prefix "$ROOT_DIR/web" run build

# --- Stage payload ----------------------------------------------------------
if [[ -n "$RUNTIME_BUNDLE" ]]; then
  python3 -I -S -B "$ROOT_DIR/packaging/macos/runtime_bundle.py" stage \
    --bundle "$RUNTIME_BUNDLE" --manifest "$RUNTIME_MANIFEST" --arch "$PKG_ARCH" \
    --manifest-sha256 "$RUNTIME_MANIFEST_SHA256" --payload "$PAYLOAD" --scripts "$SCRIPTS"
fi
install -m 0755 "$BINARY_PATH" "$PAYLOAD/usr/local/bin/memory"

if [[ -n "$APP_SIGN_IDENTITY" ]]; then
  echo "Signing binary with Developer ID Application certificate..."
  codesign --force --sign "$APP_SIGN_IDENTITY" --options runtime --timestamp "$PAYLOAD/usr/local/bin/memory"
  codesign --verify --verbose=2 "$PAYLOAD/usr/local/bin/memory"
fi

ln -sf memory "$PAYLOAD/usr/local/bin/mem-cli"
ln -sf memory "$PAYLOAD/usr/local/bin/memory-layer"
"$PAYLOAD/usr/local/bin/memory" completion bash > "$PAYLOAD/usr/local/share/bash-completion/completions/memory"
"$PAYLOAD/usr/local/bin/memory" completion zsh > "$PAYLOAD/usr/local/share/zsh/site-functions/_memory"
"$PAYLOAD/usr/local/bin/memory" completion fish > "$PAYLOAD/usr/local/share/fish/vendor_completions.d/memory.fish"

cp -R "$ROOT_DIR/.agents/skills/." "$PAYLOAD/usr/local/share/memory-layer/skill-template/"
find "$PAYLOAD/usr/local/share/memory-layer/skill-template" -type f -name '*.sh' -exec chmod 0755 {} +

install -m 0644 "$ROOT_DIR/memory-layer.toml.example" "$PAYLOAD/usr/local/share/memory-layer/memory-layer.toml.example"
install -m 0644 "$ROOT_DIR/README.md" "$PAYLOAD/usr/local/share/memory-layer/README.md"
cp -R "$ROOT_DIR/web/dist/." "$PAYLOAD/usr/local/share/memory-layer/web/"
install -m 0444 "$ROOT_DIR/.codex/config.toml" "$PAYLOAD/usr/local/share/memory-layer/onboarding/.codex/config.toml"
install -m 0444 "$ROOT_DIR/.codex/agents/astra_adviser.toml" "$PAYLOAD/usr/local/share/memory-layer/onboarding/.codex/agents/astra_adviser.toml"
for operator_file in onboard.py onboarding_files.py onboarding_native.py onboarding_journey.py onboarding_agents.py onboarding_first_run.py runtime_bundle.py operator-README.md; do
  install -m 0444 "$ROOT_DIR/packaging/macos/$operator_file" "$PAYLOAD/usr/local/share/memory-layer/onboarding/operator/$operator_file"
done
# The operator is an explicit later step. No Python/PostgreSQL execution,
# device login or service activation is executed by the package lifecycle.

# --- Post-install script ----------------------------------------------------
install -m 0755 "$ROOT_DIR/packaging/macos/postinstall.sh" "$SCRIPTS/postinstall"
install -m 0755 "$ROOT_DIR/packaging/macos/seed-user.sh" "$SCRIPTS/seed-user.sh"

# --- Build .pkg -------------------------------------------------------------
echo "Assembling .pkg..."

PKGBUILD_ARGS=(
  --root "$PAYLOAD"
  --scripts "$SCRIPTS"
  --identifier "com.memory-layer.pkg"
  --version "$VERSION"
  --install-location /
)

if [[ -n "$PKG_SIGN_IDENTITY" ]]; then
  PKGBUILD_ARGS+=(--sign "$PKG_SIGN_IDENTITY")
fi

pkgbuild "${PKGBUILD_ARGS[@]}" "$PKG_PATH"

if [[ "$NOTARIZE" -eq 1 ]]; then
  echo "Submitting package for notarization..."
  NOTARY_ARGS=(submit "$PKG_PATH" --wait)
  if [[ -n "$NOTARY_PROFILE" ]]; then
    NOTARY_ARGS+=(--keychain-profile "$NOTARY_PROFILE")
  else
    NOTARY_ARGS+=(--apple-id "$APPLE_ID" --team-id "$TEAM_ID" --password "$APPLE_PASSWORD")
  fi
  xcrun notarytool "${NOTARY_ARGS[@]}"

  echo "Stapling notarization ticket..."
  xcrun stapler staple "$PKG_PATH"
  xcrun stapler validate "$PKG_PATH"
fi

echo ""
echo "Built $PKG_PATH ($(du -h "$PKG_PATH" | cut -f1))"

if [[ -n "$APP_SIGN_IDENTITY" ]]; then
  echo "Binary signed with: $APP_SIGN_IDENTITY"
fi
if [[ -n "$PKG_SIGN_IDENTITY" ]]; then
  echo "Installer signed with: $PKG_SIGN_IDENTITY"
fi
if [[ "$NOTARIZE" -eq 1 ]]; then
  echo "Notarization: complete and stapled"
fi
