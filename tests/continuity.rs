#[path = "support/terminal_fixture.rs"]
mod terminal_fixture;
use linux_task_workspaces::Store;
use serde_json::json;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};
use terminal_fixture::{call, window, Fixture};

/// Wait for a public tmux pane to expose a durable-output marker.
/// Parameters: fixture - owned isolated server; target - exact pane; marker - independently expected output.
/// Returns: complete pane contents once the marker appears.
fn wait_output(fixture: &Fixture, target: &str, marker: &str) -> String {
    eprintln!("fixture={fixture:?}, target={target}, marker={marker}");
    let start = Instant::now();
    let result = loop {
        let text = fixture.tmux(&["capture-pane", "-p", "-S", "-", "-t", target]);
        if text.contains(marker) {
            break text;
        }
        assert!(start.elapsed() < Duration::from_secs(8), "missing {marker}");
        std::thread::sleep(Duration::from_millis(20));
    };
    eprintln!("{result:?}");
    result
}

/// Recover live structure, cwd and continuously recorded output after server loss.
/// Parameters: none.
/// Returns: None; asserts restored state and no job restart while the session remains live.
#[test]
fn checkpoint_recovers_session_continuity() {
    eprintln!("parameters=none");
    let fixture = Fixture::new();
    fixture.standalone();
    let store = Store {
        root: fixture.root.path().join("profiles"),
    };
    let pid = fixture.clients.borrow()[0].id();
    fixture.state.borrow_mut()["windows"] = json!([window("task", "kitty", pid)]);
    call(
        &["configure", "backup", "--workspace", "3"],
        &store,
        &fixture,
    )
    .unwrap();
    let pane = fixture
        .tmux(&["display-message", "-p", "-t", "=backups:0", "#{pane_id}"])
        .trim()
        .to_owned();
    fixture.tmux(&[
        "send-keys",
        "-t",
        &pane,
        "-l",
        "printf '\\n%s\\n' 'history-before-checkpoint'",
    ]);
    fixture.tmux(&["send-keys", "-t", &pane, "Enter"]);
    wait_output(&fixture, &pane, "history-before-checkpoint");
    call(&["backup", "checkpoint"], &store, &fixture).unwrap();
    let checkpoint = store.root.join(".continuity/backup/checkpoint.json");
    assert!(checkpoint.exists());
    for _ in 0..8 {
        call(&["backup", "checkpoint"], &store, &fixture).unwrap();
    }
    let generations = fs::read_dir(checkpoint.parent().unwrap())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with("generation-"))
        .count();
    assert!(
        generations <= 4,
        "unbounded snapshot duplication: {generations}"
    );
    let expected = call(&["backup", "show"], &store, &fixture).unwrap();
    for args in [
        vec!["backup", "checkpoint", "extra"],
        vec!["missing", "checkpoint"],
        vec!["_checkpoint-all", "extra"],
        vec!["unknown"],
    ] {
        assert!(call(&args, &store, &fixture).is_err());
        assert_eq!(
            call(&["backup", "show"], &store, &fixture).unwrap(),
            expected
        );
        assert!(fixture
            .tmux(&["capture-pane", "-p", "-S", "-", "-t", &pane])
            .contains("history-before-checkpoint"));
    }
    let cwd = fixture.root.path().join("work dir");
    fs::create_dir(&cwd).unwrap();
    fixture.tmux(&[
        "send-keys",
        "-t",
        &pane,
        "-l",
        &format!("cd '{}'", cwd.display()),
    ]);
    fixture.tmux(&["send-keys", "-t", &pane, "Enter"]);
    fixture.tmux(&["move-window", "-s", "=backups:0", "-t", "=backups:3"]);
    fixture.tmux(&[
        "split-window",
        "-d",
        "-t",
        "=backups:3",
        "-c",
        cwd.to_str().unwrap(),
    ]);
    fixture.tmux(&[
        "new-window",
        "-d",
        "-t",
        "=backups:7",
        "-n",
        "second",
        "-c",
        cwd.to_str().unwrap(),
    ]);
    call(&["_checkpoint-all"], &store, &fixture).unwrap();
    call(&["_startup"], &store, &fixture).unwrap();
    let before_pids = fixture.tmux(&["list-panes", "-s", "-t", "=backups", "-F", "#{pane_pid}"]);
    fixture.tmux(&[
        "send-keys",
        "-t",
        &pane,
        "-l",
        "printf '\\n%s\\n' 'output-after-last-checkpoint'",
    ]);
    fixture.tmux(&["send-keys", "-t", &pane, "Enter"]);
    wait_output(&fixture, &pane, "output-after-last-checkpoint");
    let stream = fixture.tmux(&["show-options", "-p", "-v", "-t", &pane, "@tws-record-path"]);
    let start = Instant::now();
    while !fs::read_to_string(stream.trim())
        .unwrap_or_default()
        .contains("output-after-last-checkpoint")
    {
        assert!(start.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(20));
    }
    fixture.tmux(&["kill-server"]);
    fixture.state.borrow_mut()["windows"] = json!([]);
    fixture.actions.borrow_mut().clear();
    call(&["_startup"], &store, &fixture).unwrap();
    let restored = wait_output(&fixture, "=backups:3.0", "output-after-last-checkpoint");
    assert!(restored.contains("history-before-checkpoint"));
    assert_eq!(
        fixture.tmux(&["list-windows", "-t", "=backups", "-F", "#{window_index}"]),
        "3\n7\n"
    );
    assert_eq!(
        fixture
            .tmux(&[
                "list-panes",
                "-s",
                "-t",
                "=backups",
                "-F",
                "#{pane_current_path}"
            ])
            .lines()
            .collect::<Vec<_>>(),
        vec![cwd.to_str().unwrap(); 3]
    );
    let restored_pids = fixture.tmux(&["list-panes", "-s", "-t", "=backups", "-F", "#{pane_pid}"]);
    assert_ne!(before_pids, restored_pids);
    call(&["_startup"], &store, &fixture).unwrap();
    assert_eq!(
        fixture.tmux(&["list-panes", "-s", "-t", "=backups", "-F", "#{pane_pid}"]),
        restored_pids
    );
    assert!(!fixture.actions.borrow().iter().any(|a| a["op"] == "focus"));
    assert!(fixture.state.borrow()["windows"]
        .as_array()
        .unwrap()
        .iter()
        .all(|w| w["workspace"] == 2));
    eprintln!("None");
}

/// Reject foreign pane pipes without replacing user state or saved profiles.
/// Parameters: none.
/// Returns: None; verifies public profile and existing pipe after each rejection.
#[test]
fn checkpoint_preserves_foreign_pipes() {
    eprintln!("parameters=none");
    let fixture = Fixture::new();
    let store = Store {
        root: fixture.root.path().join("profiles"),
    };
    fixture.tmux(&["new-session", "-d", "-s", "foreign"]);
    let pid = fixture.attach("foreign");
    fixture.state.borrow_mut()["windows"] = json!([window("foreign", "kitty", pid)]);
    call(
        &["configure", "foreign", "--workspace", "3"],
        &store,
        &fixture,
    )
    .unwrap();
    fixture.tmux(&["pipe-pane", "-t", "=foreign:0", "cat >/dev/null"]);
    let expected = call(&["foreign", "show"], &store, &fixture).unwrap();
    assert!(call(&["foreign", "checkpoint"], &store, &fixture).is_err());
    assert_eq!(
        call(&["foreign", "show"], &store, &fixture).unwrap(),
        expected
    );
    assert_eq!(
        fixture
            .tmux(&["display-message", "-p", "-t", "=foreign:0", "#{pane_pipe}"])
            .trim(),
        "1"
    );
    eprintln!("None");
}

/// Invoke the installed-form internal runtime grammar in isolated state.
/// Parameters: args - CLI operands; root - temporary task config; input - stdin bytes.
/// Returns: complete child output and status.
fn native(args: &[&str], root: &Path, input: &[u8]) -> Output {
    eprintln!("args={args:?}, root={root:?}, input={input:?}");
    let mut child = Command::new(env!("CARGO_BIN_EXE_tws"))
        .args(args)
        .env("TWS_CONFIG_HOME", root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    let result = child.wait_with_output().unwrap();
    eprintln!("{result:?}");
    result
}

/// Persist recorder bytes and reject malformed internal runtime forms safely.
/// Parameters: none.
/// Returns: None; verifies data, private permissions and public seeded profile reads.
#[test]
fn recorder_runtime_contract() {
    eprintln!("parameters=none");
    let fixture = Fixture::new();
    let store = Store {
        root: fixture.root.path().join("profiles"),
    };
    fixture.state.borrow_mut()["windows"] = json!([]);
    call(
        &["configure", "empty", "--workspace", "3"],
        &store,
        &fixture,
    )
    .unwrap();
    let expected = call(&["empty", "show"], &store, &fixture).unwrap();
    let file = fixture.root.path().join("durable stream");
    let path = file.to_str().unwrap();
    let output = native(
        &["_record-pane", path],
        &store.root,
        b"durable recorder output\n",
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(fs::read(&file).unwrap(), b"durable recorder output\n");
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(
        fs::metadata(&file).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let missing = fixture.root.path().join("missing-parent/log");
    let bad = fixture.root.path().join("broken-descriptor.json");
    fs::write(&bad, b"not JSON").unwrap();
    for args in [
        vec!["_record-pane"],
        vec!["_record-pane", path, "extra"],
        vec!["_restore-pane"],
        vec!["_restore-pane", path, "extra"],
        vec!["_record-pane", missing.to_str().unwrap()],
        vec!["_restore-pane", missing.to_str().unwrap()],
        vec!["_restore-pane", bad.to_str().unwrap()],
        vec!["_checkpoint-all", "extra"],
    ] {
        assert!(!native(&args, &store.root, b"").status.success());
        assert_eq!(
            call(&["empty", "show"], &store, &fixture).unwrap(),
            expected
        );
        assert_eq!(fs::read(&file).unwrap(), b"durable recorder output\n");
    }
    eprintln!("None");
}

/// Fall back to the prior durable manifest and reject loss of both manifests.
/// Parameters: none.
/// Returns: None; verifies recovered output, no mutation on rejection and preserved evidence.
#[test]
fn checkpoint_corruption_preserves_previous_state() {
    eprintln!("parameters=none");
    let fixture = Fixture::new();
    let store = Store {
        root: fixture.root.path().join("profiles"),
    };
    fixture.state.borrow_mut()["windows"] =
        json!([window("task", "kitty", fixture.clients.borrow()[0].id())]);
    call(
        &["configure", "backup", "--workspace", "3"],
        &store,
        &fixture,
    )
    .unwrap();
    fixture.tmux(&[
        "send-keys",
        "-t",
        "=backups:0",
        "-l",
        "printf '\\n%s\\n' 'previous-good-history'",
    ]);
    fixture.tmux(&["send-keys", "-t", "=backups:0", "Enter"]);
    wait_output(&fixture, "=backups:0", "previous-good-history");
    call(&["backup", "checkpoint"], &store, &fixture).unwrap();
    call(&["backup", "checkpoint"], &store, &fixture).unwrap();
    let state = store.root.join(".continuity/backup");
    fs::write(state.join("checkpoint.json"), "broken manifest").unwrap();
    fixture.tmux(&["kill-server"]);
    fixture.state.borrow_mut()["windows"] = json!([]);
    call(&["_startup"], &store, &fixture).unwrap();
    wait_output(&fixture, "=backups:0", "previous-good-history");
    // A later successful checkpoint must repair the head and preserve a valid fallback.
    call(&["backup", "checkpoint"], &store, &fixture).unwrap();
    let expected = call(&["backup", "show"], &store, &fixture).unwrap();
    fixture.tmux(&["kill-server"]);
    fixture.state.borrow_mut()["windows"] = json!([]);
    fs::write(state.join("checkpoint.json"), "broken manifest").unwrap();
    fs::write(state.join("checkpoint.previous.json"), "broken fallback").unwrap();
    assert!(call(&["_startup"], &store, &fixture).is_err());
    assert_eq!(
        call(&["backup", "show"], &store, &fixture).unwrap(),
        expected
    );
    assert!(fixture.state.borrow()["windows"]
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(
        fs::read_to_string(state.join("checkpoint.previous.json")).unwrap(),
        "broken fallback"
    );
    eprintln!("None");
}

/// Deliver stop-time checkpointing without real systemd or package changes.
/// Parameters: none.
/// Returns: None; verifies the installed unit preserves tmux children and saves on stop.
#[test]
fn installer_checkpoint_contract() {
    eprintln!("parameters=none");
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    fs::create_dir(&home).unwrap();
    let output = Command::new("bash")
        .arg("install.sh")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("TWS_INSTALL_HOME", &home)
        .env("TWS_INSTALL_OFFLINE", "1")
        .env("TWS_BINARY", env!("CARGO_BIN_EXE_tws"))
        .env(
            "ISC_FUNCTIONS_DIR",
            "/home/mk/projects/linux_installation_scripts_functions",
        )
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let unit =
        fs::read_to_string(home.join(".config/systemd/user/task-workspaces.service")).unwrap();
    assert!(unit.contains("_checkpoint-all"));
    assert!(unit.contains("KillMode=process"));
    eprintln!("None");
}

/// Recover a prior checkpoint over a fresh empty shell while preserving live work.
/// Parameters: none.
/// Returns: None; verifies saved history survives early empty-session creation.
#[test]
fn checkpoint_handles_precreated_empty_session() {
    eprintln!("parameters=none");
    let fixture = Fixture::new();
    let store = Store {
        root: fixture.root.path().join("profiles"),
    };
    fixture.state.borrow_mut()["windows"] =
        json!([window("task", "kitty", fixture.clients.borrow()[0].id())]);
    call(
        &["configure", "backup", "--workspace", "3"],
        &store,
        &fixture,
    )
    .unwrap();
    fixture.tmux(&[
        "send-keys",
        "-t",
        "=backups:0",
        "-l",
        "printf '\\n%s\\n' 'saved-before-empty-session'",
    ]);
    fixture.tmux(&["send-keys", "-t", "=backups:0", "Enter"]);
    wait_output(&fixture, "=backups:0", "saved-before-empty-session");
    call(&["backup", "checkpoint"], &store, &fixture).unwrap();
    fixture.tmux(&["kill-server"]);
    fixture.tmux(&["new-session", "-d", "-s", "backups"]);
    fixture.state.borrow_mut()["windows"] = json!([]);
    call(&["_startup"], &store, &fixture).unwrap();
    wait_output(&fixture, "=backups:0", "saved-before-empty-session");
    fixture.tmux(&["kill-server"]);
    fixture.tmux(&["new-session", "-d", "-s", "backups", "sleep 300"]);
    let live = fixture.tmux(&["list-panes", "-t", "=backups:0", "-F", "#{pane_pid}"]);
    fixture.state.borrow_mut()["windows"] = json!([]);
    call(&["_startup"], &store, &fixture).unwrap();
    assert_eq!(
        fixture.tmux(&["list-panes", "-t", "=backups:0", "-F", "#{pane_pid}"]),
        live
    );
    eprintln!("None");
}

/// Resume interrupted Harbor jobs from their own state without replaying arbitrary commands.
/// Parameters: none.
/// Returns: None; verifies a fake job resumes once after server loss and stays live on repeat load.
#[test]
fn checkpoint_resumes_harbor_job() {
    eprintln!("parameters=none");
    let fixture = Fixture::new();
    let store = Store {
        root: fixture.root.path().join("profiles"),
    };
    fixture.state.borrow_mut()["windows"] =
        json!([window("task", "kitty", fixture.clients.borrow()[0].id())]);
    call(
        &["configure", "backup", "--workspace", "3"],
        &store,
        &fixture,
    )
    .unwrap();
    let jobs = fixture.root.path().join("job state");
    fs::create_dir_all(jobs.join("demo")).unwrap();
    let mount = json!({"type":"bind","source":"/safe/auth.json","target":"/root/.codex/auth.json","read_only":true});
    fs::write(
        jobs.join("demo/config.json"),
        json!({"environment":{"mounts":[mount]}}).to_string(),
    )
    .unwrap();
    let trial = jobs.join("demo/trial");
    fs::create_dir(&trial).unwrap();
    fs::write(trial.join("config.json"),json!({"environment":{"mounts":[{"type":"bind","source":"[REDACTED]","target":"/root/.codex/auth.json","read_only":true}]}}).to_string()).unwrap();
    let marker = fixture.root.path().join("resumed-marker");
    let harbor = fixture.root.path().join("harbor");
    fs::write(&harbor,format!("#!/bin/sh\nif [ \"$1\" = jobs ] && [ \"$2\" = resume ]; then if grep -q REDACTED '{}/config.json'; then exit 7; fi; printf 'resumed\\n' >> '{}'; printf 'harbor-resumed\\n'; exit 0; fi\nprintf 'live-harbor-run\\n'\nwhile :; do sleep 1; done\n",trial.display(),marker.display())).unwrap();
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&harbor, fs::Permissions::from_mode(0o755)).unwrap();
    fixture.tmux(&[
        "new-window",
        "-d",
        "-t",
        "=backups:5",
        "-n",
        "harbor",
        harbor.to_str().unwrap(),
        "run",
        "--job-name",
        "demo",
        "--jobs-dir",
        jobs.to_str().unwrap(),
    ]);
    wait_output(&fixture, "=backups:5", "live-harbor-run");
    call(&["backup", "checkpoint"], &store, &fixture).unwrap();
    fixture.tmux(&["kill-server"]);
    fixture.state.borrow_mut()["windows"] = json!([]);
    call(&["_startup"], &store, &fixture).unwrap();
    wait_output(&fixture, "=backups:5", "harbor-resumed");
    assert!(trial.join("config.tws-before-resume.json").exists());
    assert!(!fs::read_to_string(trial.join("config.json"))
        .unwrap()
        .contains("REDACTED"));
    assert_eq!(fs::read_to_string(&marker).unwrap(), "resumed\n");
    let pids = fixture.tmux(&["list-panes", "-s", "-t", "=backups", "-F", "#{pane_pid}"]);
    call(&["backup", "load"], &store, &fixture).unwrap();
    assert_eq!(
        fixture.tmux(&["list-panes", "-s", "-t", "=backups", "-F", "#{pane_pid}"]),
        pids
    );
    assert_eq!(fs::read_to_string(&marker).unwrap(), "resumed\n");
    eprintln!("None");
}

/// Reuse durable pane history during public checkpoints while recording new output.
/// Parameters: none.
/// Returns: None; asserts no recurring history capture and complete recovery after pruning.
#[test]
fn repeated_checkpoints_reuse_history_and_keep_streamed_output() {
    eprintln!("parameters=none");
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new();
    fixture.standalone();
    let store = Store {
        root: fixture.root.path().join("profiles"),
    };
    let pid = fixture.clients.borrow()[0].id();
    fixture.state.borrow_mut()["windows"] = json!([window("task", "kitty", pid)]);
    call(
        &["configure", "incremental", "--workspace", "3"],
        &store,
        &fixture,
    )
    .unwrap();
    let pane = fixture
        .tmux(&["display-message", "-p", "-t", "=backups:0", "#{pane_id}"])
        .trim()
        .to_owned();
    fixture.tmux(&[
        "send-keys",
        "-t",
        &pane,
        "-l",
        "printf '\\nbase-history-marker\\n'",
    ]);
    fixture.tmux(&["send-keys", "-t", &pane, "Enter"]);
    wait_output(&fixture, &pane, "base-history-marker");
    call(&["incremental", "checkpoint"], &store, &fixture).unwrap();
    let manifest_path = store.root.join(".continuity/incremental/checkpoint.json");
    let first: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
    let descriptor_path = first["sessions"][0]["windows"][0]["panes"][0]["restore"]
        .as_str()
        .unwrap();
    let initial: serde_json::Value =
        serde_json::from_slice(&fs::read(descriptor_path).unwrap()).unwrap();
    let initial_capture = fs::read(initial["capture"].as_str().unwrap()).unwrap();
    let bin = fixture.root.path().join("bin");
    fs::create_dir(&bin).unwrap();
    let shim = bin.join("tmux");
    fs::write(&shim, "#!/bin/sh\nfor arg in \"$@\"; do\n if [ \"$arg\" = capture-pane ]; then echo 'repeated full-history capture blocks input' >&2; exit 91; fi\ndone\nexec /usr/bin/tmux \"$@\"\n").unwrap();
    fs::set_permissions(&shim, fs::Permissions::from_mode(0o755)).unwrap();
    std::env::set_var("PATH", format!("{}:/usr/bin:/bin", bin.display()));
    fixture.tmux(&[
        "send-keys",
        "-t",
        &pane,
        "-l",
        "printf '\\nstream-after-base-marker\\n'",
    ]);
    fixture.tmux(&["send-keys", "-t", &pane, "Enter"]);
    for _ in 0..6 {
        call(&["incremental", "checkpoint"], &store, &fixture).unwrap();
    }
    call(&["_checkpoint-all"], &store, &fixture).unwrap();
    let latest: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
    let descriptor_path = latest["sessions"][0]["windows"][0]["panes"][0]["restore"]
        .as_str()
        .unwrap();
    let current: serde_json::Value =
        serde_json::from_slice(&fs::read(descriptor_path).unwrap()).unwrap();
    assert_eq!(current["offset"], initial["offset"]);
    assert_eq!(
        fs::read(current["capture"].as_str().unwrap()).unwrap(),
        initial_capture
    );
    std::env::set_var("PATH", "/usr/bin:/bin");
    let stream = current["stream"].as_str().unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !fs::read_to_string(stream)
        .unwrap()
        .contains("stream-after-base-marker")
    {
        assert!(
            Instant::now() < deadline,
            "recorder lost output after the base capture"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    fixture.tmux(&["kill-server"]);
    fixture.state.borrow_mut()["windows"] = json!([]);
    call(&["_startup"], &store, &fixture).unwrap();
    let restored = wait_output(&fixture, "=backups:0", "stream-after-base-marker");
    assert!(restored.contains("base-history-marker"));
    eprintln!("None");
}

/// Rebase interrupted recording safely instead of silently losing later output.
/// Parameters: none.
/// Returns: None; checks legacy descriptors, truncation, pipe removal and inode replacement.
#[test]
fn checkpoints_rebase_when_recording_continuity_breaks() {
    eprintln!("parameters=none");
    let fixture = Fixture::new();
    fixture.standalone();
    let store = Store {
        root: fixture.root.path().join("profiles"),
    };
    let pid = fixture.clients.borrow()[0].id();
    fixture.state.borrow_mut()["windows"] = json!([window("task", "kitty", pid)]);
    call(
        &["configure", "rebase", "--workspace", "3"],
        &store,
        &fixture,
    )
    .unwrap();
    call(&["rebase", "checkpoint"], &store, &fixture).unwrap();
    let pane = fixture
        .tmux(&["display-message", "-p", "-t", "=backups:0", "#{pane_id}"])
        .trim()
        .to_owned();
    let manifest_path = store.root.join(".continuity/rebase/checkpoint.json");
    for mode in ["legacy", "truncated", "disconnected", "replaced"] {
        let manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        let descriptor_path = manifest["sessions"][0]["windows"][0]["panes"][0]["restore"]
            .as_str()
            .unwrap();
        let mut saved: serde_json::Value =
            serde_json::from_slice(&fs::read(descriptor_path).unwrap()).unwrap();
        let stream = saved["stream"].as_str().unwrap().to_owned();
        match mode {
            "legacy" => {
                saved.as_object_mut().unwrap().remove("stream_inode");
                fs::write(descriptor_path, serde_json::to_vec(&saved).unwrap()).unwrap();
            }
            "truncated" => {
                fs::write(&stream, b"").unwrap();
            }
            "disconnected" => {
                fixture.tmux(&["pipe-pane", "-t", &pane]);
            }
            "replaced" => {
                fs::rename(&stream, format!("{stream}.old")).unwrap();
                fs::write(&stream, b"").unwrap();
            }
            _ => unreachable!(),
        }
        call(&["rebase", "checkpoint"], &store, &fixture).unwrap();
        let marker = format!("stream-after-{mode}-rebase");
        fixture.tmux(&[
            "send-keys",
            "-t",
            &pane,
            "-l",
            &format!("printf '\\n{marker}\\n'"),
        ]);
        fixture.tmux(&["send-keys", "-t", &pane, "Enter"]);
        wait_output(&fixture, &pane, &marker);
        let deadline = Instant::now() + Duration::from_secs(5);
        while !fs::read_to_string(&stream).unwrap().contains(&marker) {
            assert!(Instant::now() < deadline, "lost output after {mode} rebase");
            std::thread::sleep(Duration::from_millis(20));
        }
        call(&["rebase", "checkpoint"], &store, &fixture).unwrap();
    }
    eprintln!("None");
}
