// Run: cargo test --locked
// Coverage: create/list/show, same-app top-monitor geometry -> snapshot_contract;
// duplicate/invalid ID and malformed dispatch preserve saved state -> rejections;
// installed binary executes with isolated home -> installed_entrypoint;
// actual tmux session/window/pane/cwd snapshot, restore and live-job reuse -> tmux_contract.
// Configuration hotkeys and two-window restoration -> hotkey_configuration;
// malformed/ordinary key input leaves profile unchanged -> hotkey_configuration.
// Permanent extra slot, mode/close shortcuts, hide ownership, startup restoration
// without focus and invalid mode/shape preserve profile -> permanent_modes.
// GNOME conflicts, modifier/case aliases, own-binding reconfiguration, other
// task open/close collisions and unchanged profile -> conflict_contract;
// profile import and invalid shape/slot leave saved profile intact -> edit_contract.
// Real GSettings registration/readback, own path exclusion and collision
// unchanged-state -> gsettings_contract (isolated shell runner).
// Per-window file-manager folder capture and explicit argv including spaces ->
// folder_contract; invalid argv/index preserves profile -> folder_contract.
// Save replaces task-only normal windows, retains settings/folders and browser
// profile/URL per window, preserves prior snapshot on capture ambiguity -> save_contract.
// Public load reuses two windows, opens two missing same-app windows separately,
// repeated load launches no duplicates, missing/extra args preserve profile,
// concurrent command is rejected without mutation -> load_contract / launch_contract.
use linux_task_workspaces::{run, Bridge, Result, Store};
use serde_json::{json, Value};
use std::cell::RefCell;
use std::process::Command;

struct Desktop {
    actions: RefCell<Vec<Value>>,
    count: i64,
}
impl Desktop {
    fn new() -> Self {
        Self {
            actions: RefCell::new(vec![]),
            count: 8,
        }
    }
}
impl Bridge for Desktop {
    fn conflicts(&self, chord: &str, _id: &str, _role: &str) -> Result<Vec<String>> {
        Ok(if chord == "<Super>w" {
            vec!["org.gnome.desktop.wm.keybindings switch-to-workspace-6".into()]
        } else {
            vec![]
        })
    }
    fn bind(&self, id: &str, role: &str, chord: &str, command: &[String]) -> Result<()> {
        self.actions
            .borrow_mut()
            .push(json!({"binding":chord,"id":id,"role":role,"command":command}));
        Ok(())
    }
    fn snapshot(&self) -> Result<Value> {
        Ok(json!({"active":2,"count":self.count,
        "monitors":[{"index":0,"connector":"DP-1","x":0,"y":-1080,"width":1920,"height":1080}],
        "windows":[{"id":"42","pid":0,"workspace":2,"app":"kitty.desktop","class":"kitty","title":"backups","monitor":"DP-1","rect":[0,-1080,960,1080]},
                   {"id":"43","pid":0,"workspace":2,"app":"kitty.desktop","class":"kitty","title":"other","monitor":"DP-1","rect":[960,-1080,960,1080]}]}))
    }
    fn action(&self, action: Value) -> Result<Value> {
        self.actions.borrow_mut().push(action);
        Ok(json!({"ok":true}))
    }
}

// Coverage: GNOME's last supported slot succeeds; exhaustion leaves the earlier
// profile and public list unchanged, without sending a desktop action.
#[test]
fn workspace_limit() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store {
        root: temp.path().join("profiles"),
    };
    let mut desktop = Desktop::new();
    desktop.count = 35;
    call(&["create", "last"], &store, &desktop).unwrap();
    let expected = call(&["last", "show"], &store, &desktop).unwrap();
    assert!(expected.contains("35"));
    let list = call(&["list"], &store, &desktop).unwrap();
    let actions = desktop.actions.borrow().len();
    assert!(call(&["create", "overflow"], &store, &desktop).is_err());
    assert_eq!(call(&["last", "show"], &store, &desktop).unwrap(), expected);
    assert_eq!(call(&["list"], &store, &desktop).unwrap(), list);
    assert_eq!(desktop.actions.borrow().len(), actions);
}

// Coverage: rapidly forked subprocesses cannot retain a completed operation's
// flock. Repeated successful public mutations must retain the seeded profile.
#[test]
fn lock_release_during_process_launches() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store {
        root: temp.path().join("profiles"),
    };
    let desktop = Desktop::new();
    call(&["create", "backup"], &store, &desktop).unwrap();
    std::thread::scope(|scope| {
        scope.spawn(|| {
            for _ in 0..100 {
                assert!(Command::new("true").status().unwrap().success());
            }
        });
        for _ in 0..500 {
            call(
                &["configure", "backup", "--mode", "on-demand"],
                &store,
                &desktop,
            )
            .unwrap();
            let p: Value =
                serde_json::from_str(&call(&["backup", "show"], &store, &desktop).unwrap())
                    .unwrap();
            assert_eq!(p["mode"], "on-demand");
            assert_eq!(p["windows"].as_array().unwrap().len(), 2);
        }
    });
}
fn call(args: &[&str], store: &Store, desktop: &Desktop) -> Result<String> {
    run(
        &args.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        store,
        desktop,
    )
}

#[test]
fn snapshot_contract() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store {
        root: temp.path().join("profiles"),
    };
    let desktop = Desktop::new();
    call(&["create", "backups"], &store, &desktop).unwrap();
    let p: Value =
        serde_json::from_str(&call(&["backups", "show"], &store, &desktop).unwrap()).unwrap();
    assert_eq!(p["workspace"], 8);
    assert_eq!(p["windows"][0]["rect"], json!([0, -1080, 960, 1080]));
    assert_eq!(p["windows"][1]["rect"], json!([960, -1080, 960, 1080]));
    assert!(call(&["list"], &store, &desktop)
        .unwrap()
        .contains("backups"));
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(
        std::fs::metadata(store.path("backups").unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
}

#[test]
fn rejections() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store {
        root: temp.path().join("profiles"),
    };
    let desktop = Desktop::new();
    call(&["create", "backups"], &store, &desktop).unwrap();
    let expected = call(&["backups", "show"], &store, &desktop).unwrap();
    for args in [
        vec!["create", "backups"],
        vec!["create", "../bad"],
        vec!["missing", "show"],
        vec!["unknown"],
        vec!["list", "extra"],
        vec!["create"],
        vec!["a", "save", "extra"],
        vec!["a", "load", "extra"],
    ] {
        assert!(call(&args, &store, &desktop).is_err());
        assert_eq!(
            call(&["backups", "show"], &store, &desktop).unwrap(),
            expected
        );
    }
    assert!(call(&[], &store, &desktop).unwrap().contains("tws"));
}

#[test]
fn installed_entrypoint() {
    assert!(Command::new("bash")
        .arg("tests/installer_contract.sh")
        .arg(env!("CARGO_BIN_EXE_tws"))
        .status()
        .unwrap()
        .success());
    let temp = tempfile::tempdir().unwrap();
    let framework = temp.path().join("framework");
    std::fs::create_dir(&framework).unwrap();
    std::fs::write(
        framework.join("component_loader.sh"),
        "isc_load_helpers() { :; }\nrun_as_target() { \"$@\"; }\nis_root() { return 1; }\n",
    )
    .unwrap();
    let p = Command::new("bash")
        .arg("install.sh")
        .env("TWS_INSTALL_HOME", temp.path())
        .env("TWS_INSTALL_OFFLINE", "1")
        .env("ISC_FUNCTIONS_DIR", &framework)
        .env("TWS_BINARY", env!("CARGO_BIN_EXE_tws"))
        .output()
        .unwrap();
    assert!(p.status.success(), "{}", String::from_utf8_lossy(&p.stderr));
    let p = Command::new(temp.path().join(".local/bin/tws"))
        .arg("list")
        .env("TWS_CONFIG_HOME", temp.path().join("profiles"))
        .output()
        .unwrap();
    assert!(p.status.success(), "{}", String::from_utf8_lossy(&p.stderr));
    assert!(temp
        .path()
        .join(".local/share/gnome-shell/extensions/task-workspaces@local/extension.js")
        .exists());
}

#[test]
fn tmux_contract() {
    struct TestServer(std::path::PathBuf);
    impl Drop for TestServer {
        fn drop(&mut self) {
            let _ = Command::new("tmux")
                .arg("-S")
                .arg(&self.0)
                .arg("kill-server")
                .output();
        }
    }
    let temp = tempfile::tempdir().unwrap();
    let socket = temp.path().join("tmux");
    let _server = TestServer(socket.clone());
    let s = socket.to_str().unwrap();
    let cmd = |args: &[&str]| {
        let p = Command::new("tmux")
            .args(["-S", s, "-f", "/dev/null"])
            .args(args)
            .output()
            .unwrap();
        assert!(p.status.success(), "{}", String::from_utf8_lossy(&p.stderr));
        String::from_utf8(p.stdout).unwrap()
    };
    cmd(&[
        "new-session",
        "-d",
        "-s",
        "backup",
        "-n",
        "zip",
        "-c",
        temp.path().to_str().unwrap(),
    ]);
    cmd(&[
        "split-window",
        "-d",
        "-t",
        "backup",
        "-c",
        temp.path().to_str().unwrap(),
    ]);
    // Session structure includes gaps in window numbering and the active window.
    cmd(&["move-window", "-s", "backup:0", "-t", "backup:3"]);
    cmd(&[
        "new-window",
        "-d",
        "-t",
        "backup:7",
        "-n",
        "upload",
        "-c",
        temp.path().to_str().unwrap(),
    ]);
    let snapshot = linux_task_workspaces::tmux::snapshot(s, "backup").unwrap();
    assert_eq!(snapshot["windows"][0]["panes"].as_array().unwrap().len(), 2);
    assert_eq!(
        snapshot["windows"][0]["panes"][0]["cwd"],
        temp.path().to_str().unwrap()
    );
    let before = cmd(&["list-panes", "-t", "backup", "-F", "#{pane_pid}"]);
    linux_task_workspaces::tmux::restore(&snapshot).unwrap();
    assert_eq!(
        cmd(&["list-panes", "-t", "backup", "-F", "#{pane_pid}"]),
        before
    );
    cmd(&["kill-session", "-t", "backup"]);
    linux_task_workspaces::tmux::restore(&snapshot).unwrap();
    let restored = linux_task_workspaces::tmux::snapshot(s, "backup").unwrap();
    assert_eq!(restored["windows"][0]["panes"].as_array().unwrap().len(), 2);
    assert_eq!(restored["windows"][0]["name"], "zip");
    assert_eq!(restored["windows"][0]["index"], 3);
    assert_eq!(restored["windows"][1]["index"], 7);
    assert_eq!(restored["windows"][1]["name"], "upload");
    assert_eq!(restored["windows"][0]["active"], true);
    cmd(&["kill-server"]); // only this isolated test socket
}

#[test]
fn hotkey_configuration() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store {
        root: temp.path().join("profiles"),
    };
    let desktop = Desktop::new();
    call(&["create", "backup"], &store, &desktop).unwrap();
    call(
        &["configure", "backup", "--open", "Super+B"],
        &store,
        &desktop,
    )
    .unwrap();
    let expected = call(&["backup", "show"], &store, &desktop).unwrap();
    for args in [
        vec!["configure", "backup", "--open", "b"],
        vec!["configure", "backup", "--open", "Ctrl+Super"],
        vec!["configure", "backup", "--open", "Super+A+B"],
        vec!["configure", "backup", "extra"],
    ] {
        assert!(call(&args, &store, &desktop).is_err());
        assert_eq!(
            call(&["backup", "show"], &store, &desktop).unwrap(),
            expected
        );
    }
    call(
        &["configure", "--launcher", "Alt+Control+Super+W"],
        &store,
        &desktop,
    )
    .unwrap();
    call(&["_activate", "backup"], &store, &desktop).unwrap();
    assert_eq!(
        desktop
            .actions
            .borrow()
            .iter()
            .filter(|a| a["op"] == "place")
            .count(),
        2
    );
    assert_eq!(desktop.actions.borrow().last().unwrap()["op"], "focus");
}

#[test]
fn permanent_modes() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store {
        root: temp.path().join("profiles"),
    };
    let desktop = Desktop::new();
    call(&["create", "backup"], &store, &desktop).unwrap();
    call(&["create", "second"], &store, &desktop).unwrap();
    assert_eq!(store.read("backup").unwrap()["workspace"], 8);
    assert_eq!(store.read("second").unwrap()["workspace"], 9);
    call(
        &["configure", "backup", "--mode", "always"],
        &store,
        &desktop,
    )
    .unwrap();
    call(
        &["configure", "backup", "--close", "Ctrl+Super+B"],
        &store,
        &desktop,
    )
    .unwrap();
    call(
        &["configure", "backup", "--close-action", "hide"],
        &store,
        &desktop,
    )
    .unwrap();
    let expected = call(&["backup", "show"], &store, &desktop).unwrap();
    for args in [
        vec!["configure", "backup", "--mode", "wrong"],
        vec!["configure", "backup", "--close-action", "wrong"],
        vec!["backup", "hide", "extra"],
        vec!["backup", "close", "extra"],
        vec!["_startup", "extra"],
    ] {
        assert!(call(&args, &store, &desktop).is_err());
        assert_eq!(
            call(&["backup", "show"], &store, &desktop).unwrap(),
            expected
        );
    }
    desktop.actions.borrow_mut().clear();
    call(&["_startup"], &store, &desktop).unwrap();
    assert_eq!(
        desktop
            .actions
            .borrow()
            .iter()
            .filter(|a| a["op"] == "place")
            .count(),
        2
    );
    assert!(!desktop.actions.borrow().iter().any(|a| a["op"] == "focus"));
    call(&["backup", "hide"], &store, &desktop).unwrap();
    assert_eq!(
        desktop.actions.borrow().last().unwrap()["ids"],
        json!(["42", "43"])
    );
    desktop.actions.borrow_mut().clear();
    call(&["_startup"], &store, &desktop).unwrap();
    assert!(!desktop.actions.borrow().iter().any(|a| a["op"] == "place"));
}

#[test]
fn conflict_contract() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store {
        root: temp.path().join("profiles"),
    };
    let desktop = Desktop::new();
    call(&["create", "backup"], &store, &desktop).unwrap();
    call(
        &["configure", "backup", "--open", "Super+B"],
        &store,
        &desktop,
    )
    .unwrap();
    call(
        &["configure", "backup", "--open", "<Super>b"],
        &store,
        &desktop,
    )
    .unwrap();
    let expected = call(&["backup", "show"], &store, &desktop).unwrap();
    for args in [
        vec!["configure", "backup", "--open", "Super+W"],
        vec!["configure", "backup", "--close", "Super+B"],
    ] {
        assert!(call(&args, &store, &desktop).is_err());
        assert_eq!(
            call(&["backup", "show"], &store, &desktop).unwrap(),
            expected
        );
    }
    call(&["create", "other"], &store, &desktop).unwrap();
    let other = call(&["other", "show"], &store, &desktop).unwrap();
    assert!(call(
        &["configure", "other", "--open", "Super+B"],
        &store,
        &desktop
    )
    .is_err());
    assert_eq!(call(&["other", "show"], &store, &desktop).unwrap(), other);
    assert!(call(&["conflicts", "Super+W"], &store, &desktop)
        .unwrap()
        .contains("switch-to-workspace-6"));
    use linux_task_workspaces::hotkeys::{accelerator, matches_output, schema};
    assert_eq!(
        accelerator("ctrl+super+shift+b").unwrap(),
        accelerator("<Shift><Super><Primary>B").unwrap()
    );
    let own = schema("backup", "open");
    assert!(matches_output("<Super>b", &format!("{own} binding '<Super>B'"), &own).is_empty());
    assert_eq!(
        matches_output("<Super>b", "extension hotkey ['<Super>B']", "").len(),
        1
    );
}

#[test]
fn edit_contract() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store {
        root: temp.path().join("profiles"),
    };
    let desktop = Desktop::new();
    call(&["create", "backup"], &store, &desktop).unwrap();
    let file = temp.path().join("edit.json");
    let mut p = store.read("backup").unwrap();
    p["mode"] = json!("always");
    p["open_hotkey"] = json!("Super+B");
    std::fs::write(&file, p.to_string()).unwrap();
    call(
        &["edit", "backup", "--file", file.to_str().unwrap()],
        &store,
        &desktop,
    )
    .unwrap();
    assert_eq!(store.read("backup").unwrap()["mode"], "always");
    let expected = call(&["backup", "show"], &store, &desktop).unwrap();
    for patch in [
        json!({"workspace":99}),
        json!({"mode":"bad"}),
        json!({"open_hotkey":"Super+W"}),
        json!({"windows":false}),
    ] {
        let mut candidate = store.read("backup").unwrap();
        for (k, v) in patch.as_object().unwrap() {
            candidate[k] = v.clone();
        }
        std::fs::write(&file, candidate.to_string()).unwrap();
        assert!(call(
            &["backup", "edit", "--file", file.to_str().unwrap()],
            &store,
            &desktop
        )
        .is_err());
        assert_eq!(
            call(&["backup", "show"], &store, &desktop).unwrap(),
            expected
        );
    }
}

#[test]
fn gsettings_contract() {
    let p = Command::new("bash")
        .arg("tests/gsettings_contract.sh")
        .arg(env!("CARGO_BIN_EXE_tws"))
        .output()
        .unwrap();
    assert!(
        p.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&p.stdout),
        String::from_utf8_lossy(&p.stderr)
    );
}

#[test]
fn folder_contract() {
    struct Files;
    impl Bridge for Files {
        fn snapshot(&self) -> Result<Value> {
            let mut s = Desktop::new().snapshot()?;
            s["windows"][0]["class"] = json!("org.gnome.Nautilus");
            s["windows"][0]["gtk_path"] = json!("/org/gnome/Nautilus/window/1");
            s["folder_locations"] =
                json!({"/org/gnome/Nautilus/window/1":["file:///home/test/Drive%20Backups"]});
            Ok(s)
        }
        fn action(&self, _: Value) -> Result<Value> {
            Ok(json!({"ok":true}))
        }
    }
    let temp = tempfile::tempdir().unwrap();
    let store = Store {
        root: temp.path().join("profiles"),
    };
    run(&["create".into(), "files".into()], &store, &Files).unwrap();
    let p: Value =
        serde_json::from_str(&run(&["files".into(), "show".into()], &store, &Files).unwrap())
            .unwrap();
    assert_eq!(
        p["windows"][0]["folders"],
        json!(["file:///home/test/Drive%20Backups"])
    );
    assert_eq!(
        p["windows"][0]["launch"],
        json!([
            "nautilus",
            "--new-window",
            "file:///home/test/Drive%20Backups"
        ])
    );
    let desktop = Desktop::new();
    call(
        &[
            "configure",
            "files",
            "--launch",
            "0",
            r#"["nautilus","--new-window","/home/test/My Backups"]"#,
        ],
        &store,
        &desktop,
    )
    .unwrap();
    assert_eq!(
        store.read("files").unwrap()["windows"][0]["launch"][2],
        "/home/test/My Backups"
    );
    let expected = call(&["files", "show"], &store, &desktop).unwrap();
    for (index, argv) in [
        ("999", "[]"),
        ("0", "[42]"),
        ("0", "[]"),
        ("0", "{}"),
        ("bad", "[]"),
    ] {
        assert!(call(
            &["configure", "files", "--launch", index, argv],
            &store,
            &desktop
        )
        .is_err());
        assert_eq!(
            call(&["files", "show"], &store, &desktop).unwrap(),
            expected
        );
    }
}

#[test]
fn save_contract() {
    struct SavedDesktop {
        state: RefCell<Value>,
    }
    impl Bridge for SavedDesktop {
        fn snapshot(&self) -> Result<Value> {
            Ok(self.state.borrow().clone())
        }
        fn action(&self, _: Value) -> Result<Value> {
            Ok(json!({"ok":true}))
        }
    }
    let temp = tempfile::tempdir().unwrap();
    let store = Store {
        root: temp.path().join("profiles"),
    };
    let desktop = Desktop::new();
    call(&["create", "backup"], &store, &desktop).unwrap();
    call(
        &["configure", "backup", "--open", "Super+B"],
        &store,
        &desktop,
    )
    .unwrap();
    let mut state = desktop.snapshot().unwrap();
    state["windows"][0]["workspace"] = json!(8);
    state["windows"][0]["class"] = json!("Google-chrome");
    state["windows"][0]["title"] = json!("Drive - Google Chrome");
    state["windows"][0]["app"] = json!("google-chrome.desktop");
    state["chrome_sessions"] = json!([{"session_id":7,"program":"google-chrome","profile":"Profile 1","user_data_dir":"/home/test/.config/google-chrome",
        "rect":[0,-1080,960,1080],"urls":["https://drive.google.com/drive/u/0/"],"titles":["Drive"],"active_title":"Drive"}]);
    let saved = SavedDesktop {
        state: RefCell::new(state),
    };
    run(&["backup".into(), "save".into()], &store, &saved).unwrap();
    let p: Value =
        serde_json::from_str(&run(&["backup".into(), "show".into()], &store, &saved).unwrap())
            .unwrap();
    assert_eq!(p["windows"].as_array().unwrap().len(), 1);
    assert_eq!(p["open_hotkey"], "<Super>b");
    assert_eq!(
        p["windows"][0]["launch"],
        json!([
            "google-chrome",
            "--new-window",
            "--user-data-dir=/home/test/.config/google-chrome",
            "--profile-directory=Profile 1",
            "https://drive.google.com/drive/u/0/"
        ])
    );
    let expected = run(&["backup".into(), "show".into()], &store, &saved).unwrap();
    saved.state.borrow_mut()["chrome_sessions"] = json!([]);
    assert!(run(&["backup".into(), "save".into()], &store, &saved).is_err());
    assert_eq!(
        run(&["backup".into(), "show".into()], &store, &saved).unwrap(),
        expected
    );
    saved.state.borrow_mut()["chrome_sessions"] = json!([
        p["windows"][0]["browser"].clone(),
        p["windows"][0]["browser"].clone()
    ]);
    assert!(run(&["backup".into(), "save".into()], &store, &saved).is_err());
    assert_eq!(
        run(&["backup".into(), "show".into()], &store, &saved).unwrap(),
        expected
    );
    call(
        &[
            "configure",
            "backup",
            "--launch",
            "0",
            r#"["google-chrome","--new-window","https://drive.google.com/"]"#,
        ],
        &store,
        &desktop,
    )
    .unwrap();
    run(&["backup".into(), "save".into()], &store, &saved).unwrap();
    assert_eq!(
        store.read("backup").unwrap()["windows"][0]["launch"][2],
        "https://drive.google.com/"
    );
}

#[test]
fn load_contract() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store {
        root: temp.path().join("profiles"),
    };
    let desktop = Desktop::new();
    call(&["create", "backup"], &store, &desktop).unwrap();
    call(&["backup", "load"], &store, &desktop).unwrap();
    call(&["backup", "load"], &store, &desktop).unwrap();
    assert_eq!(
        desktop
            .actions
            .borrow()
            .iter()
            .filter(|a| a["op"] == "place")
            .count(),
        4
    );
    let expected = call(&["backup", "show"], &store, &desktop).unwrap();
    for args in [vec!["backup", "load", "extra"], vec!["missing", "load"]] {
        assert!(call(&args, &store, &desktop).is_err());
        assert_eq!(
            call(&["backup", "show"], &store, &desktop).unwrap(),
            expected
        );
    }
    let lock = store.lock().unwrap();
    assert!(call(&["backup", "load"], &store, &desktop).is_err());
    assert_eq!(
        call(&["backup", "show"], &store, &desktop).unwrap(),
        expected
    );
    drop(lock);
    let p = Command::new(env!("CARGO_BIN_EXE_tws"))
        .env("TWS_CONFIG_HOME", &store.root)
        .env("TWS_LOAD_ID", "missing")
        .env_remove("TWS_EDIT_ID")
        .output()
        .unwrap();
    assert!(!p.status.success());
    assert!(String::from_utf8_lossy(&p.stderr).contains("Cannot read workspace missing"));
    assert_eq!(
        call(&["backup", "show"], &store, &desktop).unwrap(),
        expected
    );
}

#[test]
fn launch_contract() {
    struct Launcher {
        state: RefCell<Value>,
        launches: RefCell<Vec<Vec<String>>>,
    }
    impl Bridge for Launcher {
        fn snapshot(&self) -> Result<Value> {
            Ok(self.state.borrow().clone())
        }
        fn action(&self, a: Value) -> Result<Value> {
            if a["op"] == "place" {
                for w in self.state.borrow_mut()["windows"].as_array_mut().unwrap() {
                    if w["id"] == a["id"] {
                        w["workspace"] = a["workspace"].clone();
                    }
                }
            }
            Ok(json!({"ok":true}))
        }
        fn launch(&self, argv: &[String]) -> Result<()> {
            self.launches.borrow_mut().push(argv.to_vec());
            let n = self.launches.borrow().len();
            let mut w = Desktop::new().snapshot()?["windows"][n - 1].clone();
            w["id"] = json!(format!("new{n}"));
            w["pid"] = json!(100 + n);
            self.state.borrow_mut()["windows"]
                .as_array_mut()
                .unwrap()
                .push(w);
            Ok(())
        }
    }
    let temp = tempfile::tempdir().unwrap();
    let store = Store {
        root: temp.path().join("profiles"),
    };
    let desktop = Desktop::new();
    call(&["create", "backup"], &store, &desktop).unwrap();
    let mut state = desktop.snapshot().unwrap();
    state["windows"] = json!([]);
    let launcher = Launcher {
        state: RefCell::new(state),
        launches: RefCell::new(vec![]),
    };
    run(&["backup".into(), "load".into()], &store, &launcher).unwrap();
    assert_eq!(launcher.launches.borrow().len(), 2);
    assert_eq!(store.read("backup").unwrap()["windows"][0]["id"], "new1");
    run(&["backup".into(), "load".into()], &store, &launcher).unwrap();
    assert_eq!(launcher.launches.borrow().len(), 2);
}
