//! Regression: pairing two shared folders must surface both folder pairs on
//! *both* devices, not just on the device that initiated the pairs.
//!
//! Drives the exact API entry points used by the Flutter app: control-listener
//! device pairing, `share_folder`, `add_sync_folder` locally on the requester,
//! `browse_peer_shared_folders`, `request_folder_pairing`, and finally
//! `list_sync_folders` on each side.

use ferrisync_core::api::{
    add_sync_folder, approve_pending_pairing, browse_peer_shared_folders, close,
    init_engine_with_control, list_sync_folders, pair_with_device, pending_pairings,
    request_folder_pairing, share_folder, start_server, sync_folder, ApiState,
    BrowseSharedFoldersResult, FolderPairRequestOutcome,
};
use std::sync::Arc;
use std::time::Duration;

async fn approve_device_pairing(server: &Arc<ApiState>, client_id: &str) {
    for _ in 0..150 {
        if pending_pairings(server)
            .unwrap()
            .iter()
            .any(|(_, id)| id == client_id)
        {
            approve_pending_pairing(server, client_id.to_string(), "requester".into()).unwrap();
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("owner never surfaced the client's device pairing request");
}

fn assert_pair(
    state: &ApiState,
    local_path: &str,
    guid: &str,
    peer_id: &str,
    side: &str,
) {
    let folders = list_sync_folders(state).unwrap();
    let folder = folders
        .iter()
        .find(|f| f.local_path == local_path)
        .unwrap_or_else(|| panic!("{side} missing folder at {local_path}: {folders:?}"));
    assert_eq!(
        folder.guid.as_deref(),
        Some(guid),
        "{side} folder at {local_path} has the wrong guid: {folder:?}"
    );
    assert!(
        folder.peers.iter().any(|p| p.device_id == peer_id),
        "{side} folder at {local_path} is not paired with {peer_id}: {folder:?}"
    );
}

#[tokio::test]
async fn two_shared_folder_pairs_appear_on_both_devices() {
    let owner_data = tempfile::tempdir().unwrap();
    let requester_data = tempfile::tempdir().unwrap();
    let owner_a = tempfile::tempdir().unwrap();
    let owner_b = tempfile::tempdir().unwrap();
    let requester_a = tempfile::tempdir().unwrap();
    let requester_b = tempfile::tempdir().unwrap();

    // Ephemeral control ports keep this test isolated from other engines.
    let owner = Arc::new(
        init_engine_with_control(owner_data.path().to_str().unwrap().to_string(), 0)
            .await
            .unwrap(),
    );
    let requester = Arc::new(
        init_engine_with_control(requester_data.path().to_str().unwrap().to_string(), 0)
            .await
            .unwrap(),
    );

    let owner_id = owner.current_device().id.clone();
    let requester_id = requester.current_device().id.clone();
    let control_port = owner.control_port();
    assert_ne!(control_port, 0);

    let pairing_client = Arc::clone(&requester);
    let pairing = tokio::spawn(async move {
        pair_with_device(&pairing_client, "127.0.0.1".into(), control_port).await
    });
    approve_device_pairing(&owner, &requester_id).await;
    pairing.await.unwrap().unwrap();

    let shares = [
        ("shared-a", owner_a.path(), requester_a.path()),
        ("shared-b", owner_b.path(), requester_b.path()),
    ];
    let mut guid_by_name = Vec::new();
    for (name, owner_path, _requester_path) in shares {
        std::fs::write(owner_path.join(format!("{name}.txt")), name.as_bytes()).unwrap();
        let folder_id = add_sync_folder(
            &owner,
            owner_path.to_str().unwrap().to_string(),
            owner_id.clone(),
            "bidirectional".into(),
        )
        .unwrap();
        share_folder(&owner, folder_id, name.to_string()).unwrap();
        start_server(
            &owner,
            0,
            folder_id,
            owner_path.to_str().unwrap().to_string(),
        )
        .await
        .unwrap();
        let guid = owner
            .storage
            .folder_guid(folder_id)
            .unwrap()
            .expect("shared folder has a guid");
        guid_by_name.push((name.to_string(), guid));
    }

    // The remote-folder browser must list both published shares.
    let browsed = match browse_peer_shared_folders(&requester, "127.0.0.1".into(), control_port)
        .await
    {
        BrowseSharedFoldersResult::Success(folders) => folders,
        BrowseSharedFoldersResult::Failure(e) => panic!("browse failed: {e:?}"),
    };
    for (name, guid) in &guid_by_name {
        assert!(
            browsed.iter().any(|f| &f.folder_guid == guid && &f.name == name),
            "browser did not list share {name} ({guid}): {browsed:?}"
        );
    }

    // Mirror the app: each local folder is registered before it is wired to a
    // remote share, so `wire_folder_pair` must reuse the existing row.
    for (name, _owner_path, requester_path) in shares {
        add_sync_folder(
            &requester,
            requester_path.to_str().unwrap().to_string(),
            requester_id.clone(),
            "bidirectional".into(),
        )
        .unwrap();
        let (_, guid) = guid_by_name
            .iter()
            .find(|(n, _)| n == name)
            .expect("share was registered");
        let outcome = request_folder_pairing(
            &requester,
            "127.0.0.1".into(),
            control_port,
            owner_id.clone(),
            guid.clone(),
            name.to_string(),
            requester_path.to_str().unwrap().to_string(),
            60_000,
        )
        .await;
        match outcome {
            FolderPairRequestOutcome::Approved { folder_guid, .. } => {
                assert_eq!(folder_guid, *guid);
            }
            other => panic!("pairing {name} should be approved, got {other:?}"),
        }
    }

    for (name, guid) in &guid_by_name {
        let requester_path = if name == "shared-a" {
            requester_a.path()
        } else {
            requester_b.path()
        };
        let owner_path = if name == "shared-a" {
            owner_a.path()
        } else {
            owner_b.path()
        };
        assert_pair(
            &requester,
            requester_path.to_str().unwrap(),
            guid,
            &owner_id,
            "requester",
        );
        assert_pair(
            &owner,
            owner_path.to_str().unwrap(),
            guid,
            &requester_id,
            "owner",
        );
    }

    close(&requester).await.unwrap();
    close(&owner).await.unwrap();
}

/// The classic local-pairing surface (`add_sync_folder`) must also mirror each
/// folder relationship to the paired device. The sync Hello is the only
/// authenticated, bidirectional channel available to both apps, so a session
/// over one folder must propagate the whole folder-pair set for that peer.
#[tokio::test]
async fn locally_added_folder_pairs_propagate_on_sync() {
    let first_data = tempfile::tempdir().unwrap();
    let second_data = tempfile::tempdir().unwrap();
    let first_a = tempfile::tempdir().unwrap();
    let first_b = tempfile::tempdir().unwrap();
    let second_dummy = tempfile::tempdir().unwrap();

    let first = Arc::new(
        init_engine_with_control(first_data.path().to_str().unwrap().to_string(), 0)
            .await
            .unwrap(),
    );
    let second = Arc::new(
        init_engine_with_control(second_data.path().to_str().unwrap().to_string(), 0)
            .await
            .unwrap(),
    );

    let first_id = first.current_device().id.clone();
    let second_id = second.current_device().id.clone();
    let control_port = second.control_port();

    let pairing_client = Arc::clone(&first);
    let pairing = tokio::spawn(async move {
        pair_with_device(&pairing_client, "127.0.0.1".into(), control_port).await
    });
    approve_device_pairing(&second, &first_id).await;
    pairing.await.unwrap().unwrap();

    // The second device needs one served folder so the first can open a sync
    // session and exchange Hellos. This folder is not one of the pairs under
    // test; it is only the transport for the handshake.
    let second_folder_id = second
        .storage
        .ensure_folder_by_guid(
            "transport-only-guid",
            second_dummy.path().to_str().unwrap(),
            "transport",
            &second_id,
        )
        .unwrap();
    let second_port = start_server(
        &second,
        0,
        second_folder_id,
        second_dummy.path().to_str().unwrap().to_string(),
    )
    .await
    .unwrap();

    // Pair two different local folders with the same device — the reported
    // failure was that only the first (or none) appeared on the peer.
    let mut first_folders = Vec::new();
    for (label, dir) in [("first", &first_a), ("second", &first_b)] {
        std::fs::write(dir.path().join(format!("{label}.txt")), label.as_bytes()).unwrap();
        let path = dir.path().to_str().unwrap().to_string();
        let folder_id = add_sync_folder(
            &first,
            path.clone(),
            second_id.clone(),
            "bidirectional".into(),
        )
        .unwrap();
        let guid = first.storage.folder_guid(folder_id).unwrap().unwrap();
        first_folders.push((folder_id, path, guid));
    }

    // One authenticated session is enough: the Hello must advertise every
    // folder this device has paired with the peer, not just the served one.
    let (sync_folder_id, sync_path, _) = &first_folders[0];
    sync_folder(
        &first,
        *sync_folder_id,
        sync_path.clone(),
        "127.0.0.1".into(),
        second_port,
        second_id.clone(),
        false,
    )
    .await
    .unwrap_or_else(|e| panic!("sync failed: {e:#}"));

    for (folder_id, path, guid) in &first_folders {
        assert_pair(&first, path, guid, &second_id, "initiator");
        // The peer chooses its own local destination, so identify the mirrored
        // replica by the shared guid rather than the initiator's path.
        let peer_folders = list_sync_folders(&second).unwrap();
        let mirrored = peer_folders
            .iter()
            .find(|f| f.guid.as_deref() == Some(guid.as_str()))
            .unwrap_or_else(|| panic!("peer missing mirrored folder {guid}: {peer_folders:?}"));
        assert!(
            mirrored.peers.iter().any(|p| p.device_id == first_id),
            "peer folder {guid} is not paired with {first_id}: {mirrored:?}"
        );

        // A peer echoing its new replica back in a later Hello must not
        // rewrite our locally configured pair. Doing so would make the next
        // session relocate the peer's served root to the wrong directory.
        assert_eq!(
            first
                .storage
                .folder_pair_remote_path(*folder_id, &second_id)
                .unwrap(),
            None,
            "propagation overwrote the initiator's remote path for {guid}"
        );
    }

    // Repeat the session so the peer's Hello advertises the replicas it just
    // created. The initiator's pairs must remain untouched afterwards.
    let (sync_folder_id, sync_path, _) = &first_folders[0];
    sync_folder(
        &first,
        *sync_folder_id,
        sync_path.clone(),
        "127.0.0.1".into(),
        second_port,
        second_id.clone(),
        false,
    )
    .await
    .unwrap_or_else(|e| panic!("second sync failed: {e:#}"));
    for (folder_id, _, guid) in &first_folders {
        assert_eq!(
            first
                .storage
                .folder_pair_remote_path(*folder_id, &second_id)
                .unwrap(),
            None,
            "echoed Hello overwrote the initiator's remote path for {guid}"
        );
    }

    close(&first).await.unwrap();
    close(&second).await.unwrap();
}
