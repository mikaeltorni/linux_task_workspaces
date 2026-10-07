use crate::{command_argv, ensure, hotkeys, text, Bridge, Result, Store};
use serde_json::{json, Value};
use std::fs;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::process::Command;

pub fn check_hotkey(
    store: &Store,
    bridge: &dyn Bridge,
    id: &str,
    role: &str,
    chord: &str,
) -> Result<String> {
    let chord = hotkeys::accelerator(chord)?;
    if chord.is_empty() {
        return Ok(chord);
    }
    let mut collisions = bridge.conflicts(&chord, id, role)?;
    for profile in store.list()? {
        for other_role in ["open", "close"] {
            if text(&profile, "id") == id && other_role == role {
                continue;
            }
            if hotkeys::accelerator(text(&profile, &format!("{other_role}_hotkey")))? == chord {
                collisions.push(format!("workspace {} {other_role}", text(&profile, "id")));
            }
        }
    }
    if !collisions.is_empty() {
        return Err(format!(
            "Hotkey {chord} is in use by {}. Choose another; existing bindings were preserved.",
            collisions.join(", ")
        )
        .into());
    }
    Ok(chord)
}

pub fn validate(profile: &Value, id: &str) -> Result<()> {
    if text(profile, "id") != id || profile["version"] != 1 {
        return Err("Profile ID/version must match".into());
    }
    if !profile["workspace"].as_u64().is_some_and(|i| i <= 35) {
        return Err("Workspace must be an integer 0–35 (GNOME supports 36)".into());
    }
    if !["always", "on-demand"].contains(&text(profile, "mode")) {
        return Err("Mode must be always or on-demand".into());
    }
    if !["hide", "close"].contains(&text(profile, "close_action")) {
        return Err("Close action must be hide or close".into());
    }
    if !profile["monitors"]
        .as_array()
        .is_some_and(|m| !m.is_empty())
    {
        return Err("Profile needs saved monitors".into());
    }
    for m in profile["monitors"].as_array().unwrap() {
        if text(m, "connector").is_empty()
            || !m["width"].as_i64().is_some_and(|n| n > 0)
            || !m["height"].as_i64().is_some_and(|n| n > 0)
        {
            return Err("Invalid monitor geometry".into());
        }
    }
    for w in profile["windows"]
        .as_array()
        .ok_or("windows must be an array")?
    {
        let rect = w["rect"]
            .as_array()
            .ok_or("Window rect must be [x,y,width,height]")?;
        if rect.len() != 4
            || !rect.iter().all(|n| n.as_i64().is_some())
            || rect[2].as_i64().unwrap() <= 0
            || rect[3].as_i64().unwrap() <= 0
        {
            return Err("Invalid window frame".into());
        }
        command_argv(w)?;
        if text(w, "class").is_empty() {
            return Err("Window class is required".into());
        }
    }
    Ok(())
}

pub fn workspace_number(number: &str) -> Result<u64> {
    let number = number
        .parse::<u64>()
        .map_err(|_| "Workspace number must be an integer 1–36")?;
    if !(1..=36).contains(&number) {
        return Err("Workspace number must be 1–36".into());
    }
    Ok(number - 1)
}

pub fn apply(store: &Store, bridge: &dyn Bridge, id: &str, mut profile: Value) -> Result<String> {
    let old = store.read(id)?;
    if profile["workspace"] != old["workspace"] {
        profile["workspace_assigned"] = json!(true);
    }
    let changed = profile["workspace"] != old["workspace"]
        || profile["workspace_assigned"] != old["workspace_assigned"];
    persist(store, bridge, id, profile, changed)
}

pub fn create(store: &Store, bridge: &dyn Bridge, id: &str, profile: Value) -> Result<String> {
    if store.path(id)?.exists() {
        return Err(format!("Workspace already exists: {id}").into());
    }
    persist(store, bridge, id, profile, true)
}

fn persist(
    store: &Store,
    bridge: &dyn Bridge,
    id: &str,
    mut profile: Value,
    reserve: bool,
) -> Result<String> {
    validate(&profile, id)?;
    let open = check_hotkey(store, bridge, id, "open", text(&profile, "open_hotkey"))?;
    let close = check_hotkey(store, bridge, id, "close", text(&profile, "close_hotkey"))?;
    if !open.is_empty() && open == close {
        return Err("Open and close hotkeys must differ".into());
    }
    profile["open_hotkey"] = json!(open);
    profile["close_hotkey"] = json!(close);
    if reserve {
        ensure(&profile, bridge)?;
    }
    for (role, handler, chord) in [("open", "_activate", open), ("close", "_dismiss", close)] {
        bridge.bind(
            id,
            role,
            &chord,
            &[
                hotkeys::executable().to_string_lossy().into_owned(),
                handler.into(),
                id.into(),
            ],
        )?;
    }
    store.write(id, &profile)?;
    Ok(format!("Configured {id}\n"))
}

pub fn edit_json(store: &Store, bridge: &dyn Bridge, id: &str) -> Result<String> {
    let p = store.read(id)?;
    let draft = store
        .root
        .join(format!(".edit-{}.draft", std::process::id()));
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&draft)?;
    writeln!(f, "{}", serde_json::to_string_pretty(&p)?)?;
    let result = (|| -> Result<String> {
        let editor = std::env::var("TWS_EDITOR")
            .or_else(|_| std::env::var("EDITOR"))
            .unwrap_or_else(|_| "nano".into());
        let mut parts = editor.split_whitespace();
        let editor = parts.next().ok_or("Empty editor command")?;
        if !Command::new(editor)
            .args(parts)
            .arg(&draft)
            .status()?
            .success()
        {
            return Err("Editor cancelled; saved profile unchanged".into());
        }
        apply(
            store,
            bridge,
            id,
            serde_json::from_str(&fs::read_to_string(&draft)?)?,
        )
    })();
    fs::remove_file(draft)?;
    result
}
