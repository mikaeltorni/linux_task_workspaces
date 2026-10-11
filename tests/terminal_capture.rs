// Run: cargo test --locked --offline --test terminal_capture
#[path = "support/terminal_fixture.rs"]
mod fixture;
use fixture::{call, window, Fixture};
use linux_task_workspaces::Store;
use serde_json::{json, Value};

/// Check capture/save excludes ACC and plain terminals while retaining task tmux and GUI apps.
/// Parameters: none.
/// Returns: None.
#[test]
fn capture_exclusions() {
    eprintln!("parameters=none");
    let fixture = Fixture::new();
    fixture.standalone();
    let store = Store {
        root: fixture.root.path().join("profiles"),
    };
    let task_pid = fixture.clients.borrow()[0].id();
    let mut windows = vec![
        window("task", "kitty", task_pid),
        window("other-terminal", "org.gnome.Terminal", task_pid),
        window("acc-class", "kitty-acc-agent-codex", task_pid),
        window("acc-session", "kitty", fixture.clients.borrow()[1].id()),
        window("acc-monitor", "kitty", fixture.clients.borrow()[2].id()),
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
