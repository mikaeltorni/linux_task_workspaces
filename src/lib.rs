pub mod chrome;
pub mod config;
pub mod continuity;
pub mod folders;
mod harbor;
pub mod hotkeys;
pub mod runtime;
mod session_environment;
pub mod terminals;
pub mod tmux;

use serde_json::{json, Value};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub trait Bridge {
    fn snapshot(&self) -> Result<Value>;
    fn action(&self, action: Value) -> Result<Value>;
    fn prompt(&self, label: &str, default: &str) -> Result<String> {
        hotkeys::prompt(label, default)
    }
    fn bind(&self, id: &str, role: &str, chord: &str, command: &[String]) -> Result<()> {
        hotkeys::bind(id, role, chord, command)
    }
    fn conflicts(&self, chord: &str, id: &str, role: &str) -> Result<Vec<String>> {
        hotkeys::conflicts(chord, id, role)
    }
    /// Launch an application with missing display variables refreshed per child.
    /// Parameters: self - desktop bridge; argv - executable and literal arguments.
    /// Returns: successful process creation, or a launch error.
    fn launch(&self, argv: &[String]) -> Result<()> {
        eprintln!("self={self:p}, argv={argv:?}");
        let result = spawn(argv);
        eprintln!("{result:?}");
        result
    }
}

pub struct Desktop;

impl Desktop {
    fn call(&self, method: &str, action: Option<Value>) -> Result<Value> {
        let bus = zbus::blocking::Connection::session()?;
        let result = if let Some(a) = action {
            bus.call_method(Some("org.gnome.Shell"),
                "/org/gnome/Shell/Extensions/TaskWorkspaces",
                Some("org.gnome.Shell.Extensions.TaskWorkspaces"), method, &(a.to_string(),))
        } else {
            bus.call_method(Some("org.gnome.Shell"),
                "/org/gnome/Shell/Extensions/TaskWorkspaces",
                Some("org.gnome.Shell.Extensions.TaskWorkspaces"), method, &())
        }.map_err(|e| format!("GNOME Task Workspaces bridge unavailable: {e}. Install and enable task-workspaces@local; on Wayland a new extension needs manual logout/login."))?;
        let value: String = result.body().deserialize()?;
        let value: Value = serde_json::from_str(&value)?;
        if let Some(error) = value.get("error") {
            return Err(error.to_string().into());
        }
        Ok(value)
    }
}

impl Bridge for Desktop {
    fn snapshot(&self) -> Result<Value> {
        let mut snapshot = self.call("Snapshot", None)?;
        snapshot["folder_locations"] = folders::nautilus_locations().unwrap_or_else(|_| json!({}));
        snapshot["chrome_sessions"] = json!(chrome::discover(&snapshot));
        Ok(snapshot)
    }
    fn action(&self, action: Value) -> Result<Value> {
        self.call("Action", Some(action))
    }
}

pub struct Store {
    pub root: PathBuf,
}

pub struct OperationLock(fs::File);

impl Drop for OperationLock {
    fn drop(&mut self) {
        // A concurrent fork can briefly inherit the open file description.
        // Explicitly unlock it rather than waiting for every inherited fd to close.
        let _ = fs2::FileExt::unlock(&self.0);
    }
}

impl Store {
    pub fn lock(&self) -> Result<OperationLock> {
        fs::create_dir_all(&self.root)?;
        fs::set_permissions(&self.root, fs::Permissions::from_mode(0o700))?;
        let f = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .mode(0o600)
            .open(self.root.join(".lock"))?;
        fs2::FileExt::try_lock_exclusive(&f)
            .map_err(|_| "Another task workspace operation is in progress; retry shortly")?;
        Ok(OperationLock(f))
    }
    pub fn environment() -> Self {
        let root = std::env::var_os("TWS_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                let config = std::env::var_os("XDG_CONFIG_HOME")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| {
                        PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config")
                    });
                config.join("task-workspaces")
            });
        Self { root }
    }

    pub fn path(&self, id: &str) -> Result<PathBuf> {
        if id.is_empty()
            || id.len() > 64
            || !id.as_bytes()[0].is_ascii_alphanumeric()
            || !id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
        {
            return Err(
                "ID must contain letters, digits, underscores or hyphens (1–64 characters).".into(),
            );
        }
        Ok(self.root.join(format!("{id}.json")))
    }

    pub fn read(&self, id: &str) -> Result<Value> {
        let path = self.path(id)?;
        let text =
            fs::read_to_string(path).map_err(|e| format!("Cannot read workspace {id}: {e}"))?;
        Ok(serde_json::from_str(&text)?)
    }

    pub fn write(&self, id: &str, value: &Value) -> Result<()> {
        let path = self.path(id)?;
        fs::create_dir_all(&self.root)?;
        fs::set_permissions(&self.root, fs::Permissions::from_mode(0o700))?;
        let tmp = self.root.join(format!(".{}-snapshot", std::process::id()));
        let mut f = OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&tmp)?;
        let result = (|| -> Result<()> {
            writeln!(f, "{}", serde_json::to_string_pretty(value)?)?;
            f.sync_all()?;
            fs::rename(&tmp, path)?;
            Ok(())
        })();
        if tmp.exists() {
            fs::remove_file(tmp)?;
        }
        result
    }

    pub fn list(&self) -> Result<Vec<Value>> {
        if !self.root.exists() {
            return Ok(vec![]);
        }
        let mut ids: Vec<String> = fs::read_dir(&self.root)?
            .filter_map(|p| p.ok())
            .map(|p| p.path())
            .filter(|p| p.extension().is_some_and(|e| e == "json"))
            .filter_map(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
            .collect();
        ids.sort();
        ids.iter().map(|id| self.read(id)).collect()
    }
}

/// Capture task windows while excluding ACC windows and non-tmux terminals.
/// Parameters: snapshot - GNOME windows and monitor metadata; workspace - zero-based source slot.
/// Returns: saved eligible windows and their monitor layout, or a snapshot error.
pub fn capture(snapshot: &Value, workspace: i64) -> Result<Value> {
    eprintln!("snapshot={snapshot}, workspace={workspace}");
    let mut windows = vec![];
    for w in snapshot["windows"]
        .as_array()
        .ok_or("Invalid window snapshot")?
    {
        if w["workspace"].as_i64() != Some(workspace) || w["sticky"] == true {
            continue;
        }
        if terminals::is_acc(w, &[]) {
            continue;
        }
        let mut w = w.clone();
        let sessions = tmux::attached_sessions(w["pid"].as_u64().unwrap_or(0) as u32)?;
        if terminals::is_acc(&w, &sessions) || (sessions.is_empty() && terminals::is_terminal(&w)) {
            continue;
        }
        w["tmux"] = json!(sessions);
        w["launch"] = if let Some(s) = sessions.first() {
            json!([
                "kitty",
                "tmux",
                "-S",
                s["socket"],
                "attach-session",
                "-t",
                format!("={}", text(s, "name"))
            ])
        } else if !text(&w, "app").is_empty() {
            json!(["gtk-launch", w["app"]])
        } else {
            json!([])
        };
        folders::attach(&mut w, &snapshot["folder_locations"]);
        windows.push(w);
    }
    let result = Ok(json!({"windows": windows, "monitors": snapshot["monitors"]}));
    eprintln!("{result:?}");
    result
}

pub fn text<'a>(v: &'a Value, key: &str) -> &'a str {
    v[key].as_str().unwrap_or("")
}

/// Launch saved argv with fresh missing desktop variables for early-login services.
/// Parameters: argv - application executable and literal arguments.
/// Returns: successful child spawn, or an empty-command/process error.
pub fn spawn(argv: &[String]) -> Result<()> {
    eprintln!("argv={argv:?}");
    let (program, args) = argv.split_first().ok_or("Empty launch command")?;
    Command::new(program)
        .args(args)
        .envs(session_environment::missing())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()?;
    eprintln!("Ok(())");
    Ok(())
}

pub fn command_argv(w: &Value) -> Result<Vec<String>> {
    w["launch"]
        .as_array()
        .ok_or("launch must be an argv array")?
        .iter()
        .map(|s| {
            s.as_str()
                .map(str::to_owned)
                .ok_or_else(|| "launch arguments must be strings".into())
        })
        .collect()
}

/// Compare stable tmux session identities independently of terminal titles.
/// Parameters: left - first session snapshot; right - second session snapshot.
/// Returns: whether both sessions use the same socket and exact session name.
fn same_session(left: &Value, right: &Value) -> bool {
    eprintln!("left={left}, right={right}");
    let a = text(left, "socket");
    let b = text(right, "socket");
    let result = !a.is_empty()
        && text(left, "name") == text(right, "name")
        && (a == b
            || fs::canonicalize(a)
                .ok()
                .zip(fs::canonicalize(b).ok())
                .is_some_and(|(a, b)| a == b));
    eprintln!("{result}");
    result
}

/// Resolve a saved window using attached session identity for terminals.
/// Parameters: saved - saved window; current - live windows; used - already placed identities; workspace - destination slot.
/// Returns: a reusable eligible window, or None when a launch is needed.
fn matching_window(
    saved: &Value,
    current: &[Value],
    used: &[String],
    workspace: &Value,
) -> Result<Option<Value>> {
    eprintln!("saved={saved}, current={current:?}, used={used:?}, workspace={workspace}");
    let sessions = saved["tmux"].as_array().cloned().unwrap_or_default();
    let candidates: Vec<_> = current
        .iter()
        .filter(|w| {
            !used.iter().any(|id| id == text(w, "id"))
                && w["sticky"] != true
                && !terminals::is_acc(w, &[])
        })
        .collect();
    let found = if sessions.is_empty() {
        candidates
            .iter()
            .find(|w| {
                w["id"] == saved["id"] && w["pid"] == saved["pid"] && w["class"] == saved["class"]
            })
            .or_else(|| {
                candidates.iter().find(|w| {
                    w["workspace"] == *workspace
                        && w["class"] == saved["class"]
                        && w["title"] == saved["title"]
                })
            })
            .map(|w| (*w).clone())
    } else {
        let mut eligible = vec![];
        for w in candidates {
            let attached = tmux::attached_sessions(w["pid"].as_u64().unwrap_or(0) as u32)?;
            if !terminals::is_acc(w, &attached)
                && sessions
                    .iter()
                    .any(|saved| attached.iter().any(|actual| same_session(saved, actual)))
            {
                eligible.push(w);
            }
        }
        eligible
            .iter()
            .find(|w| w["id"] == saved["id"] && w["pid"] == saved["pid"])
            .or_else(|| eligible.iter().find(|w| w["workspace"] == *workspace))
            .or_else(|| eligible.first())
            .map(|w| (*w).clone())
    };
    let result = Ok(found);
    eprintln!("{result:?}");
    result
}

/// Bind existing attached clients to their task slot without opening applications.
/// Parameters: profile - task's session/window templates and workspace; bridge - current desktop.
/// Returns: identities moved to the assigned workspace.
fn bind_existing_tmux_windows(profile: &Value, bridge: &dyn Bridge) -> Result<Vec<String>> {
    eprintln!("profile={profile}, bridge={bridge:p}");
    let snapshot = bridge.snapshot()?;
    let mut moved = vec![];
    let templates = profile["windows"]
        .as_array()
        .ok_or("Invalid saved windows")?;
    for w in snapshot["windows"].as_array().ok_or("Invalid snapshot")? {
        if w["workspace"] == profile["workspace"]
            || w["sticky"] == true
            || terminals::is_acc(w, &[])
        {
            continue;
        }
        let attached = tmux::attached_sessions(w["pid"].as_u64().unwrap_or(0) as u32)?;
        if attached.is_empty() || terminals::is_acc(w, &attached) {
            continue;
        }
        let template = templates.iter().find(|saved| {
            let sessions = saved["tmux"].as_array().cloned().unwrap_or_default();
            !terminals::is_acc(saved, &sessions)
                && sessions
                    .iter()
                    .any(|s| attached.iter().any(|a| same_session(s, a)))
        });
        if let Some(saved) = template {
            bridge.action(json!({"op":"place","id":w["id"],"workspace":profile["workspace"],"window":saved,"monitors":profile["monitors"]}))?;
            moved.push(text(w, "id").to_owned());
        }
    }
    let result = Ok(moved);
    eprintln!("{result:?}");
    result
}

/// Restore task windows while leaving correctly bound tmux clients stationary on watcher ticks.
/// Parameters: profile - saved task updated with live identities; bridge - desktop operations; focus - explicitly restore saved geometry and activate the slot, otherwise maintain existing tmux placement.
/// Returns: restored window identities, or a restoration error.
pub fn restore_windows(
    profile: &mut Value,
    bridge: &dyn Bridge,
    focus: bool,
) -> Result<Vec<String>> {
    eprintln!("profile={profile}, bridge={bridge:p}, focus={focus}");
    let mut current = bridge.snapshot()?["windows"]
        .as_array()
        .ok_or("Invalid snapshot")?
        .clone();
    let mut used: Vec<String> = vec![];
    let workspace = profile["workspace"].clone();
    let monitors = profile["monitors"].clone();
    for saved in profile["windows"]
        .as_array_mut()
        .ok_or("Invalid saved windows")?
    {
        let sessions = saved["tmux"].as_array().cloned().unwrap_or_default();
        if terminals::is_acc(saved, &sessions)
            || (sessions.is_empty() && terminals::is_terminal(saved))
        {
            continue;
        }
        for session in saved["tmux"].as_array().unwrap_or(&vec![]) {
            tmux::restore(session)?;
        }
        let mut found = matching_window(saved, &current, &used, &workspace)?;
        let launched = found.is_none();
        if launched {
            let before: Vec<String> = current.iter().map(|w| text(w, "id").to_owned()).collect();
            bridge.launch(&command_argv(saved)?)?;
            let timeout = std::env::var("TWS_WINDOW_TIMEOUT")
                .unwrap_or_else(|_| "15".into())
                .parse::<u64>()?;
            let start = Instant::now();
            while start.elapsed() < Duration::from_secs(timeout) {
                current = bridge.snapshot()?["windows"]
                    .as_array()
                    .ok_or("Invalid snapshot")?
                    .clone();
                let appeared: Vec<_> = current
                    .iter()
                    .filter(|w| !before.iter().any(|id| id == text(w, "id")))
                    .cloned()
                    .collect();
                found = if sessions.is_empty() {
                    appeared
                        .iter()
                        .find(|w| {
                            w["class"] == saved["class"]
                                && !used.iter().any(|id| id == text(w, "id"))
                        })
                        .cloned()
                } else {
                    matching_window(saved, &appeared, &used, &workspace)?
                };
                if found.is_some() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(150));
            }
        }
        let w = found.ok_or_else(|| {
            format!(
                "No window appeared for {}; check its launch command",
                text(saved, "title")
            )
        })?;
        if focus || sessions.is_empty() || launched || w["workspace"] != workspace {
            bridge.action(json!({"op":"place", "id":w["id"], "workspace":workspace, "window":saved, "monitors":monitors}))?;
        }
        used.push(text(&w, "id").into());
        saved["id"] = w["id"].clone();
        saved["pid"] = w["pid"].clone();
    }
    used.extend(bind_existing_tmux_windows(profile, bridge)?);
    if focus {
        bridge.action(json!({"op":"focus", "workspace":workspace}))?;
    }
    let result = Ok(used);
    eprintln!("{result:?}");
    result
}

pub fn run(args: &[String], store: &Store, bridge: &dyn Bridge) -> Result<String> {
    let readonly = args.is_empty()
        || args == ["--help"]
        || args == ["-h"]
        || args == ["list"]
        || args.get(1).is_some_and(|s| s == "show")
        || args.first().is_some_and(|s| s == "conflicts");
    let _lock = if readonly { None } else { Some(store.lock()?) };
    dispatch(args, store, bridge)
}

/// Route supported CLI forms to their configuration or workspace owner.
/// Parameters: args - CLI tokens; store - task profiles; bridge - desktop operations.
/// Returns: formatted command output, or an input/domain error.
fn dispatch(args: &[String], store: &Store, bridge: &dyn Bridge) -> Result<String> {
    eprintln!("args={args:?}, store={:?}, bridge={bridge:p}", store.root);
    if let Some(command) = continuity::parse_checkpoint(args)? {
        let result = match command {
            continuity::CheckpointCommand::All => {
                continuity::all(store)?;
                Ok("Tmux checkpoints saved\n".into())
            }
            continuity::CheckpointCommand::Task(id) => {
                continuity::checkpoint(store, &store.read(&id)?)?;
                Ok(format!("Checkpointed {id}\n"))
            }
        };
        eprintln!("{result:?}");
        return result;
    }
    let a: Vec<&str> = args.iter().map(String::as_str).collect();
    let result = match a.as_slice() {
        [] | ["--help"] | ["-h"] => Ok("tws create ID | list | ID show | ID save | ID checkpoint | ID load | ID hide | ID close\ntws configure [ID] [--open Super+B | --close Ctrl+Super+B | --mode always|on-demand | --close-action hide|close | --workspace NUMBER]\ntws configure --launcher Control+Alt+Super+W\ntws edit ID | ID edit [--json | --file PATH]\ntws conflicts HOTKEY\ntws configure ID --launch INDEX JSON_ARGV\nTWS_EDIT_ID=ID tws | TWS_LOAD_ID=ID tws\nRemember named GNOME task workspaces and tmux sessions.\n".into()),
        ["configure", "--launcher", chord] => {
            let normalized = config::check_hotkey(store,bridge,"configuration","launcher",chord)?;
            let command = hotkeys::terminal(&[hotkeys::executable().to_string_lossy().into_owned(),"configure".into()]);
            bridge.bind("configuration", "launcher", &normalized, &command)?;
            Ok(format!("Configuration shortcut: {normalized}\n"))
        },
        ["configure"] => {
            let id = bridge.prompt("Workspace ID", "")?;
            dispatch(&["configure".into(),id], store, bridge)
        },
        ["configure", id] => config::wizard(store,bridge,id),
        ["configure", id, "--workspace", number] => {
            let slot = config::workspace_number(number)?;
            config::assign(store,bridge,id,slot)
        },
        ["configure", id, "--open", chord] => {
            let mut profile = store.read(id)?;
            let normalized = config::check_hotkey(store,bridge,id,"open",chord)?;
            bridge.bind(id,"open",&normalized,&[hotkeys::executable().to_string_lossy().into_owned(),"_activate".into(),(*id).into()])?;
            profile["open_hotkey"] = json!(normalized);
            store.write(id,&profile)?;
            Ok(format!("Configured {id}: {normalized}\n"))
        },
        ["_activate", id] | [id,"load"] => {
            let mut profile = store.read(id)?;
            config::validate(&profile,id)?;
            continuity::overlay(store,&mut profile,true)?;
            ensure(&profile,bridge)?;
            let result = restore_windows(&mut profile,bridge,true);
            if result.is_err() {store.write(id,&profile)?;}
            let ids=result?;
            profile["hidden"] = json!(false);
            store.write(id,&profile)?;
            continuity::checkpoint(store,&profile)?;
            Ok(format!("Opened {id}: {} windows\n",ids.len()))
        },
        ["configure", id, "--mode", mode] => {
            if !["always","on-demand"].contains(mode) { return Err("Mode must be always or on-demand".into()); }
            let mut p = store.read(id)?; p["mode"] = json!(mode); store.write(id,&p)?;
            Ok(format!("{id}: {mode}\n"))
        },
        ["configure", id, "--close-action", action] => {
            if !["hide","close"].contains(action) { return Err("Close action must be hide or close".into()); }
            let mut p = store.read(id)?; p["close_action"] = json!(action); store.write(id,&p)?;
            Ok(format!("{id}: {action}\n"))
        },
        ["configure", id, "--close", chord] => {
            let mut p = store.read(id)?; let chord = config::check_hotkey(store,bridge,id,"close",chord)?;
            bridge.bind(id,"close",&chord,&[hotkeys::executable().to_string_lossy().into_owned(),"_dismiss".into(),(*id).into()])?;
            p["close_hotkey"] = json!(chord); store.write(id,&p)?;
            Ok(format!("Hide/close shortcut for {id}: {chord}\n"))
        },
        [id, "hide"] | [id, "close"] | ["_dismiss", id] => {
            let mut p = store.read(id)?;
            let op = if a[0] == "_dismiss" { text(&p,"close_action") } else { a[1] }.to_string();
            let snapshot = bridge.snapshot()?;
            let ids: Vec<Value> = snapshot["windows"].as_array().ok_or("Invalid snapshot")?.iter()
                .filter(|w| p["windows"].as_array().is_some_and(|windows| windows.iter().any(|saved|
                    saved["id"] == w["id"] && saved["pid"] == w["pid"] && saved["class"] == w["class"])))
                .map(|w| w["id"].clone()).collect();
            bridge.action(json!({"op":op,"workspace":p["workspace"],"ids":ids}))?;
            p["hidden"] = json!(true); store.write(id,&p)?;
            Ok(format!("{id}: {op} (tmux sessions preserved)\n"))
        },
        ["_startup"] => {
            reconcile_locked(store,bridge)?; Ok("Task workspaces reconciled\n".into())
        },
        ["edit", id] | [id,"edit"] => dispatch(&["configure".into(),(*id).into()],store,bridge),
        ["edit", id, "--json"] | [id,"edit","--json"] => config::edit_json(store,bridge,id),
        ["edit", id, "--file", file] | [id,"edit","--file",file] => config::apply(store,bridge,id,serde_json::from_str(&fs::read_to_string(file)?)?),
        ["conflicts", chord] => {
            let hits = bridge.conflicts(&hotkeys::accelerator(chord)?,"","")?;
            Ok(if hits.is_empty() { "Free in GNOME settings\n".into() } else { format!("{}\n",hits.join("\n")) })
        },
        ["configure",id,"--launch",window,argv] => {
            let mut p=store.read(id)?;
            let index=window.parse::<usize>()?;
            let windows=p["windows"].as_array_mut().ok_or("Invalid windows")?;
            let w=windows.get_mut(index).ok_or("Window index out of range; inspect ID show (zero-based)")?;
            w["launch"]=serde_json::from_str(argv)?;
            w["manual_launch"]=json!(true);
            if command_argv(w)?.is_empty() {return Err("Launch command cannot be empty".into());}
            config::validate(&p,id)?;
            store.write(id,&p)?;
            Ok(format!("Updated window {index} launch for {id}\n"))
        },
        [id,"save"] => {
            let mut profile=store.read(id)?;
            let snapshot=bridge.snapshot()?;
            let slot=profile["workspace"].as_i64().ok_or("Invalid workspace slot")?;
            let mut captured=capture(&snapshot,slot)?;
            let sessions=snapshot["chrome_sessions"].as_array().cloned().unwrap_or_default();
            let mut used=std::collections::BTreeSet::new();
            for w in captured["windows"].as_array_mut().unwrap() {
                let old=profile["windows"].as_array().and_then(|a|a.iter().find(|old|old["id"]==w["id"]&&old["pid"]==w["pid"]&&old["class"]==w["class"]));
                if let Some(old)=old.filter(|p|p["manual_launch"]==true) {
                    w["launch"]=old["launch"].clone();w["manual_launch"]=json!(true);
                } else {chrome::attach(w,&sessions,&mut used)?;}
            }
            profile["windows"]=captured["windows"].clone();profile["monitors"]=captured["monitors"].clone();
            store.write(id,&profile)?;
            continuity::checkpoint(store,&profile)?;
            Ok(format!("Saved {id}: {} windows\n",profile["windows"].as_array().unwrap().len()))
        },
        ["list"] => Ok(store.list()?.iter().map(|p| format!("{}\tworkspace {}\t{} windows\n",
            text(p,"id"), p["workspace"].as_i64().unwrap_or(0)+1, p["windows"].as_array().map_or(0, Vec::len))).collect()),
        ["create", id] => {
            if store.path(id)?.exists() { return Err(format!("Workspace already exists: {id}").into()); }
            let profile = prepare_profile(store,bridge,id)?;
            config::validate(&profile,id)?;
            ensure(&profile,bridge)?;
            store.write(id, &profile)?;
            Ok(format!("Remembered {id}: {} windows\n", profile["windows"].as_array().unwrap().len()))
        },
        [id, "show"] => Ok(format!("{}\n", serde_json::to_string_pretty(&store.read(id)?)?)),
        _ => Err("Unknown command or invalid arguments; run tws --help.".into()),
    };
    eprintln!("{result:?}");
    result
}

pub fn ensure(profile: &Value, bridge: &dyn Bridge) -> Result<()> {
    bridge.action(json!({"op":"ensure","workspace":profile["workspace"],"name":format!("Task: {}",text(profile,"id")),"preserve_name":profile["workspace_assigned"] == true}))?;
    Ok(())
}

fn prepare_profile(store: &Store, bridge: &dyn Bridge, id: &str) -> Result<Value> {
    store.path(id)?;
    let snapshot = bridge.snapshot()?;
    let source = snapshot["active"].as_i64().ok_or("No active workspace")?;
    let mut profile = capture(&snapshot, source)?;
    let highest = store
        .list()?
        .iter()
        .filter_map(|p| p["workspace"].as_i64())
        .max()
        .unwrap_or(-1)
        + 1;
    let slot = snapshot["count"]
        .as_i64()
        .ok_or("Invalid workspace count")?
        .max(highest);
    profile["version"] = json!(1);
    profile["id"] = json!(id);
    // Plain create still rejects exhaustion; the wizard can choose an existing slot.
    profile["workspace"] = json!(slot);
    profile["mode"] = json!("on-demand");
    profile["close_action"] = json!("hide");
    profile["hidden"] = json!(false);
    Ok(profile)
}

pub fn reconcile(store: &Store, bridge: &dyn Bridge) -> Result<()> {
    if store.list()?.is_empty() {
        return Ok(());
    }
    let _lock = store.lock()?;
    reconcile_locked(store, bridge)
}

/// Reconcile permanent slots, tmux bindings and windows, then checkpoint live sessions.
/// Parameters: store - locked task profile store; bridge - current desktop operations.
/// Returns: successful reconciliation and checkpointing, or a profile, storage or desktop error.
fn reconcile_locked(store: &Store, bridge: &dyn Bridge) -> Result<()> {
    eprintln!("store={:?}, bridge={bridge:p}", store.root);
    for mut p in store.list()? {
        continuity::overlay(store, &mut p, true)?;
        ensure(&p, bridge)?;
        if p["mode"] == "always" && p["hidden"] != true {
            let result = restore_windows(&mut p, bridge, false);
            let id = text(&p, "id").to_owned();
            store.write(&id, &p)?;
            result?;
        } else if p["hidden"] != true {
            bind_existing_tmux_windows(&p, bridge)?;
        }
        continuity::checkpoint(store, &p)?;
    }
    let result = Ok(());
    eprintln!("{result:?}");
    result
}

pub fn output(program: &str, args: &[&str]) -> Result<String> {
    let result = Command::new(program).args(args).output()?;
    if !result.status.success() {
        return Err(format!("{program}: {}", String::from_utf8_lossy(&result.stderr)).into());
    }
    Ok(String::from_utf8(result.stdout)?)
}

pub fn home() -> PathBuf {
    Path::new(&std::env::var("HOME").unwrap_or_default()).to_path_buf()
}
