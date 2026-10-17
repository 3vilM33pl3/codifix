#!/usr/bin/env bash
# Off-CA helper for an explicitly prepared private build directory. No pulls,
# host package changes, provider settings or production mounts are supported.
set -euo pipefail

usage() {
  echo 'Usage: build.sh codifix|package PRIVATE_WORKDIR sha256:IMAGE_ID [amd64|arm64]'
  echo '       build.sh pi PRIVATE_WORKDIR sha256:IMAGE_ID PI_TOOLCHAIN'
  echo '       build.sh web PRIVATE_WORKDIR sha256:IMAGE_ID'
  echo 'Source/cache preparation and exact pins: packaging/ca-bookworm/README.md'
}
if [[ $# == 1 && ( "$1" == --help || "$1" == -h ) ]]; then usage; exit 0; fi
if [[ $# -lt 3 || $# -gt 4 ]]; then usage >&2; exit 2; fi
phase="$1"
build_work="$2"
build_image="$3"
build_arch=arm64
rust_target=aarch64-unknown-linux-gnu
case "$phase" in
  codifix|package)
    build_arch="${4:-arm64}"
    case "$build_arch" in
      amd64) rust_target=x86_64-unknown-linux-gnu ;;
      arm64) ;;
      *) usage >&2; exit 2 ;;
    esac
    ;;
  web) [[ $# == 3 ]] || { usage >&2; exit 2; } ;;
esac
if [[ ! "$phase" =~ ^(codifix|pi|web|package)$ ]] ||
   [[ ! "$build_image" =~ ^sha256:[a-f0-9]{64}$ ]] ||
   [[ "$build_work" != /* || "$build_work" == / || "$build_work" == /tmp || "$build_work" == /data || "$build_work" == *,* || "$build_work" == *$'\n'* ]] ||
   [[ ! -d "$build_work" || -L "$build_work" ]] ||
   [[ "$(realpath -e -- "$build_work")" != "$build_work" ]] ||
   [[ "$(stat -c '%u:%a' -- "$build_work")" != "$(id -u):700" ]]; then
  echo 'Require a private, owned, non-symlink work directory and exact local image ID.' >&2
  exit 2
fi
if [[ "$(docker image inspect --format '{{.Id}}' "$build_image")" != "$build_image" ]]; then
  echo 'The exact builder must already exist locally; this helper never pulls.' >&2
  exit 2
fi
build_args=(--rm --init --pull never --user "$(id -u):$(id -g)"
  --network none --read-only --cap-drop ALL --security-opt no-new-privileges
  --pids-limit 512 --cpus 4 --memory 14g
  --tmpfs /tmp:rw,nosuid,nodev,size=2g
  --mount "type=bind,src=$build_work,dst=/work"
  --env CARGO_NET_OFFLINE=true --env CARGO_BUILD_JOBS=4
  --env npm_config_cache=/work/npm-cache --env npm_config_offline=true)
case "$phase" in
  codifix|package|web)
    [[ -f "$build_work/codifix/Cargo.lock" ]] || { usage >&2; exit 2; }
    build_args+=(--workdir /work/codifix --env CARGO_HOME=/work/codifix-cargo
      --env CARGO_TARGET_DIR=/work/codifix-target)
    ;;
  pi)
    [[ $# == 4 && "$4" == /* && -x "$4/bin/rustc" && -x "$4/bin/cargo" ]] || { usage >&2; exit 2; }
    [[ "$(realpath -e -- "$4")" == "$4" && "$4" != *,* && "$4" != *$'\n'* ]] || exit 2
    build_args+=(--workdir /work/pi --env CARGO_HOME=/work/pi-cargo
      --env CARGO_TARGET_DIR=/work/pi-target --env RUSTC=/pi-toolchain/bin/rustc
      --mount "type=bind,src=$4,dst=/pi-toolchain,readonly")
    ;;
esac
case "$phase" in
  codifix)
    build_command=(cargo build --release --frozen --target "$rust_target"
      -p codifix-cli --features full --bin codifix -p codifix-area --bin codifix-area
      -p codifix-area-ops --bin codifix-area-import
      -p codifix-federation-runtime --bin codifix-federation-gateway)
    ;;
  pi)
    build_command=(/pi-toolchain/bin/cargo build --release --frozen --bin pi
      --target aarch64-unknown-linux-gnu)
    ;;
  web)
    build_command=(bash -c 'npm --prefix web ci --offline --no-audit --no-fund && npm --prefix web run test && npm --prefix web run build')
    ;;
  package)
    build_command=(bash packaging/build-deb.sh --arch "$build_arch" --output-dir /work/output)
    ;;
esac
build_log="$(mktemp "$build_work/$phase.XXXXXXXX.log")"
echo "Log: $build_log"
# No inherited host environment is passed into Docker. It receives only its
# captured image environment and the finite overrides/mounts above.
docker run "${build_args[@]}" "$build_image" bash -c '
  set -euo pipefail
  test "$(getconf GNU_LIBC_VERSION)" = "glibc 2.36"
  test "$(aarch64-linux-gnu-gcc -dumpversion)" = 12
  exec "$@"
' bash "${build_command[@]}" 2>&1 | tee "$build_log"
