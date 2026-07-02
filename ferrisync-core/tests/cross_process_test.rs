use ferrisync_core::crypto::CryptoProvider;
use ferrisync_core::storage::Storage;
use ferrisync_core::sync_engine::session;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

/// Find the ferrisync-cli binary path relative to the workspace.
fn cli_binary_path() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop(); // from ferrisync-core to workspace root
    path.push("target");
    path.push("debug");
    path.push("ferrisync-cli");
    if !path.exists() {
        // Try building it
        let status = Command::new("cargo")
            .args(["build", "-p", "ferrisync-cli"])
            .status()
            .expect("cargo should be available");
        assert!(status.success(), "cargo build -p ferrisync-cli failed");
    }
    assert!(path.exists(), "ferrisync-cli binary not found at {:?}", path);
    path
}

/// Get an available TCP port by binding to port 0.
fn get_available_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    std::thread::sleep(Duration::from_millis(50));
    port
}

/// Test cross-process sync: CLI serve + session::run_sync_session
#[tokio::test]
async fn test_cross_process_cli_serve_and_sync() {
    let server_folder = tempfile::tempdir().unwrap();
    let client_folder = tempfile::tempdir().unwrap();
    let server_data_dir = tempfile::tempdir().unwrap();
    let client_data_dir = tempfile::tempdir().unwrap();

    // Create a file on the server side
    std::fs::write(server_folder.path().join("from_server.txt"), b"Hello via CLI serve").unwrap();

    let port = get_available_port();
    let bin_path = cli_binary_path();

    // Start the CLI serve subprocess
    let mut child = Command::new(&bin_path)
        .arg("--data-dir")
        .arg(server_data_dir.path())
        .arg("serve")
        .arg("--port")
        .arg(port.to_string())
        .arg(server_folder.path().to_str().unwrap())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn ferrisync-cli serve");

    // Wait for the server to be ready
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Verify the subprocess is still alive
    match child.try_wait() {
        Ok(Some(status)) => {
            let stderr = child.stderr.take().map(|mut s| {
                use std::io::Read;
                let mut buf = String::new();
                let _ = s.read_to_string(&mut buf);
                buf
            }).unwrap_or_default();
            child.kill().ok();
            panic!("CLI server exited early with status: {status}\nstderr: {stderr}");
        }
        Ok(None) => { /* still running, good */ }
        Err(e) => {
            child.kill().ok();
            panic!("error checking child process: {e}");
        }
    }

    // Set up the test client
    let crypto = Arc::new(CryptoProvider::generate().unwrap());
    let storage = Arc::new(
        Storage::open(&client_data_dir.path().join("metadata.db")).unwrap(),
    );

    let dev_id = uuid::Uuid::new_v4().to_string();
    storage.upsert_device(&dev_id, "cli-server", None).unwrap();

    let folder_id = storage
        .add_sync_folder(
            client_folder.path().to_str().unwrap(),
            &dev_id,
            "bidirectional",
        )
        .unwrap();

    let (tx, _rx) = mpsc::channel(256);

    let addr: std::net::SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();

    // Run sync from the test client
    let result = session::run_sync_session(
        crypto.clone(),
        storage.clone(),
        client_folder.path().to_str().unwrap(),
        addr,
        folder_id,
        &dev_id,
        tx.clone(),
    )
    .await;

    match &result {
        Ok(r) => {
            println!(
                "Sync result: pushed {:?}, pulled {:?}",
                r.pushed, r.pulled
            );
        }
        Err(e) => {
            child.kill().ok();
            panic!("sync session failed: {e}");
        }
    }

    // Give the server a moment to process
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Verify the file was synced to the client
    let client_file = client_folder.path().join("from_server.txt");
    assert!(
        client_file.exists(),
        "from_server.txt should exist on client"
    );
    let content = std::fs::read_to_string(&client_file).unwrap();
    assert_eq!(content, "Hello via CLI serve");

    // Cleanup
    let _ = child.kill();
    let _ = child.wait();
}

/// Test: cross-process sync with multiple files
#[tokio::test]
async fn test_cross_process_multi_file() {
    let server_folder = tempfile::tempdir().unwrap();
    let client_folder = tempfile::tempdir().unwrap();
    let server_data_dir = tempfile::tempdir().unwrap();
    let client_data_dir = tempfile::tempdir().unwrap();

    // Create multiple files on the server
    std::fs::write(server_folder.path().join("alpha.txt"), b"Alpha").unwrap();
    std::fs::write(server_folder.path().join("beta.txt"), b"Beta").unwrap();
    std::fs::write(server_folder.path().join("gamma.txt"), b"Gamma").unwrap();

    let port = get_available_port();
    let bin_path = cli_binary_path();

    let mut child = Command::new(&bin_path)
        .arg("--data-dir")
        .arg(server_data_dir.path())
        .arg("serve")
        .arg("--port")
        .arg(port.to_string())
        .arg(server_folder.path().to_str().unwrap())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn ferrisync-cli serve");

    tokio::time::sleep(Duration::from_secs(1)).await;

    match child.try_wait() {
        Ok(Some(status)) => {
            child.kill().ok();
            panic!("CLI server exited early with status: {status}");
        }
        Ok(None) => {}
        Err(e) => {
            child.kill().ok();
            panic!("error checking child process: {e}");
        }
    }

    let crypto = Arc::new(CryptoProvider::generate().unwrap());
    let storage = Arc::new(
        Storage::open(&client_data_dir.path().join("metadata.db")).unwrap(),
    );

    let dev_id = uuid::Uuid::new_v4().to_string();
    storage.upsert_device(&dev_id, "cli-server", None).unwrap();

    let folder_id = storage
        .add_sync_folder(
            client_folder.path().to_str().unwrap(),
            &dev_id,
            "bidirectional",
        )
        .unwrap();

    let (tx, _rx) = mpsc::channel(256);
    let addr: std::net::SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();

    let result = session::run_sync_session(
        crypto.clone(),
        storage.clone(),
        client_folder.path().to_str().unwrap(),
        addr,
        folder_id,
        &dev_id,
        tx.clone(),
    )
    .await;

    assert!(result.is_ok(), "sync session failed: {:?}", result.err());

    tokio::time::sleep(Duration::from_millis(500)).await;

    for name in &["alpha.txt", "beta.txt", "gamma.txt"] {
        let path = client_folder.path().join(name);
        assert!(path.exists(), "{name} should exist on client");
    }

    assert_eq!(
        std::fs::read_to_string(client_folder.path().join("alpha.txt")).unwrap(),
        "Alpha"
    );
    assert_eq!(
        std::fs::read_to_string(client_folder.path().join("beta.txt")).unwrap(),
        "Beta"
    );
    assert_eq!(
        std::fs::read_to_string(client_folder.path().join("gamma.txt")).unwrap(),
        "Gamma"
    );

    let _ = child.kill();
    let _ = child.wait();
}

/// Test: cross-process sync with bidirectional data
#[tokio::test]
async fn test_cross_process_bidirectional() {
    let server_folder = tempfile::tempdir().unwrap();
    let client_folder = tempfile::tempdir().unwrap();
    let server_data_dir = tempfile::tempdir().unwrap();
    let client_data_dir = tempfile::tempdir().unwrap();

    // Server has a file, client has a different file
    std::fs::write(server_folder.path().join("from_server.txt"), b"Server data").unwrap();
    std::fs::write(client_folder.path().join("from_client.txt"), b"Client data").unwrap();

    let port = get_available_port();
    let bin_path = cli_binary_path();

    let mut child = Command::new(&bin_path)
        .arg("--data-dir")
        .arg(server_data_dir.path())
        .arg("serve")
        .arg("--port")
        .arg(port.to_string())
        .arg(server_folder.path().to_str().unwrap())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn ferrisync-cli serve");

    tokio::time::sleep(Duration::from_secs(1)).await;

    match child.try_wait() {
        Ok(Some(status)) => {
            child.kill().ok();
            panic!("CLI server exited early with status: {status}");
        }
        Ok(None) => {}
        Err(e) => {
            child.kill().ok();
            panic!("error checking child process: {e}");
        }
    }

    let crypto = Arc::new(CryptoProvider::generate().unwrap());
    let storage = Arc::new(
        Storage::open(&client_data_dir.path().join("metadata.db")).unwrap(),
    );

    let dev_id = uuid::Uuid::new_v4().to_string();
    storage.upsert_device(&dev_id, "cli-server", None).unwrap();

    let folder_id = storage
        .add_sync_folder(
            client_folder.path().to_str().unwrap(),
            &dev_id,
            "bidirectional",
        )
        .unwrap();

    let (tx, _rx) = mpsc::channel(256);
    let addr: std::net::SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();

    let result = session::run_sync_session(
        crypto.clone(),
        storage.clone(),
        client_folder.path().to_str().unwrap(),
        addr,
        folder_id,
        &dev_id,
        tx.clone(),
    )
    .await;

    assert!(result.is_ok(), "sync session failed: {:?}", result.err());

    tokio::time::sleep(Duration::from_millis(500)).await;

    // Client should have pulled from_server.txt from server
    assert!(
        client_folder.path().join("from_server.txt").exists(),
        "from_server.txt should exist on client"
    );
    assert_eq!(
        std::fs::read_to_string(client_folder.path().join("from_server.txt")).unwrap(),
        "Server data"
    );

    // Server should have pulled from_client.txt from client
    assert!(
        server_folder.path().join("from_client.txt").exists(),
        "from_client.txt should exist on server"
    );
    assert_eq!(
        std::fs::read_to_string(server_folder.path().join("from_client.txt")).unwrap(),
        "Client data"
    );

    let _ = child.kill();
    let _ = child.wait();
}
