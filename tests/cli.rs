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
use linux_task_workspaces::{run, Bridge, Result, Store};
use serde_json::{json, Value};
use std::cell::RefCell;
use std::process::Command;

struct Desktop {
    actions: RefCell<Vec<Value>>,
}
impl Desktop {
    fn new() -> Self {
        Self {
            actions: RefCell::new(vec![]),
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
        Ok(json!({"active":2,"count":8,
        "monitors":[{"index":0,"connector":"DP-1","x":0,"y":-1080,"width":1920,"height":1080}],
        "windows":[{"id":"42","pid":0,"workspace":2,"app":"kitty.desktop","class":"kitty","title":"backups","monitor":"DP-1","rect":[0,-1080,960,1080]},
                   {"id":"43","pid":0,"workspace":2,"app":"kitty.desktop","class":"kitty","title":"other","monitor":"DP-1","rect":[960,-1080,960,1080]}]}))
    }
    fn action(&self, action: Value) -> Result<Value> {
        self.actions.borrow_mut().push(action);
        Ok(json!({"ok":true}))
    }
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
        vec!["a", "save"],
        vec!["a", "load"],
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
    let temp = tempfile::tempdir().unwrap();
    let socket = temp.path().join("tmux");
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
