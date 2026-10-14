use linux_task_workspaces::{run, Bridge, Result, Store};
use serde_json::{json, Value};
use std::cell::{Cell, RefCell};
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug)]
struct PrivateBus {
    child: Child,
    address: String,
}

impl PrivateBus {
    /// Start an isolated D-Bus daemon for fake session-manager properties.
    /// Parameters: none.
    /// Returns: the owned private daemon and its session address.
    fn start() -> Self {
        eprintln!("parameters=none");
        let mut child = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut address = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        let result = Self {
            child,
            address: address.trim().into(),
        };
        eprintln!("{result:?}");
        result
    }
}

impl Drop for PrivateBus {
    /// Reap only this fixture's private daemon.
    /// Parameters: self - owned test bus.
    /// Returns: None.
    fn drop(&mut self) {
        eprintln!("self={self:p}");
        let _ = self.child.kill();
        let _ = self.child.wait();
        eprintln!("None");
    }
}

struct EnvironmentManager {
    calls: AtomicUsize,
    ready_after: usize,
    values: Vec<String>,
}

#[zbus::interface(name = "org.freedesktop.systemd1.Manager")]
impl EnvironmentManager {
    /// Supply display values immediately or after an early launch attempt.
    /// Parameters: self - private manager with its simulated login-readiness delay.
    /// Returns: fresh manager environment entries, or an empty pre-login environment.
    #[zbus(property)]
    fn environment(&self) -> Vec<String> {
        eprintln!("self={self:p}");
        let result = if self.calls.fetch_add(1, Ordering::SeqCst) >= self.ready_after {
            self.values.clone()
        } else {
            vec![]
        };
        eprintln!("{result:?}");
        result
    }
}

struct ProbeDesktop {
    marker: PathBuf,
    seeding: Cell<bool>,
    actions: RefCell<Vec<Value>>,
}

impl Bridge for ProbeDesktop {
    /// Expose a seed window or a newly launched headless application probe.
    /// Parameters: self - desktop state and application marker.
    /// Returns: injected GNOME metadata; no actual desktop windows are queried.
    fn snapshot(&self) -> Result<Value> {
        eprintln!("self={self:p}");
        let windows = if self.seeding.get() || self.marker.exists() {
            json!([{"id":if self.marker.exists() {"new"} else {"seed"},"pid":0,
                "workspace":1,"class":"environment-probe","app":"environment-probe.desktop",
                "title":"boot probe","monitor":"DP-test","rect":[0,0,800,600]}])
        } else {
            json!([])
        };
        let result = Ok(json!({"active":1,"count":2,"windows":windows,
            "monitors":[{"index":0,"connector":"DP-test","x":0,"y":0,"width":1920,"height":1080}]}));
        eprintln!("{result:?}");
        result
    }

    /// Record destination placement without opening or moving a GUI.
    /// Parameters: self - fake desktop; action - requested GNOME operation.
    /// Returns: successful simulated action.
    fn action(&self, action: Value) -> Result<Value> {
        eprintln!("self={self:p}, action={action}");
        self.actions.borrow_mut().push(action);
        let result = Ok(json!({"ok":true}));
        eprintln!("{result:?}");
        result
    }

    /// Keep profile configuration isolated from real GSettings.
    /// Parameters: self - fake desktop; id - task; role - binding role; chord - requested key; command - handler argv.
    /// Returns: successful simulated registration.
    fn bind(&self, id: &str, role: &str, chord: &str, command: &[String]) -> Result<()> {
        eprintln!("self={self:p}, id={id}, role={role}, chord={chord}, command={command:?}");
        eprintln!("Ok(())");
        Ok(())
    }
}

/// Run the actual public startup route with a cold process environment.
/// Parameters: none.
/// Returns: None; verifies probe environment and unchanged state after invalid startup shape.
#[test]
fn boot_probe_child() {
    eprintln!("parameters=none");
    if let Ok(root) = std::env::var("TWS_BOOT_PROBE_ROOT") {
        let root = PathBuf::from(root);
        let marker = root.join("application-environment");
        let store = Store {
            root: root.join("profiles"),
        };
        let desktop = ProbeDesktop {
            marker: marker.clone(),
            seeding: Cell::new(true),
            actions: RefCell::new(vec![]),
        };
        let args = vec![
            "configure".into(),
            "boot".into(),
            "--workspace".into(),
            "2".into(),
        ];
        run(&args, &store, &desktop).unwrap();
        let argv = json!(["/bin/sh", "-c", "printf '%s\\n' \"$DISPLAY\" \"$WAYLAND_DISPLAY\" \"$XAUTHORITY\" \"$XDG_SESSION_TYPE\" \"$UNRELATED\" > \"$1\"", "probe", marker]);
        run(
            &[
                "configure".into(),
                "boot".into(),
                "--launch".into(),
                "0".into(),
                argv.to_string(),
            ],
            &store,
            &desktop,
        )
        .unwrap();
        let expected: Vec<String> =
            serde_json::from_str(&std::env::var("TWS_BOOT_PROBE_EXPECTED").unwrap()).unwrap();
        for environment in expected {
            if marker.exists() {
                std::fs::remove_file(&marker).unwrap();
            }
            desktop.seeding.set(false);
            desktop.actions.borrow_mut().clear();
            run(&["_startup".into()], &store, &desktop).unwrap();
            assert_eq!(std::fs::read_to_string(&marker).unwrap(), environment);
            assert!(desktop
                .actions
                .borrow()
                .iter()
                .any(|a| a["op"] == "place" && a["workspace"] == 1));
            assert!(!desktop.actions.borrow().iter().any(|a| a["op"] == "focus"));
            let saved = run(&["boot".into(), "show".into()], &store, &desktop).unwrap();
            assert!(run(&["_startup".into(), "extra".into()], &store, &desktop).is_err());
            assert_eq!(
                run(&["boot".into(), "show".into()], &store, &desktop).unwrap(),
                saved
            );
        }
    }
    eprintln!("None");
}

/// Reproduce early service startup and later display import on private buses.
/// Parameters: none.
/// Returns: None; asserts X11/Wayland refresh, explicit override and standalone fallback.
#[test]
fn startup_uses_current_desktop_environment() {
    eprintln!("parameters=none");
    for (ready_after, values, display, auth, expected) in [
        (
            Some(0),
            vec![
                "DISPLAY=:91",
                "XAUTHORITY=/tmp/auth one=two",
                "XDG_SESSION_TYPE=x11",
                "UNRELATED=do-not-import",
            ],
            None,
            None,
            vec![":91\n\n/tmp/auth one=two\nx11\n\n"],
        ),
        (
            Some(1),
            vec!["WAYLAND_DISPLAY=wayland-9", "XDG_SESSION_TYPE=wayland"],
            None,
            None,
            vec!["\n\n\n\n\n", "\nwayland-9\n\nwayland\n\n"],
        ),
        (
            Some(0),
            vec![
                "DISPLAY=:91",
                "XAUTHORITY=/tmp/manager-auth",
                "XDG_SESSION_TYPE=x11",
            ],
            Some(":77"),
            Some("/tmp/client-auth"),
            vec![":77\n\n/tmp/client-auth\nx11\n\n"],
        ),
        (
            Some(0),
            vec!["DISPLAY=:91"],
            Some(""),
            None,
            vec![":91\n\n\n\n\n"],
        ),
        (None, vec![], Some(":77"), None, vec![":77\n\n\n\n\n"]),
    ] {
        let bus = PrivateBus::start();
        let connection = ready_after.map(|delay| {
            zbus::blocking::connection::Builder::address(bus.address.as_str())
                .unwrap()
                .name("org.freedesktop.systemd1")
                .unwrap()
                .serve_at(
                    "/org/freedesktop/systemd1",
                    EnvironmentManager {
                        calls: AtomicUsize::new(0),
                        ready_after: delay,
                        values: values.into_iter().map(String::from).collect(),
                    },
                )
                .unwrap()
                .build()
                .unwrap()
        });
        let root = tempfile::tempdir().unwrap();
        let mut child = Command::new(std::env::current_exe().unwrap());
        child
            .args(["--exact", "boot_probe_child"])
            .env("DBUS_SESSION_BUS_ADDRESS", &bus.address)
            .env("TWS_BOOT_PROBE_ROOT", root.path())
            .env(
                "TWS_BOOT_PROBE_EXPECTED",
                serde_json::to_string(&expected).unwrap(),
            );
        for name in [
            "DISPLAY",
            "WAYLAND_DISPLAY",
            "XAUTHORITY",
            "XDG_SESSION_TYPE",
            "TWS_EDIT_ID",
            "TWS_LOAD_ID",
            "TMUX",
            "UNRELATED",
        ] {
            child.env_remove(name);
        }
        if let Some(display) = display {
            child.env("DISPLAY", display);
        }
        if let Some(auth) = auth {
            child.env("XAUTHORITY", auth);
        }
        let output = child.output().unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        drop(connection);
    }
    eprintln!("None");
}
