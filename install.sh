#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
TARGET_USER=${SUDO_USER:-${TARGET_USER:-$(id -un)}}
TARGET_HOME=${TWS_INSTALL_HOME:-$(getent passwd "$TARGET_USER" | cut -d: -f6)}
RUNTIME_DIR=/run/user/$(id -u "$TARGET_USER")
USER_BUS=unix:path=$RUNTIME_DIR/bus
DISPLAY_VAL=${DISPLAY:-:0}
for candidate in "${ISC_FUNCTIONS_DIR:-}" "$SCRIPT_DIR/../linux_installation_scripts_functions" "$TARGET_HOME/projects/linux_installation_scripts_functions"; do
    if [[ -n "$candidate" && -f "$candidate/component_loader.sh" ]]; then
        source "$candidate/component_loader.sh"
        break
    fi
done
if ! declare -F isc_load_helpers >/dev/null; then
    loader=$(mktemp)
    trap 'rm -f "$loader"' EXIT
    curl -fsSL "https://raw.githubusercontent.com/mikaeltorni/linux_installation_scripts_functions/${ISC_FUNCTIONS_REF:-master}/component_loader.sh" -o "$loader"
    source "$loader"
fi
isc_load_helpers
[[ "$TARGET_USER" != root ]] || { printf 'Set TARGET_USER to the desktop user.\n' >&2; exit 1; }
case "${1:-}" in
    --help|-h) printf 'bash install.sh [--default|--all]\n'; exit 0 ;;
    --default|--all|'') ;;
    *) printf 'Unknown installer option: %s\n' "$1" >&2; exit 1 ;;
esac
SUDO_REQUIRED_STEPS=()
for package in tmux libglib2.0-bin; do
    if ! dpkg-query -W -f='${Status}' "$package" 2>/dev/null | grep -q 'install ok installed'; then
        if is_root && [[ ${TWS_INSTALL_OFFLINE:-0} != 1 ]]; then
            apt_install "$package"
        else
            SUDO_REQUIRED_STEPS+=("sudo apt-get install -y $package")
        fi
    fi
done
binary=${TWS_BINARY:-$SCRIPT_DIR/target/release/tws}
if [[ -z ${TWS_BINARY:-} ]]; then
    if ! command -v cargo >/dev/null; then
        printf 'Rust compiler required. Install Rust, or run: sudo apt-get install -y cargo rustc\n' >&2
        exit 1
    fi
    run_as_target cargo build --manifest-path "$SCRIPT_DIR/Cargo.toml" --locked --release
fi
extension="$TARGET_HOME/.local/share/gnome-shell/extensions/task-workspaces@local"
run_as_target mkdir -p "$extension" "$TARGET_HOME/.local/bin"
run_as_target cp "$SCRIPT_DIR/extension/extension.js" "$SCRIPT_DIR/extension/geometry.js" "$SCRIPT_DIR/extension/metadata.json" "$extension/"
run_as_target install -m 755 "$binary" "$TARGET_HOME/.local/bin/tws"
if [[ ${TWS_INSTALL_OFFLINE:-0} != 1 ]]; then
    if ! run_as_target gnome-extensions enable task-workspaces@local; then
        current=$(run_as_target gsettings get org.gnome.shell enabled-extensions)
        if [[ "$current" != *"'task-workspaces@local'"* ]]; then
            if [[ "$current" == '[]' || "$current" == '@as []' ]]; then
                current="['task-workspaces@local']"
            else
                current="${current%]}, 'task-workspaces@local']"
            fi
            run_as_target gsettings set org.gnome.shell enabled-extensions "$current"
        fi
        printf 'Extension registered for next login; GNOME will discover it after logout/login on Wayland.\n'
    fi
    run_as_target gsettings get org.gnome.shell enabled-extensions
fi
printf 'Installed %s/.local/bin/tws\n' "$TARGET_HOME"
if ((${#SUDO_REQUIRED_STEPS[@]})); then
    printf 'Root-only dependencies pending:\n'
    printf '  %s\n' "${SUDO_REQUIRED_STEPS[@]}"
fi
