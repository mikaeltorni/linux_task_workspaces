// Run: cargo test --locked --offline --test terminal_capture
// Real tmux clients use headless pseudo-terminals. No GUI or ACC service is used.
use linux_task_workspaces::{run, Bridge, Result, Store};
use serde_json::{json, Value};
use std::cell::RefCell;
use std::ffi::OsString;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Debug)]
struct Fixture {
    root: tempfile::TempDir,
    socket: String,
    clients: Vec<Child>,
    previous_tmux: Option<OsString>,
    previous_data_home: Option<OsString>,
    state: RefCell<Value>,
    actions: RefCell<Vec<Value>>,
}

impl Fixture {
    /// Start isolated sessions and attached clients for capture checks.
    /// Parameters: none.
    /// Returns: a fresh fixture containing real tmux clients and fake GNOME windows.
    fn new() -> Self {
        eprintln!("parameters=none");
        let root = tempfile::tempdir().unwrap();
        let socket = root.path().join("tmux").to_string_lossy().into_owned();
        let previous_tmux = std::env::var_os("TMUX");
        let previous_data_home = std::env::var_os("XDG_DATA_HOME");
        let mut fixture = Self {
            root,
            socket,
            clients: vec![],
            previous_tmux,
            previous_data_home,
            state: RefCell::new(json!({"active":2,"count":8,"windows":[],
                "monitors":[{"connector":"DP-1","width":1920,"height":1080,"x":0,"y":0}]})),
            actions: RefCell::new(vec![]),
        };
        for name in ["backups", "acc-ca-test", "agent_command_center"] {
            fixture.tmux(&["new-session", "-d", "-s", name]);
            let client = Command::new("script")
                .args([
                    "-q",
                    "-c",
                    &format!("tmux -S '{}' attach-session -t '={name}'", fixture.socket),
                    "/dev/null",
                ])
                .env_remove("TMUX")
                .env("TERM", "xterm-256color")
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            fixture.clients.push(client);
        }
        let start = Instant::now();
        while fixture
            .tmux(&["list-clients", "-F", "#{session_name}"])
            .lines()
            .count()
            != 3
        {
            assert!(
                start.elapsed() < Duration::from_secs(5),
                "tmux clients did not attach"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        // This existing discovery source is scoped to this test executable.
        std::env::set_var("TMUX", format!("{},0,0", fixture.socket));
        let data = fixture.root.path().join("share/applications");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(
            data.join("company-shell.desktop"),
            "[Desktop Entry]\nType=Application\nCategories=System;TerminalEmulator;\n",
        )
        .unwrap();
        std::env::set_var("XDG_DATA_HOME", fixture.root.path().join("share"));
        eprintln!("{fixture:?}");
        fixture
    }

    /// Execute one command against this fixture's isolated tmux server.
    /// Parameters: self - isolated server fixture; args - tmux command operands.
    /// Returns: the command's UTF-8 standard output.
    fn tmux(&self, args: &[&str]) -> String {
        eprintln!("self={self:?}, args={args:?}");
        let output = Command::new("tmux")
            .args(["-S", &self.socket, "-f", "/dev/null"])
            .args(args)
            .env_remove("TMUX")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let result = String::from_utf8(output.stdout).unwrap();
        eprintln!("{result:?}");
        result
    }
}

impl Drop for Fixture {
    /// Close only the fixture's clients and isolated server, restoring its environment.
    /// Parameters: self - isolated fixture being released.
    /// Returns: None.
    fn drop(&mut self) {
        eprintln!("self={self:?}");
        let _ = Command::new("tmux")
            .args(["-S", &self.socket, "kill-server"])
            .output();
        for client in &mut self.clients {
            let _ = client.kill();
            let _ = client.wait();
        }
        if let Some(value) = &self.previous_tmux {
            std::env::set_var("TMUX", value);
        } else {
            std::env::remove_var("TMUX");
        }
        if let Some(value) = &self.previous_data_home {
            std::env::set_var("XDG_DATA_HOME", value);
        } else {
            std::env::remove_var("XDG_DATA_HOME");
        }
        eprintln!("None");
    }
}

impl Bridge for Fixture {
    /// Return the current simulated desktop without opening any windows.
    /// Parameters: self - fixture containing GNOME snapshot data.
    /// Returns: the desktop snapshot.
    fn snapshot(&self) -> Result<Value> {
        eprintln!("self={self:?}");
        let result = Ok(self.state.borrow().clone());
        eprintln!("{result:?}");
        result
    }

    /// Record GNOME mutations for public-state and placement checks.
    /// Parameters: self - fixture recording actions; action - requested GNOME operation.
    /// Returns: a successful bridge acknowledgement.
    fn action(&self, action: Value) -> Result<Value> {
        eprintln!("self={self:?}, action={action}");
        self.actions.borrow_mut().push(action);
        let result = Ok(json!({"ok":true}));
        eprintln!("{result:?}");
        result
    }
}

/// Build a realistic saved-window candidate with stable monitor geometry.
/// Parameters: id - stable window identity; class - application class; pid - terminal ancestor PID.
/// Returns: a GNOME window snapshot on workspace 3.
fn window(id: &str, class: &str, pid: u32) -> Value {
    eprintln!("id={id}, class={class}, pid={pid}");
    let result = json!({"id":id,"class":class,"app":format!("{class}.desktop"),
        "pid":pid,"title":id,"workspace":2,"monitor":"DP-1","rect":[0,0,960,1080]});
    eprintln!("{result}");
    result
}

/// Exercise the saved public CLI dispatcher with this isolated fixture.
/// Parameters: args - CLI tokens; store - temporary profile store; fixture - fake GNOME/real tmux bridge.
/// Returns: the public command output or rejection.
fn call(args: &[&str], store: &Store, fixture: &Fixture) -> Result<String> {
    eprintln!("args={args:?}, store={:?}, fixture={fixture:?}", store.root);
    let result = run(
        &args.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        store,
        fixture,
    );
    eprintln!("{result:?}");
    result
}

/// Check capture/save excludes ACC and plain terminals while retaining task tmux and GUI apps.
/// Parameters: none.
/// Returns: None.
#[test]
fn capture_exclusions() {
    eprintln!("parameters=none");
    let fixture = Fixture::new();
    let store = Store {
        root: fixture.root.path().join("profiles"),
    };
    let task_pid = fixture.clients[0].id();
    let mut windows = vec![
        window("task", "kitty", task_pid),
        window("other-terminal", "org.gnome.Terminal", task_pid),
        window("acc-class", "kitty-acc-agent-codex", task_pid),
        window("acc-session", "kitty", fixture.clients[1].id()),
        window("acc-monitor", "kitty", fixture.clients[2].id()),
        window("chrome", "google-chrome", 0),
        window("files", "org.gnome.Nautilus", 0),
    ];
    windows[6]["gtk_path"] = json!("/org/gnome/Nautilus/window/1");
    windows.push(window("plain-company-shell", "company-shell", 0));
    for class in [
        "kitty",
        "org.gnome.Terminal",
        "Gnome-terminal",
        "gnome-terminal-server",
        "org.gnome.Console",
        "Alacritty",
        "org.kde.konsole",
        "foot",
        "org.wezfurlong.wezterm",
        "com.mitchellh.ghostty",
        "XTerm",
        "tilix",
        "terminator",
        "xfce4-terminal",
    ] {
        windows.push(window(&format!("plain-{class}"), class, 0));
    }
    let mut sticky = window("sticky", "google-chrome", 0);
    sticky["sticky"] = json!(true);
    windows.push(sticky);
    let mut elsewhere = window("elsewhere", "google-chrome", 0);
    elsewhere["workspace"] = json!(1);
    windows.push(elsewhere);
    fixture.state.borrow_mut()["windows"] = json!(windows);
    fixture.state.borrow_mut()["folder_locations"] =
        json!({"/org/gnome/Nautilus/window/1":["file:///tmp/My%20Backups"]});
    fixture.state.borrow_mut()["chrome_sessions"] = json!([
        {"active_title":"chrome","titles":["chrome"],"rect":[0,0,960,1080],
         "urls":["https://drive.example.test/backups"],"program":"google-chrome","profile":"Default","user_data_dir":"/tmp/browser-fixture"},
        {"active_title":"elsewhere","titles":["elsewhere"],"rect":[0,0,960,1080],
         "urls":["https://files.example.test/"],"program":"google-chrome","profile":"Default","user_data_dir":"/tmp/browser-fixture"}
    ]);
    call(&["create", "backup"], &store, &fixture).unwrap();
    let profile: Value =
        serde_json::from_str(&call(&["backup", "show"], &store, &fixture).unwrap()).unwrap();
    let ids: Vec<_> = profile["windows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["task", "other-terminal", "chrome", "files"]);
    assert_eq!(profile["windows"][0]["tmux"][0]["name"], "backups");
    assert_eq!(profile["windows"][0]["launch"][3], fixture.socket);
    assert_eq!(
        profile["windows"][3]["folders"],
        json!(["file:///tmp/My%20Backups"])
    );
    assert_eq!(profile["windows"][0]["rect"], json!([0, 0, 960, 1080]));
    for w in fixture.state.borrow_mut()["windows"]
        .as_array_mut()
        .unwrap()
    {
        w["workspace"] = json!(8);
    }
    call(&["backup", "save"], &store, &fixture).unwrap();
    let expected = call(&["backup", "show"], &store, &fixture).unwrap();
    let profile: Value = serde_json::from_str(&expected).unwrap();
    assert_eq!(profile["windows"].as_array().unwrap().len(), 5); // former elsewhere now belongs to this slot
    assert_eq!(
        profile["windows"][2]["launch"][4],
        "https://drive.example.test/backups"
    );
    assert!(profile["windows"]
        .as_array()
        .unwrap()
        .iter()
        .all(|w| !w["id"].as_str().unwrap().starts_with("acc-")
            && !w["id"].as_str().unwrap().starts_with("plain-")));
    for args in [
        vec!["backup", "save", "extra"],
        vec!["configure", "backup", "--workspace", "bad"],
        vec!["unknown"],
    ] {
        assert!(call(&args, &store, &fixture).is_err());
        assert_eq!(
            call(&["backup", "show"], &store, &fixture).unwrap(),
            expected
        );
    }
    fixture.state.borrow_mut()["windows"] = json!(null);
    assert!(call(&["backup", "save"], &store, &fixture).is_err());
    assert_eq!(
        call(&["backup", "show"], &store, &fixture).unwrap(),
        expected
    );
    eprintln!("None");
}
