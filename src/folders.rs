use crate::{text, Result};
use serde_json::{json, Value};
use std::collections::HashMap;

pub fn nautilus_locations() -> Result<Value> {
    let bus = zbus::blocking::Connection::session()?;
    // No auto-start: capture must not launch a file manager just to probe it.
    let proxy = zbus::blocking::Proxy::new(
        &bus,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
    )?;
    let owned: bool = proxy.call("NameHasOwner", &("org.freedesktop.FileManager1",))?;
    if !owned {
        return Ok(json!({}));
    }
    let manager = zbus::blocking::Proxy::new(
        &bus,
        "org.freedesktop.FileManager1",
        "/org/freedesktop/FileManager1",
        "org.freedesktop.FileManager1",
    )?;
    let locations: HashMap<String, Vec<String>> =
        manager.get_property("OpenWindowsWithLocations")?;
    Ok(json!(locations))
}

pub fn attach(window: &mut Value, locations: &Value) {
    let object = text(window, "gtk_path");
    if !text(window, "class").to_lowercase().contains("nautilus") {
        return;
    }
    if let Some(uris) = locations[object].as_array() {
        let uris: Vec<Value> = uris
            .iter()
            .filter(|u| u.as_str().is_some())
            .cloned()
            .collect();
        if uris.is_empty() {
            return;
        }
        window["folders"] = json!(uris);
        let mut argv = vec![json!("nautilus"), json!("--new-window")];
        argv.extend(uris);
        window["launch"] = json!(argv);
    }
}
