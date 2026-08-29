//! Mode-selection tests for the unified `ferrisync` binary.
//!
//! The binary has one user-facing entry point: `ferrisync` with no arguments
//! launches the interactive TUI, while any subcommand runs headlessly. The
//! deterministic parse-layer contract (None → TUI, Some → CLI) is pinned by
//! unit tests in `cli.rs`; these tests pin the headless behavior of real
//! subcommands and confirm the no-subcommand path never behaves like the old
//! standalone CLI.

use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

fn binary_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("target/debug/ferrisync")
}

/// `ferrisync status` must run headlessly and exit successfully. While the
/// status output is TTY-free anyway, the point is that a subcommand never
/// reaches the raw-mode TUI.
#[test]
fn status_runs_headlessly() {
    let data_dir = tempfile::tempdir().unwrap();
    let out = Command::new(binary_path())
        .arg("--data-dir")
        .arg(data_dir.path())
        .arg("status")
        .output()
        .expect("run ferrisync status");
    assert!(
        out.status.success(),
        "status exited {:?}:\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Paired devices:")
            && stdout.contains("Device ID:") && stdout.contains("Device name:"),
        "status output missing expected sections:\n{stdout}"
    );
}

/// `ferrisync --help` must print help and exit without launching anything.
#[test]
fn help_exits_immediately() {
    let out = Command::new(binary_path())
        .arg("--help")
        .output()
        .expect("run ferrisync --help");
    assert!(out.status.success());
    let help = String::from_utf8_lossy(&out.stdout);
    for cmd in ["status", "sync", "pair", "serve", "watch", "rename", "remove"] {
        assert!(help.contains(cmd), "help missing {cmd:?}:\n{help}");
    }
}

/// With no subcommand the binary heads for the TUI (raw mode needs a real
/// terminal). In a TTY-less test it must exit non-zero — never print the old
/// standalone-CLI info banner and exit 0. On a real TTY it blocks in the
/// event loop until we kill it.
#[test]
fn no_subcommand_never_behaves_like_standalone_cli() {
    let data_dir = tempfile::tempdir().unwrap();
    let mut child = Command::new(binary_path())
        .arg("--data-dir")
        .arg(data_dir.path())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .stdin(std::process::Stdio::null())
        .spawn()
        .expect("spawn ferrisync with no subcommand");

    let deadline = Instant::now() + Duration::from_secs(3);
    let early = loop {
        match child.try_wait().expect("try_wait") {
            Some(status) => break Some(status),
            None if Instant::now() >= deadline => break None,
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    };

    if let Some(status) = early {
        assert!(
            !status.success(),
            "no-subcommand invocation exited 0 in a TTY-less test — \
             expected the TUI path to fail (raw mode) rather than fall back \
             to the old standalone CLI"
        );
    } else {
        // Still running: real TTY, TUI event loop active. Good.
        let _ = child.kill();
        let _ = child.wait();
    }
}