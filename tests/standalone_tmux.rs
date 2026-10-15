// Run: cargo test --locked --offline --test standalone_tmux
#[path = "support/terminal_fixture.rs"]
mod fixture;
use fixture::{call, window, Fixture};
use linux_task_workspaces::Store;
use serde_json::{json, Value};

/// Verify custom-socket tmux capture and ongoing placement with ACC unavailable.
/// Parameters: none.
/// Returns: None.
#[test]
fn binds_without_acc() {
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
    let saved: Value =
        serde_json::from_str(&call(&["backup", "show"], &store, &fixture).unwrap()).unwrap();
    assert_eq!(
        saved["windows"].as_array().unwrap().len(),
        1,
        "custom-socket tmux must be discoverable outside tmux and ACC"
    );
    assert_eq!(saved["windows"][0]["tmux"][0]["socket"], fixture.socket);
    assert_eq!(saved["windows"][0]["tmux"][0]["name"], "backups");
    call(&["backup", "save"], &store, &fixture).unwrap();
    let panes = fixture.tmux(&["list-panes", "-t", "=backups", "-F", "#{pane_pid}"]);
    let mut moved = window("reattached", "kitty", pid);
    moved["title"] = json!("renamed terminal");
    moved["workspace"] = json!(0);
    fixture.state.borrow_mut()["windows"] = json!([moved]);
    fixture.actions.borrow_mut().clear();
    // Binding an existing attached client must work even in on-demand mode.
    call(&["_startup"], &store, &fixture).unwrap();
    assert_eq!(fixture.state.borrow()["windows"][0]["workspace"], 2);
    assert!(!fixture
        .actions
        .borrow()
        .iter()
        .any(|a| a.get("launch").is_some() || a["op"] == "focus"));
    call(
        &["configure", "backup", "--mode", "always"],
        &store,
        &fixture,
    )
    .unwrap();
    for operation in ["_startup", "_startup"] {
        call(&[operation], &store, &fixture).unwrap();
        assert!(!fixture
            .actions
            .borrow()
            .iter()
            .any(|a| a.get("launch").is_some() || a["op"] == "focus"));
    }
    call(&["backup", "load"], &store, &fixture).unwrap();
    assert_eq!(
        fixture.tmux(&["list-panes", "-t", "=backups", "-F", "#{pane_pid}"]),
        panes
    );
    let expected = call(&["backup", "show"], &store, &fixture).unwrap();
    for args in [
        vec!["backup", "load", "extra"],
        vec!["_startup", "extra"],
        vec!["missing", "load"],
    ] {
        assert!(call(&args, &store, &fixture).is_err());
        assert_eq!(
            call(&["backup", "show"], &store, &fixture).unwrap(),
            expected
        );
    }
    call(&["backup", "hide"], &store, &fixture).unwrap();
    fixture.actions.borrow_mut().clear();
    call(&["_startup"], &store, &fixture).unwrap();
    assert!(!fixture
        .actions
        .borrow()
        .iter()
        .any(|a| a["op"] == "place" || a.get("launch").is_some()));
    eprintln!("None");
}

/// Match terminal sessions across new identities/titles and recreate detached structures safely.
/// Parameters: none.
/// Returns: None.
#[test]
fn restores_by_session() {
    eprintln!("parameters=none");
    let fixture = Fixture::new();
    let store = Store {
        root: fixture.root.path().join("profiles"),
    };
    let pid = fixture.clients.borrow()[0].id();
    fixture.state.borrow_mut()["windows"] = json!([window("task", "org.gnome.Terminal", pid)]);
    call(
        &["configure", "backup", "--workspace", "3"],
        &store,
        &fixture,
    )
    .unwrap();
    fixture.standalone();
    fixture.tmux(&["new-session", "-d", "-s", "unrelated"]);
    let other_pid = fixture.attach("unrelated");
    let mut wrong = window("task", "org.gnome.Terminal", other_pid);
    wrong["workspace"] = json!(2);
    let mut matching = window("moved", "kitty", pid);
    matching["workspace"] = json!(0);
    matching["title"] = json!("changed title");
    let second_pid = fixture.attach("backups");
    let mut additional = window("additional", "kitty", second_pid);
    additional["workspace"] = json!(1);
    fixture.state.borrow_mut()["windows"] = json!([wrong.clone(), matching, additional]);
    fixture.actions.borrow_mut().clear();
    let panes = fixture.tmux(&["list-panes", "-t", "=backups", "-F", "#{pane_pid}"]);
    call(&["backup", "load"], &store, &fixture).unwrap();
    assert!(!fixture
        .actions
        .borrow()
        .iter()
        .any(|a| a["op"] == "place" && a["id"] == "task"));
    assert_eq!(fixture.state.borrow()["windows"][1]["workspace"], 2);
    assert_eq!(fixture.state.borrow()["windows"][2]["workspace"], 2);
    assert!(!fixture
        .actions
        .borrow()
        .iter()
        .any(|a| a.get("launch").is_some()));
    assert_eq!(
        fixture.tmux(&["list-panes", "-t", "=backups", "-F", "#{pane_pid}"]),
        panes
    );
    fixture.tmux(&["kill-session", "-t", "=backups"]);
    fixture.state.borrow_mut()["windows"] = json!([wrong]);
    fixture.actions.borrow_mut().clear();
    call(&["backup", "load"], &store, &fixture).unwrap();
    let actions = fixture.actions.borrow();
    let launches: Vec<_> = actions
        .iter()
        .filter(|a| a.get("launch").is_some())
        .collect();
    assert_eq!(launches.len(), 1);
    assert_eq!(launches[0]["launch"][3], fixture.socket);
    assert_eq!(launches[0]["launch"][6], "=backups");
    assert!(actions
        .iter()
        .any(|a| a["op"] == "place" && a["id"] == "restored" && a["workspace"] == 2));
    drop(actions);
    call(&["backup", "load"], &store, &fixture).unwrap();
    assert_eq!(
        fixture
            .actions
            .borrow()
            .iter()
            .filter(|a| a.get("launch").is_some())
            .count(),
        1
    );
    // Older profiles must also leave excluded terminal identities alone on load.
    let mut saved: Value =
        serde_json::from_str(&call(&["backup", "show"], &store, &fixture).unwrap()).unwrap();
    let mut acc = saved["windows"][0].clone();
    acc["id"] = json!("acc-legacy");
    acc["class"] = json!("kitty-acc-agent-codex");
    let mut plain = window("plain-legacy", "kitty", 0);
    plain["tmux"] = json!([]);
    plain["launch"] = json!(["kitty"]);
    saved["windows"]
        .as_array_mut()
        .unwrap()
        .extend([acc.clone(), plain.clone()]);
    fixture.state.borrow_mut()["windows"]
        .as_array_mut()
        .unwrap()
        .extend([acc, plain]);
    let file = fixture.root.path().join("legacy.json");
    std::fs::write(&file, saved.to_string()).unwrap();
    call(
        &["edit", "backup", "--file", file.to_str().unwrap()],
        &store,
        &fixture,
    )
    .unwrap();
    fixture.actions.borrow_mut().clear();
    call(&["backup", "load"], &store, &fixture).unwrap();
    assert!(!fixture
        .actions
        .borrow()
        .iter()
        .any(|a| a.get("launch").is_some()
            || (a["op"] == "place" && (a["id"] == "acc-legacy" || a["id"] == "plain-legacy"))));
    eprintln!("None");
}

/// Discover a client using a named socket in its own TMUX_TMPDIR with preceding options.
/// Parameters: none.
/// Returns: None.
#[test]
fn discovers_client_socket_label() {
    eprintln!("parameters=none");
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};
    let mut fixture = Fixture::new();
    fixture.tmux(&["kill-server"]);
    fixture.standalone();
    let uid = std::fs::metadata("/proc/self").unwrap().uid();
    let dir = fixture.root.path().join(format!("tmux-{uid}"));
    std::fs::create_dir(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    fixture.socket = dir.join("task-test").to_string_lossy().into_owned();
    fixture.tmux(&["new-session", "-d", "-s", "backups"]);
    let client = Command::new("script")
        .args([
            "-q",
            "-c",
            "tmux -f /dev/null -L task-test attach-session -t '=backups'",
            "/dev/null",
        ])
        .env("TMUX_TMPDIR", fixture.root.path())
        .env("TERM", "xterm-256color")
        .env_remove("TMUX")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let pid = client.id();
    fixture.clients.borrow_mut().push(client);
    let start = Instant::now();
    while fixture
        .tmux(&["list-clients", "-F", "#{client_pid}"])
        .trim()
        .is_empty()
    {
        assert!(start.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(20));
    }
    fixture.state.borrow_mut()["windows"] = json!([window("labelled", "kitty", pid)]);
    let store = Store {
        root: fixture.root.path().join("profiles"),
    };
    call(
        &["configure", "backup", "--workspace", "3"],
        &store,
        &fixture,
    )
    .unwrap();
    let saved: Value =
        serde_json::from_str(&call(&["backup", "show"], &store, &fixture).unwrap()).unwrap();
    assert_eq!(saved["windows"].as_array().unwrap().len(), 1);
    assert_eq!(saved["windows"][0]["tmux"][0]["socket"], fixture.socket);
    eprintln!("None");
}

/// Keep a managed terminal stationary during watcher ticks while retaining explicit restoration.
/// Parameters: none.
/// Returns: None; checks geometry, restored identities, workspace binding and missing-window recovery.
#[test]
fn watcher_preserves_existing_terminal_geometry() {
    eprintln!("parameters=none");
    let fixture = Fixture::new();
    let store = Store {
        root: fixture.root.path().join("profiles"),
    };
    let pid = fixture.clients.borrow()[0].id();
    let saved_rect = json!([3840, 3440, 2160, 1896]);
    let live_rect = json!([3840, 1544, 2160, 1264]);
    let mut terminal = window("task", "kitty", pid);
    terminal["workspace"] = json!(1);
    terminal["monitor"] = json!("HDMI-0");
    terminal["rect"] = saved_rect.clone();
    fixture.state.borrow_mut()["active"] = json!(1);
    fixture.state.borrow_mut()["monitors"] = json!([
        {"connector":"DP-0","index":0,"x":0,"y":0,"width":3840,"height":2160},
        {"connector":"DP-4","index":1,"x":0,"y":2160,"width":3840,"height":2160},
        {"connector":"HDMI-0","index":2,"x":3840,"y":1544,"width":2160,"height":3840}
    ]);
    fixture.state.borrow_mut()["windows"] = json!([terminal]);
    call(
        &["configure", "backup", "--workspace", "2"],
        &store,
        &fixture,
    )
    .unwrap();
    let panes = fixture.tmux(&["list-panes", "-t", "=backups", "-F", "#{pane_pid}"]);

    // Simulate normal use after login, including a new window identity after reboot.
    for identity in ["task", "after-reboot"] {
        fixture.state.borrow_mut()["windows"][0]["id"] = json!(identity);
        fixture.state.borrow_mut()["windows"][0]["rect"] = live_rect.clone();
        fixture.state.borrow_mut()["windows"][0]["maximized"] = json!(3);
        for _ in 0..3 {
            fixture.actions.borrow_mut().clear();
            call(&["_startup"], &store, &fixture).unwrap();
            assert!(
                !fixture.actions.borrow().iter().any(|a| a["op"] == "place" || a["op"] == "focus" || a.get("launch").is_some()),
                "watcher must not move, resize, unmaximize or relaunch an existing terminal on its assigned workspace"
            );
            assert_eq!(fixture.state.borrow()["windows"][0]["rect"], live_rect);
            assert_eq!(fixture.state.borrow()["windows"][0]["maximized"], 3);
        }
    }
    let profile: Value =
        serde_json::from_str(&call(&["backup", "show"], &store, &fixture).unwrap()).unwrap();
    assert_eq!(profile["windows"][0]["rect"], saved_rect);
    assert_eq!(profile["windows"][0]["id"], "after-reboot");
    assert!(store
        .root
        .join(".continuity/backup/checkpoint.json")
        .exists());

    fixture.actions.borrow_mut().clear();
    call(&["backup", "load"], &store, &fixture).unwrap();
    assert!(fixture
        .actions
        .borrow()
        .iter()
        .any(|a| a["op"] == "place" && a["window"]["rect"] == saved_rect));
    assert!(fixture.actions.borrow().iter().any(|a| a["op"] == "focus"));

    // An attached client opened on another workspace still returns to its task.
    fixture.state.borrow_mut()["windows"][0]["workspace"] = json!(0);
    fixture.actions.borrow_mut().clear();
    call(&["_startup"], &store, &fixture).unwrap();
    assert_eq!(fixture.state.borrow()["windows"][0]["workspace"], 1);
    assert!(fixture
        .actions
        .borrow()
        .iter()
        .any(|a| a["op"] == "place" && a["window"]["rect"] == saved_rect));
    assert!(!fixture.actions.borrow().iter().any(|a| a["op"] == "focus"));

    // A missing GUI client is reopened and placed, preserving its live tmux job.
    fixture.state.borrow_mut()["windows"] = json!([]);
    fixture.actions.borrow_mut().clear();
    call(&["_startup"], &store, &fixture).unwrap();
    assert_eq!(fixture.state.borrow()["windows"][0]["workspace"], 1);
    assert_eq!(
        fixture
            .actions
            .borrow()
            .iter()
            .filter(|a| a.get("launch").is_some())
            .count(),
        1
    );
    assert!(fixture
        .actions
        .borrow()
        .iter()
        .any(|a| a["op"] == "place" && a["id"] == "restored"));
    assert_eq!(
        fixture.tmux(&["list-panes", "-t", "=backups", "-F", "#{pane_pid}"]),
        panes
    );
    fixture.actions.borrow_mut().clear();
    call(&["_startup"], &store, &fixture).unwrap();
    assert!(!fixture
        .actions
        .borrow()
        .iter()
        .any(|a| a["op"] == "place" || a.get("launch").is_some()));
    eprintln!("None");
}
