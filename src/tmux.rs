use crate::{home, text, Result};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::fs;
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::Path;
use std::process::Command;

fn run(socket: &str, args: &[&str]) -> Result<String> {
    let p = Command::new("tmux")
        .args(["-S", socket])
        .args(args)
        .output()?;
    if !p.status.success() {
        return Err(String::from_utf8_lossy(&p.stderr).into_owned().into());
    }
    Ok(String::from_utf8(p.stdout)?)
}

fn ancestor(mut pid: u32, parent: u32) -> bool {
    for _ in 0..64 {
        if pid == parent {
            return true;
        }
        let Ok(stat) = fs::read_to_string(format!("/proc/{pid}/stat")) else {
            return false;
        };
        let Some(rest) = stat.rsplit_once(')') else {
            return false;
        };
        let Some(next) = rest
            .1
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse().ok())
        else {
            return false;
        };
        pid = next;
        if pid <= 1 {
            break;
        }
    }
    false
}

pub fn attached_sessions(pid: u32) -> Result<Vec<Value>> {
    if pid == 0 {
        return Ok(vec![]);
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
    Ok(sessions)
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

pub fn restore(session: &Value) -> Result<()> {
    let socket = text(session, "socket");
    let name = text(session, "name");
    let exact = format!("={name}");
    if run(socket, &["has-session", "-t", &exact]).is_ok() {
        return Ok(());
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
            run(
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
            )?
        } else {
            run(
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
            )?
        };
        let wid = wid.trim();
        for p in &panes[1..] {
            run(socket, &["split-window", "-d", "-t", wid, "-c", &dir(p)])?;
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
    Ok(())
}
