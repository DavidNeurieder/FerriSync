//! Mode-selection tests for the unified `ferrisync` binary.
//!
//! The binary has one user-facing entry point: `ferrisync` with no arguments
//! launches the interactive REPL, while any subcommand runs headlessly. The
//! deterministic parse-layer contract (None → REPL, Some → CLI) is pinned by
//! unit tests in `cli.rs`; these tests pin the headless behavior of real
//! subcommands and confirm the no-subcommand path never behaves like a
//! one-shot CLI.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn binary_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("target/debug/ferrisync")
}

/// `ferrisync status` must run headlessly and exit successfully. The point is
/// that a subcommand always runs one-shot and never enters the REPL loop.
/// The internal device id is hidden by default and only shown with `--verbose`.
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
        stdout.contains("FerriSync ·")
            && stdout.contains("Start by connecting a device.")
            && stdout.contains("DEVICES")
            && stdout.contains("FOLDERS"),
        "status output missing expected sections:\n{stdout}"
    );
    assert!(
        !stdout.contains("Device ID:"),
        "default status should hide the internal id:\n{stdout}"
    );

    let verbose = Command::new(binary_path())
        .arg("--data-dir")
        .arg(data_dir.path())
        .args(["status", "--verbose"])
        .output()
        .expect("run ferrisync status --verbose");
    assert!(verbose.status.success());
    assert!(
        String::from_utf8_lossy(&verbose.stdout).contains("Device ID:"),
        "--verbose should expose the device id"
    );
}

/// The new home-screen commands must run headlessly too (empty data dir).
#[test]
fn home_screen_commands_run_headlessly() {
    for args in [
        vec!["devices"],
        vec!["folders"],
        vec!["activity"],
        vec!["conflicts"],
        vec!["doctor", "--explain", "pairings"],
        vec!["doctor", "--json"],
        vec!["doctor"],
        vec!["status", "--verbose"],
    ] {
        let data_dir = tempfile::tempdir().unwrap();
        let out = Command::new(binary_path())
            .arg("--data-dir")
            .arg(data_dir.path())
            .args(&args)
            .output()
            .expect("run ferrisync subcommand");
        assert!(
            out.status.success(),
            "{args:?} exited {:?}:\n{}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

/// `devices --json`, `status --json` and `doctor --json` should emit
/// well-formed, machine-readable JSON on an empty store (arrays, not errors).
#[test]
fn json_output_is_well_formed() {
    let data_dir = tempfile::tempdir().unwrap();
    for args in [
        vec!["devices", "--json"],
        vec!["status", "--json"],
        vec!["doctor", "--json"],
    ] {
        let out = Command::new(binary_path())
            .arg("--data-dir")
            .arg(data_dir.path())
            .args(&args)
            .output()
            .expect("run json subcommand");
        assert!(
            out.status.success(),
            "{args:?} exited {:?}: {}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
        );
        let text = String::from_utf8_lossy(&out.stdout);
        let value = serde_json::from_str::<serde_json::Value>(&text)
            .unwrap_or_else(|e| panic!("{args:?} produced invalid JSON: {e}\n{text}"));
        // doctor --json must expose the top-level healthy flag.
        if args.first() == Some(&"doctor") {
            assert!(
                value.get("healthy").is_some() && value.get("checks").is_some(),
                "doctor --json missing healthy/checks:\n{value}"
            );
        }
    }
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
    for cmd in [
        "status", "sync", "pair", "serve", "watch", "rename", "remove",
    ] {
        assert!(help.contains(cmd), "help missing {cmd:?}:\n{help}");
    }
}

/// With no subcommand the binary enters the interactive REPL. Fed a script
/// through a non-TTY stdin it must run those commands and exit cleanly on
/// EOF — never exit 0 silently (the old standalone-CLI one-shot behavior).
#[test]
fn no_subcommand_enters_the_repl() {
    let data_dir = tempfile::tempdir().unwrap();
    let mut child = Command::new(binary_path())
        .arg("--data-dir")
        .arg(data_dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn ferrisync with no subcommand");

    let mut stdin = child.stdin.take().expect("stdin");
    writeln!(stdin, "status").expect("send status");
    writeln!(stdin, "exit").expect("send exit");
    drop(stdin);

    let out = child.wait_with_output().expect("wait for ferrisync");
    assert!(
        out.status.success(),
        "no-subcommand invocation exited {:?}:\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("interactive shell") && stdout.contains("FerriSync ·") && stdout.contains("DEVICES"),
        "no-subcommand output missing REPL banner and status:\n{stdout}"
    );
}
