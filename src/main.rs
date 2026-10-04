fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let store = linux_task_workspaces::Store::environment();
    match linux_task_workspaces::run(&args, &store, &linux_task_workspaces::Desktop) {
        Ok(text) => print!("{text}"),
        Err(e) => {
            eprintln!("tws: {e}");
            std::process::exit(1);
        }
    }
}
