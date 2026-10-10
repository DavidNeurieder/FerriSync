//! Reproduction for the Flutter receiving-app bug: after pairing to a peer's
//! shared folder, the receiving side's `list_sync_folders` must surface the
//! wired replica (this is what the app's Folders screen renders).
//!
//! Drives everything through the exact API entry points the app uses:
//! `init_engine` on both devices, owner `share_folder` + `start_server`,
//! requester `pair_with_device`, then `request_folder_pairing` and finally
//! `list_sync_folders`.

use ferrisync_core::api::{
    approve_pending_pairing, browse_peer_shared_folders, init_engine, list_sync_folders,
    pair_with_device, pending_pairings, request_folder_pairing, share_folder, start_server,
    stop_server,
};
use std::time::Duration;

#[tokio::test]
async fn receiving_app_surfaces_the_wired_folder() {
    let owner_dir = tempfile::tempdir().unwrap();
    let owner_data = tempfile::tempdir().unwrap();
    let requester_dir = tempfile::tempdir().unwrap();
    let requester_data = tempfile::tempdir().unwrap();
    let req_local_path = requester_dir.path().to_str().unwrap().to_string();

    // ── Owner app: init, add folder, publish share, serve ──
    let owner = init_engine(owner_data.path().to_str().unwrap().to_string())
        .await
        .unwrap();
    let owner_id = owner.current_device().id.clone();
    let owner_folder_id = owner
        .storage
        .add_sync_folder(
            owner_dir.path().to_str().unwrap(),
            &owner_id,
            "bidirectional",
        )
        .unwrap();
    let share_guid = owner
        .storage
        .folder_guid(owner_folder_id)
        .unwrap()
        .expect("owner folder has a guid");
    share_folder(&owner, owner_folder_id, "shared-notes".into()).unwrap();
    // Bind port 0 and read the actual assigned port, so the test never races
    // with a helper that reserves-and-releases an ephemeral port.
    let port = start_server(
        &owner,
        0,
        owner_folder_id,
        owner_dir.path().to_str().unwrap().to_string(),
    )
    .await
    .unwrap();
    assert_ne!(port, 0);

    // ── Requester app: init, device-pair (becomes trusted), then request ──
    //
    // The owner serves every folder with PairPolicy::Confirm (as the app does),
    // so the requester's *device* pairing is held for the host to approve before
    // it is trusted. Mirror that: pair in the background, approve on the owner,
    // then let the requester continue.
    let requester = std::sync::Arc::new(
        init_engine(requester_data.path().to_str().unwrap().to_string())
            .await
            .unwrap(),
    );
    let requester_id = requester.current_device().id.clone();
    let pair_requester = std::sync::Arc::clone(&requester);
    let pair_task =
        tokio::spawn(
            async move { pair_with_device(&pair_requester, "127.0.0.1".into(), port).await },
        );

    let mut approved = false;
    for _ in 0..150 {
        if let Ok(pending) = pending_pairings(&owner) {
            if pending.iter().any(|(_, id)| id == &requester_id) {
                approve_pending_pairing(&owner, requester_id.clone(), "requester-device".into())
                    .unwrap();
                approved = true;
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(approved, "owner never saw the requester's device pairing");
    pair_task.await.unwrap().unwrap();

    // ── The Android app's "remote folders" list: browse the owner's server ──
    // (Mirrors `remoteFoldersFor` in the Flutter provider.)
    let remote = match browse_peer_shared_folders(&requester, "127.0.0.1".into(), port).await {
        ferrisync_core::api::BrowseSharedFoldersResult::Success(folders) => folders,
        ferrisync_core::api::BrowseSharedFoldersResult::Failure(e) => {
            panic!("browse should reach the owner: {e:?}")
        }
    };
    let shared = remote
        .iter()
        .find(|f| f.folder_guid == share_guid)
        .expect("owner's share should appear in the browser's remote folders");
    assert_eq!(shared.name, "shared-notes");

    let result = request_folder_pairing(
        &requester,
        "127.0.0.1".into(),
        port,
        owner_id.clone(),
        share_guid.clone(),
        "shared-notes".into(),
        req_local_path.clone(),
        60000,
    )
    .await;

    match &result {
        ferrisync_core::api::FolderPairRequestOutcome::Approved { folder_guid, .. } => {
            assert_eq!(folder_guid, &share_guid);
        }
        other => panic!("expected Approved, got {other:?}"),
    }

    // ── The receiving app's Folders screen must now show the wired replica ──
    let folders = list_sync_folders(&requester).unwrap();
    let wired = folders
        .iter()
        .find(|f| f.local_path == req_local_path)
        .expect("receiving app should have the wired folder in its Folders list");
    assert_eq!(wired.guid.as_deref(), Some(share_guid.as_str()));
    assert!(
        wired.peers.iter().any(|p| p.device_id == owner_id),
        "wired folder should list the owner as a peer"
    );

    owner_cleanup(&owner).await;
}

async fn owner_cleanup(owner: &ferrisync_core::api::ApiState) {
    stop_server(owner).await.ok();
}
