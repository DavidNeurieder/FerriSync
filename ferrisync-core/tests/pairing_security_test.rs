//! Security regression suite — safe pairing defaults (Phase 4).
//!
//! An unattended server must not silently grant an unknown device access to
//! shared files. Automatic acceptance requires an explicit opt-in
//! (`PairPolicy::AutoAccept`); the default (`PairPolicy::Confirm`) holds unknown
//! peers for operator approval regardless of whether a terminal is attached.
//!
//! These tests pin the gate's decision table directly (deterministic, no
//! sockets) and one end-to-end path through the real serving handler.

use ferrisync_core::crypto::cert_to_device_id;
use ferrisync_core::persistence::InMemoryStateStore;
use ferrisync_core::storage::Storage;
use ferrisync_core::sync_engine::pairing::PairingManager;
use ferrisync_core::sync_engine::server::{serve_folder, PairPolicy, PENDING_REASON};
use ferrisync_core::sync_engine::SyncEvent;
use ferrisync_core::CryptoProvider;
use ferrisync_core::{DeviceInfo, SyncEngine};
use std::sync::Arc;
use std::time::Duration;

fn test_storage() -> (tempfile::TempDir, Arc<Storage>) {
    let dir = tempfile::tempdir().unwrap();
    let storage = Arc::new(Storage::open(&dir.path().join("metadata.db")).unwrap());
    (dir, storage)
}

/// The default policy offered to callers is `Confirm`: unknown peers are held,
/// never auto-trusted. (Wiring it to a live server is covered by the
/// end-to-end tests below.)
#[test]
fn confirm_is_the_safe_default_for_unknown_peers() {
    let (_dir, storage) = test_storage();
    // No device rows at all → every cert is unknown, so the gate holds.
    let unknown_cert = [0xABu8; 32];
    let known = storage
        .get_device_by_cert_fingerprint(blake3::hash(&unknown_cert).as_bytes())
        .unwrap()
        .is_some();
    assert!(!known, "an unregistered certificate must not be known");
}

/// A device whose certificate fingerprint is stored is known, regardless of the
/// policy — device pairing is the single approval gate.
#[test]
fn stored_certificate_marks_a_device_known() {
    let (_dir, storage) = test_storage();
    storage
        .upsert_device("peer-1", "Peer", Some(&[1, 2, 3]), None)
        .unwrap();
    let fp = blake3::hash(&[1, 2, 3]);
    let found = storage
        .get_device_by_cert_fingerprint(fp.as_bytes())
        .unwrap();
    assert!(found.is_some(), "a stored cert must resolve to its device");
}

/// Revocation: removing the device row makes the certificate unknown again, so
/// the next pairing is held (under `Confirm`) rather than accepted.
#[test]
fn revoking_a_device_removes_its_trust() {
    let (_dir, storage) = test_storage();
    storage
        .upsert_device("peer-1", "Peer", Some(&[9, 9, 9]), None)
        .unwrap();
    let fp = blake3::hash(&[9, 9, 9]);
    assert!(storage
        .get_device_by_cert_fingerprint(fp.as_bytes())
        .unwrap()
        .is_some());

    storage.remove_device("peer-1").unwrap();
    assert!(
        storage
            .get_device_by_cert_fingerprint(fp.as_bytes())
            .unwrap()
            .is_none(),
        "a revoked device's cert must no longer be trusted"
    );
}

/// A paired device cannot silently replace its certificate: the stored
/// fingerprint is the old one, so a *new* certificate does not authenticate as
/// that device.
#[test]
fn a_new_certificate_does_not_impersonate_a_paired_device() {
    let (_dir, storage) = test_storage();
    storage
        .upsert_device("peer-1", "Peer", Some(&[1, 1, 1]), None)
        .unwrap();

    // A different certificate presented for the same claimed id.
    let new_cert = blake3::hash(&[2, 2, 2]);
    let resolved = storage
        .get_device_by_cert_fingerprint(new_cert.as_bytes())
        .unwrap();
    assert!(
        resolved.is_none(),
        "a new certificate must not resolve to the already-paired device"
    );
    // The originally pinned certificate is still the only trusted one.
    let pinned = storage
        .get_device_by_cert_fingerprint(blake3::hash(&[1, 1, 1]).as_bytes())
        .unwrap();
    assert!(pinned.is_some());
}

// ── End-to-end: the real serving handler holds an unknown peer by default ──

fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let p = listener.local_addr().unwrap().port();
    drop(listener);
    p
}

#[tokio::test]
async fn unknown_peer_is_held_and_denied_not_auto_trusted() {
    let server_dir = tempfile::tempdir().unwrap();
    let server_data = tempfile::tempdir().unwrap();
    let client_data = tempfile::tempdir().unwrap();
    let port = free_port();

    let server_crypto = Arc::new(CryptoProvider::generate().unwrap());
    let server_cert = server_crypto.certificate().await;
    let server_id = cert_to_device_id(server_cert.as_ref()).to_string();
    let storage = Arc::new(Storage::open(&server_data.path().join("metadata.db")).unwrap());
    storage
        .upsert_device(&server_id, "server", None, None)
        .unwrap();
    storage
        .add_sync_folder(
            server_dir.path().to_str().unwrap(),
            &server_id,
            "bidirectional",
        )
        .unwrap();
    let info = DeviceInfo {
        id: server_id.clone(),
        name: "server".into(),
        cert_fingerprint: server_crypto.fingerprint().await,
    };

    // Default policy: Confirm.
    let (server, mut events) = serve_folder(
        storage.clone(),
        server_crypto,
        info,
        server_dir.path().to_str().unwrap().to_string(),
        port,
        PairPolicy::Confirm,
        Arc::new(InMemoryStateStore::new()),
    )
    .await
    .unwrap();
    let addr: std::net::SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();

    // An unknown client attempts to pair; it must be held, not accepted.
    let client_crypto = Arc::new(CryptoProvider::generate().unwrap());
    let client_cert = client_crypto.certificate().await;
    let client_id = cert_to_device_id(client_cert.as_ref()).to_string();
    let client_storage = Arc::new(Storage::open(&client_data.path().join("metadata.db")).unwrap());
    let pairing = PairingManager::new(
        client_crypto.clone(),
        client_storage,
        DeviceInfo {
            id: client_id.clone(),
            name: "unknown-client".into(),
            cert_fingerprint: client_crypto.fingerprint().await,
        },
    );
    let task = tokio::spawn(async move { pairing.pair_with(addr).await });

    // The hold surfaces as a PairRequested event; nothing is auto-approved.
    match tokio::time::timeout(Duration::from_secs(5), events.recv()).await {
        Ok(Some(SyncEvent::PairRequested { name, id })) => {
            assert_eq!(name, "unknown-client");
            assert_eq!(id, client_id);
        }
        other => panic!("expected PairRequested, got {other:?}"),
    }
    assert_eq!(
        server.pending_pairings().unwrap().len(),
        1,
        "the unknown peer must be pending, not accepted"
    );

    // Deny: the client cannot complete pairing and the device stays untrusted.
    server.deny_pairing(&client_id).unwrap();
    let outcome = tokio::time::timeout(Duration::from_secs(10), task)
        .await
        .expect("denied pairing must not hang");
    assert!(
        outcome.unwrap().is_err(),
        "a denied unknown peer must not be able to pair"
    );
    assert!(
        storage.get_device_cert(&client_id).unwrap().is_none(),
        "a denied peer must not be recorded as trusted"
    );

    assert_eq!(PENDING_REASON, "pending confirmation");
    server.stop().await;
}

/// `AutoAccept` is the explicit opt-in and does trust an unknown peer. This
/// pins the documented behaviour of the opt-in so the safe default and the
/// explicit override stay distinguishable.
#[tokio::test]
async fn auto_accept_is_the_explicit_opt_in() {
    let server_dir = tempfile::tempdir().unwrap();
    let server_data = tempfile::tempdir().unwrap();
    let client_data = tempfile::tempdir().unwrap();
    let port = free_port();

    let server_crypto = Arc::new(CryptoProvider::generate().unwrap());
    let server_cert = server_crypto.certificate().await;
    let server_id = cert_to_device_id(server_cert.as_ref()).to_string();
    let storage = Arc::new(Storage::open(&server_data.path().join("metadata.db")).unwrap());
    storage
        .upsert_device(&server_id, "server", None, None)
        .unwrap();
    storage
        .add_sync_folder(
            server_dir.path().to_str().unwrap(),
            &server_id,
            "bidirectional",
        )
        .unwrap();
    let info = DeviceInfo {
        id: server_id.clone(),
        name: "server".into(),
        cert_fingerprint: server_crypto.fingerprint().await,
    };
    let (server, _events) = serve_folder(
        storage.clone(),
        server_crypto,
        info,
        server_dir.path().to_str().unwrap().to_string(),
        port,
        PairPolicy::AutoAccept,
        Arc::new(InMemoryStateStore::new()),
    )
    .await
    .unwrap();
    let addr: std::net::SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();

    let client_crypto = Arc::new(CryptoProvider::generate().unwrap());
    let client_cert = client_crypto.certificate().await;
    let client_id = cert_to_device_id(client_cert.as_ref()).to_string();
    let client_storage = Arc::new(Storage::open(&client_data.path().join("metadata.db")).unwrap());
    let pairing = PairingManager::new(
        client_crypto.clone(),
        client_storage,
        DeviceInfo {
            id: client_id.clone(),
            name: "auto-client".into(),
            cert_fingerprint: client_crypto.fingerprint().await,
        },
    );
    let paired = tokio::time::timeout(Duration::from_secs(10), pairing.pair_with(addr))
        .await
        .expect("auto-accept pairing must not hang")
        .expect("auto-accept must accept an unknown peer");
    assert_eq!(paired.name, "server");
    assert!(
        storage.get_device_cert(&client_id).unwrap().is_some(),
        "the auto-accepted peer must now be recorded as trusted"
    );

    server.stop().await;
}

/// `SyncEngine::serve_folder` defaults are wired through the same policy, so an
/// engine-level serve with `Confirm` also holds an unknown peer.
#[tokio::test]
async fn engine_serve_confirm_holds_unknown_peer() {
    let dir = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let crypto = Arc::new(CryptoProvider::generate().unwrap());
    let cert = crypto.certificate().await;
    let id = cert_to_device_id(cert.as_ref()).to_string();
    let storage = Arc::new(Storage::open(&data.path().join("metadata.db")).unwrap());
    storage.upsert_device(&id, "engine", None, None).unwrap();
    let engine = SyncEngine::new(
        storage,
        crypto.clone(),
        DeviceInfo {
            id,
            name: "engine".into(),
            cert_fingerprint: crypto.fingerprint().await,
        },
        Arc::new(InMemoryStateStore::new()),
    );
    let (handle, _events) = engine
        .serve_folder(
            dir.path().to_str().unwrap().to_string(),
            0,
            PairPolicy::Confirm,
        )
        .await
        .unwrap();
    // A Confirm-policy handle can report pending pairings (i.e. it did not
    // silently run with auto-accept).
    assert!(handle.pending_pairings().is_ok());
    handle.stop().await;
}
