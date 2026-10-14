use crate::{tmux, Result};
use serde_json::{json, Value};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::{fs, path::Path, process::Command};

/// Discover resumable Harbor descendants without persisting arbitrary command lines.
/// Parameters: pane_pid - process owning the managed pane.
/// Returns: safe job-resume descriptors with durable job paths and plugin configuration.
pub(crate) fn discover(pane_pid: u32) -> Result<Vec<Value>> {
    eprintln!("pane_pid={pane_pid}");
    let mut jobs = vec![];
    for entry in fs::read_dir("/proc")?.flatten() {
        let Some(pid) = entry.file_name().to_str().and_then(|v| v.parse().ok()) else {
            continue;
        };
        let raw = fs::read(entry.path().join("cmdline")).unwrap_or_default();
        let args: Vec<_> = raw
            .split(|b| *b == 0)
            .filter(|b| !b.is_empty())
            .map(|b| String::from_utf8_lossy(b).into_owned())
            .collect();
        let Some(index) = args
            .iter()
            .take(2)
            .position(|s| Path::new(s).file_name().is_some_and(|n| n == "harbor"))
        else {
            continue;
        };
        if !tmux::ancestor(pid, pane_pid) {
            continue;
        }
        let options = &args[index + 1..];
        let option = |names: &[&str]| {
            options
                .windows(2)
                .find(|v| names.contains(&v[0].as_str()))
                .map(|v| v[1].clone())
        };
        let Ok(cwd) = fs::read_link(entry.path().join("cwd")) else {
            continue;
        };
        let absolute = |value: String| {
            let path = Path::new(&value);
            if path.is_absolute() {
                path.to_path_buf()
            } else {
                cwd.join(path)
            }
        };
        let job = if options.starts_with(&["jobs".into(), "resume".into()]) {
            option(&["--job-path", "-p"]).map(absolute)
        } else if options.first().is_some_and(|s| s == "run") {
            option(&["--job-name"]).map(|name| {
                absolute(option(&["--jobs-dir", "-o"]).unwrap_or("jobs".into())).join(name)
            })
        } else {
            None
        };
        let Some(job) = job.filter(|p| p.join("config.json").is_file()) else {
            continue;
        };
        let binary = absolute(args[index].clone());
        let plugins: Vec<_> = options
            .windows(2)
            .filter(|v| v[0] == "--plugin")
            .map(|v| v[1].clone())
            .collect();
        let environment = fs::read(entry.path().join("environ")).unwrap_or_default();
        let pythonpath = environment
            .split(|b| *b == 0)
            .filter_map(|v| std::str::from_utf8(v).ok())
            .find_map(|v| v.strip_prefix("PYTHONPATH="))
            .unwrap_or("");
        let descriptor =
            json!({"binary":binary,"job":job,"plugins":plugins,"pythonpath":pythonpath});
        if !jobs.contains(&descriptor) {
            jobs.push(descriptor);
        }
    }
    let result = Ok(jobs);
    eprintln!("{result:?}");
    result
}

/// Repair Harbor's redacted mount references using the original durable job configuration.
/// Parameters: job - existing Harbor job directory.
/// Returns: success preserving original trial configs beside each repaired file.
fn prepare(job: &Path) -> Result<()> {
    eprintln!("job={job:?}");
    let config: Value = serde_json::from_slice(&fs::read(job.join("config.json"))?)?;
    let originals = config["environment"]["mounts"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    for entry in fs::read_dir(job)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let path = entry.path().join("config.json");
        if !path.is_file() {
            continue;
        }
        let bytes = fs::read(&path)?;
        let mut trial: Value = serde_json::from_slice(&bytes)?;
        let mut changed = false;
        if let Some(mounts) = trial["environment"]["mounts"].as_array_mut() {
            for mount in mounts {
                if mount["source"] != "[REDACTED]" {
                    continue;
                }
                let matching: Vec<_> = originals
                    .iter()
                    .filter(|m| {
                        m["target"] == mount["target"]
                            && m["type"] == mount["type"]
                            && m["read_only"] == mount["read_only"]
                            && m["source"].as_str().is_some_and(|s| s != "[REDACTED]")
                    })
                    .collect();
                if matching.len() != 1 {
                    return Err(format!(
                        "Cannot safely recover redacted Harbor mount in {}",
                        path.display()
                    )
                    .into());
                }
                mount["source"] = matching[0]["source"].clone();
                changed = true;
            }
        }
        if changed {
            let backup = entry.path().join("config.tws-before-resume.json");
            if !backup.exists() {
                let mut file = fs::OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .mode(0o600)
                    .open(backup)?;
                file.write_all(&bytes)?;
                file.sync_all()?;
            }
            let temporary = path.with_extension(format!("tws-{}.tmp", std::process::id()));
            let mut file = fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .mode(0o600)
                .open(&temporary)?;
            serde_json::to_writer(&mut file, &trial)?;
            file.sync_all()?;
            fs::rename(temporary, &path)?;
            fs::File::open(entry.path())?.sync_all()?;
        }
    }
    eprintln!("Ok(())");
    Ok(())
}

/// Resume a known Harbor job using durable application state and credential path references.
/// Parameters: saved - safe Harbor executable, job and plugin descriptor.
/// Returns: success or the application resume error; completed trial artifacts are retained.
pub(crate) fn resume(saved: &Value) -> Result<()> {
    eprintln!("saved={saved}");
    let job = Path::new(crate::text(saved, "job"));
    prepare(job)?;
    let config: Value = serde_json::from_slice(&fs::read(job.join("config.json"))?)?;
    let mut command = Command::new(crate::text(saved, "binary"));
    command.args(["jobs", "resume", "--job-path", crate::text(saved, "job")]);
    for plugin in saved["plugins"]
        .as_array()
        .ok_or("Invalid Harbor plugins")?
    {
        command.args(["--plugin", plugin.as_str().ok_or("Invalid Harbor plugin")?]);
    }
    if !crate::text(saved, "pythonpath").is_empty() {
        command.env("PYTHONPATH", crate::text(saved, "pythonpath"));
    }
    if let Some(mount) = config["environment"]["mounts"]
        .as_array()
        .and_then(|m| m.iter().find(|m| m["target"] == "/root/.codex/auth.json"))
    {
        if let Some(path) = mount["source"].as_str().filter(|s| *s != "[REDACTED]") {
            command.env("CODEX_AUTH_JSON_PATH", path);
        }
    }
    let status = command.status()?;
    if !status.success() {
        return Err(format!("Harbor resume exited {status}; saved artifacts retained").into());
    }
    eprintln!("Ok(())");
    Ok(())
}
