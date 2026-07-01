use crate::crypto::CryptoProvider;
use crate::protocol::{frame_message, parse_frame, PairRequest, PairResponse, SyncMessage};
use crate::storage::Storage;
use crate::transport::tcp::TcpTransport;
use crate::transport::TransportConnector;
use crate::DeviceInfo;
use anyhow::Result;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// Manages device pairing (TOFU-based).
#[derive(Debug)]
pub struct PairingManager {
    crypto: Arc<CryptoProvider>,
    storage: Arc<Storage>,
    device_info: DeviceInfo,
}

impl PairingManager {
    pub fn new(crypto: Arc<CryptoProvider>, storage: Arc<Storage>, device_info: DeviceInfo) -> Self {
        Self {
            crypto,
            storage,
            device_info,
        }
    }

    /// Initiate pairing with a remote device at the given address.
    pub async fn pair_with(&self, addr: SocketAddr) -> Result<DeviceInfo> {
        let transport = TcpTransport::new(self.crypto.clone());
        let mut conn = transport.connect(addr).await?;

        // Send our pairing request
        let req = SyncMessage::PairRequest(PairRequest {
            device_id: self.device_info.id.to_string(),
            device_name: self.device_info.name.clone(),
            cert_fingerprint: self.device_info.cert_fingerprint.clone(),
        });
        let framed = frame_message(&req)?;
        conn.write_all(&framed).await?;

        // Read response
        let mut buf = vec![0u8; 4096];
        let n = conn.read(&mut buf).await?;
        let (response, _) = parse_frame(&buf[..n])?;

        match response {
            SyncMessage::PairResponse(resp) if resp.accepted => {
                let peer_info = DeviceInfo {
                    id: resp.device_id.clone(),
                    name: resp.device_name,
                    cert_fingerprint: resp.cert_fingerprint,
                };
                self.storage
                    .upsert_device(&peer_info.id, &peer_info.name, None)?;
                Ok(peer_info)
            }
            SyncMessage::PairResponse(resp) => {
                anyhow::bail!(
                    "pairing rejected: {}",
                    resp.reason.unwrap_or_default()
                );
            }
            _ => anyhow::bail!("unexpected response during pairing"),
        }
    }

    /// Listen for incoming pairing requests.
    pub async fn listen(&self, addr: SocketAddr) -> Result<()> {
        let listener = TcpListener::bind(addr).await?;
        let crypto = self.crypto.clone();
        let storage = self.storage.clone();
        let device_info = self.device_info.clone();

        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((tcp, _)) => {
                        let crypto = crypto.clone();
                        let storage = storage.clone();
                        let device_info = device_info.clone();

                        tokio::spawn(async move {
                            let config = crypto.server_config().await.unwrap();
                            let acceptor = tokio_rustls::TlsAcceptor::from(config);
                            let mut tls = match acceptor.accept(tcp).await {
                                Ok(tls) => tokio_rustls::TlsStream::Server(tls),
                                Err(e) => {
                                    log::error!("TLS accept failed: {e}");
                                    return;
                                }
                            };

                            // Read pairing request
                            let mut buf = vec![0u8; 4096];
                            let n = tls.read(&mut buf).await.unwrap_or(0);
                            if n == 0 {
                                return;
                            }

                            let (msg, _) = parse_frame(&buf[..n]).unwrap();
                            match msg {
                                SyncMessage::PairRequest(req) => {
                                    let accepted = true;
                                    let resp = SyncMessage::PairResponse(PairResponse {
                                        accepted,
                                        device_id: device_info.id.to_string(),
                                        device_name: device_info.name.clone(),
                                        cert_fingerprint: device_info.cert_fingerprint.clone(),
                                        reason: None,
                                    });
                                    let framed = frame_message(&resp).unwrap();
                                    let _ = tls.write_all(&framed).await;

                                    if accepted {
                                        let _ = storage.upsert_device(
                                            &req.device_id,
                                            &req.device_name,
                                            None,
                                        );
                                        log::info!("Paired with {} ({})", req.device_name, req.device_id);
                                    }
                                }
                                _ => {
                                    log::warn!("unexpected message during pair listen");
                                }
                            }
                        });
                    }
                    Err(e) => {
                        log::error!("accept error: {e}");
                    }
                }
            }
        });

        Ok(())
    }
}
