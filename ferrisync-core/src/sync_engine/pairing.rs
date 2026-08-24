use crate::crypto::CryptoProvider;
use crate::protocol::{frame_message, parse_frame, PairRequest, PairResponse, SyncMessage};
use crate::storage::Storage;
use crate::transport::tcp::TcpTransport;
use crate::transport::TransportConnector;
use crate::DeviceInfo;
use anyhow::bail;
use anyhow::Context;
use anyhow::Result;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::RwLock;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::time::timeout;

/// How long to wait for the peer's pairing response before giving up.
const PAIR_RESPONSE_TIMEOUT: Duration = Duration::from_secs(5);
/// Delay between pairing retries while the host holds us for confirmation.
const PAIR_RETRY_INTERVAL: Duration = Duration::from_secs(3);
/// Total time `pair_with` keeps retrying before surfacing "awaiting approval".
const PAIR_TOTAL_BUDGET: Duration = Duration::from_secs(60);

/// Manages device pairing (TOFU-based).
#[derive(Debug)]
pub struct PairingManager {
    crypto: Arc<CryptoProvider>,
    storage: Arc<Storage>,
    /// Behind a lock so a device rename propagates to in-flight pairing
    /// without rebuilding the manager.
    device_info: RwLock<DeviceInfo>,
}

impl PairingManager {
    pub fn new(
        crypto: Arc<CryptoProvider>,
        storage: Arc<Storage>,
        device_info: DeviceInfo,
    ) -> Self {
        Self {
            crypto,
            storage,
            device_info: RwLock::new(device_info),
        }
    }

    /// Update our advertised name for future pairing exchanges.
    pub fn set_name(&self, name: &str) {
        self.device_info.write().unwrap().name = name.to_string();
    }

    /// Snapshot of the current identity used in pairing.
    pub fn current_device(&self) -> DeviceInfo {
        self.device_info.read().unwrap().clone()
    }

    /// Initiate pairing with a remote device at the given address.
    ///
    /// If the host holds unknown devices for confirmation
    /// ([`crate::sync_engine::server::PairPolicy::Confirm`]), this keeps
    /// re-requesting every [`PAIR_RETRY_INTERVAL`] for up to
    /// [`PAIR_TOTAL_BUDGET`] while the operator decides.
    pub async fn pair_with(&self, addr: SocketAddr) -> Result<DeviceInfo> {
        let deadline = tokio::time::Instant::now() + PAIR_TOTAL_BUDGET;
        loop {
            match self.pair_attempt(addr).await {
                Err(e) if e.to_string().contains(super::server::PENDING_REASON) => {
                    if tokio::time::Instant::now() >= deadline {
                        bail!(
                            "pairing awaiting host approval (timed out after {}s)",
                            PAIR_TOTAL_BUDGET.as_secs()
                        );
                    }
                    tokio::time::sleep(PAIR_RETRY_INTERVAL).await;
                }
                other => return other,
            }
        }
    }

    async fn pair_attempt(&self, addr: SocketAddr) -> Result<DeviceInfo> {
        let transport = TcpTransport::new(self.crypto.clone());
        let mut conn = transport.connect(addr).await?;

        // Send our pairing request
        let own = self.current_device();
        let req = SyncMessage::PairRequest(PairRequest {
            device_id: own.id.to_string(),
            device_name: own.name.clone(),
            cert_fingerprint: own.cert_fingerprint.clone(),
        });
        let framed = frame_message(&req)?;
        conn.write_all(&framed).await?;

        // Read response
        let mut buf = vec![0u8; 4096];
        let n = timeout(PAIR_RESPONSE_TIMEOUT, conn.read(&mut buf))
            .await
            .context("timed out waiting for pairing response")??;
        let (response, _) = parse_frame(&buf[..n])?;

        match response {
            SyncMessage::PairResponse(resp) if resp.accepted => {
                let peer_info = DeviceInfo {
                    id: resp.device_id.clone(),
                    name: resp.device_name,
                    cert_fingerprint: resp.cert_fingerprint,
                };
                self.storage.upsert_device(
                    &peer_info.id,
                    &peer_info.name,
                    None,
                    Some(&addr.to_string()),
                )?;
                Ok(peer_info)
            }
            SyncMessage::PairResponse(resp) => {
                anyhow::bail!("pairing rejected: {}", resp.reason.unwrap_or_default());
            }
            _ => anyhow::bail!("unexpected response during pairing"),
        }
    }

    /// Listen for incoming pairing requests.
    pub async fn listen(&self, addr: SocketAddr) -> Result<()> {
        let listener = TcpListener::bind(addr).await?;
        let crypto = self.crypto.clone();
        let storage = self.storage.clone();
        let device_info = self.current_device();

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
                                            None,
                                        );
                                        log::info!(
                                            "Paired with {} ({})",
                                            req.device_name,
                                            req.device_id
                                        );
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
