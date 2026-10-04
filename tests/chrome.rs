// Run: cargo test --locked --test chrome
// Coverage: two browser windows, current navigation (back/forward), URL/title
// pickle alignment, closed tabs/windows and partial tail -> session_contract;
// encrypted/bad/truncated header and malformed payload -> rejects_bad_formats.
use linux_task_workspaces::chrome::decode;
use serde_json::json;

fn ints(values: &[i32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}
fn record(data: &mut Vec<u8>, id: u8, payload: Vec<u8>) {
    data.extend_from_slice(&((payload.len() + 1) as u16).to_le_bytes());
    data.push(id);
    data.extend(payload);
}
fn pickle_string(bytes: &mut Vec<u8>, value: &str, utf16: bool) {
    let data = if utf16 {
        value
            .encode_utf16()
            .flat_map(|c| c.to_le_bytes())
            .collect::<Vec<_>>()
    } else {
        value.as_bytes().to_vec()
    };
    bytes.extend_from_slice(
        &((if utf16 { data.len() / 2 } else { data.len() }) as i32).to_le_bytes(),
    );
    let len = data.len();
    bytes.extend(data);
    bytes.extend(vec![0; len.div_ceil(4) * 4 - len]);
}
fn navigation(data: &mut Vec<u8>, tab: i32, index: i32, url: &str, title: &str) {
    let mut payload = ints(&[0, tab, index]);
    pickle_string(&mut payload, url, false);
    pickle_string(&mut payload, title, true);
    let len = payload.len() - 4;
    payload[..4].copy_from_slice(&(len as i32).to_le_bytes());
    record(data, 6, payload);
}

#[test]
fn session_contract() {
    let mut d = b"SNSS\x03\0\0\0".to_vec();
    record(&mut d, 0, ints(&[10, 1]));
    record(&mut d, 2, ints(&[1, 0]));
    navigation(&mut d, 1, 0, "https://drive.google.com/", "Drive Å");
    navigation(&mut d, 1, 1, "https://example.com/next", "Next");
    record(&mut d, 7, ints(&[1, 0]));
    record(&mut d, 8, ints(&[10, 0]));
    record(&mut d, 14, ints(&[10, 0, -1080, 960, 1080, 1]));
    record(&mut d, 0, ints(&[20, 2]));
    navigation(&mut d, 2, 0, "https://cloud.example/", "Cloud");
    record(&mut d, 0, ints(&[20, 3]));
    navigation(&mut d, 3, 0, "https://closed.example/", "Closed");
    record(&mut d, 16, ints(&[3, 0, 0, 0]));
    let windows = decode(&d).unwrap();
    assert_eq!(windows.len(), 2);
    assert_eq!(windows[0]["urls"], json!(["https://drive.google.com/"]));
    assert_eq!(windows[0]["active_title"], "Drive Å");
    assert_eq!(windows[0]["rect"], json!([0, -1080, 960, 1080]));
    assert_eq!(windows[1]["urls"], json!(["https://cloud.example/"]));
    d.extend_from_slice(&100u16.to_le_bytes());
    d.extend_from_slice(&[6, 0]);
    assert_eq!(decode(&d).unwrap(), windows);
}

#[test]
fn rejects_bad_formats() {
    for bytes in [
        b"bad".as_slice(),
        b"SNSS\x05\0\0\0",
        b"SNSS\x03",
        b"SNSS\x03\0\0\0\x01\0\x06",
    ] {
        assert!(decode(bytes).is_err());
    }
}

#[test]
#[ignore = "Opt-in read-only check of a supplied browser session; prints count only"]
fn supplied_session() {
    let path = std::env::var("TWS_TEST_CHROME_FILE").unwrap();
    let count =
        linux_task_workspaces::chrome::validate_session_file(std::path::Path::new(&path)).unwrap();
    assert!(count > 0);
    println!("Decoded {count} browser windows");
}
