use crate::{continuity, Desktop, Result, Store};
use std::path::PathBuf;

#[derive(Debug)]
enum Mode {
    Watch,
    Record(PathBuf),
    Replay(PathBuf),
    CheckpointAll,
    Cli(Vec<String>),
}

/// Parse internal runtime forms once, converting path operands before dispatch.
/// Parameters: args - raw native CLI tokens.
/// Returns: a validated runtime selector and converted operands.
fn parse(args: &[String]) -> Result<Mode> {
    eprintln!("args={args:?}");
    let tokens: Vec<_> = args.iter().map(String::as_str).collect();
    let result = match tokens.as_slice() {
        ["_watch"] => Ok(Mode::Watch),
        ["_checkpoint-all"] => Ok(Mode::CheckpointAll),
        ["_record-pane", path] if !path.is_empty() => Ok(Mode::Record(PathBuf::from(path))),
        ["_restore-pane", path] if !path.is_empty() => Ok(Mode::Replay(PathBuf::from(path))),
        [name, ..]
            if ["_watch", "_checkpoint-all", "_record-pane", "_restore-pane"].contains(name) =>
        {
            Err("Invalid internal runtime operands".into())
        }
        _ => Ok(Mode::Cli(args.to_vec())),
    };
    eprintln!("{result:?}");
    result
}

/// Run a validated native command through its dedicated operation owner.
/// Parameters: args - CLI tokens after environment shorthand expansion; store - task store.
/// Returns: successful command completion or its error.
pub fn execute(args: &[String], store: &Store) -> Result<()> {
    eprintln!("args={args:?}, store={:?}", store.root);
    match parse(args)? {
        Mode::Record(path) => continuity::record(&path)?,
        Mode::Replay(path) => continuity::replay(&path)?,
        Mode::CheckpointAll => {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
            let _lock = loop {
                match store.lock() {
                    Ok(lock) => break lock,
                    Err(_) if std::time::Instant::now() < deadline => {
                        std::thread::sleep(std::time::Duration::from_millis(100))
                    }
                    Err(error) => return Err(error),
                }
            };
            continuity::all(store)?;
        }
        Mode::Watch => loop {
            if let Err(error) = crate::reconcile(store, &Desktop) {
                eprintln!("tws reconciliation: {error}");
            }
            std::thread::sleep(std::time::Duration::from_secs(20));
        },
        Mode::Cli(args) => print!("{}", crate::run(&args, store, &Desktop)?),
    }
    eprintln!("Ok(())");
    Ok(())
}
