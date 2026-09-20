#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
test_root="$(mktemp -d)"
trap 'rm -rf -- "$test_root"' EXIT
export PACKAGING_TEST_LOG="$test_root/commands.log"

fail() { echo "FAIL: $*" >&2; exit 1; }
log_command() { printf '%s\n' "$*" >> "$PACKAGING_TEST_LOG"; }
id() { log_command "id $*"; return "${PACKAGING_TEST_ID_STATUS:-0}"; }
useradd() { log_command "useradd $*"; }
install() { log_command "install $*"; }
systemctl() { log_command "systemctl $*"; return "${PACKAGING_TEST_SYSTEMCTL_STATUS:-0}"; }
memory() { log_command "FORBIDDEN memory $*"; return 99; }
export -f log_command id useradd install systemctl memory

# Fail before executing an old hook with an absolute memory path: the test must
# never fall through to a real installed CLI with access to host credentials.
if grep -Eq '/usr/(local/)?bin/memory|ensure-api-token|restart-all|rotate-placeholder' \
    "$repo_root/packaging/debian/postinst"; then
    fail "postinst includes an implicit credential or global lifecycle action"
fi

for previous_version in '' '2.0.0'; do
    : > "$PACKAGING_TEST_LOG"
    bash "$repo_root/packaging/debian/postinst" configure "$previous_version" >/dev/null
    grep -qx 'systemctl daemon-reload' "$PACKAGING_TEST_LOG" || fail "configure did not reload unit definitions"
    if grep -Eq 'FORBIDDEN|systemctl .*\b(start|restart|stop|enable|try-restart|reload-or-restart)\b' "$PACKAGING_TEST_LOG"; then
        fail "configure changed a service or a credential"
    fi
done

: > "$PACKAGING_TEST_LOG"
PACKAGING_TEST_ID_STATUS=1 bash "$repo_root/packaging/debian/postinst" configure >/dev/null
grep -qx 'useradd --system --home /var/lib/memory-layer --shell /usr/sbin/nologin memory-layer' \
    "$PACKAGING_TEST_LOG" || fail "fresh setup did not create the legacy user"

# A container/chroot without a running systemd is still a valid staging target.
PACKAGING_TEST_SYSTEMCTL_STATUS=1 bash "$repo_root/packaging/debian/postinst" configure >/dev/null

for action in abort-upgrade abort-remove abort-deconfigure triggered ''; do
    : > "$PACKAGING_TEST_LOG"
    bash "$repo_root/packaging/debian/postinst" "$action" >/dev/null
    [[ ! -s "$PACKAGING_TEST_LOG" ]] || fail "postinst $action changed state"
done

for action in upgrade failed-upgrade ''; do
    : > "$PACKAGING_TEST_LOG"
    bash "$repo_root/packaging/debian/prerm" "$action" >/dev/null
    [[ ! -s "$PACKAGING_TEST_LOG" ]] || fail "prerm $action changed state"
done
for action in remove deconfigure; do
    : > "$PACKAGING_TEST_LOG"
    bash "$repo_root/packaging/debian/prerm" "$action" >/dev/null
    [[ "$(< "$PACKAGING_TEST_LOG")" == 'systemctl stop memory-watch.service memory-layer.service' ]] ||
        fail "prerm $action selected anything except legacy system units"
done
PACKAGING_TEST_SYSTEMCTL_STATUS=1 bash "$repo_root/packaging/debian/prerm" remove

unit="$repo_root/packaging/debian/memory-layer@.service"
for required in \
    'DynamicUser=true' \
    'StateDirectoryMode=0700' \
    'ConditionPathExists=/etc/memory-layer/instances/%i/activation-reviewed' \
    'LoadCredential=memory-layer.toml:/etc/memory-layer/instances/%i/memory-layer.toml' \
    'ExecStartPre=/usr/bin/env -i PATH=/usr/bin:/bin /usr/libexec/memory-layer/verify-instance-release %i %d/memory.sha256' \
    'ExecStart=/usr/libexec/memory-layer/run-instance %i %d' \
    'ProtectSystem=strict' \
    'ProtectHome=true' \
    'IPAddressDeny=any'; do
    grep -Fxq "$required" "$unit" || fail "missing named-instance boundary: $required"
done
if grep -Eq 'EnvironmentFile=|ExecStart=.*/usr/bin/memory|ReadWritePaths=.*/etc' "$unit"; then
    fail "named instance inherits mutable global executable/configuration"
fi

# Invalid instance names/digests are rejected before touching /opt. Valid-file
# verification is covered by a fixture copy with only its /opt prefix rebased.
for name in '../trust' 'trust/other' 'Trust' '-trust' 'this-name-exceeds-19-characters'; do
    if bash "$repo_root/packaging/debian/verify-instance-release" "$name" /dev/null 2>/dev/null; then
        fail "accepted invalid instance name $name"
    fi
done
if bash "$repo_root/packaging/debian/verify-instance-release" trust /dev/null 2>/dev/null; then
    fail "accepted missing release digest"
fi

unset -f id useradd install systemctl memory
export -f log_command

# Exercise the verifier against a private filesystem tree. Rebase only the
# hard-coded installation prefix; root ownership is simulated, while real file
# modes, symlinks, executable bits and SHA-256 verification remain in force.
release_fixture="$test_root/opt/memory-layer/instances/trust/release"
mkdir -p "$release_fixture/bin"
chmod 0755 "$test_root/opt" "$test_root/opt/memory-layer" \
    "$test_root/opt/memory-layer/instances" "$test_root/opt/memory-layer/instances/trust" \
    "$release_fixture" "$release_fixture/bin"
printf '#!/usr/bin/env bash\nexit 0\n' > "$release_fixture/bin/memory"
chmod 0755 "$release_fixture/bin/memory"
sed "s|/opt|$test_root/opt|g" "$repo_root/packaging/debian/verify-instance-release" > "$test_root/verifier"
sha256sum "$release_fixture/bin/memory" | cut -d ' ' -f 1 > "$test_root/digest"
stat() {
    if [[ "$1" == -c && "$2" == %u ]]; then
        printf '%s\n' "${PACKAGING_TEST_OWNER:-0}"
    else
        command stat "$@"
    fi
}
export -f stat
bash "$test_root/verifier" trust "$test_root/digest"
if PACKAGING_TEST_OWNER=1000 bash "$test_root/verifier" trust "$test_root/digest" 2>/dev/null; then
    fail "accepted release owned by another user"
fi
chmod 0775 "$release_fixture/bin"
if bash "$test_root/verifier" trust "$test_root/digest" 2>/dev/null; then
    fail "accepted group-writable release directory"
fi
chmod 0755 "$release_fixture/bin"
printf '# modified\n' >> "$release_fixture/bin/memory"
if bash "$test_root/verifier" trust "$test_root/digest" 2>/dev/null; then
    fail "accepted executable with incorrect digest"
fi
sha256sum "$release_fixture/bin/memory" | cut -d ' ' -f 1 > "$test_root/digest"
mv "$release_fixture/bin/memory" "$release_fixture/bin/real-memory"
ln -s real-memory "$release_fixture/bin/memory"
if bash "$test_root/verifier" trust "$test_root/digest" 2>/dev/null; then
    fail "accepted symlink executable"
fi
rm "$release_fixture/bin/memory"
mv "$release_fixture/bin/real-memory" "$release_fixture/bin/memory"
mv "$release_fixture/bin" "$release_fixture/real-bin"
ln -s real-bin "$release_fixture/bin"
if bash "$test_root/verifier" trust "$test_root/digest" 2>/dev/null; then
    fail "accepted symlink release ancestor"
fi
unset -f stat

if command -v systemd-analyze >/dev/null; then
    mkdir -p "$test_root/units"
    # Syntax verification is read-only. Substitute only executable locations,
    # because this test deliberately never installs either production file.
    sed -E 's|^ExecStart(Pre)?=.*|ExecStart\1=/usr/bin/true|' "$unit" > "$test_root/units/memory-layer@.service"
    systemd-analyze verify --man=no "$test_root/units/memory-layer@.service"
fi

# Build a small fixture repository and run the actual package builder with
# cargo/npm/dpkg-deb stubbed. Nothing is built, installed, published or started.
fixture="$test_root/repo"
mkdir -p "$fixture/packaging" "$fixture/deploy/areas" "$fixture/.agents/skills" \
    "$fixture/crates/mem-area-ops" "$fixture/scripts/areas/releases" \
    "$fixture/web/dist" "$fixture/target/debian" "$test_root/binaries/x86_64-unknown-linux-gnu/release"
cp "$repo_root/packaging/build-deb.sh" "$fixture/packaging/"
cp -R "$repo_root/packaging/debian" "$fixture/packaging/"
cp "$repo_root/deploy/areas/README.md" "$repo_root/deploy/areas/instance.toml.example" "$fixture/deploy/areas/"
cp "$repo_root/deploy/areas/ca-preflight.sh" "$repo_root/deploy/areas/CA-CONSOLE-HANDOFF.md" "$fixture/deploy/areas/"
cp "$repo_root/crates/mem-area-ops/README.md" "$fixture/crates/mem-area-ops/"
cp "$repo_root/scripts/areas/releases/release.py" "$repo_root/scripts/areas/releases/README.md" "$fixture/scripts/areas/releases/"
printf '[workspace.package]\nversion = "2.1.0-rc.1"\n' > "$fixture/Cargo.toml"
printf 'Fixture README\n' > "$fixture/README.md"
printf '[service]\n' > "$fixture/memory-layer.toml.example"
printf 'Fixture\n' > "$fixture/.agents/skills/SKILL.md"
printf 'Fixture\n' > "$fixture/web/dist/index.html"
printf '#!/usr/bin/env bash\nprintf "fixture completion\\n"\n' > "$test_root/binaries/x86_64-unknown-linux-gnu/release/memory"
chmod +x "$test_root/binaries/x86_64-unknown-linux-gnu/release/memory"
for operator in memory-area memory-area-import memory-federation-gateway; do
    printf '#!/usr/bin/env bash\nexit 99 # package build must never execute an importer\n' > "$test_root/binaries/x86_64-unknown-linux-gnu/release/$operator"
    chmod +x "$test_root/binaries/x86_64-unknown-linux-gnu/release/$operator"
done
mkdir "$test_root/unrelated-build"
printf 'preserve\n' > "$test_root/unrelated-build/evidence"
ln -s "$test_root/unrelated-build" "$fixture/target/debian/memory-layer-amd64"

cargo() { log_command "cargo $*"; }
npm() { log_command "npm $*"; }
rustc() { [[ "$1" == -vV ]] || return 99; printf 'host: x86_64-unknown-linux-gnu\n'; }
dpkg-shlibdeps() {
    [[ "${PACKAGING_TEST_SHLIB_STATUS:-0}" == 0 ]] || return 1
    [[ -f debian/control ]] || return 1
    local executables=0 arg
    for arg in "$@"; do
        # Exercise the Debian 12 option surface, not the host's newer dpkg.
        [[ "$arg" != --package=* ]] || return 1
        if [[ "$arg" == -e* ]]; then
            [[ -x "${arg#-e}" ]] || return 1
            executables=$((executables + 1))
        fi
    done
    [[ "$executables" == 4 ]] || return 1
    if [[ "${PACKAGING_TEST_SHLIB_INVALID:-0}" == 1 ]]; then
        printf 'shlibs:Depends=libc6 (>= 2.39), libunknown-cross, libstdc++6\n'
    elif [[ "$DEB_HOST_ARCH" == arm64 ]]; then
        printf 'shlibs:Depends=libc6:arm64 (>= 2.43), libgcc-s1-arm64-cross (>= 4.2), libstdc++6-arm64-cross (>= 14)\n'
    else
        printf 'shlibs:Depends=libc6 (>= 2.39), libgcc-s1 (>= 4.2), libstdc++6 (>= 14)\n'
    fi
}
dpkg-deb() {
    [[ "$1" == --root-owner-group && "$2" == --build ]] || return 90
    local staging="$3" architecture rust_target
    [[ "$(stat -c %a "$(dirname "$staging")")" == 700 ]] || return 89
    [[ -z "$(find "$staging" -type d ! -perm 0755 -print -quit)" ]] || return 89
    [[ -z "$(find "$staging" -type f ! -perm 0644 ! -perm 0755 -print -quit)" ]] || return 89
    architecture="$(sed -n 's/^Architecture: //p' "$staging/DEBIAN/control")"
    case "$architecture" in
        amd64) rust_target=x86_64-unknown-linux-gnu ;;
        arm64) rust_target=aarch64-unknown-linux-gnu ;;
        *) return 90 ;;
    esac
    grep -Fxq 'Version: 2.1.0~rc.1' "$staging/DEBIAN/control" || return 90
    if [[ "$architecture" == arm64 ]]; then
        grep -Fxq 'Depends: libc6:arm64 (>= 2.43), libgcc-s1:arm64 (>= 4.2), libstdc++6:arm64 (>= 14), python3, gpg' "$staging/DEBIAN/control" || return 90
    else
        grep -Fxq 'Depends: libc6 (>= 2.39), libgcc-s1 (>= 4.2), libstdc++6 (>= 14), python3, gpg' "$staging/DEBIAN/control" || return 90
    fi
    grep -Fxq 'Suggests: postgresql' "$staging/DEBIAN/control" || return 90
    [[ -f "$staging/lib/systemd/system/memory-layer@.service" ]] || return 91
    [[ -x "$staging/usr/libexec/memory-layer/verify-instance-release" ]] || return 92
    [[ -x "$staging/usr/libexec/memory-layer/run-instance" ]] || return 92
    [[ -f "$staging/usr/share/doc/memory-layer/areas/instance.toml.example" ]] || return 93
    [[ ! -e "$staging/opt/memory-layer" ]] || return 94
    [[ ! -e "$staging/etc/memory-layer/instances" ]] || return 95
    [[ ! -e "$staging/var/lib/memory-layer" ]] || return 96
    for operator in memory memory-area memory-area-import memory-federation-gateway; do
        [[ -x "$staging/usr/bin/$operator" ]] || return 97
        cmp "$staging/usr/bin/$operator" "$CARGO_TARGET_DIR/$rust_target/release/$operator" || return 97
    done
    for completion in bash-completion/completions/memory zsh/vendor-completions/_memory fish/vendor_completions.d/memory.fish; do
        [[ "$(< "$staging/usr/share/$completion")" == 'fixture completion' ]] || return 97
    done
    [[ -x "$staging/usr/libexec/memory-layer/ca-preflight" ]] || return 98
    [[ -f "$staging/usr/libexec/memory-layer/area-release.py" ]] || return 98
    for guide in CA-CONSOLE-HANDOFF.md area-import.md release-staging.md; do
        [[ -s "$staging/usr/share/doc/memory-layer/areas/$guide" ]] || return 98
    done
    printf 'fixture deb\n' > "$4"
    log_command "dpkg-deb $*"
}
export -f cargo npm rustc dpkg-deb dpkg-shlibdeps
export CARGO_TARGET_DIR="$test_root/binaries"
: > "$PACKAGING_TEST_LOG"
(umask 077; bash "$fixture/packaging/build-deb.sh" --arch amd64 >/dev/null)
[[ "$(< "$test_root/unrelated-build/evidence")" == preserve ]] || fail "build deleted unrelated staging files"
[[ -L "$fixture/target/debian/memory-layer-amd64" ]] || fail "build replaced an unrelated staging symlink"
if compgen -G "$fixture/target/debian/.memory-layer-amd64.*" >/dev/null; then
    fail "private build staging directory was not cleaned up"
fi
grep -q '^dpkg-deb ' "$PACKAGING_TEST_LOG" || fail "builder did not assemble the expected package"
grep -q '^cargo build --release --locked -p mem-area -p mem-area-ops .*--target x86_64-unknown-linux-gnu$' "$PACKAGING_TEST_LOG" || fail "operators were not built with locked dependencies and selected architecture"
grep -q '^cargo build --release --locked -p mem-federation-runtime .*--bin memory-federation-gateway --target x86_64-unknown-linux-gnu$' "$PACKAGING_TEST_LOG" || fail "gateway was not built with locked dependencies and selected architecture"
if grep -q 'cargo build --locked -p mem-cli' "$PACKAGING_TEST_LOG"; then
    fail "native packaging unnecessarily rebuilt a lean CLI"
fi
: > "$PACKAGING_TEST_LOG"
if bash "$fixture/packaging/build-deb.sh" --arch amd64 >/dev/null 2>&1; then
    fail "builder overwrote an existing release artifact"
fi
[[ ! -s "$PACKAGING_TEST_LOG" ]] || fail "existing artifact triggered a build"
[[ "$(< "$fixture/target/debian/memory-layer_2.1.0-rc.1_amd64.deb")" == 'fixture deb' ]] ||
    fail "existing artifact contents changed"

# Model a non-executable ARM64 target: running it for completions fails the test.
# Only the separately built host CLI may execute. Explicit output stays off root.
mkdir -p "$test_root/binaries/aarch64-unknown-linux-gnu/release" "$test_root/binaries/x86_64-unknown-linux-gnu/debug"
cp "$test_root/binaries/x86_64-unknown-linux-gnu/release/memory" "$test_root/binaries/x86_64-unknown-linux-gnu/debug/memory"
for operator in memory memory-area memory-area-import memory-federation-gateway; do
    printf '#!/usr/bin/env bash\nexit 99 # target binaries must not execute during cross packaging\n' > "$test_root/binaries/aarch64-unknown-linux-gnu/release/$operator"
    chmod +x "$test_root/binaries/aarch64-unknown-linux-gnu/release/$operator"
done
: > "$PACKAGING_TEST_LOG"
(umask 077; bash "$fixture/packaging/build-deb.sh" --arch arm64 --output-dir "$test_root/release output" >/dev/null)
[[ -s "$test_root/release output/memory-layer_2.1.0-rc.1_arm64.deb" ]] || fail "cross package missing from explicit output directory"
grep -q '^cargo build --locked -p mem-cli --no-default-features --features tui .*--bin memory --target x86_64-unknown-linux-gnu$' "$PACKAGING_TEST_LOG" || fail "cross completions did not build the host-native lean CLI"
grep -q '^cargo build --release --locked -p mem-federation-runtime .*--target aarch64-unknown-linux-gnu$' "$PACKAGING_TEST_LOG" || fail "cross gateway targeted the wrong architecture"
dpkg --compare-versions 2.1.0~rc.1 lt 2.1.0 || fail "Debian candidate does not sort before stable"
dpkg --compare-versions 2.1.0~rc.1 gt 2.0.0 || fail "Debian candidate does not sort after the prior stable"

if PACKAGING_TEST_SHLIB_STATUS=1 bash "$fixture/packaging/build-deb.sh" --output-dir "$test_root/missing-dependencies" >/dev/null 2>&1; then
    fail "builder ignored missing runtime metadata"
fi
if PACKAGING_TEST_SHLIB_INVALID=1 bash "$fixture/packaging/build-deb.sh" --output-dir "$test_root/invalid-dependencies" >/dev/null 2>&1; then
    fail "builder accepted unresolved or unversioned runtime metadata"
fi

for arguments in '--arch' '--target' '--output-dir' '--arch mips' '--arch amd64 --target aarch64-unknown-linux-gnu' '--target ../../other'; do
    : > "$PACKAGING_TEST_LOG"
    # Intentional word splitting of fixed, test-owned argument fixtures.
    # shellcheck disable=SC2086
    if bash "$fixture/packaging/build-deb.sh" $arguments >/dev/null 2>&1; then
        fail "accepted invalid builder arguments: $arguments"
    fi
    [[ ! -s "$PACKAGING_TEST_LOG" ]] || fail "invalid arguments triggered a build"
done

echo "PASS: Debian lifecycle, named-instance boundaries and isolated package staging"
