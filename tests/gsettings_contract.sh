#!/usr/bin/env bash
# Run: bash tests/gsettings_contract.sh "$PWD/target/debug/tws"
# Public-interface coverage: launcher registration/readback; own-path rebinding;
# conflict preserves the existing chord and custom path list; clear unregisters.
# Installer bootstrap assigns a free launcher chord, retains user rebinding on
# reinstall, and preserves settings if the default chord belongs to GNOME.
# Private D-Bus and temporary HOME prevent changes to the real desktop.
set -euo pipefail
if [[ ${TWS_TEST_BUS:-0} != 1 ]]; then
    test_home=$(mktemp -d)
    trap 'rm -rf "$test_home"' EXIT
    HOME="$test_home" XDG_CONFIG_HOME="$test_home/config" XDG_DATA_HOME="$test_home/data" \
        XDG_STATE_HOME="$test_home/state" TWS_TEST_BUS=1 dbus-run-session -- bash "$0" "$1"
    exit
fi
bin=$1
schema=org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/tws-configuration-launcher/
media=org.gnome.settings-daemon.plugins.media-keys
"$bin" configure --launcher Control+Alt+Super+F12
[[ $(gsettings get "$schema" binding) == "'<Alt><Control><Super>F12'" ]]
"$bin" configure --launcher Control+Alt+Super+F12
expected=$(gsettings get "$media" custom-keybindings)
gsettings set org.gnome.desktop.wm.keybindings switch-to-workspace-6 "['<Super>w']"
if "$bin" configure --launcher Super+W; then exit 1; fi
[[ $(gsettings get "$schema" binding) == "'<Alt><Control><Super>F12'" ]]
[[ $(gsettings get "$media" custom-keybindings) == "$expected" ]]
"$bin" configure --launcher ''
[[ $(gsettings get "$media" custom-keybindings) == '@as []' || $(gsettings get "$media" custom-keybindings) == '[]' ]]
fixture=$(mktemp -d)
trap 'rm -rf "$fixture"' EXIT
mkdir -p "$fixture/bin" "$fixture/framework"
root=$(cd "$(dirname "$0")/.." && pwd)
cp -r "$root/.installer-support/." "$fixture/framework/"
for name in systemctl gnome-extensions; do
    printf '#!/bin/bash\nexit 0\n' > "$fixture/bin/$name"
    chmod +x "$fixture/bin/$name"
done
cat > "$fixture/framework/installer_helpers.sh" <<'HELPERS'
# Run a fixture command with isolated user context.
# Parameters: $@ - command and arguments. Returns: command exit status.
run_as_target() {
    printf 'arguments=%s\n' "$*" >&2
    local status=0
    "$@" || status=$?
    printf '%s\n' "$status" >&2
    return "$status"
}
# Emulate a non-root installation.
# Parameters: none. Returns: one.
is_root() { printf 'parameters=none\n1\n' >&2; return 1; }
HELPERS
export ISC_FUNCTIONS_DIR="$fixture/framework" TWS_INSTALL_HOME="$HOME" TWS_BINARY="$bin"
export PATH="$fixture/bin:$PATH"
root=$(cd "$(dirname "$0")/.." && pwd)
bash "$root/install.sh"
[[ $(gsettings get "$schema" binding) == "'<Alt><Control><Super>w'" ]]
"$HOME/.local/bin/tws" configure --launcher Control+Alt+Super+F12
bash "$root/install.sh"
[[ $(gsettings get "$schema" binding) == "'<Alt><Control><Super>F12'" ]]
"$HOME/.local/bin/tws" configure --launcher ''
gsettings set org.gnome.desktop.wm.keybindings switch-to-workspace-6 "['<Alt><Control><Super>w']"
before=$(gsettings get "$media" custom-keybindings)
bash "$root/install.sh"
[[ $(gsettings get "$media" custom-keybindings) == "$before" ]]
[[ $(gsettings get org.gnome.desktop.wm.keybindings switch-to-workspace-6) == "['<Alt><Control><Super>w']" ]]
