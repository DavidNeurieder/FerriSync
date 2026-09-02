//! Client-side RPC for browsing and pairing to a remote device's *shared
//! folders*.
//!
//! This is a post-trust surface: it runs over TLS with a pinned peer cert and
//! is *only* shown after the device has paired. It is intentionally separate
//! from mDNS (which advertises `id`-only and never folder names) and from the
//! single-folder `Hello.folders` used for the classic one-folder flow.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};

use crate::crypto::CryptoProvider;
use crate::protocol::{frame_message, SyncMessage, MAX_CONTROL_FRAME};
use crate::protocol_v2::shared::{RemoteFolderPair, RemoteFolderPairRequest, SharedFolderInfo};
use crate::transport::tcp::TcpTransport;
use crate::transport::{TransportConnection, TransportConnector};

/// Result of driving a folder-pairing request over a single authenticated
/// session against the serving (owner) device.
#[derive(Debug, Clone)]
pub enum FolderPairReply {
    /// Owner approved; the grant carries the remote share path.
    Approved(RemoteFolderPair),
    /// Owner explicitly denied it.
    Rejected(String),
    /// Held for owner approval; requester should poll again later.
    Pending,
}

/// One-shot authenticated RPC connection to a peer's serving device, used to
/// enumerate its shared folders and to request/collect folder pairing.
pub struct SharedFolderClient {
    crypto: Arc<CryptoProvider>,
    addr: SocketAddr,
}

impl SharedFolderClient {
    /// `addr` is the peer's serving address (typically recorded from a prior
    /// device pairing / sync). The peer is expected to already be a known,
    /// paired device so the TLS client accepts its pinned cert (the crypto
    /// provider's client config embeds the paired-device trust).
    pub fn new(crypto: Arc<CryptoProvider>, addr: SocketAddr) -> Self {
        Self { crypto, addr }
    }

    async fn open(&self) -> Result<Box<dyn TransportConnection>> {
        let transport = TcpTransport::new(self.crypto.clone());
        transport
            .connect(self.addr)
            .await
            .context("connect for shared-folder RPC")
    }

    /// List the peer's discoverable shared folders. Returns `None` when the
    /// peer does not understand the request (older protocol peer).
    pub async fn list_shared_folders(&self) -> Result<Vec<SharedFolderInfo>> {
        let mut conn = self.open().await?;
        conn.write_all(&frame_message(&SyncMessage::ListSharedFolders)?)
            .await
            .context("send ListSharedFolders")?;
        let msg = read_frame(&mut *conn).await?;
        match msg {
            SyncMessage::SharedFolders(folders) => Ok(folders),
            other => Err(anyhow!("unexpected reply to ListSharedFolders: {other:?}")),
        }
    }

    /// Send a folder-pairing request and, when the owner holds it for
    /// approval, poll until a grant (or rejection) arrives. The poll loop
    /// reconnects so the in-memory `PairGate` can serve the approval grant on
    /// a fresh session.
    ///
    /// `lifetime` bounds the total time spent polling; `None` retries
    /// indefinitely.
    pub async fn request_and_collect_pairing(
        &self,
        device_id: &str,
        device_name: &str,
        folder_guid: &str,
        name: &str,
        lifetime: Option<Duration>,
    ) -> Result<FolderPairReply> {
        let deadline = lifetime.map(|d| tokio::time::Instant::now() + d);
        let req = RemoteFolderPairRequest {
            device_id: device_id.to_string(),
            device_name: device_name.to_string(),
            folder_guid: folder_guid.to_string(),
            name: name.to_string(),
            mode: "bidirectional".into(),
        };
        loop {
            let reply = self.single_request(req.clone()).await?;
            match reply {
                FolderPairReply::Approved(_) | FolderPairReply::Rejected(_) => return Ok(reply),
                FolderPairReply::Pending => {
                    if let Some(d) = deadline {
                        if tokio::time::Instant::now() >= d {
                            return Ok(FolderPairReply::Pending);
                        }
                    }
                    tokio::time::sleep(Duration::from_millis(1500)).await;
                }
            }
        }
    }

    /// One request/response round on a single session.
    async fn single_request(&self, req: RemoteFolderPairRequest) -> Result<FolderPairReply> {
        let mut conn = self.open().await?;
        conn.write_all(&frame_message(&SyncMessage::RequestFolderPair(req))?)
            .await
            .context("send RequestFolderPair")?;
        let msg = read_frame(&mut *conn).await?;
        match msg {
            SyncMessage::FolderPairApproved(grant) => Ok(FolderPairReply::Approved(grant)),
            SyncMessage::FolderPairRejected(reason) => Ok(FolderPairReply::Rejected(reason)),
            SyncMessage::FolderPairPending => Ok(FolderPairReply::Pending),
            other => Err(anyhow!("unexpected reply to RequestFolderPair: {other:?}")),
        }
    }
}

/// Read one framed [`SyncMessage`] from an established connection.
async fn read_frame(conn: &mut dyn TransportConnection) -> Result<SyncMessage> {
    let mut len_buf = [0u8; 4];
    read_exact(conn, &mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > MAX_CONTROL_FRAME {
        anyhow::bail!("frame too large: {len} bytes (max {MAX_CONTROL_FRAME})");
    }
    let mut payload = vec![0u8; len];
    read_exact(conn, &mut payload).await?;
    Ok(bincode::deserialize(&payload)?)
}

/// Read until `buf` is filled (the transport exposes a plain `read`).
async fn read_exact(conn: &mut dyn TransportConnection, mut buf: &mut [u8]) -> Result<()> {
    while !buf.is_empty() {
        let n = conn.read(buf).await?;
        if n == 0 {
            anyhow::bail!("peer closed connection mid-frame");
        }
        buf = &mut buf[n..];
    }
    Ok(())
}