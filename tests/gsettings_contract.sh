#!/usr/bin/env bash
# Run: bash tests/gsettings_contract.sh "$PWD/target/debug/tws"
# Public-interface coverage: launcher registration/readback; own-path rebinding;
# conflict preserves the existing chord and custom path list; clear unregisters.
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
