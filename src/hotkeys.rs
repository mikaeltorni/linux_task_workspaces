use crate::{home, output, Result};
use std::collections::BTreeSet;
use std::io::{self, Write};
use std::path::PathBuf;

pub const CUSTOM: &str = "org.gnome.settings-daemon.plugins.media-keys.custom-keybinding";
pub const MEDIA: &str = "org.gnome.settings-daemon.plugins.media-keys";

pub fn accelerator(raw: &str) -> Result<String> {
    if raw.trim().is_empty() {
        return Ok(String::new());
    }
    let parts: Vec<String> = raw
        .replace('<', "")
        .replace('>', "+")
        .split('+')
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().to_ascii_lowercase())
        .collect();
    let mut modifiers = Vec::new();
    let mut key = None;
    for p in parts {
        match p.as_str() {
            "ctrl" | "control" | "primary" => modifiers.push("Control"),
            "super" | "meta" | "mod4" | "win" => modifiers.push("Super"),
            "alt" | "mod1" => modifiers.push("Alt"),
            "shift" => modifiers.push("Shift"),
            _ if key.is_none() && p.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_') => {
                key = Some(p)
            }
            _ => return Err("Hotkey needs modifiers plus exactly one key, e.g. Super+B.".into()),
        }
    }
    modifiers.sort_unstable();
    modifiers.dedup();
    let key = key.ok_or("Hotkey is missing its key")?;
    let key = match key.as_str() {
        "escape" | "esc" => "Escape".to_owned(),
        "return" | "enter" => "Return".to_owned(),
        "tab" => "Tab".to_owned(),
        "backspace" => "BackSpace".to_owned(),
        "delete" => "Delete".to_owned(),
        "home" => "Home".to_owned(),
        "end" => "End".to_owned(),
        "pageup" | "page_up" => "Page_Up".to_owned(),
        "pagedown" | "page_down" => "Page_Down".to_owned(),
        "left" => "Left".to_owned(),
        "right" => "Right".to_owned(),
        "up" => "Up".to_owned(),
        "down" => "Down".to_owned(),
        _ if key.starts_with('f')
            && key[1..].parse::<u8>().is_ok_and(|n| (1..=35).contains(&n)) =>
        {
            key.to_uppercase()
        }
        _ => key,
    };
    if modifiers.is_empty() {
        return Err("Use at least one modifier for a task shortcut".into());
    }
    Ok(format!(
        "{}{}",
        modifiers
            .iter()
            .map(|m| format!("<{m}>"))
            .collect::<String>(),
        key
    ))
}

pub fn variant_string(s: &str) -> String {
    format!("'{}'", s.replace('\\', "\\\\").replace('\'', "\\'"))
}

pub fn strings(value: &str) -> Vec<String> {
    let mut result = vec![];
    let mut quote = '\0';
    let mut current = String::new();
    let mut escape = false;
    for c in value.chars() {
        if quote == '\0' {
            if c == '\'' || c == '"' {
                quote = c;
            }
        } else if escape {
            current.push(c);
            escape = false;
        } else if c == '\\' {
            escape = true;
        } else if c == quote {
            result.push(std::mem::take(&mut current));
            quote = '\0';
        } else {
            current.push(c);
        }
    }
    result
}

pub fn schema(id: &str, role: &str) -> String {
    format!("{CUSTOM}:/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/tws-{id}-{role}/")
}

pub fn bind(id: &str, role: &str, chord: &str, command: &[String]) -> Result<()> {
    let binding = accelerator(chord)?;
    let schema = schema(id, role);
    let path = schema.split_once(':').unwrap().1;
    let root = output("gsettings", &["get", MEDIA, "custom-keybindings"])?;
    let mut paths = strings(&root);
    let shell = command
        .iter()
        .map(|s| format!("'{}'", s.replace('\'', "'\\''")))
        .collect::<Vec<_>>()
        .join(" ");
    output(
        "gsettings",
        &[
            "set",
            &schema,
            "name",
            &variant_string(&format!("Task Workspace {id} {role}")),
        ],
    )?;
    output(
        "gsettings",
        &["set", &schema, "command", &variant_string(&shell)],
    )?;
    output(
        "gsettings",
        &["set", &schema, "binding", &variant_string(&binding)],
    )?;
    if binding.is_empty() {
        paths.retain(|p| p != path);
    } else if !paths.iter().any(|p| p == path) {
        paths.push(path.into());
    }
    let value = format!(
        "[{}]",
        paths
            .iter()
            .map(|s| variant_string(s))
            .collect::<Vec<_>>()
            .join(",")
    );
    output("gsettings", &["set", MEDIA, "custom-keybindings", &value])?;
    let actual = output("gsettings", &["get", &schema, "binding"])?;
    if strings(&actual).first() != Some(&binding) {
        return Err("Shortcut readback did not match".into());
    }
    Ok(())
}

pub fn executable() -> PathBuf {
    std::env::current_exe().unwrap_or_else(|_| home().join(".local/bin/tws"))
}

pub fn matches_output(chord: &str, contents: &str, excluded: &str) -> Vec<String> {
    let mut matches = vec![];
    for line in contents.lines() {
        let mut parts = line.splitn(3, ' ');
        let schema = parts.next().unwrap_or("");
        let key = parts.next().unwrap_or("");
        let value = parts.next().unwrap_or("");
        if schema == excluded && key == "binding" {
            continue;
        }
        if strings(value)
            .iter()
            .any(|s| accelerator(s).is_ok_and(|a| a == chord))
        {
            matches.push(format!("{schema} {key}"));
        }
    }
    matches
}

pub fn conflicts(chord: &str, id: &str, role: &str) -> Result<Vec<String>> {
    let chord = accelerator(chord)?;
    if chord.is_empty() {
        return Ok(vec![]);
    }
    let excluded = schema(id, role);
    let mut collisions: BTreeSet<String> = matches_output(
        &chord,
        &output("gsettings", &["list-recursively"])?,
        &excluded,
    )
    .into_iter()
    .collect();
    for path in strings(&output("gsettings", &["get", MEDIA, "custom-keybindings"])?) {
        let target = format!("{CUSTOM}:{path}");
        // list-recursively prints the bare schema, so exclude by its path here.
        if target == excluded {
            continue;
        }
        collisions.extend(matches_output(
            &chord,
            &output("gsettings", &["list-recursively", &target])?,
            &excluded,
        ));
    }
    for root in [
        home().join(".local/share/gnome-shell/extensions"),
        PathBuf::from("/usr/share/gnome-shell/extensions"),
    ] {
        if let Ok(entries) = std::fs::read_dir(root) {
            for extension in entries.flatten() {
                let dir = extension.path().join("schemas");
                if !dir.join("gschemas.compiled").exists() {
                    continue;
                }
                let dir = dir.to_string_lossy();
                let schemas = output("gsettings", &["--schemadir", &dir, "list-schemas"])?;
                for name in schemas
                    .lines()
                    .filter(|s| s.starts_with("org.gnome.shell.extensions."))
                {
                    collisions.extend(matches_output(
                        &chord,
                        &output(
                            "gsettings",
                            &["--schemadir", &dir, "list-recursively", name],
                        )?,
                        &excluded,
                    ));
                }
            }
        }
    }
    Ok(collisions.into_iter().collect())
}

pub fn terminal(args: &[String]) -> Vec<String> {
    let chosen = std::env::var("TWS_TERMINAL").unwrap_or_else(|_| "kitty".into());
    let mut command = vec![chosen.clone()];
    if chosen.ends_with("gnome-terminal") {
        command.push("--".into());
    } else if !chosen.ends_with("kitty") {
        command.push("-e".into());
    }
    command.extend_from_slice(args);
    command
}

pub fn prompt(label: &str, default: &str) -> Result<String> {
    print!("{label} [{default}]: ");
    io::stdout().flush()?;
    let mut line = String::new();
    if io::stdin().read_line(&mut line)? == 0 {
        return Err("Configuration cancelled (end of input)".into());
    }
    let line = line.trim();
    Ok(if line.is_empty() {
        default.into()
    } else if line == "-" {
        String::new()
    } else {
        line.into()
    })
}
