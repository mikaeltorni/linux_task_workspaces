//! Read-only Chromium SNSS v1/v3 session decoding. The saved schema is ours;
//! command IDs and pickle layout follow Chromium's components/sessions/core.
use crate::{home, text, Result};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

#[derive(Default)]
struct Tab {
    window: i32,
    order: i32,
    selected: i32,
    navigations: BTreeMap<i32, (String, String)>,
}

fn int(data: &[u8], offset: usize) -> Result<i32> {
    Ok(i32::from_le_bytes(
        data.get(offset..offset + 4)
            .ok_or("Truncated session record")?
            .try_into()?,
    ))
}
fn string(data: &[u8], offset: &mut usize, utf16: bool) -> Result<String> {
    let len = int(data, *offset)?;
    *offset += 4;
    if len < 0 {
        return Err("Negative session string size".into());
    }
    let bytes = (len as usize)
        .checked_mul(if utf16 { 2 } else { 1 })
        .ok_or("Invalid string size")?;
    let raw = data
        .get(*offset..*offset + bytes)
        .ok_or("Truncated session string")?;
    *offset += bytes.div_ceil(4) * 4;
    Ok(if utf16 {
        String::from_utf16_lossy(
            &raw.chunks_exact(2)
                .map(|p| u16::from_le_bytes([p[0], p[1]]))
                .collect::<Vec<_>>(),
        )
    } else {
        String::from_utf8_lossy(raw).into_owned()
    })
}

pub fn decode(data: &[u8]) -> Result<Vec<Value>> {
    if data.get(..4) != Some(b"SNSS") {
        return Err("Not a Chromium session file".into());
    }
    if ![1, 3].contains(&int(data, 4)?) {
        return Err(
            "Encrypted/unsupported Chromium session format; configure explicit browser URLs".into(),
        );
    }
    let mut tabs: BTreeMap<i32, Tab> = BTreeMap::new();
    let mut windows: BTreeMap<i32, Value> = BTreeMap::new();
    let mut offset = 8;
    while offset + 2 <= data.len() {
        let size = u16::from_le_bytes([data[offset], data[offset + 1]]) as usize;
        offset += 2;
        if size == 0 {
            return Err("Empty Chromium session command".into());
        }
        if offset + size > data.len() {
            break;
        } // Chrome can be writing the tail.
        let id = data[offset];
        let p = &data[offset + 1..offset + size];
        offset += size;
        match id {
            0 => {
                let wid = int(p, 0)?;
                tabs.entry(int(p, 4)?).or_default().window = wid;
                windows.entry(wid).or_insert(json!({"session_id":wid}));
            }
            2 => {
                tabs.entry(int(p, 0)?).or_default().order = int(p, 4)?;
            }
            6 => {
                let tid = int(p, 4)?;
                let nav = int(p, 8)?;
                let mut at = 12;
                let url = string(p, &mut at, false)?;
                let title = string(p, &mut at, true)?;
                tabs.entry(tid)
                    .or_default()
                    .navigations
                    .insert(nav, (url, title));
            }
            7 => {
                tabs.entry(int(p, 0)?).or_default().selected = int(p, 4)?;
            }
            8 => {
                let wid = int(p, 0)?;
                windows.entry(wid).or_insert(json!({"session_id":wid}))["selected_tab"] =
                    json!(int(p, 4)?);
            }
            14 => {
                let wid = int(p, 0)?;
                let w = windows.entry(wid).or_insert(json!({"session_id":wid}));
                w["rect"] = json!([int(p, 4)?, int(p, 8)?, int(p, 12)?, int(p, 16)?]);
            }
            16 => {
                tabs.remove(&int(p, 0)?);
            }
            17 => {
                let wid = int(p, 0)?;
                windows.remove(&wid);
                tabs.retain(|_, t| t.window != wid);
            }
            23 => {
                let wid = int(p, 4)?;
                let mut at = 8;
                let workspace = string(p, &mut at, false)?;
                windows.entry(wid).or_insert(json!({"session_id":wid}))["workspace"] =
                    json!(workspace);
            }
            24 => {
                let tid = int(p, 0)?;
                let start = int(p, 4)?;
                let count = int(p, 8)?;
                if let Some(t) = tabs.get_mut(&tid) {
                    t.navigations
                        .retain(|idx, _| *idx < start || *idx >= start + count);
                }
            }
            _ => {}
        }
    }
    let mut result = vec![];
    for (wid, mut w) in windows {
        let mut ordered: Vec<&Tab> = tabs.values().filter(|t| t.window == wid).collect();
        ordered.sort_by_key(|t| t.order);
        let mut urls = vec![];
        let mut titles = vec![];
        let mut active_title = String::new();
        for t in ordered {
            let n = t.navigations.get(&t.selected).or_else(|| {
                t.navigations
                    .range(..=t.selected)
                    .next_back()
                    .map(|(_, n)| n)
            });
            if let Some((url, title)) = n {
                if url.starts_with("http://")
                    || url.starts_with("https://")
                    || url.starts_with("file://")
                    || url.starts_with("chrome://")
                {
                    urls.push(url.clone());
                    titles.push(title.clone());
                    if Some(t.order) == w["selected_tab"].as_i64().map(|v| v as i32) {
                        active_title = title.clone();
                    }
                }
            }
        }
        if !urls.is_empty() {
            w["urls"] = json!(urls);
            w["titles"] = json!(titles);
            w["active_title"] = json!(active_title);
            result.push(w);
        }
    }
    Ok(result)
}

pub fn discover(snapshot: &Value) -> Vec<Value> {
    let mut roots: BTreeSet<(String, String)> = BTreeSet::new();
    for (path, program) in [
        (home().join(".config/google-chrome"), "google-chrome"),
        (home().join(".config/chromium"), "chromium"),
        (home().join("snap/chromium/common/chromium"), "chromium"),
    ] {
        if path.exists() {
            roots.insert((path.to_string_lossy().into_owned(), program.into()));
        }
    }
    for w in snapshot["windows"].as_array().unwrap_or(&vec![]) {
        if !is_browser(w) {
            continue;
        }
        if let Ok(bytes) = fs::read(format!("/proc/{}/cmdline", w["pid"].as_u64().unwrap_or(0))) {
            let args: Vec<String> = bytes
                .split(|b| *b == 0)
                .filter(|b| !b.is_empty())
                .map(|b| String::from_utf8_lossy(b).into_owned())
                .collect();
            for (i, a) in args.iter().enumerate() {
                let dir = a
                    .strip_prefix("--user-data-dir=")
                    .map(str::to_owned)
                    .or_else(|| {
                        (a == "--user-data-dir")
                            .then(|| args.get(i + 1).cloned())
                            .flatten()
                    });
                if let Some(dir) = dir {
                    roots.insert((
                        dir,
                        args.first()
                            .cloned()
                            .unwrap_or_else(|| "google-chrome".into()),
                    ));
                }
            }
        }
    }
    let mut all = vec![];
    for (root, program) in roots {
        let Ok(profiles) = fs::read_dir(&root) else {
            continue;
        };
        for profile in profiles.flatten() {
            let dir = profile.path().join("Sessions");
            let Ok(files) = fs::read_dir(dir) else {
                continue;
            };
            let newest = files
                .flatten()
                .filter(|e| e.file_name().to_string_lossy().starts_with("Session_"))
                .max_by_key(|e| e.metadata().and_then(|m| m.modified()).ok());
            if let Some(file) = newest {
                // Never modify profiles, cookies, or session files.
                if let Ok(mut windows) = fs::read(file.path())
                    .map_err(Into::into)
                    .and_then(|b| decode(&b))
                {
                    for w in &mut windows {
                        w["profile"] = json!(profile.file_name().to_string_lossy());
                        w["user_data_dir"] = json!(root);
                        w["program"] = json!(program);
                    }
                    all.extend(windows);
                }
            }
        }
    }
    all
}

pub fn is_browser(w: &Value) -> bool {
    let class = text(w, "class").to_lowercase();
    class.contains("chrome") || class.contains("chromium")
}

pub fn attach(window: &mut Value, sessions: &[Value], used: &mut BTreeSet<usize>) -> Result<()> {
    if !is_browser(window) {
        return Ok(());
    }
    let title = text(window, "title");
    let title = title
        .strip_suffix(" - Google Chrome")
        .or_else(|| title.strip_suffix(" - Chromium"))
        .unwrap_or(title);
    let mut candidates: Vec<(usize, i64)> = sessions
        .iter()
        .enumerate()
        .filter(|(i, _)| !used.contains(i))
        .filter(|(_, s)| {
            text(s, "active_title") == title
                || s["titles"]
                    .as_array()
                    .is_some_and(|t| t.iter().any(|v| v.as_str() == Some(title)))
        })
        .map(|(i, s)| {
            let score = window["rect"]
                .as_array()
                .zip(s["rect"].as_array())
                .map(|(a, b)| {
                    a.iter()
                        .zip(b)
                        .map(|(a, b)| (a.as_i64().unwrap_or(0) - b.as_i64().unwrap_or(0)).abs())
                        .sum()
                })
                .unwrap_or(i64::MAX / 2);
            (i, score)
        })
        .collect();
    candidates.sort_by_key(|(_, score)| *score);
    let Some((index, score)) = candidates.first().copied() else {
        return Err(format!("Cannot capture Chrome tabs for '{title}'. Wait for Chrome to flush its session and retry, or configure an explicit --launch argv with URLs.").into());
    };
    if candidates.get(1).is_some_and(|(_, next)| *next == score) {
        return Err(format!(
            "Ambiguous Chrome window '{title}'; configure explicit URLs before saving"
        )
        .into());
    }
    let s = &sessions[index];
    used.insert(index);
    let mut argv = vec![
        json!(text(s, "program")),
        json!("--new-window"),
        json!(format!("--user-data-dir={}", text(s, "user_data_dir"))),
        json!(format!("--profile-directory={}", text(s, "profile"))),
    ];
    argv.extend(
        s["urls"]
            .as_array()
            .ok_or("Missing browser URLs")?
            .iter()
            .cloned(),
    );
    window["browser"] = s.clone();
    window["launch"] = json!(argv);
    Ok(())
}

pub fn validate_session_file(path: &Path) -> Result<usize> {
    Ok(decode(&fs::read(path)?)?.len())
}
