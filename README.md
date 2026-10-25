# Linux Task Workspaces

Rust CLI for named GNOME workspace window layouts, monitor positions, and durable
tmux sessions with saved pane output. Standalone, user-owned installation for Ubuntu/GNOME 45–48.

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
tws configure backups --workspace 3  # persistent on a chosen existing/new slot
tws backups load        # moves the remembered windows into workspace 3
tws backups save        # capture its arranged windows, Chrome URLs and tmux state
```

Numbered assignment defaults to `always`, clears any previous task shortcuts,
and needs no hotkey. `tws configure backups` and both edit aliases ask only for
the numbered destination when it is assigned. You can still explicitly override
its mode with `--mode on-demand` or request a shortcut with `--open` / `--close`.

For a task without an explicitly assigned workspace, run `tws configure backups`
and keep `-` at the workspace question. A new task reserves an automatic slot;
an existing task keeps its current slot. The wizard requires a free open hotkey,
then asks for mode, an optional close hotkey and the close action. Switching from
an assigned destination to automatic defaults to `on-demand`; reopening an already
automatic task's wizard preserves its current mode and shortcuts. Invalid or
cancelled configuration leaves the saved task and registered bindings unchanged.

Later, use your configured open shortcut or `tws backups load`. After rearranging
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
tws configure backups   # choose numbered persistent destination, or - for shortcuts
tws configure backups --open Super+B
tws configure --launcher Control+Alt+Super+W
tws configure backups --mode always     # keep windows available after login
tws configure backups --workspace 3    # GNOME workspace 3, always mode, no task keys
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
tws backups checkpoint    # save current tmux state without rearranging GUI windows
tws backups load          # open or reuse windows and restore monitor placement
TWS_LOAD_ID=backups tws
```

`tws configure` prompts for a workspace ID. Its shortcut opens that CLI in Kitty
(override the terminal program with `TWS_TERMINAL`). The open shortcut restores
and focuses its saved workspace. `_activate ID` is the internal hotkey handler.
Installation assigns **Ctrl+Alt+Super+W** to this configuration wizard if free,
preserving an existing launcher on reinstall. Use `TWS_CONFIG_HOTKEY=CHORD` to
choose another install-time key or `TWS_CONFIG_HOTKEY=none` to skip registration.
The wizard assigns task shortcuts only when you choose an automatic destination.
Numbered destinations do not prompt for them; the global configuration launcher
remains separate from task open/close shortcuts.

Every assigned shortcut is normalized and checked against GNOME settings,
registered custom shortcuts, installed GNOME extension schemas, and other saved
task bindings. A collision is rejected with its owner, preserving the colliding
bindings. App-local shortcuts and hardcoded third-party input grabs are
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
tws backups load
```

The CLI and wizard accept GNOME workspace numbers 1–36; the JSON `workspace`
field uses indices 0–35. Direct assignment can create a missing task from the
current workspace's windows without allocating an extra slot, including when
all 36 workspaces already exist. Existing destination names are preserved;
unnamed destinations receive `Task: ID`. Tasks may share a workspace. Changing
the assignment retains the saved windows, monitor layout, launch commands and
tmux sessions and defaults to persistent `always` mode without task shortcuts;
the previous static workspace remains available. `load` moves
the remembered windows to the chosen workspace immediately; `always` mode does
so during automatic reconciliation. Later `save` captures the chosen workspace.
Attach the desired tmux session in each terminal before capturing the task;
its saved terminal windows will then reopen on that workspace after login.

For an existing task such as `cli_task` that already remembers the `ppt` tmux
session, use `tws configure cli_task --workspace 2`; it needs no further wizard
or hotkey setup. `tws cli_task save` updates its layout on workspace 2, and
`tws cli_task load` restores it manually when needed. Do not run `create` again
to change the destination: that command reserves a new automatic slot.

The `task-workspaces.service` reconciles permanent slots and `always` windows
every 20 seconds without focusing them. In both modes, existing terminals attached
to a saved tmux session are moved back to its assigned workspace, including new
clients opened elsewhere. Matching uses the local socket and exact session name,
so changed titles, terminal classes or window IDs do not route an unrelated
terminal into the task. `on-demand` opens missing applications only when
requested; `always` also restores missing windows automatically. Hide minimizes
just saved windows and preserves running tmux jobs;
close asks those application windows to close, keeping detached tmux jobs alive.
Either suspends automatic reopening until the next open action. `_dismiss ID`,
`_startup`, and `_watch` are internal shortcut/service entrypoints.

Once a tmux terminal is on its assigned workspace, background reconciliation
leaves its current monitor, position, size and maximized/fullscreen state alone.
This prevents the 20-second watcher from repeatedly moving or resizing a terminal
during normal work. Newly opened terminals and clients returning from another
workspace still receive the saved placement. `tws ID load` explicitly restores
the saved layout even for existing terminals; `tws ID save` records an updated
layout for future restoration.

After login, the watcher can start before GNOME's bridge or display environment
is ready. It retries reconciliation as the session becomes available. Before
launching an application, `tws` fetches the current systemd user manager's desktop
environment and fills missing/empty `DISPLAY`, `WAYLAND_DISPLAY`, `XAUTHORITY`,
`XDG_RUNTIME_DIR` and `XDG_SESSION_TYPE` values for that child only. This also
works when the watcher started before GNOME imported those values; no service
restart is needed for that timing case. Explicit nonempty environment values
are preserved, and unrelated manager variables are not copied. Without a user
manager, applications continue to inherit the invoking terminal's environment.
Application launch errors remain on stderr and are visible in the service journal.
To inspect a login restoration failure, use:

```sh
systemctl --user status task-workspaces.service --no-pager
journalctl --user -u task-workspaces.service -b --no-pager
```

Snapshots live in `${XDG_CONFIG_HOME:-~/.config}/task-workspaces/ID.json` with
private permissions; `TWS_CONFIG_HOME` overrides that directory. Window frames
are saved relative to named monitor connectors and scaled if resolution changes.
Disconnected monitors fall back to a visible monitor. Sticky windows are excluded.
Tmux clients are linked to their terminal process; saved sessions record their
socket, names, windows, panes, directories and layout. Discovery includes default
sockets, `tmux -S /path/to/socket`, and named `-L` sockets, including a client's
own `TMUX_TMPDIR`; running `tws` outside tmux works too. Session discovery and
workspace binding are implemented by standalone `tws` and its own service:
ACC files, commands and services are not required. Tmux retains running jobs while
you hide a workspace. After reboot, saved sessions recover their current layout,
directories and pane output, with supported Harbor jobs resumed from application
state as described below. Commands such as `za` must be started again. Terminal windows
that share one process may expose several attached sessions; those structures
are saved, and the first session is attached when reopening that window.

## Tmux continuity after reboot

The watcher checkpoints managed tmux sessions every 20 seconds. `save` and
`tws ID checkpoint` also checkpoint them; the latter leaves the saved GUI layout
alone. Current tmux window indices, names, layouts, active panes and directories
are saved independently of the older manual window snapshot. Hidden/on-demand
tasks keep recording their existing sessions without reopening their windows.
Missing sessions do not overwrite their last usable checkpoint with empty state.

Once checkpointing starts, pane output streams to private files and synchronizes
each received chunk. The first checkpoint captures complete pane history.
Later checkpoints reuse that durable base and its original stream offset while
saving the current layout, directories and supported jobs. They do not copy
the full scrollback every 20 seconds, which would block typing on the shared
tmux server for large histories. Each generation links its base capture so
pruning older generations preserves recovery. Legacy checkpoints upgrade on
their next capture; an interrupted recorder, changed stream identity or
detected truncation establishes a fresh base.

Recovery displays the captured tmux history followed by
output recorded after that capture. Displayed text is never submitted as shell
input. There can be slight overlap at the capture boundary. Live sessions keep
their existing processes; a single fresh idle shell created before restoration
can be recovered when its server generation differs from the saved checkpoint.
Active jobs and user-owned `pipe-pane` collectors are preserved; a foreign collector
causes an explicit checkpoint error rather than being replaced.

The same behavior applies to `tws ID checkpoint`, `tws ID save`, the `_watch`
service and its `_checkpoint-all` stop action. After installing an updated
`tws` binary, activate it silently with
`systemctl --user restart task-workspaces.service` and verify it with
`systemctl --user is-active task-workspaces.service`. Existing tmux jobs remain
running during this service restart.

Private state lives under `TWS_CONFIG_HOME/.continuity/ID/` (or the default task
config directory). `checkpoint.json` is atomic and synchronized; a validated
`checkpoint.previous.json` supplies fallback if the current manifest is damaged.
If both are unusable, restoration reports the error and preserves the evidence.
Four recent snapshot generations and any generations referenced by the two
manifests are retained. Output streams remain saved, so disk use grows with
terminal output. Keep this directory with the profiles when backing them up.

The service runs `_checkpoint-all` on stop without needing the GNOME bridge.
`KillMode=process` keeps a service restart from terminating tmux and terminal
children. The native internal forms `_record-pane FILE` and
`_restore-pane DESCRIPTOR` implement output recording and cold pane restoration;
they are created by `tws`, not required for normal setup. `TWS_RECORDER_BINARY`
can override the executable used for those detached helpers; normally they use
the current `tws` executable. After an offline installation, activate the unit:

```sh
systemctl --user daemon-reload
systemctl --user enable task-workspaces.service
systemctl --user restart task-workspaces.service
```

For the `ppt` task on workspace 2, the normal commands are:

```sh
tws cli_task checkpoint
tws cli_task load
```

Harbor descendants launched as `harbor run --job-name NAME --jobs-dir DIRECTORY`
(`-o` also works; the default is `jobs`) or `harbor jobs resume --job-path DIRECTORY`
(`-p` also works) are recognized when their durable `config.json` exists. After
server loss, `tws` invokes `harbor jobs resume` with the saved plugin names and
`PYTHONPATH`, retaining completed trials and retrying cancelled/unfinished work
according to Harbor's own resume rules. Reopening a live session does not resume
the same job again. If Harbor has redacted a trial's mount source, its original
job configuration restores that reference only when the match is unambiguous;
the trial's original config is saved as `config.tws-before-resume.json` first.
The Codex credential-file path is recovered from that job's existing mount
configuration. Other credential environment variables, valid credentials,
Harbor, its plugins and writable job artifacts must remain available. A resume
failure is shown in the restored pane and leaves its job artifacts intact.

Reboot ends processes and their in-memory state. Continuity restores durable
terminal/application state; it does not preserve arbitrary running processes.
Only output received by the recorder and synchronized files can survive power
loss. Layout changes since the last checkpoint, unavailable credentials, disk
failures, and output from before recording was enabled cannot be guaranteed.
Restoration is tested through isolated tmux server loss; tests never reboot the
desktop or run paid Harbor jobs.

`create`, a new-task configuration snapshot, and `save` exclude ordinary terminal
windows without an attached local tmux client. Terminal detection uses common
window/application classes, process executables, and installed desktop entries
with the `TerminalEmulator` category. ACC Kitty classes (`kitty-acc-*`), ACC
agent sessions (`acc-*`) and the `agent_command_center` monitor are excluded even
when attached to tmux. An exported `AGENT_COMMAND_CENTER_TMUX_SESSION` also
identifies a custom monitor name; it is optional. Load skips excluded terminal
entries in older snapshots too. Ordinary Chrome/folder windows remain eligible.

`save` captures eligible normal non-sticky windows in the task's assigned slot,
including Chrome/Chromium windows and task tmux clients. To update an older task
with these exclusions, arrange its windows and run `tws backups save`. Continue
using `tws configure backups --workspace 3` to select its destination and
`tws configure backups --mode always` for automatic missing-window restoration.
Avoid assigning the same tmux session to tasks on different workspace slots;
the session identity supplies the binding. Chrome tabs are
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
tmux server and installation home. Terminal checks use headless pseudo-terminals
(`script` from util-linux), real isolated tmux clients, and simulated GNOME
windows. Standalone checks use an empty home and system-only PATH with no ACC
installation. See [tests/COVERAGE.md](tests/COVERAGE.md) for the saved cases.
Startup-environment checks use private D-Bus managers and headless application
probes, including display values imported after an initial attempt in the same
watcher-like process. They never open GUI windows or reboot the machine.
Capture/restoration diagnostics print parameters and results to stderr; CLI
output, including `tws ID show` JSON, stays on stdout.

MIT licensed; see [LICENSE](LICENSE).

## Installer lifecycle

Run these commands from this checkout. The component IDs printed by discovery
are the same IDs used by the terminal menu and programmatic actions.

| Command | Behavior |
| --- | --- |
| `bash install.sh` | Open component selection on a terminal; use the default/config selection otherwise. |
| `bash install.sh --help` | Show accepted options (`-h` is equivalent). |
| `bash install.sh --list-components` | Print component IDs, labels and defaults. |
| `bash install.sh --default` | Apply default-on components. |
| `bash install.sh --all` | Apply every component. |
| `bash install.sh --select ID,...` | Apply exactly the named components. |
| `bash install.sh --reconfigure ID,...` | Reapply those components through the same install operations. |
| `bash install.sh --uninstall ID,...` | Run the selected removal operations. |
| `bash install.sh --detect` | Print `ID<TAB>installed` or `ID<TAB>absent`. |
| `bash install.sh --export-selection` | Print selection JSON without applying it. |

The shared interface also accepts `--config NAME` (an installation JSON name
or path) and `--auth` (explicit permission for authenticated component actions).
`--config=NAME`, `--select=ID,...`, `--reconfigure=ID,...`,
`--uninstall=ID,...` and `--configure-component=ID` are equivalent forms.
Advanced discovery is available through `--list-configurable-components`,
`--list-select-configure-components` and `--list-component-config-values`;
`--configure-component ID` opens an available nested screen. Atomic owners
return empty advanced listings and reject unavailable nested configuration.

Explicit `--select ""` selects no optional components; a defined mandatory core
or installation preflight still runs. `--uninstall ""` is a no-op. Unknown or
duplicate IDs, missing operands and conflicting modes reject before component
installation/removal. Help, discovery, detection and export perform no installation.
Cancelling the component menu performs no install/removal. Configuration screens
may save their own settings when explicitly accepted.

The shared terminal screen uses arrows to move, Space to toggle, Enter to
apply and `q`/Escape to cancel; installed components support `u` for removal.
A right-arrow detail action appears where the owner provides configuration.
Removal-only selections skip core installation.

The licensed `.installer-support/` package is generated by
`linux_installation_scripts_functions`; `manifest.json` records provenance and
SHA-256 hashes, and `LICENSE.md` retains the MIT grant. It is a shared library
subset, not an application checkout. The default bootstrap requires no private
download or sibling repository. Explicit `ISC_COMPONENT_LIB_DIR` /
`ISC_FUNCTIONS_DIR` overrides remain available for development and the old setup
chain. Regenerate support from its owner rather than editing copies. Bash and
Python 3 are needed for this library; project build/runtime requirements above
still apply.

The atomic `task_workspaces` component installs the CLI, GNOME bridge and
user service. `TWS_BINARY=/path/to/tws` supplies a prebuilt binary;
`TWS_INSTALL_OFFLINE=1` suppresses package/toolchain downloads and live desktop
configuration. `TWS_COMMAND` selects the recorded command alias and
`TWS_INSTALL_HOME` selects the target home. Removal retains saved profiles and
tmux sessions while removing the owned CLI, extension and service.

For example, inspect IDs first, then use the same ID with either interface:

```bash
bash install.sh --list-components
bash install.sh --select task_workspaces
bash install.sh --detect
bash install.sh --uninstall task_workspaces
```
