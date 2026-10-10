//! Coverage for alternate-endpoint selection: an unreachable or wrong-identity
//! candidate must never prevent FerriSync from using the advertised address
//! that actually authenticates as the paired device.
//!
//! A TCP connection succeeding is not evidence of identity, so these tests spin
//! up real TLS servers with known certificates and verify that
//! `first_authenticated_candidate` matches on the certificate-derived device id.

use ferrisync_core::crypto::cert_to_device_id;
use ferrisync_core::sync_engine::bulk::{first_authenticated_candidate, IDENTITY_PROBE_TIMEOUT};
use ferrisync_core::CryptoProvider;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

/// Spawn a TLS server that completes the handshake with `crypto`'s identity and
/// then holds the connection open. The client only needs the presented leaf
/// certificate to authenticate the peer.
async fn spawn_tls_identity_server(crypto: Arc<CryptoProvider>) -> SocketAddr {
    use tokio::net::TcpListener;
    use tokio_rustls::TlsAcceptor;

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let config = crypto.server_config().await.unwrap();
    let acceptor = TlsAcceptor::from(config);
    tokio::spawn(async move {
        loop {
            let Ok((tcp, _)) = listener.accept().await else {
                break;
            };
            let acceptor = acceptor.clone();
            tokio::spawn(async move {
                // Complete the handshake, then keep the stream alive until the
                // client drops it.
                let _ = acceptor.accept(tcp).await;
            });
        }
    });
    addr
}

/// A loopback address with nothing listening on it.
fn unused_addr() -> SocketAddr {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    addr
}

#[tokio::test]
async fn unreachable_and_wrong_identity_candidates_are_skipped() {
    let peer = Arc::new(CryptoProvider::generate().unwrap());
    let peer_id = cert_to_device_id(&peer.certificate().await);

    // A different device that *is* reachable but does not authenticate as the
    // peer, plus a dead address ahead of the real one.
    let impostor = Arc::new(CryptoProvider::generate().unwrap());
    let impostor_addr = spawn_tls_identity_server(impostor).await;
    let peer_addr = spawn_tls_identity_server(peer.clone()).await;

    let candidates = vec![unused_addr(), impostor_addr, peer_addr];
    let chosen =
        first_authenticated_candidate(peer, &peer_id, &candidates, Duration::from_millis(750))
            .await;

    assert_eq!(
        chosen,
        Some(peer_addr),
        "must skip the dead and the wrong-identity candidates and pick the authenticated one"
    );
}

#[tokio::test]
async fn all_candidates_failing_returns_none() {
    let peer = Arc::new(CryptoProvider::generate().unwrap());
    let peer_id = cert_to_device_id(&peer.certificate().await);

    let impostor = Arc::new(CryptoProvider::generate().unwrap());
    let impostor_addr = spawn_tls_identity_server(impostor).await;

    let chosen = first_authenticated_candidate(
        peer,
        &peer_id,
        &[unused_addr(), impostor_addr],
        Duration::from_millis(500),
    )
    .await;
    assert_eq!(chosen, None);
}

#[tokio::test]
async fn empty_candidate_list_is_a_noop() {
    let peer = Arc::new(CryptoProvider::generate().unwrap());
    let peer_id = cert_to_device_id(&peer.certificate().await);
    assert_eq!(
        first_authenticated_candidate(peer, &peer_id, &[], IDENTITY_PROBE_TIMEOUT).await,
        None
    );
}
