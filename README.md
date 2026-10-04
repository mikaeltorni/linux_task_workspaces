# Linux Task Workspaces

Rust CLI for named GNOME workspace window layouts, monitor positions, and attached
tmux sessions. Standalone, user-owned installation for Ubuntu/GNOME 45–48.

## Disclaimer

This software is provided under the MIT License on an **“as is”** basis, without warranties of any kind. To the maximum extent permitted by applicable law, the authors and copyright holders shall not be liable for any claims, damages, losses, or other liability arising from the use of this software.

You are solely responsible for determining whether this software is suitable, safe, lawful, and appropriate for your intended use. Unless explicitly stated otherwise, this project is general-purpose software and is not designed, tested, certified, or approved for safety-critical, medical, automotive, aviation, industrial-control, life-support, cybersecurity-critical, financial-critical, or other high-risk use cases.

The authors and copyright holders make no guarantees regarding security, reliability, availability, correctness, compliance, non-infringement, or fitness for any particular purpose.

This notice is intended to clarify the nature of the project and does not impose additional restrictions beyond the MIT License.

Install: `bash install.sh` (also `--default` / `--all`). The installer uses the
shared installer helpers from a sibling checkout or downloads them. User mode
reports exact sudo commands for missing packages; sudo targets the desktop user.
The public entrypoint is the compiled binary `~/.local/bin/tws`. Building requires
a current Rust toolchain; runtime needs tmux, GTK's `gtk-launch`, and GNOME.

```sh
tws create backups       # remember windows on the current workspace
tws list
tws backups show         # inspect the saved snapshot
tws --help
tws configure backups   # terminal wizard; creates a snapshot if needed
tws configure backups --open Super+B
tws configure --launcher Control+Alt+Super+W
```

`tws configure` prompts for a workspace ID. Its shortcut opens that CLI in Kitty
(override the terminal program with `TWS_TERMINAL`). The open shortcut restores
and focuses its saved workspace. `_activate ID` is the internal hotkey handler.
No shortcuts are assigned until you configure them.

Snapshots live in `${XDG_CONFIG_HOME:-~/.config}/task-workspaces/ID.json` with
private permissions; `TWS_CONFIG_HOME` overrides that directory. Window frames
are saved relative to named monitor connectors and scaled if resolution changes.
Disconnected monitors fall back to a visible monitor. Sticky windows are excluded.
Tmux clients are linked to their terminal process; saved sessions record their
socket, names, windows, panes, directories and layout. Running processes are not
replayed after reboot. Browser URLs and file-manager paths are not yet captured.

The GNOME bridge must be active: on Wayland a newly installed extension is
discovered after manual logout/login; on X11 use GNOME's run-dialog reload.
No installer terminates or restarts your session.

Checks: `cargo test --locked`, `cargo clippy --locked --all-targets -- -D warnings`,
`cargo fmt --check`, and `node --test tests/*.test.mjs`. Tests isolate the desktop,
tmux server and installation home.
