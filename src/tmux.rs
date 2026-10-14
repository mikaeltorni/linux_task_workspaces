use crate::{home, text, Result};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::fs;
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::Path;
use std::process::Command;

/// Execute a tmux control command on an explicit socket.
/// Parameters: socket - server path; args - literal control operands.
/// Returns: command stdout or the tmux error.
pub(crate) fn run(socket: &str, args: &[&str]) -> Result<String> {
    eprintln!("socket={socket}, args={args:?}");
    let mut attempts = 0;
    let p = loop {
        let output = Command::new("tmux")
            .args(["-S", socket])
            .args(args)
            .output()?;
        if args.first() == Some(&"new-session")
            && String::from_utf8_lossy(&output.stderr).contains("server exited unexpectedly")
            && attempts < 3
        {
            attempts += 1;
            std::thread::sleep(std::time::Duration::from_millis(50));
        } else {
            break output;
        }
    };
    if !p.status.success() {
        return Err(String::from_utf8_lossy(&p.stderr).into_owned().into());
    }
    let result = Ok(String::from_utf8(p.stdout)?);
    eprintln!("{result:?}");
    result
}

/// Test process ancestry using the kernel process tree.
/// Parameters: pid - candidate process; parent - required ancestor.
/// Returns: whether the candidate descends from the ancestor.
pub(crate) fn ancestor(mut pid: u32, parent: u32) -> bool {
    eprintln!("pid={pid}, parent={parent}");
    for _ in 0..64 {
        if pid == parent {
            eprintln!("true");
            return true;
        }
        let Ok(stat) = fs::read_to_string(format!("/proc/{pid}/stat")) else {
            eprintln!("false");
            return false;
        };
        let Some(rest) = stat.rsplit_once(')') else {
            eprintln!("false");
            return false;
        };
        let Some(next) = rest
            .1
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse().ok())
        else {
            eprintln!("false");
            return false;
        };
        pid = next;
        if pid <= 1 {
            break;
        }
    }
    eprintln!("false");
    false
}

/// Discover socket options in a tmux client's raw argument vector.
/// Parameters: argv - process arguments; root - client TMUX_TMPDIR; cwd - client working directory; uid - client owner.
/// Returns: the explicitly selected socket path, if one was supplied.
fn argument_socket(argv: &[String], root: &Path, cwd: &Path, uid: u32) -> Option<String> {
    eprintln!("argv={argv:?}, root={root:?}, cwd={cwd:?}, uid={uid}");
    let mut selected = None;
    let mut args = argv.iter().skip(1);
    while let Some(arg) = args.next() {
        if ["-f", "-T", "-c"].contains(&arg.as_str()) {
            args.next();
            continue;
        }
        let option = if arg == "-S" || arg == "-L" {
            args.next().map(|value| (&arg[..], &value[..]))
        } else if let Some(value) = arg.strip_prefix("-S") {
            Some(("-S", value))
        } else {
            arg.strip_prefix("-L").map(|value| ("-L", value))
        };
        if let Some((flag, value)) = option {
            let socket = if flag == "-L" {
                root.join(format!("tmux-{uid}")).join(value)
            } else if Path::new(value).is_absolute() {
                Path::new(value).to_path_buf()
            } else {
                cwd.join(value)
            };
            selected = Some(socket.to_string_lossy().into_owned());
        }
        if !arg.starts_with('-') {
            break;
        }
    }
    eprintln!("{selected:?}");
    selected
}

/// Find sockets used by this terminal's own tmux client descendants.
/// Parameters: pid - terminal process ancestor; uid - current user's identity.
/// Returns: discovered client socket paths, independent of ACC and the caller's environment.
fn descendant_sockets(pid: u32, uid: u32) -> Result<BTreeSet<String>> {
    eprintln!("pid={pid}, uid={uid}");
    let mut sockets = BTreeSet::new();
    for entry in fs::read_dir("/proc")?.flatten() {
        let Some(client) = entry
            .file_name()
            .to_str()
            .and_then(|s| s.parse::<u32>().ok())
        else {
            continue;
        };
        if !entry.metadata().is_ok_and(|m| m.uid() == uid)
            || !fs::read_link(entry.path().join("exe"))
                .is_ok_and(|p| p.file_name() == Some(std::ffi::OsStr::new("tmux")))
            || !ancestor(client, pid)
        {
            continue;
        }
        let environment = fs::read(entry.path().join("environ")).unwrap_or_default();
        let variable = |name: &str| {
            environment
                .split(|b| *b == 0)
                .filter_map(|v| std::str::from_utf8(v).ok())
                .find_map(|v| v.strip_prefix(name))
        };
        let root = Path::new(variable("TMUX_TMPDIR=").unwrap_or("/tmp"));
        let cwd = fs::read_link(entry.path().join("cwd")).unwrap_or_else(|_| Path::new("/").into());
        let argv: Vec<String> = fs::read(entry.path().join("cmdline"))
            .unwrap_or_default()
            .split(|b| *b == 0)
            .filter(|b| !b.is_empty())
            .map(|b| String::from_utf8_lossy(b).into_owned())
            .collect();
        if let Some(socket) = argument_socket(&argv, root, &cwd, uid) {
            sockets.insert(socket);
        }
        if let Some(tmux) = variable("TMUX=") {
            sockets.insert(tmux.split(',').next().unwrap_or("").into());
        }
    }
    sockets.retain(|socket| {
        fs::metadata(socket).is_ok_and(|m| m.file_type().is_socket() && m.uid() == uid)
    });
    let result = Ok(sockets);
    eprintln!("{result:?}");
    result
}

/// Capture sessions attached beneath a terminal, including explicitly selected sockets.
/// Parameters: pid - terminal process ancestor; zero means no process is available.
/// Returns: full snapshots of its attached tmux sessions, or a discovery error.
pub fn attached_sessions(pid: u32) -> Result<Vec<Value>> {
    eprintln!("pid={pid}");
    if pid == 0 {
        let result = Ok(vec![]);
        eprintln!("{result:?}");
        return result;
    }
    let uid = fs::metadata("/proc/self")?.uid();
    let mut sockets = BTreeSet::new();
    let root = std::env::var("TMUX_TMPDIR").unwrap_or_else(|_| "/tmp".into());
    for base in [root.as_str(), "/tmp"] {
        if let Ok(entries) = fs::read_dir(format!("{base}/tmux-{uid}")) {
            for e in entries.flatten() {
                if e.file_type()?.is_socket() {
                    sockets.insert(e.path().to_string_lossy().into_owned());
                }
            }
        }
    }
    if let Ok(t) = std::env::var("TMUX") {
        sockets.insert(t.split(',').next().unwrap_or("").into());
    }
    sockets.extend(descendant_sockets(pid, uid)?);
    let mut sessions = vec![];
    for socket in sockets {
        if let Ok(clients) = run(
            &socket,
            &["list-clients", "-F", "#{client_pid}\t#{session_name}"],
        ) {
            for line in clients.lines() {
                let Some((client, name)) = line.split_once('\t') else {
                    continue;
                };
                if client
                    .parse::<u32>()
                    .is_ok_and(|client| ancestor(client, pid))
                    && !sessions
                        .iter()
                        .any(|s: &Value| text(s, "name") == name && text(s, "socket") == socket)
                {
                    sessions.push(snapshot(&socket, name)?);
                }
            }
        }
    }
    let result = Ok(sessions);
    eprintln!("{result:?}");
    result
}

pub fn snapshot(socket: &str, name: &str) -> Result<Value> {
    let mut windows = vec![];
    let exact = format!("={name}");
    for line in run(
        socket,
        &[
            "list-windows",
            "-t",
            &exact,
            "-F",
            "#{window_id}\t#{window_index}\t#{window_name}\t#{window_layout}\t#{window_active}",
        ],
    )?
    .lines()
    {
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() != 5 {
            return Err("Unexpected tmux window output".into());
        }
        let mut panes = vec![];
        for p in run(
            socket,
            &[
                "list-panes",
                "-t",
                f[0],
                "-F",
                "#{pane_current_path}\t#{pane_current_command}\t#{pane_active}",
            ],
        )?
        .lines()
        {
            let p: Vec<&str> = p.split('\t').collect();
            if p.len() != 3 {
                return Err("Unexpected tmux pane output".into());
            }
            panes.push(json!({"cwd":p[0],"command":p[1],"active":p[2]=="1"}));
        }
        windows.push(json!({"index":f[1].parse::<u32>()?,"name":f[2],"layout":f[3],"active":f[4]=="1","panes":panes}));
    }
    Ok(json!({"socket":socket,"name":name,"windows":windows}))
}

/// Restore a missing session with saved pane replay helpers, preserving active sessions.
/// Parameters: session - saved structure and optional durable pane descriptors.
/// Returns: successful creation or reuse of the saved session.
pub fn restore(session: &Value) -> Result<()> {
    eprintln!("session={session}");
    let socket = text(session, "socket");
    let name = text(session, "name");
    let exact = format!("={name}");
    if run(socket, &["has-session", "-t", &exact]).is_ok() {
        if session["replace_empty"] == true && fresh_empty(session)? {
            run(socket, &["kill-session", "-t", &exact])?;
        } else {
            eprintln!("Ok(())");
            return Ok(());
        }
    }
    if let Some(parent) = Path::new(socket).parent() {
        fs::create_dir_all(parent)?;
    }
    let mut active = String::new();
    for (i, w) in session["windows"]
        .as_array()
        .ok_or("Invalid tmux windows")?
        .iter()
        .enumerate()
    {
        let panes = w["panes"].as_array().ok_or("Invalid tmux panes")?;
        if panes.is_empty() {
            continue;
        }
        let home = home();
        let dir = |p: &Value| {
            let d = text(p, "cwd");
            if Path::new(d).is_dir() {
                d.to_string()
            } else {
                home.to_string_lossy().into_owned()
            }
        };
        let cwd = dir(&panes[0]);
        let wid = if i == 0 {
            create_pane(
                socket,
                &[
                    "new-session",
                    "-d",
                    "-P",
                    "-F",
                    "#{window_id}",
                    "-s",
                    name,
                    "-n",
                    text(w, "name"),
                    "-c",
                    &cwd,
                ],
                &panes[0],
            )?
        } else {
            create_pane(
                socket,
                &[
                    "new-window",
                    "-d",
                    "-P",
                    "-F",
                    "#{window_id}",
                    "-t",
                    &format!("{exact}:"),
                    "-n",
                    text(w, "name"),
                    "-c",
                    &cwd,
                ],
                &panes[0],
            )?
        };
        let wid = wid.trim();
        let index = w["index"].as_u64().ok_or("Invalid tmux window index")?;
        let current_index = run(
            socket,
            &["display-message", "-p", "-t", wid, "#{window_index}"],
        )?;
        if current_index.trim().parse::<u64>()? != index {
            run(
                socket,
                &["move-window", "-s", wid, "-t", &format!("{exact}:{index}")],
            )?;
        }
        for p in &panes[1..] {
            create_pane(socket, &["split-window", "-d", "-t", wid, "-c", &dir(p)], p)?;
            run(socket, &["select-layout", "-t", wid, "tiled"])?;
        }
        let _ = run(socket, &["select-layout", "-t", wid, text(w, "layout")]);
        let ids = run(socket, &["list-panes", "-t", wid, "-F", "#{pane_id}"])?;
        for (p, id) in panes.iter().zip(ids.lines()) {
            if p["active"] == true {
                run(socket, &["select-pane", "-t", id])?;
            }
        }
        if w["active"] == true {
            active = wid.into();
        }
    }
    if !active.is_empty() {
        run(socket, &["select-window", "-t", &active])?;
    }
    eprintln!("Ok(())");
    Ok(())
}

/// Append native replay arguments when creating a saved pane.
/// Parameters: socket - server path; args - tmux creation operands; pane - saved pane descriptor.
/// Returns: command stdout or a creation error.
fn create_pane(socket: &str, args: &[&str], pane: &Value) -> Result<String> {
    eprintln!("socket={socket}, args={args:?}, pane={pane}");
    let mut argv: Vec<String> = args.iter().map(|s| (*s).into()).collect();
    if let Some(path) = pane["restore"].as_str() {
        argv.extend([
            crate::continuity::executable()?
                .to_string_lossy()
                .into_owned(),
            "_restore-pane".into(),
            path.into(),
        ]);
    }
    let result = run(socket, &argv.iter().map(String::as_str).collect::<Vec<_>>());
    eprintln!("{result:?}");
    result
}

/// Recognize a single fresh idle shell created before the checkpoint is restored.
/// Parameters: session - checkpoint carrying the previous server identity.
/// Returns: whether replacement is safe without discarding active work.
fn fresh_empty(session: &Value) -> Result<bool> {
    eprintln!("session={session}");
    let socket = text(session, "socket");
    let target = format!("={}", text(session, "name"));
    if crate::continuity::epoch(socket, &target)? == text(session, "epoch") {
        eprintln!("false");
        return Ok(false);
    }
    let panes = run(
        socket,
        &[
            "list-panes",
            "-s",
            "-t",
            &target,
            "-F",
            "#{pane_pid}\t#{pane_current_command}\t#{history_size}\t#{@tws-record-path}",
        ],
    )?;
    let fields: Vec<_> = panes.trim_end_matches('\n').split('\t').collect();
    let result = if panes.lines().count() != 1
        || fields.len() != 4
        || fields[2] != "0"
        || !fields[3].is_empty()
        || !["bash", "zsh", "fish", "sh"].contains(&fields[1])
    {
        false
    } else {
        let pid = fields[0].parse::<u32>()?;
        let mut content = String::new();
        let mut children = String::new();
        for _ in 0..20 {
            children =
                fs::read_to_string(format!("/proc/{pid}/task/{pid}/children")).unwrap_or_default();
            content = run(socket, &["capture-pane", "-p", "-t", &format!("{target}:")])?;
            if !content.trim().is_empty() && children.trim().is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        children.trim().is_empty()
            && content.lines().filter(|s| !s.trim().is_empty()).count() <= 4
            && content.trim_end().ends_with(['$', '#', '%', '>'])
    };
    eprintln!("{result:?}");
    Ok(result)
}
