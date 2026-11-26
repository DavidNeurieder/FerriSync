//! Regression test: device pairing must work before either device shares a
//! folder. The control listener owns PairRequest so it no longer depends on a
//! folder server existing, and folders can be shared afterwards.

use ferrisync_core::api::{
    approve_pending_pairing, browse_peer_shared_folders, close, init_engine_with_control,
    list_devices, list_sync_folders, local_pairing_addresses, pair_with_device, pending_pairings,
    request_folder_pairing, share_folder, start_server, FolderPairRequestOutcome,
};
use std::sync::Arc;
use std::time::Duration;

#[tokio::test]
async fn pairs_with_no_shared_folders_then_shares_one() {
    let server_data = tempfile::tempdir().unwrap();
    let client_data = tempfile::tempdir().unwrap();
    let shared_dir = tempfile::tempdir().unwrap();

    // Port 0 keeps each engine's control listener isolated from other tests.
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

    assert!(server.storage.list_sync_folders().unwrap().is_empty());
    assert!(client.storage.list_sync_folders().unwrap().is_empty());

    let control_port = server.control_port();
    assert_ne!(control_port, 0);
    let addresses = local_pairing_addresses(&server);
    assert_eq!(addresses.len(), 1, "fresh device has only the control address");
    assert!(addresses[0].ends_with(&format!(":{control_port}")));

    let client_id = client.current_device().id.clone();
    let server_id = server.current_device().id.clone();
    let pair_client = Arc::clone(&client);
    let task = tokio::spawn(async move {
        pair_with_device(&pair_client, "127.0.0.1".into(), control_port).await
    });

    let mut approved = false;
    for _ in 0..150 {
        if pending_pairings(&server)
            .unwrap()
            .iter()
            .any(|(_, id)| id == &client_id)
        {
            approve_pending_pairing(&server, client_id.clone(), "test-client".into()).unwrap();
            approved = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(approved, "control listener never surfaced the pairing request");

    let paired = task.await.unwrap().unwrap();
    assert!(paired.contains(&server_id));
    assert!(list_devices(&server)
        .unwrap()
        .iter()
        .any(|d| d.id == client_id));
    assert!(list_devices(&client)
        .unwrap()
        .iter()
        .any(|d| d.id == server_id));

    // Sharing a folder after pairing must not require re-pairing.
    let owner_id = server.current_device().id.clone();
    let folder_id = server
        .storage
        .add_sync_folder(shared_dir.path().to_str().unwrap(), &owner_id, "bidirectional")
        .unwrap();
    share_folder(&server, folder_id, "late-share".into()).unwrap();
    let folder_port = start_server(
        &server,
        0,
        folder_id,
        shared_dir.path().to_str().unwrap().to_string(),
    )
    .await
    .unwrap();

    match browse_peer_shared_folders(&client, "127.0.0.1".into(), folder_port).await {
        ferrisync_core::api::BrowseSharedFoldersResult::Success(folders) => {
            assert!(folders.iter().any(|f| f.name == "late-share"));
        }
        ferrisync_core::api::BrowseSharedFoldersResult::Failure(e) => {
            panic!("paired client could not browse the late share: {e:?}")
        }
    }

    close(&client).await.unwrap();
    close(&server).await.unwrap();
}

#[tokio::test]
async fn pair_defers_folder_choice_to_a_separate_request() {
    let server_data = tempfile::tempdir().unwrap();
    let client_data = tempfile::tempdir().unwrap();
    let shared_dir = tempfile::tempdir().unwrap();
    let replica_dir = tempfile::tempdir().unwrap();

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

    // The owner publishes a share *before* pairing. Device pairing must not
    // silently adopt it: the caller decides which folder to use after browsing.
    let owner_id = server.current_device().id.clone();
    let server_id = owner_id.clone();
    let client_id = client.current_device().id.clone();
    let folder_id = server
        .storage
        .add_sync_folder(
            shared_dir.path().to_str().unwrap(),
            &owner_id,
            "bidirectional",
        )
        .unwrap();
    let share_guid = server.storage.folder_guid(folder_id).unwrap().unwrap();
    share_folder(&server, folder_id, "auto-share".into()).unwrap();
    let folder_port = start_server(
        &server,
        0,
        folder_id,
        shared_dir.path().to_str().unwrap().to_string(),
    )
    .await
    .unwrap();

    let control_port = server.control_port();
    let pair_client = Arc::clone(&client);
    let task = tokio::spawn(async move {
        pair_with_device(&pair_client, "127.0.0.1".into(), control_port).await
    });

    let mut approved = false;
    for _ in 0..150 {
        if pending_pairings(&server)
            .unwrap()
            .iter()
            .any(|(_, id)| id == &client_id)
        {
            approve_pending_pairing(&server, client_id.clone(), "test-client".into()).unwrap();
            approved = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(approved, "control listener never surfaced the pairing request");

    let paired = task.await.unwrap().unwrap();
    assert!(paired.contains(&server_id), "{paired}");
    assert!(!paired.contains("auto-paired"), "{paired}");
    assert!(
        client.storage.list_sync_folders().unwrap().is_empty(),
        "device pairing alone must not create folder rows"
    );

    // Now do what the UI does after a successful handshake: browse, pick the
    // sole share, and request it against the local replica directory.
    match browse_peer_shared_folders(&client, "127.0.0.1".into(), folder_port).await {
        ferrisync_core::api::BrowseSharedFoldersResult::Success(folders) => {
            assert!(folders.iter().any(|f| f.folder_guid == share_guid));
        }
        ferrisync_core::api::BrowseSharedFoldersResult::Failure(e) => {
            panic!("paired client could not browse the share: {e:?}")
        }
    }
    let outcome = request_folder_pairing(
        &client,
        "127.0.0.1".into(),
        folder_port,
        server_id.clone(),
        share_guid.clone(),
        "auto-share".into(),
        replica_dir.path().to_str().unwrap().to_string(),
        5_000,
    )
    .await;
    assert!(
        matches!(outcome, FolderPairRequestOutcome::Approved { .. }),
        "folder request should be approved for a trusted peer: {outcome:?}"
    );

    let folders = list_sync_folders(&client).unwrap();
    let replica = folders
        .iter()
        .find(|f| f.guid.as_deref() == Some(share_guid.as_str()))
        .expect("explicit folder request should register the peer's share");
    assert!(
        replica.peers.iter().any(|p| p.device_id == owner_id),
        "replica should be paired with the owner: {:?}",
        replica.peers
    );

    let owner_pairs = server.storage.folder_pairs(folder_id).unwrap();
    assert!(
        owner_pairs
            .iter()
            .any(|(id, _, _, _)| *id == client_id),
        "owner should have the initiator as a folder pair: {owner_pairs:?}"
    );

    close(&client).await.unwrap();
    close(&server).await.unwrap();
}
