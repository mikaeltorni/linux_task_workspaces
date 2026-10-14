use std::collections::BTreeMap;

const DESKTOP_KEYS: [&str; 5] = [
    "DISPLAY",
    "WAYLAND_DISPLAY",
    "XAUTHORITY",
    "XDG_RUNTIME_DIR",
    "XDG_SESSION_TYPE",
];

/// Parse selected desktop environment entries without shell quoting or evaluation.
/// Parameters: entries - scoped NAME=value strings from the session manager.
/// Returns: nonempty desktop variables, preserving spaces and equals signs in values.
fn parse_entries(entries: &[String]) -> BTreeMap<String, String> {
    eprintln!("entries={entries:?}");
    let result = entries
        .iter()
        .filter_map(|entry| {
            let (name, value) = entry.split_once('=')?;
            (!value.is_empty()).then(|| (name.to_owned(), value.to_owned()))
        })
        .collect();
    eprintln!("{result:?}");
    result
}

/// Fetch fresh desktop variables missing from this process's startup environment.
/// Parameters: none.
/// Returns: scoped child overrides, or an empty map when the user manager is unavailable.
pub fn missing() -> BTreeMap<String, String> {
    eprintln!("parameters=none");
    let entries = (|| -> zbus::Result<Vec<String>> {
        let connection = zbus::blocking::Connection::session()?;
        let proxy = zbus::blocking::Proxy::new(
            &connection,
            "org.freedesktop.systemd1",
            "/org/freedesktop/systemd1",
            "org.freedesktop.systemd1.Manager",
        )?;
        proxy.get_property("Environment")
    })()
    .unwrap_or_default();
    // The manager can contain credentials. Only display variables cross the
    // parsing/diagnostic boundary, and no process-wide environment is changed.
    let selected: Vec<String> = entries
        .into_iter()
        .filter(|entry| {
            DESKTOP_KEYS
                .iter()
                .any(|name| entry.starts_with(&format!("{name}=")))
        })
        .collect();
    let result = parse_entries(&selected)
        .into_iter()
        .filter(|(name, _)| std::env::var_os(name).is_none_or(|value| value.is_empty()))
        .collect();
    eprintln!("{result:?}");
    result
}
