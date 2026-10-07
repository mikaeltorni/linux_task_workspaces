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
a Rust 1.85+ toolchain. A missing toolchain is bootstrapped with rustup into the
desktop user's home; an existing older toolchain needs `rustup update stable`.
Runtime needs tmux, GTK's `gtk-launch`, Kitty (or `TWS_TERMINAL`), and GNOME.
The installer declares those packages plus compiler prerequisites. For offline
deployment, supply `TWS_BINARY=/path/to/prebuilt/tws TWS_INSTALL_OFFLINE=1`;
this copies assets without downloading or touching desktop settings/services.
Offline installation also needs the shared helpers present as a sibling or
through `ISC_FUNCTIONS_DIR=/path/to/linux_installation_scripts_functions`.

For the backup workflow, first activate the GNOME bridge with a manual
logout/login on Wayland. On a workspace containing the windows you want to save,
arrange the two Chrome windows on your top monitor, open your folder windows,
and attach your tmux sessions in Kitty. Then run:

```sh
tws create backups
tws backups load        # moves the remembered windows into a new permanent slot
tws backups save        # capture its arranged windows, Chrome URLs and tmux state
tws configure backups  # choose free open/close hotkeys and the startup mode
```

Later, use your chosen open shortcut or `tws backups load`. After rearranging
windows or changing tabs/folders, run `tws backups save` from any workspace;
it saves the task's assigned slot. **Ctrl+Alt+Super+W** opens the configuration
wizard. To edit an existing task, use `tws edit backups` or
`TWS_EDIT_ID=backups tws`. The command can also be renamed during installation.

The repository is local. The master setup chain accepts an existing sibling
checkout or `TWS_REPO_SOURCE=/path/to/linux_task_workspaces` (a transferred Git
checkout or bare repository) for a clean installation; no published GitHub
remote is assumed. Its atomic installer uses the shared helper repository and
the setup menu automatically selects tmux when required.

```sh
tws create backups       # remember windows on the current workspace
tws list
tws backups show         # inspect the saved snapshot
tws --help
tws configure backups   # terminal wizard; creates a snapshot if needed
tws configure backups --open Super+B
tws configure --launcher Control+Alt+Super+W
tws configure backups --mode always     # keep windows available after login
tws configure backups --workspace 3    # choose GNOME workspace 3 (numbered from 1)
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
tws backups load          # open or reuse windows and restore monitor placement
TWS_LOAD_ID=backups tws
```

`tws configure` prompts for a workspace ID. Its shortcut opens that CLI in Kitty
(override the terminal program with `TWS_TERMINAL`). The open shortcut restores
and focuses its saved workspace. `_activate ID` is the internal hotkey handler.
Installation assigns **Ctrl+Alt+Super+W** to this configuration wizard if free,
preserving an existing launcher on reinstall. Use `TWS_CONFIG_HOTKEY=CHORD` to
choose another install-time key or `TWS_CONFIG_HOTKEY=none` to skip registration.
Task open/close shortcuts are assigned when you configure each task.

Every assigned shortcut is normalized and checked against GNOME settings,
registered custom shortcuts, installed GNOME extension schemas, and other saved
task bindings. A collision is rejected with its owner; existing bindings are
never cleared. App-local shortcuts and hardcoded third-party input grabs are
outside GNOME's settings registry. JSON edits use a private draft and validate
before replacing the saved profile. `TWS_COMMAND=tasks bash install.sh` installs
an additional `tasks` command; `tws` remains available for service compatibility.

New task slots are appended after existing workspaces and named `Task: ID`.
GNOME supports 36 static workspaces in total; creation rejects exhausted slots.
Creation preserves the snapshot's source workspace, reserving the new slot until
opened. Static workspace count is retained, and workspaces span all monitors.

Choose an existing or new numbered workspace in `tws configure backups` or
`tws edit backups`, or assign it directly:

```sh
tws configure backups --workspace 3
tws configure backups --mode always
tws backups load
```

The CLI and wizard accept GNOME workspace numbers 1–36; the JSON `workspace`
field uses indices 0–35. Direct assignment can create a missing task from the
current workspace's windows without allocating an extra slot, including when
all 36 workspaces already exist. Existing destination names are preserved;
unnamed destinations receive `Task: ID`. Tasks may share a workspace. Changing
the assignment retains the saved windows, monitor layout, launch commands and
tmux sessions; the previous static workspace remains available. `load` moves
the remembered windows to the chosen workspace immediately; `always` mode does
so during automatic reconciliation. Later `save` captures the chosen workspace.
Attach the desired tmux session in each terminal before capturing the task;
its saved terminal windows will then reopen on that workspace after login.

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
replayed after reboot. Tmux retains your running compression/upload jobs while
you hide a workspace; after reboot, session structure and directories can be
recreated, but commands such as `za` must be started again. Terminal windows
that share one process may expose several attached sessions; those structures
are saved, and the first session is attached when reopening that window.

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

The GNOME bridge must be active: on Wayland a newly installed or updated extension
requires manual logout/login; on X11 use GNOME's run-dialog reload.
No installer terminates or restarts your session.

Checks: `cargo test --locked`, `cargo clippy --locked --all-targets -- -D warnings`,
`cargo fmt --check`, and `node --test tests/*.test.mjs`. Tests isolate the desktop,
tmux server and installation home.

MIT licensed; see [LICENSE](LICENSE).
