//! Regression: pairing through the folder-less control listener must not make
//! subsequent folder syncs fail. Each folder must reach its own folder server,
//! even when the stored device address is the control endpoint.

use ferrisync_core::api::{
    approve_pending_pairing, close, init_engine_with_control, pair_with_device, pending_pairings,
    start_server, sync_folder,
};
use std::sync::Arc;
use std::time::Duration;

async fn approve(server: &Arc<ferrisync_core::api::ApiState>, client_id: &str) {
    for _ in 0..100 {
        if pending_pairings(server)
            .unwrap()
            .iter()
            .any(|(_, id)| id == client_id)
        {
            approve_pending_pairing(server, client_id.to_string(), "test-client".into()).unwrap();
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("control listener never surfaced the pairing request");
}

#[tokio::test]
async fn two_folders_sync_after_pairing_via_control_port() {
    let server_data = tempfile::tempdir().unwrap();
    let client_data = tempfile::tempdir().unwrap();
    let server_a = tempfile::tempdir().unwrap();
    let client_a = tempfile::tempdir().unwrap();
    let server_b = tempfile::tempdir().unwrap();
    let client_b = tempfile::tempdir().unwrap();

    let server = Arc::new(
        init_engine_with_control(server_data.path().to_str().unwrap().to_string(), 0)
            .await
            .unwrap(),
    );
    let client = Arc::new(
        init_engine_with_control(client_data.path().to_str().unwrap().to_string(), 0)
            .await
            .unwrap(),
    );

    let client_id = client.current_device().id.clone();
    let server_id = server.current_device().id.clone();
    let control_port = server.control_port();
    let pair_client = Arc::clone(&client);
    let pairing = tokio::spawn(async move {
        pair_with_device(&pair_client, "127.0.0.1".into(), control_port).await
    });
    approve(&server, &client_id).await;
    pairing.await.unwrap().unwrap();

    std::fs::write(server_a.path().join("from_server_a.txt"), b"a").unwrap();
    std::fs::write(server_b.path().join("from_server_b.txt"), b"b").unwrap();

    let folders = [
        (
            "guid-a",
            server_a.path().to_str().unwrap(),
            client_a.path().to_str().unwrap(),
        ),
        (
            "guid-b",
            server_b.path().to_str().unwrap(),
            client_b.path().to_str().unwrap(),
        ),
    ];
    let mut client_folders = Vec::new();
    for (guid, server_path, client_path) in folders {
        let server_folder = server
            .storage
            .ensure_folder_by_guid(guid, server_path, guid, &server_id)
            .unwrap();
        server
            .storage
            .add_folder_device(server_folder, &client_id, "bidirectional", None)
            .unwrap();
        let server_port = start_server(
            &server,
            0,
            server_folder,
            server_path.to_string(),
        )
        .await
        .unwrap();
        assert_ne!(server_port, control_port, "folder server must be distinct");

        let client_folder = client
            .storage
            .ensure_folder_by_guid(guid, client_path, guid, &client_id)
            .unwrap();
        client
            .storage
            .add_folder_device(client_folder, &server_id, "bidirectional", None)
            .unwrap();
        client_folders.push((client_folder, client_path.to_string(), server_port));
    }

    // Simulate a device row written before the control-port mapping was added:
    // sync must still recover when the stored address is the control endpoint.
    client
        .storage
        .set_device_last_addr(&server_id, &format!("127.0.0.1:{control_port}"))
        .unwrap();
    for (folder_id, path, server_port) in client_folders {
        // Deliberately pass the control port, mirroring syncFolderNow's use of
        // the address recorded at pairing time.
        let result = sync_folder(
            &client,
            folder_id,
            path,
            "127.0.0.1".into(),
            control_port,
            server_id.clone(),
            false,
        )
        .await
        .unwrap_or_else(|e| panic!("sync via control port failed: {e:#}"));
        assert!(!result.pulled.is_empty(), "folder {folder_id} pulled nothing");
        let _ = server_port;
    }

    close(&client).await.unwrap();
    close(&server).await.unwrap();
}
