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
    'ExecStartPre=/usr/libexec/memory-layer/verify-instance-release %i %d/memory.sha256' \
    'ExecStart=/opt/memory-layer/instances/%i/release/bin/memory --config %d/memory-layer.toml service run' \
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
    "$fixture/web/dist" "$fixture/target/debian" "$test_root/binaries/x86_64-unknown-linux-gnu/release"
cp "$repo_root/packaging/build-deb.sh" "$fixture/packaging/"
cp -R "$repo_root/packaging/debian" "$fixture/packaging/"
cp "$repo_root/deploy/areas/README.md" "$repo_root/deploy/areas/instance.toml.example" "$fixture/deploy/areas/"
printf '[workspace.package]\nversion = "2.1.0-rc.1"\n' > "$fixture/Cargo.toml"
printf 'Fixture README\n' > "$fixture/README.md"
printf '[service]\n' > "$fixture/memory-layer.toml.example"
printf 'Fixture\n' > "$fixture/.agents/skills/SKILL.md"
printf 'Fixture\n' > "$fixture/web/dist/index.html"
printf '#!/usr/bin/env bash\nprintf "fixture completion\\n"\n' > "$test_root/binaries/x86_64-unknown-linux-gnu/release/memory"
chmod +x "$test_root/binaries/x86_64-unknown-linux-gnu/release/memory"
mkdir "$test_root/unrelated-build"
printf 'preserve\n' > "$test_root/unrelated-build/evidence"
ln -s "$test_root/unrelated-build" "$fixture/target/debian/memory-layer-amd64"

cargo() { log_command "cargo $*"; }
npm() { log_command "npm $*"; }
dpkg-deb() {
    [[ "$1" == --root-owner-group && "$2" == --build ]] || return 90
    local staging="$3"
    [[ -f "$staging/lib/systemd/system/memory-layer@.service" ]] || return 91
    [[ -x "$staging/usr/libexec/memory-layer/verify-instance-release" ]] || return 92
    [[ -f "$staging/usr/share/doc/memory-layer/areas/instance.toml.example" ]] || return 93
    [[ ! -e "$staging/opt/memory-layer" ]] || return 94
    [[ ! -e "$staging/etc/memory-layer/instances" ]] || return 95
    [[ ! -e "$staging/var/lib/memory-layer" ]] || return 96
    printf 'fixture deb\n' > "$4"
    log_command "dpkg-deb $*"
}
export -f cargo npm dpkg-deb
export CARGO_TARGET_DIR="$test_root/binaries"
: > "$PACKAGING_TEST_LOG"
bash "$fixture/packaging/build-deb.sh" --arch amd64 >/dev/null
[[ "$(< "$test_root/unrelated-build/evidence")" == preserve ]] || fail "build deleted unrelated staging files"
[[ -L "$fixture/target/debian/memory-layer-amd64" ]] || fail "build replaced an unrelated staging symlink"
if compgen -G "$fixture/target/debian/.memory-layer-amd64.*" >/dev/null; then
    fail "private build staging directory was not cleaned up"
fi
grep -q '^dpkg-deb ' "$PACKAGING_TEST_LOG" || fail "builder did not assemble the expected package"
: > "$PACKAGING_TEST_LOG"
if bash "$fixture/packaging/build-deb.sh" --arch amd64 >/dev/null 2>&1; then
    fail "builder overwrote an existing release artifact"
fi
[[ ! -s "$PACKAGING_TEST_LOG" ]] || fail "existing artifact triggered a build"
[[ "$(< "$fixture/target/debian/memory-layer_2.1.0-rc.1_amd64.deb")" == 'fixture deb' ]] ||
    fail "existing artifact contents changed"

for arguments in '--arch' '--target' '--arch mips' '--arch amd64 --target aarch64-unknown-linux-gnu' '--target ../../other'; do
    : > "$PACKAGING_TEST_LOG"
    # Intentional word splitting of fixed, test-owned argument fixtures.
    # shellcheck disable=SC2086
    if bash "$fixture/packaging/build-deb.sh" $arguments >/dev/null 2>&1; then
        fail "accepted invalid builder arguments: $arguments"
    fi
    [[ ! -s "$PACKAGING_TEST_LOG" ]] || fail "invalid arguments triggered a build"
done

echo "PASS: Debian lifecycle, named-instance boundaries and isolated package staging"
