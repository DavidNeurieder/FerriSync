//! Security regression suite — TLS identity and version policy (Phase 2).
//!
//! A successful TLS handshake alone must never imply a peer is trusted, and the
//! transport must not be negotiable down from TLS 1.3. These tests cover the
//! version policy and the certificate-replacement / revocation lifecycle at the
//! crypto layer; end-to-end authorization is covered by `authorization_test.rs`.

use ferrisync_core::crypto::{cert_to_device_id, CryptoProvider};
use ferrisync_core::transport::tcp::TcpTransport;
use ferrisync_core::transport::TransportConnector;
use rustls::ProtocolVersion;
use std::sync::Arc;

/// Both peers negotiate TLS 1.3 — never 1.2 — through the production config.
#[tokio::test]
async fn production_handshake_negotiates_tls13() {
    let server_crypto = Arc::new(CryptoProvider::generate().unwrap());
    let server_config = server_crypto.server_config().await.unwrap();
    let acceptor = tokio_rustls::TlsAcceptor::from(server_config);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server_task = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        acceptor.accept(tcp).await
    });

    let client_crypto = Arc::new(CryptoProvider::generate().unwrap());
    let client_config = client_crypto.client_config().await.unwrap();
    let connector = tokio_rustls::TlsConnector::from(client_config);
    let tcp = tokio::net::TcpStream::connect(addr).await.unwrap();
    let server_name = rustls::pki_types::ServerName::try_from("FerriSync").unwrap();
    let client_stream = connector.connect(server_name, tcp).await.unwrap();

    // The client's connection reports the negotiated protocol version.
    let (_, client_conn) = client_stream.get_ref();
    assert_eq!(
        client_conn.protocol_version(),
        Some(ProtocolVersion::TLSv1_3),
        "the production client must negotiate TLS 1.3, not an older version"
    );

    let server_stream = server_task.await.unwrap().unwrap();
    let (_, server_conn) = server_stream.get_ref();
    assert_eq!(
        server_conn.protocol_version(),
        Some(ProtocolVersion::TLSv1_3),
        "the production server must negotiate TLS 1.3"
    );
}

/// A peer that sends a raw TLS 1.2 ClientHello must not complete a handshake:
/// the server is pinned to 1.3, and TLS 1.2 support is not even compiled in.
///
/// This drives the wire directly (rather than relying on a rustls 1.2 config,
/// which the build no longer provides) so the refusal is proven end-to-end.
#[tokio::test]
async fn raw_tls12_clienthello_is_refused() {
    let server_crypto = Arc::new(CryptoProvider::generate().unwrap());
    let acceptor = tokio_rustls::TlsAcceptor::from(server_crypto.server_config().await.unwrap());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let _ = acceptor.accept(tcp).await;
    });

    // A minimal, syntactically valid TLS 1.2 ClientHello record (version
    // 0x0303 in the record and handshake, empty cipher list). A TLS 1.3-only
    // server must reject it rather than negotiate down.
    let client_hello: Vec<u8> = vec![
        0x16, 0x03, 0x03, 0x00, 0x2f, // record: handshake, version TLS 1.2, len 47
        0x01, 0x00, 0x00, 0x2b, // ClientHello, length 43
        0x03, 0x03, // client_version = TLS 1.2
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, // session_id length 0
        0x00, 0x02, 0x00, 0x2f, // cipher_suites: one arbitrary suite
        0x01, 0x00, // compression: null
        0x00, 0x00, // extensions: none
    ];

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut tcp = tokio::net::TcpStream::connect(addr).await.unwrap();
    tcp.write_all(&client_hello).await.unwrap();
    tcp.flush().await.unwrap();

    // The server must reject the 1.2 offer. A TLS alert record
    // (content type 0x15) is the expected refusal; a ServerHello
    // (content type 0x16) would mean the downgrade was accepted. EOF/reset
    // also count as refusal.
    let mut buf = [0u8; 64];
    let outcome = tokio::time::timeout(std::time::Duration::from_secs(5), tcp.read(&mut buf)).await;
    match outcome {
        Ok(Ok(0)) | Ok(Err(_)) => {}
        Ok(Ok(n)) => {
            assert_eq!(
                buf[0], 0x15,
                "server answered a TLS 1.2 ClientHello with content type {:#04x}; expected a TLS alert (0x15)",
                buf[0]
            );
            let _ = n;
        }
        Err(_) => panic!("server neither accepted nor refused a TLS 1.2 ClientHello"),
    }
}

/// The device id is a pure function of the certificate: two different
/// certificates never yield the same id, so an identity cannot be spoofed.
#[tokio::test]
async fn device_id_is_bound_to_the_certificate() {
    let a = CryptoProvider::generate().unwrap();
    let b = CryptoProvider::generate().unwrap();
    let id_a = cert_to_device_id(a.certificate().await.as_ref());
    let id_b = cert_to_device_id(b.certificate().await.as_ref());
    assert_ne!(id_a, id_b);
    // Stable across calls for the same certificate.
    assert_eq!(id_a, cert_to_device_id(a.certificate().await.as_ref()));
}

/// The fingerprint is derived from the presented certificate, not self-declared.
#[tokio::test]
async fn fingerprint_matches_certificate_hash() {
    let crypto = CryptoProvider::generate().unwrap();
    let cert = crypto.certificate().await;
    assert_eq!(
        crypto.fingerprint().await,
        blake3::hash(cert.as_ref()).as_bytes().to_vec(),
        "the advertised fingerprint must be the certificate's BLAKE3 hash"
    );
}

/// The connection reports the peer's actual certificate, which is what
/// authorization keys on (never a self-claimed id in a message).
#[tokio::test]
async fn connection_exposes_the_peer_certificate() {
    let server_crypto = Arc::new(CryptoProvider::generate().unwrap());
    let acceptor = tokio_rustls::TlsAcceptor::from(server_crypto.server_config().await.unwrap());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let _ = acceptor.accept(tcp).await;
    });

    let client_crypto = Arc::new(CryptoProvider::generate().unwrap());
    let transport = TcpTransport::new(client_crypto);
    let conn = transport.connect(addr).await.unwrap();
    let presented = conn
        .peer_cert_der()
        .expect("the server presents a certificate");
    assert_eq!(
        presented,
        server_crypto.certificate().await.as_ref(),
        "the connection must expose the peer's real certificate"
    );
}
