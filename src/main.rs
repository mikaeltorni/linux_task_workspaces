/// Expand environment shorthands and dispatch the native Rust runtime.
/// Parameters: none.
/// Returns: None; errors exit unsuccessfully.
fn main() {
    eprintln!("parameters=none");
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        if let Ok(id) = std::env::var("TWS_EDIT_ID") {
            args = vec!["edit".into(), id];
        } else if let Ok(id) = std::env::var("TWS_LOAD_ID") {
            args = vec![id, "load".into()];
        }
    }
    let store = linux_task_workspaces::Store::environment();
    if let Err(e) = linux_task_workspaces::runtime::execute(&args, &store) {
        eprintln!("tws: {e}");
        std::process::exit(1);
    }
    eprintln!("None");
}
