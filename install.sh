#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
case "${1:-}" in
    --help|-h) printf 'bash install.sh [--default|--all]\n'; exit 0 ;;
    --default|--all|'') ;;
    *) printf 'Unknown installer option: %s\n' "$1" >&2; exit 1 ;;
esac
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
SUDO_REQUIRED_STEPS=()
for package in tmux libglib2.0-bin libgtk-3-bin kitty build-essential curl ca-certificates; do
    if ! dpkg-query -W -f='${Status}' "$package" 2>/dev/null | grep -q 'install ok installed'; then
        if is_root && [[ ${TWS_INSTALL_OFFLINE:-0} != 1 ]]; then
            apt_install "$package"
        else
            SUDO_REQUIRED_STEPS+=("sudo apt-get install -y $package")
        fi
    fi
done
binary=${TWS_BINARY:-$SCRIPT_DIR/target/release/tws}
command_name=${TWS_COMMAND:-tws}
[[ "$command_name" =~ ^[a-zA-Z][a-zA-Z0-9_-]*$ ]] || { printf 'Invalid TWS_COMMAND\n' >&2; exit 1; }
if [[ -z ${TWS_BINARY:-} ]]; then
    build_path="$TARGET_HOME/.cargo/bin:$PATH"
    if ! run_as_target env PATH="$build_path" sh -c 'command -v cargo >/dev/null'; then
        if [[ ${TWS_INSTALL_OFFLINE:-0} == 1 ]]; then
            printf 'Offline build requires Rust 1.85+, or TWS_BINARY=/path/to/prebuilt/tws.\n' >&2
            exit 1
        fi
        bootstrap=$(mktemp)
        curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs -o "$bootstrap"
        chmod 644 "$bootstrap"
        if ! run_as_target env CARGO_HOME="$TARGET_HOME/.cargo" RUSTUP_HOME="$TARGET_HOME/.rustup" \
            sh "$bootstrap" -y --profile minimal --no-modify-path; then
            rm -f "$bootstrap"
            exit 1
        fi
        rm -f "$bootstrap"
    fi
    run_as_target env PATH="$build_path" CARGO_HOME="$TARGET_HOME/.cargo" RUSTUP_HOME="$TARGET_HOME/.rustup" \
        cargo build --manifest-path "$SCRIPT_DIR/Cargo.toml" --locked --release
fi
extension="$TARGET_HOME/.local/share/gnome-shell/extensions/task-workspaces@local"
run_as_target mkdir -p "$extension" "$TARGET_HOME/.local/bin"
run_as_target cp "$SCRIPT_DIR/extension/extension.js" "$SCRIPT_DIR/extension/geometry.js" "$SCRIPT_DIR/extension/metadata.json" "$extension/"
run_as_target install -m 755 "$binary" "$TARGET_HOME/.local/bin/tws"
if [[ "$command_name" != tws ]]; then
    run_as_target install -m 755 "$binary" "$TARGET_HOME/.local/bin/$command_name"
fi
run_as_target mkdir -p "$TARGET_HOME/.config/systemd/user"
unit=$(mktemp)
trap 'rm -f "$unit"' EXIT
cat > "$unit" <<UNIT
[Unit]
Description=Keep named task workspaces and always-on windows available
After=graphical-session.target
PartOf=graphical-session.target
[Service]
ExecStart="$TARGET_HOME/.local/bin/tws" _watch
Restart=on-failure
RestartSec=10
[Install]
WantedBy=graphical-session.target
UNIT
run_as_target install -m 644 "$unit" "$TARGET_HOME/.config/systemd/user/task-workspaces.service"
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
    run_as_target systemctl --user daemon-reload
    run_as_target systemctl --user enable task-workspaces.service
    run_as_target systemctl --user restart task-workspaces.service
fi
printf 'Installed %s/.local/bin/tws\n' "$TARGET_HOME"
if ((${#SUDO_REQUIRED_STEPS[@]})); then
    printf 'Root-only dependencies pending:\n'
    printf '  %s\n' "${SUDO_REQUIRED_STEPS[@]}"
fi
