#!/usr/bin/env bash
# Install a previously built Linear TUI for the current operator.
set -euo pipefail

source_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
[[ $# == 1 ]] || { printf 'Usage: %s BIN_DIRECTORY\n' "$0" >&2; exit 2; }
bin_dir=$(cd -- "$1" && pwd)
for executable in codifix codifix-agency; do
    [[ -x "$bin_dir/$executable" ]] || { printf 'Missing %s\n' "$bin_dir/$executable" >&2; exit 1; }
done

local_bin=$HOME/.local/bin
libexec=$HOME/.local/libexec/codifix-linear
install -d -m 0755 "$local_bin" "$libexec" "$libexec/private-pds"
install -m 0755 "$bin_dir/codifix" "$bin_dir/codifix-agency" "$libexec/"
install -m 0644 "$source_dir/linear-tui.py" "$libexec/"
install -m 0644 "$source_dir/private-pds/"{compose.yaml,directory.cjs,install.sh} "$libexec/private-pds/"
wrapper=$(mktemp "$local_bin/.codifix-linear.XXXXXX")
trap 'rm -f -- "$wrapper"' EXIT
cat > "$wrapper" <<'WRAPPER'
#!/usr/bin/env bash
set -euo pipefail
libexec=$HOME/.local/libexec/codifix-linear
exec python3 "$libexec/linear-tui.py" --bin-dir "$libexec" "$@"
WRAPPER
chmod 0755 "$wrapper"
mv -fT -- "$wrapper" "$local_bin/codifix-linear"
printf 'Installed %s\n' "$local_bin/codifix-linear"
