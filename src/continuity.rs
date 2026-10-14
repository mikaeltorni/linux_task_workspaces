use crate::{harbor, text, tmux, Result, Store};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug)]
pub(crate) enum CheckpointCommand {
    All,
    Task(String),
}

/// Parse checkpoint CLI forms before operation selection.
/// Parameters: args - raw CLI tokens, including a task ID where required.
/// Returns: a validated checkpoint selector or no selector for legacy commands.
pub(crate) fn parse_checkpoint(args: &[String]) -> Result<Option<CheckpointCommand>> {
    eprintln!("args={args:?}");
    let tokens: Vec<_> = args.iter().map(String::as_str).collect();
    let result = match tokens.as_slice() {
        ["_checkpoint-all"] => Ok(Some(CheckpointCommand::All)),
        [id, "checkpoint"] => Ok(Some(CheckpointCommand::Task((*id).into()))),
        ["_checkpoint-all", ..] | [_, "checkpoint", ..] => {
            Err("Invalid checkpoint operands".into())
        }
        _ => Ok(None),
    };
    eprintln!("{result:?}");
    result
}

/// Select the executable used by detached pane helpers.
/// Parameters: none.
/// Returns: an absolute native tws executable path.
pub(crate) fn executable() -> Result<PathBuf> {
    eprintln!("parameters=none");
    let result = std::env::var_os("TWS_RECORDER_BINARY")
        .map(PathBuf::from)
        .map(Ok)
        .unwrap_or_else(std::env::current_exe);
    let result = Ok(result?);
    eprintln!("{result:?}");
    result
}

/// Create a private continuity directory.
/// Parameters: path - directory to create.
/// Returns: success after setting private permissions.
fn directory(path: &Path) -> Result<()> {
    eprintln!("path={path:?}");
    fs::create_dir_all(path)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    eprintln!("Ok(())");
    Ok(())
}

/// Atomically persist metadata and synchronize the containing directory.
/// Parameters: path - final file; value - metadata without terminal content.
/// Returns: success after the metadata reaches durable storage.
fn atomic(path: &Path, value: &Value) -> Result<()> {
    eprintln!("path={path:?}, value={value}");
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&tmp)?;
    let result = (|| -> Result<()> {
        serde_json::to_writer(&mut file, value)?;
        file.sync_all()?;
        fs::rename(&tmp, path)?;
        File::open(path.parent().ok_or("Missing metadata parent")?)?.sync_all()?;
        Ok(())
    })();
    if tmp.exists() {
        let _ = fs::remove_file(tmp);
    }
    eprintln!("{result:?}");
    result
}

/// Validate a manifest and the pane descriptors it references.
/// Parameters: path - checkpoint manifest.
/// Returns: complete validated session metadata.
fn manifest(path: &Path) -> Result<Value> {
    eprintln!("path={path:?}");
    let value: Value = serde_json::from_slice(&fs::read(path)?)?;
    if value["version"] != 1 {
        return Err("Unsupported continuity manifest".into());
    }
    for session in value["sessions"]
        .as_array()
        .ok_or("Invalid continuity sessions")?
    {
        if text(session, "socket").is_empty() || text(session, "name").is_empty() {
            return Err("Invalid continuity session identity".into());
        }
        for window in session["windows"]
            .as_array()
            .ok_or("Invalid continuity windows")?
        {
            if window["index"].as_u64().is_none() {
                return Err("Invalid continuity window index".into());
            }
            for pane in window["panes"]
                .as_array()
                .ok_or("Invalid continuity panes")?
            {
                descriptor(Path::new(text(pane, "restore")))?;
            }
        }
    }
    eprintln!("{value:?}");
    Ok(value)
}

/// Read the latest usable checkpoint, falling back only to a validated predecessor.
/// Parameters: root - private task continuity directory.
/// Returns: optional checkpoint, or an actionable error retaining corrupted evidence.
fn latest(root: &Path) -> Result<Option<Value>> {
    eprintln!("root={root:?}");
    let head = root.join("checkpoint.json");
    let prior = root.join("checkpoint.previous.json");
    let result = if !head.exists() && !prior.exists() {
        Ok(None)
    } else {
        manifest(&head)
            .or_else(|_| manifest(&prior))
            .map(Some)
            .map_err(|e| {
                format!(
                    "No valid tmux checkpoint in {}: {e}; corrupted files were preserved",
                    root.display()
                )
                .into()
            })
    };
    eprintln!("{result:?}");
    result
}

/// Identify the current tmux server generation independently of pane-number reuse.
/// Parameters: socket - owned session server; target - exact session target.
/// Returns: a boot and server-start identity.
pub(crate) fn epoch(socket: &str, target: &str) -> Result<String> {
    eprintln!("socket={socket}, target={target}");
    let pid = tmux::run(socket, &["display-message", "-p", "-t", target, "#{pid}"])?;
    let stat = fs::read_to_string(format!("/proc/{}/stat", pid.trim()))?;
    let start = stat
        .rsplit_once(')')
        .and_then(|(_, rest)| rest.split_whitespace().nth(19))
        .ok_or("Missing server start time")?;
    let boot = fs::read_to_string("/proc/sys/kernel/random/boot_id")?;
    let result = format!("{}-{}-{start}", boot.trim(), pid.trim());
    eprintln!("{result:?}");
    Ok(result)
}

/// Snapshot only live sessions belonging to this saved task and attach durable output streams.
/// Parameters: store - private task store; profile - saved task identity and session bindings.
/// Returns: success without modifying the saved GUI layout or replacing missing sessions.
pub(crate) fn checkpoint(store: &Store, profile: &Value) -> Result<()> {
    eprintln!("store={:?}, profile={profile}", store.root);
    let id = text(profile, "id");
    store.path(id)?;
    let root = store.root.join(".continuity").join(id);
    directory(&store.root.join(".continuity"))?;
    directory(&root)?;
    let previous = latest(&root).ok().flatten();
    let generation = root.join(format!(
        "generation-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    ));
    directory(&generation)?;
    let mut sessions = vec![];
    let mut seen = BTreeSet::new();
    for window in profile["windows"]
        .as_array()
        .ok_or("Invalid task windows")?
    {
        let bound = window["tmux"].as_array().cloned().unwrap_or_default();
        if crate::terminals::is_acc(window, &bound) {
            continue;
        }
        for binding in bound {
            let socket = text(&binding, "socket");
            let name = text(&binding, "name");
            if !seen.insert((socket.to_owned(), name.to_owned())) {
                continue;
            }
            let target = format!("={name}");
            if tmux::run(socket, &["has-session", "-t", &target]).is_err() {
                if let Some(old) = previous
                    .as_ref()
                    .and_then(|p| p["sessions"].as_array())
                    .and_then(|a| {
                        a.iter()
                            .find(|s| text(s, "socket") == socket && text(s, "name") == name)
                    })
                {
                    sessions.push(old.clone());
                }
                continue;
            }
            let identity = epoch(socket, &target)?;
            let stream_root = store.root.join(".continuity/streams").join(&identity);
            directory(&stream_root)?;
            let mut snapshot = tmux::snapshot(socket, name)?;
            snapshot["epoch"] = json!(identity);
            let panes = tmux::run(
                socket,
                &[
                    "list-panes",
                    "-s",
                    "-t",
                    &target,
                    "-F",
                    "#{window_index}\t#{pane_id}\t#{pane_pid}\t#{pane_pipe}",
                ],
            )?;
            for line in panes.lines() {
                let fields: Vec<_> = line.split('\t').collect();
                if fields.len() != 4 {
                    return Err("Invalid tmux continuity pane data".into());
                }
                let number = fields[0].parse::<u64>()?;
                let pane = fields[1];
                let pid = fields[2].parse()?;
                let stream = stream_root.join(format!("{}.ansi", pane.trim_start_matches('%')));
                let registered = tmux::run(
                    socket,
                    &["show-options", "-p", "-v", "-t", pane, "@tws-record-path"],
                )
                .unwrap_or_default();
                if fields[3] == "1" && registered.trim() != stream.to_string_lossy() {
                    return Err(format!("Pane {pane} already has a foreign output pipe; leave it intact or stop it before checkpointing").into());
                }
                if fields[3] != "1" {
                    OpenOptions::new()
                        .create(true)
                        .append(true)
                        .mode(0o600)
                        .open(&stream)?
                        .sync_all()?;
                    let binary = executable()?;
                    let quote = |s: &str| format!("'{}'", s.replace('\'', "'\\''"));
                    let pipe = format!(
                        "exec {} _record-pane {}",
                        quote(&binary.to_string_lossy()),
                        quote(&stream.to_string_lossy())
                    );
                    tmux::run(socket, &["pipe-pane", "-o", "-t", pane, &pipe])?;
                    tmux::run(
                        socket,
                        &[
                            "set-option",
                            "-p",
                            "-t",
                            pane,
                            "@tws-record-path",
                            &stream.to_string_lossy(),
                        ],
                    )?;
                }
                let offset = fs::metadata(&stream)?.len();
                let capture = generation.join(format!(
                    "session-{}-{}.ansi",
                    sessions.len(),
                    pane.trim_start_matches('%')
                ));
                let file = OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .mode(0o600)
                    .open(&capture)?;
                let status = Command::new("tmux")
                    .args([
                        "-S",
                        socket,
                        "capture-pane",
                        "-e",
                        "-p",
                        "-S",
                        "-",
                        "-t",
                        pane,
                    ])
                    .stdout(file.try_clone()?)
                    .stderr(Stdio::piped())
                    .output()?;
                if !status.status.success() {
                    return Err("Failed to capture tmux pane history".into());
                }
                file.sync_all()?;
                let restore = generation.join(format!(
                    "session-{}-{}.json",
                    sessions.len(),
                    pane.trim_start_matches('%')
                ));
                atomic(
                    &restore,
                    &json!({"version":1,"capture":capture,"stream":stream,"offset":offset,"jobs":harbor::discover(pid)?}),
                )?;
                let window = snapshot["windows"]
                    .as_array_mut()
                    .unwrap()
                    .iter_mut()
                    .find(|w| w["index"] == number)
                    .ok_or("Checkpoint window disappeared")?;
                let pane_number = tmux::run(
                    socket,
                    &["display-message", "-p", "-t", pane, "#{pane_index}"],
                )?;
                // Pane base-index may be nonzero; list order establishes the saved layout mapping.
                let ordered = tmux::run(
                    socket,
                    &[
                        "list-panes",
                        "-t",
                        &format!("{target}:{number}"),
                        "-F",
                        "#{pane_index}",
                    ],
                )?;
                let index = ordered
                    .lines()
                    .position(|s| s == pane_number.trim())
                    .ok_or("Checkpoint pane disappeared")?;
                window["panes"][index]["restore"] = json!(restore);
            }
            sessions.push(snapshot);
        }
    }
    if sessions.is_empty() {
        fs::remove_dir_all(generation)?;
        eprintln!("Ok(())");
        return Ok(());
    }
    if let Some(old) = previous {
        atomic(&root.join("checkpoint.previous.json"), &old)?;
    }
    atomic(
        &root.join("checkpoint.json"),
        &json!({"version":1,"sessions":sessions}),
    )?;
    prune(&root)?;
    eprintln!("Ok(())");
    Ok(())
}

/// Remove obsolete snapshot generations while retaining manifests and recent replay inputs.
/// Parameters: root - this task's private continuity directory.
/// Returns: success preserving every referenced generation and the four newest generations.
fn prune(root: &Path) -> Result<()> {
    eprintln!("root={root:?}");
    let mut keep = BTreeSet::new();
    for name in ["checkpoint.json", "checkpoint.previous.json"] {
        if let Ok(value) = manifest(&root.join(name)) {
            for session in value["sessions"].as_array().unwrap() {
                for window in session["windows"].as_array().unwrap() {
                    for pane in window["panes"].as_array().unwrap() {
                        if let Some(parent) = Path::new(text(pane, "restore")).parent() {
                            keep.insert(parent.to_path_buf());
                        }
                    }
                }
            }
        }
    }
    let mut generations = vec![];
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if entry.file_type()?.is_dir()
            && entry
                .file_name()
                .to_string_lossy()
                .starts_with("generation-")
        {
            generations.push((entry.metadata()?.modified()?, entry.path()));
        }
    }
    generations.sort_by_key(|entry| std::cmp::Reverse(entry.0));
    keep.extend(generations.iter().take(4).map(|(_, p)| p.clone()));
    for (_, path) in generations {
        if !keep.contains(&path) {
            fs::remove_dir_all(path)?;
        }
    }
    eprintln!("Ok(())");
    Ok(())
}

/// Apply durable session state to a temporary restoration profile.
/// Parameters: store - task store; profile - restoration profile; replace_empty - recover freshly precreated empty shells.
/// Returns: success after overlaying checkpoint session metadata only.
pub(crate) fn overlay(store: &Store, profile: &mut Value, replace_empty: bool) -> Result<()> {
    eprintln!(
        "store={:?}, profile={profile}, replace_empty={replace_empty}",
        store.root
    );
    let root = store.root.join(".continuity").join(text(profile, "id"));
    if let Some(saved) = latest(&root)? {
        for window in profile["windows"]
            .as_array_mut()
            .ok_or("Invalid task windows")?
        {
            if let Some(sessions) = window["tmux"].as_array_mut() {
                for session in sessions {
                    if let Some(old) = saved["sessions"].as_array().unwrap().iter().find(|s| {
                        text(s, "name") == text(session, "name")
                            && text(s, "socket") == text(session, "socket")
                    }) {
                        *session = old.clone();
                        if replace_empty {
                            session["replace_empty"] = json!(true);
                        }
                    }
                }
            }
        }
    }
    eprintln!("Ok(())");
    Ok(())
}

/// Validate a pane replay descriptor before producing any terminal output.
/// Parameters: path - private saved descriptor.
/// Returns: validated file references and supported job resumes.
fn descriptor(path: &Path) -> Result<Value> {
    eprintln!("path={path:?}");
    let value: Value = serde_json::from_slice(&fs::read(path)?)?;
    if value["version"] != 1 || value["offset"].as_u64().is_none() || !value["jobs"].is_array() {
        return Err("Invalid pane descriptor".into());
    }
    for key in ["capture", "stream"] {
        if !Path::new(text(&value, key)).is_file() {
            return Err(format!("Missing saved pane {key}").into());
        }
    }
    for job in value["jobs"].as_array().unwrap() {
        if text(job, "binary").is_empty()
            || !Path::new(text(job, "job")).join("config.json").is_file()
            || job["plugins"]
                .as_array()
                .is_none_or(|p| p.iter().any(|v| !v.is_string()))
        {
            return Err("Invalid saved Harbor job".into());
        }
    }
    eprintln!("{value:?}");
    Ok(value)
}

/// Append pane output durably as tmux produces bytes.
/// Parameters: path - output stream file with an existing parent directory.
/// Returns: success after every input chunk and EOF have been synchronized.
pub fn record(path: &Path) -> Result<()> {
    eprintln!("path={path:?}");
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(path)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    let mut input = std::io::stdin().lock();
    let mut buffer = [0u8; 16384];
    loop {
        let count = input.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        file.write_all(&buffer[..count])?;
        file.sync_data()?;
    }
    file.sync_all()?;
    eprintln!("Ok(())");
    Ok(())
}

/// Replay saved output as terminal display and resume supported jobs before entering a shell.
/// Parameters: path - validated pane replay descriptor.
/// Returns: an error on failure; normal execution becomes an interactive shell.
pub fn replay(path: &Path) -> Result<()> {
    eprintln!("path={path:?}");
    let value = descriptor(path)?;
    let mut capture = File::open(text(&value, "capture"))?;
    let mut stream = File::open(text(&value, "stream"))?;
    stream.seek(SeekFrom::Start(value["offset"].as_u64().unwrap()))?;
    let mut output = std::io::stdout().lock();
    std::io::copy(&mut capture, &mut output)?;
    std::io::copy(&mut stream, &mut output)?;
    writeln!(output, "\r\n[tws: restored saved pane output]")?;
    output.flush()?;
    drop(output);
    for job in value["jobs"].as_array().unwrap() {
        if let Err(error) = harbor::resume(job) {
            eprintln!("tws Harbor resume failed: {error}; job artifacts retained");
        }
    }
    let shell = std::env::var("SHELL").unwrap_or("/bin/bash".into());
    Err(Command::new(shell).arg("-l").exec().into())
}

/// Save every managed live session without accessing the desktop bridge.
/// Parameters: store - locked task store.
/// Returns: successful durable checkpoints or the first preservation error.
pub(crate) fn all(store: &Store) -> Result<()> {
    eprintln!("store={:?}", store.root);
    for profile in store.list()? {
        checkpoint(store, &profile)?;
    }
    eprintln!("Ok(())");
    Ok(())
}
