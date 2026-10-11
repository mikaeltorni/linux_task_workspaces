// Run: cargo test --locked --offline --test terminal_capture
// Real tmux clients use headless pseudo-terminals. No GUI or ACC service is used.
use linux_task_workspaces::{run, Bridge, Result, Store};
use serde_json::{json, Value};
use std::cell::RefCell;
use std::ffi::OsString;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Debug)]
pub(super) struct Fixture {
    pub(super) root: tempfile::TempDir,
    pub(super) socket: String,
    pub(super) clients: RefCell<Vec<Child>>,
    previous_tmux: Option<OsString>,
    previous_data_home: Option<OsString>,
    previous_home: Option<OsString>,
    previous_path: Option<OsString>,
    previous_monitor: Option<OsString>,
    _guard: std::sync::MutexGuard<'static, ()>,
    pub(super) state: RefCell<Value>,
    pub(super) actions: RefCell<Vec<Value>>,
}

static ENVIRONMENT: std::sync::Mutex<()> = std::sync::Mutex::new(());

impl Fixture {
    /// Start isolated sessions and attached clients for capture checks.
    /// Parameters: none.
    /// Returns: a fresh fixture containing real tmux clients and fake GNOME windows.
    pub(super) fn new() -> Self {
        eprintln!("parameters=none");
        let guard = ENVIRONMENT
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let root = tempfile::tempdir().unwrap();
        let socket = root.path().join("tmux").to_string_lossy().into_owned();
        let previous_tmux = std::env::var_os("TMUX");
        let previous_data_home = std::env::var_os("XDG_DATA_HOME");
        let fixture = Self {
            root,
            socket,
            clients: RefCell::new(vec![]),
            previous_tmux,
            previous_data_home,
            previous_home: std::env::var_os("HOME"),
            previous_path: std::env::var_os("PATH"),
            previous_monitor: std::env::var_os("AGENT_COMMAND_CENTER_TMUX_SESSION"),
            _guard: guard,
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
            fixture.clients.borrow_mut().push(client);
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

    /// Remove ACC installation surfaces and launcher environment for standalone checks.
    /// Parameters: self - isolated fixture containing the temporary home.
    /// Returns: None.
    pub(super) fn standalone(&self) {
        eprintln!("self={self:?}");
        let home = self.root.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        std::env::set_var("HOME", &home);
        std::env::set_var("PATH", "/usr/bin:/bin");
        std::env::remove_var("TMUX");
        std::env::remove_var("AGENT_COMMAND_CENTER_TMUX_SESSION");
        assert!(!home.join(".config/codex-agent-tracker").exists());
        assert!(!home.join(".local/bin/acc").exists());
        eprintln!("None");
    }

    /// Attach a real headless client to a session and wait until it is discoverable.
    /// Parameters: self - isolated tmux server; name - exact session name to attach.
    /// Returns: the client terminal ancestor PID.
    pub(super) fn attach(&self, name: &str) -> u32 {
        eprintln!("self={self:?}, name={name}");
        let before = self
            .tmux(&["list-clients", "-F", "#{client_pid}"])
            .lines()
            .count();
        let client = Command::new("script")
            .args([
                "-q",
                "-c",
                &format!("tmux -S '{}' attach-session -t '={name}'", self.socket),
                "/dev/null",
            ])
            .env_remove("TMUX")
            .env("TERM", "xterm-256color")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let result = client.id();
        self.clients.borrow_mut().push(client);
        let start = Instant::now();
        while self
            .tmux(&["list-clients", "-F", "#{client_pid}"])
            .lines()
            .count()
            <= before
        {
            assert!(start.elapsed() < Duration::from_secs(5));
            std::thread::sleep(Duration::from_millis(20));
        }
        eprintln!("{result}");
        result
    }

    /// Execute one command against this fixture's isolated tmux server.
    /// Parameters: self - isolated server fixture; args - tmux command operands.
    /// Returns: the command's UTF-8 standard output.
    pub(super) fn tmux(&self, args: &[&str]) -> String {
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
        for client in self.clients.borrow_mut().iter_mut() {
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
        for (key, previous) in [
            ("HOME", &self.previous_home),
            ("PATH", &self.previous_path),
            ("AGENT_COMMAND_CENTER_TMUX_SESSION", &self.previous_monitor),
        ] {
            if let Some(value) = previous {
                std::env::set_var(key, value);
            } else {
                std::env::remove_var(key);
            }
        }
        eprintln!("None");
    }
}

impl Bridge for Fixture {
    /// Record a shortcut registration without touching real GSettings.
    /// Parameters: self - fixture; id - task ID; role - shortcut purpose; chord - accelerator; command - launcher argv.
    /// Returns: a successful binding acknowledgement.
    fn bind(&self, id: &str, role: &str, chord: &str, command: &[String]) -> Result<()> {
        eprintln!("self={self:?}, id={id}, role={role}, chord={chord}, command={command:?}");
        self.actions
            .borrow_mut()
            .push(json!({"binding":chord,"id":id,"role":role,"command":command}));
        let result = Ok(());
        eprintln!("{result:?}");
        result
    }

    /// Simulate Kitty by opening only a headless attached tmux client.
    /// Parameters: self - isolated fixture; argv - requested terminal/tmux launch command.
    /// Returns: a successful simulated launch acknowledgement.
    fn launch(&self, argv: &[String]) -> Result<()> {
        eprintln!("self={self:?}, argv={argv:?}");
        let name = argv.windows(2).find(|a| a[0] == "-t").unwrap()[1].trim_start_matches('=');
        let pid = self.attach(name);
        self.actions.borrow_mut().push(json!({"launch":argv}));
        let mut w = window("restored", "kitty", pid);
        w["workspace"] = json!(0);
        self.state.borrow_mut()["windows"]
            .as_array_mut()
            .unwrap()
            .push(w);
        let result = Ok(());
        eprintln!("{result:?}");
        result
    }

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
        if action["op"] == "place" {
            for w in self.state.borrow_mut()["windows"].as_array_mut().unwrap() {
                if w["id"] == action["id"] {
                    w["workspace"] = action["workspace"].clone();
                }
            }
        }
        self.actions.borrow_mut().push(action);
        let result = Ok(json!({"ok":true}));
        eprintln!("{result:?}");
        result
    }
}

/// Build a realistic saved-window candidate with stable monitor geometry.
/// Parameters: id - stable window identity; class - application class; pid - terminal ancestor PID.
/// Returns: a GNOME window snapshot on workspace 3.
pub(super) fn window(id: &str, class: &str, pid: u32) -> Value {
    eprintln!("id={id}, class={class}, pid={pid}");
    let result = json!({"id":id,"class":class,"app":format!("{class}.desktop"),
        "pid":pid,"title":id,"workspace":2,"monitor":"DP-1","rect":[0,0,960,1080]});
    eprintln!("{result}");
    result
}

/// Exercise the saved public CLI dispatcher with this isolated fixture.
/// Parameters: args - CLI tokens; store - temporary profile store; fixture - fake GNOME/real tmux bridge.
/// Returns: the public command output or rejection.
pub(super) fn call(args: &[&str], store: &Store, fixture: &Fixture) -> Result<String> {
    eprintln!("args={args:?}, store={:?}, fixture={fixture:?}", store.root);
    let result = run(
        &args.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        store,
        fixture,
    );
    eprintln!("{result:?}");
    result
}
