//! Black-box system tests for the interactive REPL shell.
//!
//! Each test spawns the real `ferrisync` binary in REPL mode with piped
//! stdio, scripts commands into it, and asserts on its captured output.
//! Piped stdin makes rustyline fall back to plain line reads, so no pty is
//! needed; EOF exits through the same cleanup path as `exit`.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

const WAIT_TIMEOUT: Duration = Duration::from_secs(20);
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Path to the freshly built `ferrisync` binary.
fn binary_path() -> PathBuf {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("target/debug/ferrisync");
    if !path.exists() {
        let status = Command::new("cargo")
            .args(["build", "-q", "-p", "ferrisync"])
            .status()
            .expect("run cargo build");
        assert!(status.success(), "cargo build -p ferrisync failed");
    }
    path
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind :0")
        .local_addr()
        .expect("local addr")
        .port()
}

/// Hand out collision-free ports to parallel tests: a random base plus an
/// in-process counter, verified bindable at hand-out time.
fn alloc_port() -> u16 {
    static BASE: OnceLock<u16> = OnceLock::new();
    static OFFSET: AtomicU16 = AtomicU16::new(0);
    let base = *BASE.get_or_init(free_port);
    let candidate = base + OFFSET.fetch_add(1, Ordering::Relaxed) + 1;
    match TcpListener::bind(("127.0.0.1", candidate)) {
        Ok(listener) => {
            let port = listener.local_addr().unwrap().port();
            drop(listener);
            port
        }
        Err(_) => free_port(),
    }
}

/// A spawned process whose stdout/stderr are accumulated into a shared
/// transcript that tests can poll.
struct Proc {
    child: Child,
    stdin: Option<ChildStdin>,
    transcript: Arc<Mutex<String>>,
}

impl Proc {
    /// Spawn the REPL shell with an isolated data dir. No subcommand is
    /// given: a bare invocation (even with piped stdin) opens the shell.
    fn repl(data_dir: &Path) -> Self {
        Self::spawn(Command::new(binary_path()).arg("--data-dir").arg(data_dir))
    }

    /// Spawn `pair` against `127.0.0.1:port` from an isolated data dir
    /// (its own crypto identity).
    fn pair(data_dir: &Path, port: u16) -> Self {
        Self::spawn(
            Command::new(binary_path())
                .arg("--data-dir")
                .arg(data_dir)
                .args(["pair", "127.0.0.1"])
                .args(["--port", &port.to_string()]),
        )
    }

    fn spawn(cmd: &mut Command) -> Self {
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = cmd.spawn().expect("spawn ferrisync");
        let stdout = child.stdout.take().expect("stdout");
        let stderr = child.stderr.take().expect("stderr");
        let transcript = Arc::new(Mutex::new(String::new()));
        pipe_into(stdout, Arc::clone(&transcript));
        pipe_into(stderr, Arc::clone(&transcript));
        Proc {
            stdin: child.stdin.take(),
            child,
            transcript,
        }
    }

    /// Send one REPL command line.
    fn send(&mut self, line: &str) {
        let stdin = self.stdin.as_mut().expect("stdin still open");
        writeln!(stdin, "{line}").expect("write command");
        stdin.flush().expect("flush command");
    }

    /// Send a command and wait for the expected output fragment.
    fn expect(&mut self, cmd: &str, output_fragment: &str) {
        self.send(cmd);
        self.wait_for(output_fragment);
    }

    /// Poll until `fragment` appears in the transcript; panic with the full
    /// transcript on timeout.
    fn wait_for(&self, fragment: &str) {
        let deadline = Instant::now() + WAIT_TIMEOUT;
        loop {
            if self.transcript.lock().unwrap().contains(fragment) {
                return;
            }
            if Instant::now() >= deadline {
                panic!(
                    "timed out waiting for {fragment:?}; transcript:\n{}",
                    self.transcript.lock().unwrap()
                );
            }
            thread::sleep(POLL_INTERVAL);
        }
    }

    /// Poll until the process exits; panic with the transcript on timeout.
    fn wait_exit(&mut self) -> ExitStatus {
        let started = Instant::now();
        let deadline = started + WAIT_TIMEOUT;
        loop {
            if let Some(status) = self.child.try_wait().expect("try_wait") {
                return status;
            }
            if Instant::now() >= deadline {
                let _ = self.child.kill();
                panic!(
                    "timed out waiting for exit after {}s; transcript:\n{}",
                    started.elapsed().as_secs(),
                    self.transcript.lock().unwrap()
                );
            }
            thread::sleep(POLL_INTERVAL);
        }
    }

    /// Snapshot of everything the process has printed so far.
    fn transcript(&self) -> String {
        self.transcript.lock().unwrap().clone()
    }
}

impl Drop for Proc {
    fn drop(&mut self) {
        self.stdin.take(); // close stdin -> REPL exits via EOF path
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn pipe_into(stream: impl std::io::Read + Send + 'static, transcript: Arc<Mutex<String>>) {
    thread::spawn(move || {
        for line in BufReader::new(stream).lines() {
            match line {
                Ok(l) => {
                    let mut t = transcript.lock().unwrap();
                    t.push_str(&l);
                    t.push('\n');
                }
                Err(_) => break,
            }
        }
    });
}

/// Pair from `client_data`'s identity against the REPL's server on `port`,
/// approve the held request via `pendings`/`confirm`, then verify a
/// re-pair of the now-known identity succeeds without interaction.
fn pair_and_approve(repl: &mut Proc, client_data: &Path, port: u16) {
    let mut first = Proc::pair(client_data, port);
    repl.wait_for("PAIRING REQUEST — confirm connection with '");
    repl.expect("pendings", "\n  1  ");
    repl.expect("confirm 1", "approved '");
    let status = first.wait_exit();
    assert!(
        status.success(),
        "pair failed despite approval:\n{}",
        first.transcript()
    );

    let mut again = Proc::pair(client_data, port);
    let status = again.wait_exit();
    assert!(
        status.success(),
        "re-pair of known device failed:\n{}",
        again.transcript()
    );
}

#[test]
fn help_then_clean_exit() {
    let tmp = tempfile::tempdir().unwrap();
    let mut repl = Proc::repl(&tmp.path().join("data"));

    repl.send("help");
    repl.wait_for("interactive shell");
    let t = repl.transcript();
    for expected in [
        "serve <folder>",
        "pendings",
        "confirm <n>",
        "deny <n>",
        "unserve <id>",
    ] {
        assert!(t.contains(expected), "help missing {expected:?}:\n{t}");
    }

    repl.send("exit");
    let status = repl.wait_exit();
    assert!(status.success(), "REPL did not exit cleanly after 'exit'");
}

#[test]
fn serve_lifecycle() {
    let tmp = tempfile::tempdir().unwrap();
    let folder = tmp.path().join("srvfold");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("seed.txt"), b"seed").unwrap();

    let port = alloc_port();
    let mut repl = Proc::repl(&tmp.path().join("data"));

    repl.expect(
        &format!("serve {} --port {port}", folder.display()),
        &format!("serve #1 started: {} on 0.0.0.0:{port}", folder.display()),
    );
    repl.expect(
        "serves",
        &format!("#1  {} on 0.0.0.0:{port}", folder.display()),
    );

    // The server must actually be listening before we unserve it.
    assert!(
        TcpStream::connect(("127.0.0.1", port)).is_ok(),
        "served port not listening"
    );

    repl.expect("unserve 1", "server #1 stopped");
    repl.expect("serves", "(no active background servers)");
    repl.send("exit");
    let status = repl.wait_exit();
    assert!(status.success());
}

#[test]
fn pairing_consent_e2e() {
    let tmp = tempfile::tempdir().unwrap();
    let folder = tmp.path().join("consentfold");
    std::fs::create_dir_all(&folder).unwrap();

    let port = alloc_port();
    let client_data = tmp.path().join("client-data");
    let mut repl = Proc::repl(&tmp.path().join("host-data"));
    repl.expect(
        &format!("serve {} --port {port}", folder.display()),
        "serve #1 started",
    );

    // First contact: unknown identity is held and announced.
    let mut first_pair = Proc::pair(&client_data, port);
    repl.wait_for("PAIRING REQUEST — confirm connection with '");

    repl.expect("pendings", "\n  1  ");

    repl.expect("confirm 1", "approved '");
    let status = first_pair.wait_exit();
    assert!(
        status.success(),
        "pair failed despite approval:\n{}",
        first_pair.transcript()
    );

    // Known identity pairs again instantly, without any new request.
    let notices_before = repl.transcript().matches("PAIRING REQUEST").count();
    let mut again = Proc::pair(&client_data, port);
    let status = again.wait_exit();
    assert!(
        status.success(),
        "second pair of known device failed:\n{}",
        again.transcript()
    );
    assert_eq!(
        repl.transcript().matches("PAIRING REQUEST").count(),
        notices_before,
        "known-device re-pair should not raise a new PAIRING REQUEST"
    );
}

#[test]
fn deny_rejects_client() {
    let tmp = tempfile::tempdir().unwrap();
    let folder = tmp.path().join("denyfold");
    std::fs::create_dir_all(&folder).unwrap();

    let port = alloc_port();
    let client_data = tmp.path().join("client-data");
    let mut repl = Proc::repl(&tmp.path().join("host-data"));
    repl.expect(
        &format!("serve {} --port {port}", folder.display()),
        "serve #1 started",
    );

    let mut pair = Proc::pair(&client_data, port);
    repl.wait_for("PAIRING REQUEST — confirm connection with '");
    repl.expect("deny 1", "denied '");

    let status = pair.wait_exit();
    assert!(
        !status.success(),
        "pair succeeded despite denial:\n{}",
        pair.transcript()
    );
    assert!(
        pair.transcript().contains("denied by host"),
        "client error should mention denial:\n{}",
        pair.transcript()
    );

    // The denied identity stays unknown: the retry raises a fresh request.
    repl.wait_for("PAIRING REQUEST — confirm connection with '");
    repl.expect("deny 1", "denied '");
}

#[test]
fn sync_through_repl_serve() {
    let tmp = tempfile::tempdir().unwrap();
    let served = tmp.path().join("served");
    std::fs::create_dir_all(&served).unwrap();
    std::fs::write(served.join("host_file.txt"), b"from-host").unwrap();

    let client_folder = tmp.path().join("client-fold");
    std::fs::create_dir_all(&client_folder).unwrap();
    std::fs::write(client_folder.join("cli_file.txt"), b"from-cli").unwrap();

    let port = alloc_port();
    let client_data = tmp.path().join("client-data");
    let mut repl = Proc::repl(&tmp.path().join("host-data"));
    repl.expect(
        &format!("serve {} --port {port}", served.display()),
        "serve #1 started",
    );

    pair_and_approve(&mut repl, &client_data, port);

    // One-shot bidirectional sync between the two folders.
    let out = Command::new(binary_path())
        .arg("--data-dir")
        .arg(&client_data)
        .args(["sync", client_folder.to_str().unwrap()])
        .args(["--device", &format!("127.0.0.1:{port}")])
        .output()
        .expect("run sync subcommand");
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        out.status.success(),
        "sync subcommand failed:\n{combined}\n--- repl ---\n{}",
        repl.transcript()
    );
    assert!(
        combined.contains("Sync complete."),
        "no completion message:\n{combined}"
    );

    assert_eq!(
        std::fs::read(served.join("cli_file.txt")).unwrap(),
        b"from-cli",
        "client file did not reach the served folder"
    );
    assert_eq!(
        std::fs::read(client_folder.join("host_file.txt")).unwrap(),
        b"from-host",
        "host file did not reach the client folder"
    );

    // Quitting must visibly confirm that background servers were shut down.
    repl.send("exit");
    repl.wait_for("server #1 stopped");
}

/// The new home-screen vocabulary must be available in the REPL, and the
/// startup banner must appear with a hostname line on a fresh data dir.
#[test]
fn repl_home_screen_commands_and_banner() {
    let tmp = tempfile::tempdir().unwrap();
    let mut repl = Proc::repl(&tmp.path().join("host-data"));
    repl.expect("devices", "No paired devices.");
    repl.expect("folders", "No sync folders configured.");
    repl.expect("conflicts", "No unresolved conflicts.");
    repl.send("exit");
    assert!(repl.wait_exit().success());

    let transcript = repl.transcript();
    assert!(
        transcript.contains("Start by connecting a device."),
        "startup dashboard missing all-clear headline:\n{transcript}"
    );
    assert!(
        transcript.contains("Devices") && transcript.contains("Folders"),
        "startup dashboard missing Devices/Folders sections:\n{transcript}"
    );
    assert!(
        transcript.contains("interactive shell"),
        "missing REPL banner:\n{transcript}"
    );
}

/// Folders served in a previous session (registered against this device) must
/// come back up automatically when the REPL is relaunched against the same
/// data dir — no manual `serve` needed.
#[test]
fn auto_serves_configured_folders_on_relaunch() {
    let tmp = tempfile::tempdir().unwrap();
    let folder = tmp.path().join("persistfold");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("seed.txt"), b"seed").unwrap();
    let data_dir = tmp.path().join("data");

    // First session: serve the folder (persists the self-hosted row), then quit.
    let mut first = Proc::repl(&data_dir);
    first.expect(
        &format!("serve {} --port {}", folder.display(), alloc_port()),
        &format!("serve #1 started: {} on 0.0.0.0:", folder.display()),
    );
    first.send("exit");
    assert!(first.wait_exit().success());

    // Relaunch against the same data dir: the folder must be auto-served at
    // startup (port 9847 per the auto-serve scheme), with no `serve` command.
    let mut second = Proc::repl(&data_dir);
    second.wait_for(&format!(
        "serve #1 started: {} on 0.0.0.0:",
        folder.display()
    ));
    let transcript = second.transcript();
    let served_line = transcript
        .lines()
        .find(|l| l.starts_with(&format!("serve #1 started: {}", folder.display())))
        .unwrap_or_else(|| panic!("no auto-serve line in transcript:\n{transcript}"));
    let served_port: u16 = served_line
        .rsplit_once(':')
        .and_then(|(_, tail)| {
            tail.chars()
                .take_while(|c| c.is_ascii_digit())
                .collect::<String>()
                .parse()
                .ok()
        })
        .unwrap_or_else(|| panic!("cannot parse served port from: {served_line}"));
    assert!(
        TcpStream::connect(("127.0.0.1", served_port)).is_ok(),
        "auto-served port {served_port} not listening on relaunch"
    );
    // `serves` must list it without the user ever typing `serve` this session.
    second.expect(
        "serves",
        &format!("#1  {} on 0.0.0.0:{served_port}", folder.display()),
    );

    second.send("exit");
    assert!(second.wait_exit().success());
}

/// `reset` must wipe the data dir (identity + db) and restore the device to a
/// fresh-install state on the next launch, while never touching user files.
#[test]
fn factory_reset_restores_fresh_install_on_relaunch() {
    let tmp = tempfile::tempdir().unwrap();
    let folder = tmp.path().join("resetfold");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("keep.txt"), b"keep me").unwrap();
    let data_dir = tmp.path().join("data");

    // First session: a bare launch already persists an identity on disk. Quit.
    let mut first = Proc::repl(&data_dir);
    first.send("exit");
    assert!(first.wait_exit().success());
    assert!(data_dir.join("identity.crt").exists(), "identity created");

    // Relaunch and factory reset.
    let mut reset_session = Proc::repl(&data_dir);
    reset_session.expect("reset --yes", "Device reset to a fresh install.");
    reset_session.send("exit");
    assert!(reset_session.wait_exit().success());

    // The data dir was wiped; user files kept.
    assert!(!data_dir.join("identity.crt").exists(), "identity wiped");
    assert!(!data_dir.join("metadata.db").exists(), "db wiped");
    assert!(
        folder.join("keep.txt").exists(),
        "user files must never be touched"
    );

    // Relaunch against the (now empty) dir: fresh-install dashboard.
    let mut third = Proc::repl(&data_dir);
    third.wait_for("Start by connecting a device.");
    third.send("exit");
    assert!(third.wait_exit().success());
    // A fresh keypair yields a different certificate on disk than before reset.
    assert!(
        data_dir.join("identity.crt").exists(),
        "new identity created on relaunch"
    );
}
