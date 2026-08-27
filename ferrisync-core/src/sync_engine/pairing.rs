use crate::crypto::CryptoProvider;
use crate::protocol::{frame_message, parse_frame, PairRequest, SyncMessage};
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
                let peer_cert = conn.peer_cert_der();
                // Derive the peer's authoritative identity from its TLS
                // certificate, not from the self-claimed resp.device_id.
                // The certificate is the cryptographic binding; the claimed
                // id is only used as the display name fallback.
                let peer_id = peer_cert
                    .as_deref()
                    .map(crate::crypto::cert_to_device_id)
                    .unwrap_or_else(|| resp.device_id.clone());
                let peer_info = DeviceInfo {
                    id: peer_id,
                    name: resp.device_name,
                    cert_fingerprint: resp.cert_fingerprint,
                };
                self.storage.upsert_device(
                    &peer_info.id,
                    &peer_info.name,
                    peer_cert.as_deref(),
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
}
