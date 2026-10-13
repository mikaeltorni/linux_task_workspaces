# Public-interface coverage

Run from the repository root with `cargo test --locked --offline` and
`node --test tests/*.test.mjs`. Cargo discovers all current CLI and browser
checks; the opt-in browser-session file check is ignored unless explicitly
selected. No GUI application is opened by these checks. Each Rust case has a
fresh temporary store; GNOME actions and prompts are injected through Bridge.
Installer/GSettings checks run with a temporary home and private session bus.

## Shared input validation

All unmatched command shapes use the single dispatch fallback. Missing/extra
shapes below are asserted by `shared_dispatch_rejections`, seeded with backup's
two remembered windows; `backup show` and `list` immediately verify unchanged
state after each rejection. Numeric conversions and domain failures are checked
at their independent owners, rather than inferred from dispatch errors.

| Public command | Operands | Missing / extra | Conversion / domain / lookup | Saved assertions |
| --- | --- | --- | --- | --- |
| help, empty input | none | empty is supported; extra rejected | no numeric input or lookup | rejections, shared_dispatch_rejections |
| list | none | extra rejected | no numeric input | snapshot_contract, shared_dispatch_rejections |
| create | ID | both rejected | invalid/duplicate ID; slot exhaustion | rejections, workspace_limit, shared_dispatch_rejections |
| ID show | ID + show | missing operation/extra rejected | missing ID via Store.read | rejections, shared_dispatch_rejections |
| configure (wizard) | optional ID | omission prompts for ID; extra rejected | shared workspace parser; hotkey conflict; cancellation before mutation | workspace_assignment_new_and_wizard, conflict_contract |
| configure --launcher | chord | both rejected | shared accelerator/check_hotkey; own-binding reuse and GNOME collision | gsettings_contract, shared_dispatch_rejections |
| configure ID --open / --close | ID, chord | both rejected per flag | shared accelerator/check_hotkey; missing Store.read resource | hotkey_configuration, conflict_contract, shared_dispatch_rejections |
| configure ID --mode / --close-action | ID, value | both rejected per flag | independent invalid mode/action paths; Store.read resource | permanent_modes, shared_dispatch_rejections |
| configure ID --workspace | ID, number | both rejected | nonnumeric, 0, 37, 99; valid 1 and 36; creates a missing task without allocating an extra slot | workspace_assignment_contract, workspace_assignment_new_and_wizard |
| configure ID --launch | ID, index, JSON argv | missing index/argv and extra rejected | independent usize conversion; missing ID/index; malformed/empty/non-string argv | folder_contract, shared_dispatch_rejections |
| edit ID / ID edit | ID | missing ID / extra rejected; ID alone is unknown | aliases delegate wizard | workspace_assignment_new_and_wizard, shared_dispatch_rejections |
| edit --file / --json (both aliases) | ID, optional file path | both rejected for file; extra for json | Store.read/file lookup, JSON decode and shared profile validation | edit_contract, workspace_assignment_import_and_rejections, shared_dispatch_rejections |
| ID save | ID | missing operation/extra rejected | missing Store.read; Chrome capture ambiguity | save_contract, shared_dispatch_rejections |
| ID load / _activate | ID | missing/extra rejected | shared profile validation; missing Store.read | load_contract, launch_contract, workspace_assignment_contract, shared_dispatch_rejections |
| ID hide / close / _dismiss | ID | missing/extra rejected | shared Store.read and owned-window selection | permanent_modes, shared_dispatch_rejections |
| conflicts | chord | both rejected | shared accelerator parser | conflict_contract, shared_dispatch_rejections |
| _startup | none | extra rejected | store reconciliation | permanent_modes, workspace_assignment_contract |

## Workspace assignment

`assigned_workspace_without_hotkeys` seeds task open/close shortcuts, configures
workspace 2 through the wizard, and verifies no hotkey prompt and both bindings
cleared while saved windows survive. Malformed command/workspace input immediately
reads the populated profile and public list. Assigned wizard rejection/cancellation
coverage is retained in `workspace_assignment_new_and_wizard`; shortcut conflicts
remain protected through the explicit shortcut interface in `conflict_contract`.

`assigned_workspace_persistence` verifies direct and wizard assignment default
to always with empty shortcuts, without mode/close-action prompts. Public startup
places saved windows on workspace 2 without focusing. Explicit mode overrides
still work, and range/mode rejection immediately reads the profile and task list.

`automatic_workspace_hotkeys` chooses an automatic slot with a required free
open shortcut and optional close shortcut; repeated wizard defaults retain its
automatic slot and bindings. Empty/invalid/conflicting/duplicate shortcuts,
cancelled questions, invalid mode, equal open/close bindings and exhausted slots
leave both populated profiles, public list and desktop effects unchanged. Switching
between automatic shortcuts and a persistent numbered destination is verified.

`workspace_assignment_contract` assigns GNOME workspace 3 (stored index 2),
preserves the other profile and remembered windows, and verifies `load` and
always startup place every saved window at index 2 without startup focusing.
It accepts endpoint workspaces 1 and 36, then observes both profiles and list
immediately after each malformed shape/token/range rejection.

`workspace_assignment_new_and_wizard` creates directly on workspace 3, including
when all static slots exist, and verifies only that slot is ensured. The real
wizard dispatch obtains 3 through its injected prompt; both edit aliases can
choose another slot. Failed new-task input must not leave a profile or bindings.

`workspace_assignment_import_and_rejections` accepts a changed zero-based JSON
workspace, retains mode/launches, and rejects bad geometry/schema/conflicts
without changing the saved profile, public list, or recorded desktop actions.
Missing import files and malformed JSON use the same immediate observations.
The old fixed-slot rejection was last valid at baseline
7a72066e396811574d21f9a84a0ff385303e9f95; its replacement is this acceptance case.
The earlier out-of-range JSON rejection remains in `edit_contract`.

`tmux_contract` supplies a real isolated saved tmux session and verifies startup
on an assigned slot preserves live pane processes, window indices, and CWDs.
Node's destination-name case verifies named workspace 2 remains Code, an unnamed
chosen slot receives Task: backup, other names/count survive, and no focus occurs.
GNOME's 36-slot bound is an existing platform limitation, not a new user rule.

## Terminal capture exclusions

`terminal_capture::capture_exclusions` uses real isolated tmux clients in headless
pseudo-terminals and fake GNOME windows. It proves create/save exclude ACC class
windows even with ordinary tmux, reserved ACC sessions/monitor, plain terminals
across common classes and custom TerminalEmulator desktop entries; task tmux,
Chrome tabs and folder locations remain. Malformed CLI and invalid snapshot
rejections immediately read the populated saved profile. General layout fixtures
now use ordinary application windows; pid-zero Kitty inclusion was superseded.

## Standalone tmux binding

`standalone_tmux` runs with an empty home, system-only PATH and no TMUX or ACC
monitor environment. `binds_without_acc` covers custom-socket capture/save,
on-demand rebinding without launches/focus, always mode, repeated load, live pane
PID reuse, malformed/missing-resource rejection reads and hide suspension.
`restores_by_session` covers changed title/class/ID, unrelated same-title clients,
additional attaches, missing-session recreation, idempotent reopening and legacy
ACC/plain-terminal exclusions. `discovers_client_socket_label` covers client-only
TMUX_TMPDIR and -L with preceding -f. Shared fixtures serialize environment
changes and clean up their own server/client processes. The existing tmux_contract
now obtains its terminal session from a real attached client rather than injecting
session metadata into an unrelated PID-zero GUI window; structure/CWD/index and
post-attach layout expectations remain verified.
