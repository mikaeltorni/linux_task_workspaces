# Task workspaces

This standalone repository owns named task-workspace snapshots, their GNOME
bridge, tmux restoration, and task-specific shortcuts. General desktop settings
and numbered workspace shortcuts remain in their existing repositories.

The CLI and manager are Rust; only the GNOME bridge is JavaScript.
Use `cargo test --locked`, `cargo clippy --locked --all-targets -- -D warnings`,
`cargo fmt --check`, and
`node --test tests/*.test.mjs`; tests must mock the real desktop and isolate tmux.
Install with `bash install.sh` as the desktop user. Follow the selected shared
Linux configuration policy for deployment and activation.
