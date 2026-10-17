#!/usr/bin/env bash
# Build one public Linux candidate from a frozen Git object. No live state mounts.
set -euo pipefail
if [[ $# != 4 || ! "$1" =~ ^[0-9a-f]{40}$ || ! "$3" =~ ^sha256:[0-9a-f]{64}$ || ! "$4" =~ ^(amd64|arm64)$ ]]; then
  echo 'Usage: release.sh SOURCE_COMMIT PRIVATE_WORKDIR sha256:BUILDER_IMAGE amd64|arm64' >&2
  exit 2
fi
release_revision=$1
release_work=$2
release_image=$3
release_arch=$4
release_root="$(git rev-parse --show-toplevel)"
[[ "$release_work" == /* && "$release_work" != *,* && "$release_work" != *$'\n'* && -d "$release_work" && ! -L "$release_work" ]]
[[ "$(realpath -e "$release_work")" == "$release_work" && "$(stat -c '%u:%a' "$release_work")" == "$(id -u):700" ]]
[[ "$(docker image inspect --format '{{.Id}}' "$release_image")" == "$release_image" ]]
# Cache directories may be restored by CI; source and published artifacts may not.
[[ ! -e "$release_work/memory" && ! -e "$release_work/source.tar" && ! -e "$release_work/output" ]]
mkdir -p "$release_work/memory" "$release_work/memory-cargo" "$release_work/memory-target" "$release_work/npm-cache" "$release_work/output"
git -C "$release_root" archive "$release_revision" > "$release_work/source.tar"
tar -xf "$release_work/source.tar" -C "$release_work/memory"
printf '%s\n' "$release_revision" > "$release_work/source-revision.txt"
common=(--rm --init --pull never --user "$(id -u):$(id -g)" --read-only --cap-drop ALL
  --security-opt no-new-privileges --pids-limit 512 --cpus 4 --memory 14g
  --tmpfs /tmp:rw,nosuid,nodev,size=2g --mount "type=bind,src=$release_work,dst=/work"
  --workdir /work/memory --env HOME=/tmp --env CARGO_HOME=/work/memory-cargo
  --env npm_config_cache=/work/npm-cache --env npm_config_registry=https://registry.npmjs.org/)
# Separate public dependency preparation. No host HOME, credentials or config is
# mounted; compilation below is offline. Git credentials are never forwarded.
docker run "${common[@]}" "$release_image" bash -euc '
  cargo fetch --locked --target x86_64-unknown-linux-gnu --target aarch64-unknown-linux-gnu
  npm --prefix web ci --ignore-scripts --no-audit --no-fund
' 2>&1 | tee "$release_work/dependency-preparation.log"
bash "$release_work/memory/packaging/ca-bookworm/build.sh" package "$release_work" "$release_image" "$release_arch"
docker run "${common[@]}" --network none --tmpfs /candidate:rw,nosuid,nodev,exec,size=1g "$release_image" bash -euc '
  arch=$1
  package=$(find /work/output -maxdepth 1 -name "*_${arch}.deb" -print)
  test -n "$package"
  mkdir /candidate/package
  dpkg-deb -x "$package" /candidate/package
  binaries=(/candidate/package/usr/bin/memory /candidate/package/usr/bin/memory-area /candidate/package/usr/bin/memory-area-import /candidate/package/usr/bin/memory-federation-gateway)
  if [[ "$arch" == amd64 ]]; then
    mkdir -p /tmp/sysroot/lib/x86_64-linux-gnu /tmp/sysroot/lib64
    for library in libc.so.6 libm.so.6 libgcc_s.so.1 libstdc++.so.6 libpthread.so.0 libdl.so.2 librt.so.1 libresolv.so.2; do
      cp -L "/lib/x86_64-linux-gnu/$library" /tmp/sysroot/lib/x86_64-linux-gnu/
    done
    cp -L /lib64/ld-linux-x86-64.so.2 /tmp/sysroot/lib64/
    sysroot=/tmp/sysroot
    runner=()
  else
    sysroot=/usr/aarch64-linux-gnu
    runner=(qemu-aarch64 -L "$sysroot")
  fi
  python3 packaging/ca-bookworm/abi.py --architecture "$arch" --sysroot "$sysroot" "${binaries[@]}" > "/work/output/abi-${arch}.json"
  "${runner[@]}" "${binaries[0]}" --version
  for executable in "${binaries[@]:0:3}"; do "${runner[@]}" "$executable" --help > /dev/null; done
  # The gateway takes a private config path, not Clap flags. With no arguments
  # it must launch and stop before config access, networking or database work.
  gateway_status=0
  gateway_output=$("${runner[@]}" "${binaries[3]}" 2>&1) || gateway_status=$?
  [[ $gateway_status == 1 && $gateway_output == "Error: \"expected private gateway configuration file\"" ]]
  test -s /candidate/package/usr/share/memory-layer/web/index.html
  test -s /candidate/package/usr/libexec/memory-layer/releases/release.py
  dpkg-deb -f "$package" Package Version Architecture Depends
  rustc -vV
  node --version
  dpkg-query -W
' bash "$release_arch" 2>&1 | tee "$release_work/output/validation-$release_arch.log"
python3 - "$release_work" "$release_image" "$release_arch" <<'PY'
import hashlib, json, pathlib, sys, tomllib
work, image, arch = pathlib.Path(sys.argv[1]), sys.argv[2], sys.argv[3]
def sha(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()
files = sorted(p for p in (work / 'output').iterdir() if p.is_file())
metadata = {'format': 'memory-linux-build-v1', 'architecture': arch,
            'application_version': tomllib.loads((work / 'memory/Cargo.toml').read_text())['workspace']['package']['version'],
            'source_commit': (work / 'source-revision.txt').read_text().strip(),
            'source_archive_sha256': sha(work / 'source.tar'), 'builder_image': image,
            'signing_status': 'unsigned', 'installed_on_host': False,
            'artifacts': {p.name: sha(p) for p in files}}
(work / 'output' / f'build-{arch}.json').write_text(json.dumps(metadata, indent=2, sort_keys=True) + '\n')
with (work / 'output' / f'SHA256SUMS-{arch}').open('w') as stream:
    for path in sorted((work / 'output').iterdir()):
        if path.is_file() and not path.name.startswith('SHA256SUMS'):
            stream.write(f'{sha(path)}  {path.name}\n')
PY
