#!/usr/bin/env bash
# Run: bash tests/installer_contract.sh /absolute/path/to/tws
# Coverage: help without helper/network access; clean user installation bootstraps
# a target-home toolchain; reuse is idempotent; offline skips network/desktop.
# Fake commands intercept every download, build, settings and service operation.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
fixture=$(mktemp -d)
trap 'rm -rf "$fixture"' EXIT
mkdir -p "$fixture/repo" "$fixture/home" "$fixture/bin" "$fixture/framework"
cp "$root/install.sh" "$root/Cargo.toml" "$fixture/repo/"
cp -r "$root/extension" "$fixture/repo/"
export TWS_INSTALL_HOME="$fixture/home" ISC_FUNCTIONS_DIR="$fixture/framework"
export TWS_TEST_BINARY="$1" TWS_TEST_CALLS="$fixture/calls"
export PATH="$fixture/bin:/usr/bin:/bin"
cat > "$fixture/bin/curl" <<'SCRIPT'
#!/bin/bash
printf 'curl\n' >> "$TWS_TEST_CALLS"
while (($#)); do
    if [[ "$1" == -o ]]; then destination=$2; shift 2; else shift; fi
done
cat > "$destination" <<'BOOTSTRAP'
#!/bin/bash
printf 'rustup\n' >> "$TWS_TEST_CALLS"
mkdir -p "$HOME/.cargo/bin"
cat > "$HOME/.cargo/bin/cargo" <<'CARGO'
#!/bin/bash
printf 'cargo\n' >> "$TWS_TEST_CALLS"
while (($#)); do
    if [[ "$1" == --manifest-path ]]; then manifest=$2; shift 2; else shift; fi
done
mkdir -p "$(dirname "$manifest")/target/release"
cp "$TWS_TEST_BINARY" "$(dirname "$manifest")/target/release/tws"
CARGO
chmod +x "$HOME/.cargo/bin/cargo"
BOOTSTRAP
SCRIPT
cat > "$fixture/framework/component_loader.sh" <<'HELPERS'
isc_load_helpers() {
    run_as_target() { env HOME="$TARGET_HOME" "$@"; }
    is_root() { return 1; }
}
HELPERS
for name in gsettings gnome-extensions systemctl dpkg-query; do
    cat > "$fixture/bin/$name" <<'COMMAND'
#!/bin/bash
printf '%s\n' "${0##*/}" >> "$TWS_TEST_CALLS"
if [[ "${0##*/}" == dpkg-query ]]; then printf 'install ok installed\n'; fi
if [[ "${0##*/}" == gsettings ]]; then printf "['task-workspaces@local']\n"; fi
COMMAND
done
chmod +x "$fixture/bin/"*
ISC_FUNCTIONS_DIR="$fixture/missing" bash "$fixture/repo/install.sh" --help
[[ ! -e "$TWS_TEST_CALLS" ]]
bash "$fixture/repo/install.sh"
[[ $(rg -c '^rustup$' "$TWS_TEST_CALLS") == 1 ]]
"$fixture/home/.local/bin/tws" --help | rg -q 'ID load'
[[ -f "$fixture/home/.config/systemd/user/task-workspaces.service" ]]
bash "$fixture/repo/install.sh"
[[ $(rg -c '^rustup$' "$TWS_TEST_CALLS") == 1 ]]
before=$(wc -l < "$TWS_TEST_CALLS")
TWS_INSTALL_OFFLINE=1 TWS_BINARY="$1" bash "$fixture/repo/install.sh"
tail -n +$((before + 1)) "$TWS_TEST_CALLS" > "$fixture/offline"
! rg 'curl|rustup|systemctl|gsettings|gnome-extensions' "$fixture/offline"
