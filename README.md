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
tws configure backups --mode always     # keep windows available after login
tws configure backups --mode on-demand  # open on its shortcut
tws configure backups --close Ctrl+Super+B
tws configure backups --close-action hide  # default; or close
tws backups hide
tws backups close
tws conflicts Super+B       # inspect existing GNOME/extension shortcuts
tws edit backups            # CLI wizard; tws backups edit also works
tws backups edit --json     # $TWS_EDITOR / $EDITOR / nano
tws backups edit --file /path/to/profile.json
TWS_EDIT_ID=backups tws
tws configure backups --launch 0 '["nautilus","--new-window","/home/mk/My Backups"]'
tws backups save          # replace snapshot from the task's permanent workspace
```

`tws configure` prompts for a workspace ID. Its shortcut opens that CLI in Kitty
(override the terminal program with `TWS_TERMINAL`). The open shortcut restores
and focuses its saved workspace. `_activate ID` is the internal hotkey handler.
No shortcuts are assigned until you configure them.

Every assigned shortcut is normalized and checked against GNOME settings,
registered custom shortcuts, installed GNOME extension schemas, and other saved
task bindings. A collision is rejected with its owner; existing bindings are
never cleared. App-local shortcuts and hardcoded third-party input grabs are
outside GNOME's settings registry. JSON edits use a private draft and validate
before replacing the saved profile. `TWS_COMMAND=tasks bash install.sh` installs
an additional `tasks` command; `tws` remains available for service compatibility.

New task slots are appended after existing workspaces and named `Task: ID`;
creation preserves the snapshot's source workspace, reserving the new slot until
opened. Static workspace count is retained, and workspaces span all monitors.
The `task-workspaces.service` reconciles permanent slots and `always` windows
every 20 seconds without focusing them. `on-demand` windows open only when
requested. Hide minimizes just saved windows and preserves running tmux jobs;
close asks those application windows to close, keeping detached tmux jobs alive.
Either suspends automatic reopening until the next open action. `_dismiss ID`,
`_startup`, and `_watch` are internal shortcut/service entrypoints.

Snapshots live in `${XDG_CONFIG_HOME:-~/.config}/task-workspaces/ID.json` with
private permissions; `TWS_CONFIG_HOME` overrides that directory. Window frames
are saved relative to named monitor connectors and scaled if resolution changes.
Disconnected monitors fall back to a visible monitor. Sticky windows are excluded.
Tmux clients are linked to their terminal process; saved sessions record their
socket, names, windows, panes, directories and layout. Running processes are not
replayed after reboot.

`save` captures every normal non-sticky window in the task's permanent slot,
including Chrome/Chromium windows and attached tmux clients. Chrome tabs are
read from its local SNSS v1/v3 session files, matched by current title and saved
geometry, and reopened as separate windows with the same profile and tab URLs.
There is no remote-debugging port or browser restart. Chrome can delay disk
flushes; retry save after a few seconds if needed. Incognito, encrypted session
formats and ambiguous window matches require an explicit URL argv via
`configure ID --launch INDEX JSON_ARGV`; failed capture preserves the last save.
Manual launch commands survive subsequent saves. `create` initially remembers
window identities; Chrome URL capture happens when you save the arranged task.

Nautilus folders are captured through its `OpenWindowsWithLocations` D-Bus
property and matched to GNOME's GTK window object path, preserving each window's
URI list (including remote folders). Reopening uses `nautilus --new-window`;
Nautilus may open saved tab locations in separate windows. Other file managers
can use `--launch INDEX JSON_ARGV` (zero-based snapshot window index) or a JSON
edit to set their launch command and folder. Launch arrays preserve spaces and
never pass through a shell; use `["bash","-lc","..."]` only if you want a shell.

The GNOME bridge must be active: on Wayland a newly installed extension is
discovered after manual logout/login; on X11 use GNOME's run-dialog reload.
No installer terminates or restarts your session.

Checks: `cargo test --locked`, `cargo clippy --locked --all-targets -- -D warnings`,
`cargo fmt --check`, and `node --test tests/*.test.mjs`. Tests isolate the desktop,
tmux server and installation home.
