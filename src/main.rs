fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let store = linux_task_workspaces::Store::environment();
    if args == ["_watch"] {
        loop {
            if let Err(e) =
                linux_task_workspaces::reconcile(&store, &linux_task_workspaces::Desktop)
            {
                eprintln!("tws reconciliation: {e}");
            }
            std::thread::sleep(std::time::Duration::from_secs(20));
        }
    }
    match linux_task_workspaces::run(&args, &store, &linux_task_workspaces::Desktop) {
        Ok(text) => print!("{text}"),
        Err(e) => {
            eprintln!("tws: {e}");
            std::process::exit(1);
        }
    }
}
