use crate::{command_argv, ensure, hotkeys, text, Bridge, Result, Store};
use serde_json::{json, Value};
use std::fs;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::process::Command;

/// Configure persistent explicit destinations or shortcuts for automatic task slots.
/// Parameters: store - task storage; bridge - prompts and desktop actions; id - task identifier.
/// Returns: configuration confirmation, or an error with the profile unchanged before persistence.
pub fn wizard(store: &Store, bridge: &dyn Bridge, id: &str) -> Result<String> {
    eprintln!("store={:?}, bridge={bridge:p}, id={id}", store.root);
    let exists = store.path(id)?.exists();
    let mut current = if exists {
        store.read(id)?
    } else {
        crate::prepare_profile(store, bridge, id)?
    };
    let default = if current["workspace_assigned"] == true {
        (current["workspace"].as_u64().ok_or("Invalid workspace")? + 1).to_string()
    } else {
        "-".into()
    };
    let number = bridge.prompt("GNOME workspace (1–36; - for automatic slot)", &default)?;
    let destination = workspace_choice(&number)?;
    match destination {
        Some(slot) => assign_settings(&mut current, slot),
        None => automatic_settings(store, bridge, id, &mut current)?,
    }
    let result = if exists {
        apply(store, bridge, id, current)
    } else {
        create(store, bridge, id, current)
    };
    eprintln!("{result:?}");
    result
}

/// Parse a wizard destination before choosing its configuration operation.
/// Parameters: number - prompt response, either a workspace number or automatic marker.
/// Returns: parsed zero-based destination, None for automatic, or an invalid-number error.
fn workspace_choice(number: &str) -> Result<Option<u64>> {
    eprintln!("number={number}");
    let result = if number.is_empty() || number == "-" {
        Ok(None)
    } else {
        workspace_number(number).map(Some)
    };
    eprintln!("{result:?}");
    result
}

/// Configure required open shortcuts and optional behavior for automatic task slots.
/// Parameters: store - task storage; bridge - prompts and conflict checks; id - task identifier; profile - draft configuration.
/// Returns: success with an updated draft, or rejection before saved state or bindings change.
fn automatic_settings(
    store: &Store,
    bridge: &dyn Bridge,
    id: &str,
    profile: &mut Value,
) -> Result<()> {
    eprintln!(
        "store={:?}, bridge={bridge:p}, id={id}, profile={profile}",
        store.root
    );
    if profile["workspace_assigned"] == true {
        profile["mode"] = json!("on-demand");
    }
    profile["workspace_assigned"] = json!(false);
    let chord = bridge.prompt(
        "Open hotkey (required, e.g. Super+B)",
        text(profile, "open_hotkey"),
    )?;
    let open = check_hotkey(store, bridge, id, "open", &chord)?;
    if open.is_empty() {
        return Err("An automatic task workspace needs an open hotkey; choose a numbered workspace for persistent use without shortcuts".into());
    }
    profile["open_hotkey"] = json!(open);
    let mode = bridge.prompt("Mode (always / on-demand)", text(profile, "mode"))?;
    profile["mode"] = json!(mode);
    let close = bridge.prompt(
        "Hide/close hotkey (Ctrl+Super+B; - clears)",
        text(profile, "close_hotkey"),
    )?;
    profile["close_hotkey"] = json!(close);
    let action = bridge.prompt("Close action (hide / close)", text(profile, "close_action"))?;
    profile["close_action"] = json!(action);
    eprintln!("Ok(())");
    Ok(())
}

/// Apply persistence and empty shortcuts to a chosen destination.
/// Parameters: profile - mutable saved task; slot - parsed zero-based workspace.
/// Returns: None; updates workspace assignment and default mode in place.
fn assign_settings(profile: &mut Value, slot: u64) {
    eprintln!("profile={profile}, slot={slot}");
    profile["workspace"] = json!(slot);
    profile["workspace_assigned"] = json!(true);
    profile["mode"] = json!("always");
    profile["open_hotkey"] = json!("");
    profile["close_hotkey"] = json!("");
    eprintln!("None");
}

/// Create or update a persistent task on an explicitly chosen workspace.
/// Parameters: store - task storage; bridge - desktop operations; id - task identifier; slot - parsed zero-based workspace.
/// Returns: configuration confirmation, or a profile/desktop error.
pub fn assign(store: &Store, bridge: &dyn Bridge, id: &str, slot: u64) -> Result<String> {
    eprintln!(
        "store={:?}, bridge={bridge:p}, id={id}, slot={slot}",
        store.root
    );
    let exists = store.path(id)?.exists();
    let mut profile = if exists {
        store.read(id)?
    } else {
        crate::prepare_profile(store, bridge, id)?
    };
    assign_settings(&mut profile, slot);
    let result = if exists {
        apply(store, bridge, id, profile)
    } else {
        create(store, bridge, id, profile)
    };
    eprintln!("{result:?}");
    result
}

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
