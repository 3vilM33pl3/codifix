#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CARGO_TARGET_ROOT="${CARGO_TARGET_DIR:-$ROOT_DIR/target}"
if [[ "$CARGO_TARGET_ROOT" != /* ]]; then
  CARGO_TARGET_ROOT="$PWD/$CARGO_TARGET_ROOT"
fi
VERSION="$(awk -F '\"' '/^version = / { print $2; exit }' "$ROOT_DIR/Cargo.toml" 2>/dev/null || true)"
if [[ -z "${VERSION:-}" ]]; then
  VERSION="0.1.0"
fi

DEB_ARCH="amd64"
RUST_TARGET=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --arch)
      [[ $# -ge 2 ]] || { echo "--arch requires a value" >&2; exit 1; }
      DEB_ARCH="$2"
      shift 2
      ;;
    --target)
      [[ $# -ge 2 ]] || { echo "--target requires a value" >&2; exit 1; }
      RUST_TARGET="$2"
      shift 2
      ;;
    --help|-h)
      cat <<'HELP'
Build a Memory Layer Debian package.

Usage:
  ./packaging/build-deb.sh [--arch amd64|arm64] [--target <rust-target>]

Defaults:
  amd64 -> x86_64-unknown-linux-gnu
  arm64 -> aarch64-unknown-linux-gnu
HELP
      exit 0
      ;;
    *)
      echo "Unknown option: $1" >&2
      exit 1
      ;;
  esac
done

case "$DEB_ARCH" in
  amd64)
    RUST_TARGET="${RUST_TARGET:-x86_64-unknown-linux-gnu}"
    ;;
  arm64)
    RUST_TARGET="${RUST_TARGET:-aarch64-unknown-linux-gnu}"
    ;;
  *)
    echo "Unsupported Debian architecture: $DEB_ARCH" >&2
    echo "Supported values: amd64, arm64" >&2
    exit 1
    ;;
esac

case "$DEB_ARCH:$RUST_TARGET" in
  amd64:x86_64-unknown-linux-gnu|arm64:aarch64-unknown-linux-gnu) ;;
  *) echo "Rust target does not match a supported Debian architecture: $DEB_ARCH:$RUST_TARGET" >&2; exit 1 ;;
esac
if [[ ! "$VERSION" =~ ^[0-9][0-9A-Za-z.+:~-]*$ ]]; then
  echo "Invalid Debian package version: $VERSION" >&2
  exit 1
fi

# Each build owns a fresh staging directory. Never recursively delete the old
# predictable path: it might contain another build or be an operator symlink.
mkdir -p "$ROOT_DIR/target/debian"
DEB_PATH="$ROOT_DIR/target/debian/memory-layer_${VERSION}_${DEB_ARCH}.deb"
if [[ -e "$DEB_PATH" || -L "$DEB_PATH" ]]; then
  echo "Refusing to overwrite an existing release artifact: $DEB_PATH" >&2
  echo "Preserve or explicitly remove that exact artifact before rebuilding." >&2
  exit 1
fi
BUILD_ROOT="$(mktemp -d "$ROOT_DIR/target/debian/.memory-layer-$DEB_ARCH.XXXXXX")"
trap 'rm -rf -- "$BUILD_ROOT"' EXIT
PKG_ROOT="$BUILD_ROOT/package"
BIN_PATH="$CARGO_TARGET_ROOT/$RUST_TARGET/release/memory"

mkdir -p \
  "$PKG_ROOT/DEBIAN" \
  "$PKG_ROOT/usr/bin" \
  "$PKG_ROOT/usr/libexec/memory-layer" \
  "$PKG_ROOT/etc/memory-layer" \
  "$PKG_ROOT/lib/systemd/system" \
  "$PKG_ROOT/usr/share/doc/memory-layer" \
  "$PKG_ROOT/usr/share/doc/memory-layer/areas" \
  "$PKG_ROOT/usr/share/memory-layer/skill-template" \
  "$PKG_ROOT/usr/share/memory-layer/web" \
  "$PKG_ROOT/usr/share/bash-completion/completions" \
  "$PKG_ROOT/usr/share/zsh/vendor-completions" \
  "$PKG_ROOT/usr/share/fish/vendor_completions.d"

echo "Building release binary for $DEB_ARCH ($RUST_TARGET)..."
cargo build --release --locked -p mem-cli --features full --manifest-path "$ROOT_DIR/Cargo.toml" --bin memory --target "$RUST_TARGET"
echo "Building explicit offline area migration operators..."
cargo build --release --locked -p mem-area -p mem-area-ops --manifest-path "$ROOT_DIR/Cargo.toml" --bins --target "$RUST_TARGET"
echo "Building web UI..."
npm --prefix "$ROOT_DIR/web" ci
npm --prefix "$ROOT_DIR/web" run build

install -m 0755 "$BIN_PATH" "$PKG_ROOT/usr/bin/memory"
install -m 0755 "$CARGO_TARGET_ROOT/$RUST_TARGET/release/memory-area" "$PKG_ROOT/usr/bin/memory-area"
install -m 0755 "$CARGO_TARGET_ROOT/$RUST_TARGET/release/memory-area-import" "$PKG_ROOT/usr/bin/memory-area-import"
"$PKG_ROOT/usr/bin/memory" completion bash > "$PKG_ROOT/usr/share/bash-completion/completions/memory"
"$PKG_ROOT/usr/bin/memory" completion zsh > "$PKG_ROOT/usr/share/zsh/vendor-completions/_memory"
"$PKG_ROOT/usr/bin/memory" completion fish > "$PKG_ROOT/usr/share/fish/vendor_completions.d/memory.fish"
install -m 0644 "$ROOT_DIR/packaging/debian/memory-layer.service" "$PKG_ROOT/lib/systemd/system/memory-layer.service"
install -m 0644 "$ROOT_DIR/packaging/debian/memory-watch.service" "$PKG_ROOT/lib/systemd/system/memory-watch.service"
install -m 0644 "$ROOT_DIR/packaging/debian/memory-layer@.service" "$PKG_ROOT/lib/systemd/system/memory-layer@.service"
install -m 0755 "$ROOT_DIR/packaging/debian/verify-instance-release" "$PKG_ROOT/usr/libexec/memory-layer/verify-instance-release"
install -m 0755 "$ROOT_DIR/deploy/areas/ca-preflight.sh" "$PKG_ROOT/usr/libexec/memory-layer/ca-preflight"
install -m 0644 "$ROOT_DIR/scripts/areas/releases/release.py" "$PKG_ROOT/usr/libexec/memory-layer/area-release.py"
install -m 0644 "$ROOT_DIR/packaging/debian/memory-layer.env" "$PKG_ROOT/etc/memory-layer/memory-layer.env"
install -m 0644 "$ROOT_DIR/memory-layer.toml.example" "$PKG_ROOT/etc/memory-layer/memory-layer.toml"
install -m 0644 "$ROOT_DIR/README.md" "$PKG_ROOT/usr/share/doc/memory-layer/README.md"
install -m 0644 "$ROOT_DIR/deploy/areas/README.md" "$PKG_ROOT/usr/share/doc/memory-layer/areas/README.md"
install -m 0644 "$ROOT_DIR/deploy/areas/instance.toml.example" "$PKG_ROOT/usr/share/doc/memory-layer/areas/instance.toml.example"
install -m 0644 "$ROOT_DIR/deploy/areas/CA-CONSOLE-HANDOFF.md" "$PKG_ROOT/usr/share/doc/memory-layer/areas/CA-CONSOLE-HANDOFF.md"
install -m 0644 "$ROOT_DIR/crates/mem-area-ops/README.md" "$PKG_ROOT/usr/share/doc/memory-layer/areas/area-import.md"
install -m 0644 "$ROOT_DIR/scripts/areas/releases/README.md" "$PKG_ROOT/usr/share/doc/memory-layer/areas/release-staging.md"
cp -R "$ROOT_DIR/.agents/skills/." "$PKG_ROOT/usr/share/memory-layer/skill-template/"
find "$PKG_ROOT/usr/share/memory-layer/skill-template" -type f -name '*.sh' -exec chmod 0755 {} +
cp -R "$ROOT_DIR/web/dist/." "$PKG_ROOT/usr/share/memory-layer/web/"

sed \
  -e "s/^Version: .*/Version: $VERSION/" \
  -e "s/^Architecture: .*/Architecture: $DEB_ARCH/" \
  "$ROOT_DIR/packaging/debian/control" > "$PKG_ROOT/DEBIAN/control"
install -m 0644 "$ROOT_DIR/packaging/debian/conffiles" "$PKG_ROOT/DEBIAN/conffiles"
install -m 0755 "$ROOT_DIR/packaging/debian/postinst" "$PKG_ROOT/DEBIAN/postinst"
install -m 0755 "$ROOT_DIR/packaging/debian/prerm" "$PKG_ROOT/DEBIAN/prerm"

BUILT_DEB="$BUILD_ROOT/memory-layer.deb"
dpkg-deb --root-owner-group --build "$PKG_ROOT" "$BUILT_DEB"
# Publish without following or overwriting an existing output path, even if one
# appeared while the build was running. Both files are on the same filesystem.
ln -T -- "$BUILT_DEB" "$DEB_PATH"

echo "Built $DEB_PATH"
