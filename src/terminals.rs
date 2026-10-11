use crate::text;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

/// Recognize terminal identities independently of optional ACC installation.
/// Parameters: name - window class, desktop application ID or terminal executable name.
/// Returns: whether the identity denotes a known terminal emulator.
fn terminal_name(name: &str) -> bool {
    eprintln!("name={name}");
    let name = name.trim().to_ascii_lowercase();
    let name = name.strip_suffix(".desktop").unwrap_or(&name);
    let result = name.starts_with("kitty-")
        || matches!(
            name,
            "kitty"
                | "gnome-terminal"
                | "gnome-terminal-server"
                | "org.gnome.terminal"
                | "kgx"
                | "org.gnome.console"
                | "konsole"
                | "org.kde.konsole"
                | "xterm"
                | "uxterm"
                | "urxvt"
                | "rxvt"
                | "alacritty"
                | "foot"
                | "footclient"
                | "wezterm"
                | "wezterm-gui"
                | "org.wezfurlong.wezterm"
                | "terminator"
                | "tilix"
                | "com.gexperts.tilix"
                | "terminology"
                | "ghostty"
                | "com.mitchellh.ghostty"
                | "sakura"
                | "mate-terminal"
                | "xfce4-terminal"
                | "lxterminal"
                | "guake"
                | "yakuake"
                | "qterminal"
                | "st"
                | "org.kde.yakuake"
                | "org.pantheon.terminal"
                | "io.elementary.terminal"
                | "org.gnome.ptyxis"
                | "ptyxis"
                | "org.gnome.blackbox"
                | "com.raggesilver.blackbox"
        );
    eprintln!("{result}");
    result
}

/// Recognize custom terminal apps using freedesktop application categories.
/// Parameters: app - desktop entry basename returned by GNOME.
/// Returns: whether an installed desktop entry identifies a TerminalEmulator.
fn terminal_desktop_entry(app: &str) -> bool {
    eprintln!("app={app}");
    let result = if app.is_empty() || Path::new(app).file_name() != Some(std::ffi::OsStr::new(app))
    {
        false
    } else {
        let user_data = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| crate::home().join(".local/share"));
        let system_data =
            std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".into());
        std::iter::once(user_data)
            .chain(
                system_data
                    .split(':')
                    .filter(|s| !s.is_empty())
                    .map(PathBuf::from),
            )
            .find_map(|base| fs::read_to_string(base.join("applications").join(app)).ok())
            .is_some_and(|entry| {
                let mut desktop_section = false;
                for line in entry.lines().map(str::trim) {
                    if line.starts_with('[') {
                        desktop_section = line == "[Desktop Entry]";
                    }
                    if desktop_section
                        && line.strip_prefix("Categories=").is_some_and(|categories| {
                            categories.split(';').any(|c| c == "TerminalEmulator")
                        })
                    {
                        return true;
                    }
                }
                false
            })
    };
    eprintln!("{result}");
    result
}

/// Classify normal terminal windows even when they use custom WM classes.
/// Parameters: window - GNOME window fields including class, app and process PID.
/// Returns: whether the window is a terminal emulator.
pub fn is_terminal(window: &Value) -> bool {
    eprintln!("window={window}");
    let executable = window["pid"]
        .as_u64()
        .and_then(|pid| fs::read_link(format!("/proc/{pid}/exe")).ok());
    let result = terminal_name(text(window, "class"))
        || terminal_name(text(window, "app"))
        || executable
            .as_ref()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .is_some_and(terminal_name)
        || terminal_desktop_entry(text(window, "app"));
    eprintln!("{result}");
    result
}

/// Exclude ACC agent and monitor windows using identities, without invoking ACC.
/// Parameters: window - GNOME class/application identity; sessions - attached tmux session snapshots.
/// Returns: whether the terminal belongs to Agent Command Center.
pub fn is_acc(window: &Value, sessions: &[Value]) -> bool {
    eprintln!("window={window}, sessions={sessions:?}");
    let configured_monitor = std::env::var("AGENT_COMMAND_CENTER_TMUX_SESSION").ok();
    let result = [text(window, "class"), text(window, "app")]
        .iter()
        .any(|name| name.to_ascii_lowercase().starts_with("kitty-acc-"))
        || sessions.iter().any(|s| {
            let name = text(s, "name");
            name.starts_with("acc-")
                || name == "agent_command_center"
                || configured_monitor
                    .as_deref()
                    .is_some_and(|monitor| !monitor.is_empty() && name == monitor)
        });
    eprintln!("{result}");
    result
}
