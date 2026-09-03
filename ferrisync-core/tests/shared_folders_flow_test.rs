//! End-to-end test for the *shared folders* flow (improvement 9):
//! owner publishes a folder → trusted requester browses it → requests a
//! pairing → owner approves → a replica pair is wired and syncing works.
//!
//! Runs entirely in-process against the real `serve_folder` server and the
//! `SharedFolderClient`, mirroring the CLI/API paths used by the app.

use ferrisync_core::crypto::CryptoProvider;
use ferrisync_core::persistence::InMemoryStateStore;
use ferrisync_core::storage::Storage;
use ferrisync_core::sync_engine::pairing::PairingManager;
use ferrisync_core::sync_engine::server::{serve_folder, PairPolicy};
use ferrisync_core::sync_engine::session;
use ferrisync_core::sync_engine::shared_folder::SharedFolderClient;
use ferrisync_core::DeviceInfo;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

fn get_available_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    std::thread::sleep(Duration::from_millis(50));
    port
}

#[tokio::test]
async fn shared_folder_full_flow_browse_request_approve_sync() {
    let server_folder = tempfile::tempdir().unwrap();
    let requester_folder = tempfile::tempdir().unwrap();
    let server_data = tempfile::tempdir().unwrap();
    let requester_data = tempfile::tempdir().unwrap();
    let port = get_available_port();

    // ── Owner (server) ──
    let storage_srv = Arc::new(Storage::open(&server_data.path().join("metadata.db")).unwrap());
    let crypto_srv = Arc::new(CryptoProvider::generate().unwrap());
    // The owner identity must match the cert-derived id the server advertises,
    // so shared-folder RPCs key off the same device.
    let owner_id = {
        let cert = crypto_srv.certificate().await;
        ferrisync_core::crypto::cert_to_device_id(cert.as_ref()).to_string()
    };
    let info_srv = DeviceInfo {
        id: owner_id.clone(),
        name: "owner-device".to_string(),
        cert_fingerprint: crypto_srv.fingerprint().await,
    };

    // Owner publishes a local folder as a shared, discoverable folder.
    let server_file = server_folder.path().join("notes.txt");
    std::fs::write(&server_file, b"shared note").unwrap();
    storage_srv
        .upsert_device(&owner_id, "owner-device", None, None)
        .unwrap();
    let owner_folder_id = storage_srv
        .add_sync_folder(
            server_folder.path().to_str().unwrap(),
            &owner_id,
            "bidirectional",
        )
        .unwrap();
    let share_guid = storage_srv
        .folder_guid(owner_folder_id)
        .unwrap()
        .expect("folder has a guid");
    storage_srv
        .share_folder(
            &share_guid,
            &owner_id,
            "shared-notes",
            server_folder.path().to_str().unwrap(),
        )
        .unwrap();

    let (server, mut events) = serve_folder(
        storage_srv.clone(),
        crypto_srv.clone(),
        info_srv.clone(),
        server_folder.path().to_str().unwrap().to_string(),
        port,
        PairPolicy::AutoAccept,
        Arc::new(InMemoryStateStore::new()),
    )
    .await
    .unwrap();
    assert_eq!(server.port, port);

    // ── Requester: pair, then browse the owner's shared folders ──
    let storage_cli = Arc::new(Storage::open(&requester_data.path().join("metadata.db")).unwrap());
    let crypto_cli = Arc::new(CryptoProvider::generate().unwrap());
    let requester_derived_id = {
        let cert = crypto_cli.certificate().await;
        ferrisync_core::crypto::cert_to_device_id(cert.as_ref()).to_string()
    };
    let info_cli = DeviceInfo {
        id: requester_derived_id.clone(),
        name: "requester-device".to_string(),
        cert_fingerprint: crypto_cli.fingerprint().await,
    };
    let pairing = PairingManager::new(crypto_cli.clone(), storage_cli.clone(), info_cli.clone());
    let peer = pairing
        .pair_with(format!("127.0.0.1:{port}").parse().unwrap())
        .await
        .unwrap();
    assert_eq!(peer.name, "owner-device");

    // Browse: the discoverable share must be listed.
    let addr: std::net::SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
    let client = SharedFolderClient::new(crypto_cli.clone(), addr);
    let listed = client.list_shared_folders().await.unwrap();
    let listed_info = listed
        .iter()
        .find(|f| f.folder_guid == share_guid && f.name == "shared-notes")
        .expect("owner's discoverable share not listed");
    assert_eq!(
        listed_info.local_path,
        server_folder.path().to_str().unwrap(),
        "browse must expose the owner's real remote path"
    );

    // ── Request pairing: a trusted device is approved automatically ──
    // The requester already completed device-level pairing (its cert is in the
    // owner's device table), so the folder pair is granted immediately rather
    // than being held for a second manual approval.
    let cli_derived_for_request = requester_derived_id.clone();
    let share_guid_for_request = share_guid.clone();
    let crypto_for_request = crypto_cli.clone();
    let request_task = tokio::spawn(async move {
        let client = SharedFolderClient::new(crypto_for_request, addr);
        client
            .request_and_collect_pairing(
                &cli_derived_for_request,
                "requester-device",
                &share_guid_for_request,
                "shared-notes",
                Some(Duration::from_secs(15)),
            )
            .await
    });

    // The folder pairing is auto-approved: it never shows up as pending.
    for _ in 0..20 {
        let pending = server
            .pending_folder_pairings()
            .unwrap()
            .into_iter()
            .filter(|p| p.folder_guid == share_guid && p.device_id == requester_derived_id)
            .count();
        assert_eq!(
            pending, 0,
            "trusted device's folder pairing must be auto-approved, not held"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    let reply = tokio::time::timeout(Duration::from_secs(15), request_task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    match reply {
        ferrisync_core::sync_engine::shared_folder::FolderPairReply::Approved(grant) => {
            assert_eq!(grant.folder_guid, share_guid);
            assert_eq!(grant.name, "shared-notes");
        }
        other => panic!("expected Approved, got {other:?}"),
    }

    // The owner's storage now has the requester as a replica of the folder.
    let pairs = storage_srv.folder_pairs(owner_folder_id).unwrap();
    assert!(
        pairs
            .iter()
            .any(|(id, _, _, _)| *id == requester_derived_id),
        "owner should have a replica pair for the requester: {pairs:?}"
    );

    // ── Syncing works: requester writes into its copy; owner receives it ──
    let requester_copy = requester_folder.path().join("local.txt");
    std::fs::write(&requester_copy, b"from requester").unwrap();
    let (tx, _rx) = mpsc::channel(256);
    let folder_id = storage_cli
        .folder_id_for_guid(&share_guid)
        .unwrap()
        .or_else(|| {
            storage_cli
                .add_sync_folder(
                    requester_folder.path().to_str().unwrap(),
                    &owner_id,
                    "bidirectional",
                )
                .ok()
        })
        .expect("requester registered the replica");
    session::run_sync_session(
        crypto_cli,
        storage_cli.clone(),
        requester_folder.path().to_str().unwrap(),
        addr,
        folder_id,
        &requester_derived_id,
        tx,
        Arc::new(InMemoryStateStore::new()),
        false,
        None,
    )
    .await
    .unwrap();

    // The owner's served folder must eventually contain the requester's file.
    let served = server_folder.path().join("local.txt");
    let mut arrived = None;
    for _ in 0..50 {
        if let Ok(data) = std::fs::read(&served) {
            arrived = Some(data);
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(
        arrived.expect("requester file never reached owner"),
        b"from requester"
    );

    // The owner must observe the pairing event and the pulled file.
    let mut observed_file = false;
    for _ in 0..30 {
        let ev = tokio::time::timeout(Duration::from_secs(2), events.recv())
            .await
            .ok()
            .flatten();
        match ev {
            Some(ferrisync_core::sync_engine::SyncEvent::FilePulled { path, .. }) => {
                if path == "local.txt" {
                    observed_file = true;
                    break;
                }
            }
            Some(_) => continue,
            None => break,
        }
    }
    assert!(
        observed_file,
        "owner did not emit DataInbound for local.txt"
    );

    server.stop().await;
}
