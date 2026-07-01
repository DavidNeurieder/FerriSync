pub mod pairing;
pub mod session;
mod index_builder;

use crate::crypto::CryptoProvider;
use crate::protocol::{
    frame_message, parse_frame, Ack, FileRequest, Index, IndexEntry, SyncMessage,
};
use crate::storage::Storage;
use crate::transport::tcp::TcpTransport;
use crate::transport::TransportConnector;
use crate::DeviceInfo;
use anyhow::Result;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

#[derive(Debug, Clone)]
pub enum SyncEvent {
    Syncing { folder_id: String },
    Idle,
    Error { message: String },
    FilePulled { path: String, device: String },
    FilePushed { path: String, device: String },
    Conflict { path: String, winner: String, loser: String },
}

/// Core sync engine that orchestrates index exchange, diff, and transfer.
#[derive(Debug)]
pub struct SyncEngine {
    storage: Arc<Storage>,
    crypto: Arc<CryptoProvider>,
    #[allow(dead_code)]
    device_info: DeviceInfo,
    event_tx: mpsc::Sender<SyncEvent>,
    event_rx: Mutex<mpsc::Receiver<SyncEvent>>,
}

impl SyncEngine {
    pub fn new(
        storage: Arc<Storage>,
        crypto: Arc<CryptoProvider>,
        device_info: DeviceInfo,
    ) -> Self {
        let (tx, rx) = mpsc::channel(256);
        Self {
            storage,
            crypto,
            device_info,
            event_tx: tx,
            event_rx: Mutex::new(rx),
        }
    }

    pub fn event_sender(&self) -> mpsc::Sender<SyncEvent> {
        self.event_tx.clone()
    }

    pub async fn events(&self) -> mpsc::Receiver<SyncEvent> {
        let (tx, rx) = mpsc::channel(256);
        let mut old_rx = self.event_rx.lock().await;
        while let Ok(event) = old_rx.try_recv() {
            let _ = tx.send(event).await;
        }
        rx
    }

    /// Full sync loop: connect, exchange indices, transfer files.
    pub async fn sync_folder(
        &self,
        local_folder_id: i64,
        local_path: &str,
        remote_addr: std::net::SocketAddr,
        remote_device_id: &str,
    ) -> Result<()> {
        let _ = self
            .event_tx
            .send(SyncEvent::Syncing {
                folder_id: local_folder_id.to_string(),
            })
            .await;

        // Build local index
        let local_index =
            index_builder::build_index(local_folder_id.to_string(), PathBuf::from(local_path).as_path())?;

        // Connect to remote
        let transport = TcpTransport::new(self.crypto.clone());
        let mut conn = transport.connect(remote_addr).await?;

        // Send our index
        let msg = SyncMessage::Index(local_index);
        let framed = frame_message(&msg)?;
        conn.write_all(&framed).await?;

        // Read their index
        let mut buf = vec![0u8; 1024 * 1024];
        let n = conn.read(&mut buf).await?;
        let (response, _) = parse_frame(&buf[..n])?;

        let remote_index = match response {
            SyncMessage::Index(idx) => idx,
            other => anyhow::bail!("expected Index, got {:?}", std::mem::discriminant(&other)),
        };

        // Compute what we need to pull
        let to_pull = self
            .reconcile_index(local_folder_id, &remote_index, remote_device_id)
            .await?;

        // Request and pull files we need
        if !to_pull.is_empty() {
            let paths: Vec<String> = to_pull.iter().map(|e| e.path.clone()).collect();
            let req = SyncMessage::FileRequest(FileRequest {
                folder_id: local_folder_id.to_string(),
                paths: paths.clone(),
            });
            let framed = frame_message(&req)?;
            conn.write_all(&framed).await?;

            // Receive file chunks
            for _path in &paths {
                let mut file_buf = Vec::new();
                loop {
                    let n = conn.read(&mut buf).await?;
                    if n == 0 {
                        break;
                    }
                    let (chunk_msg, consumed) = parse_frame(&buf[..n])?;
                    match chunk_msg {
                        SyncMessage::FileChunk(chunk) => {
                            let offset = chunk.offset as usize;
                            let end = offset + chunk.data.len();
                            if end > file_buf.len() {
                                file_buf.resize(end, 0);
                            }
                            file_buf[offset..end].copy_from_slice(&chunk.data);

                            if chunk.offset as u64 + chunk.data.len() as u64 >= chunk.total_size {
                                // Complete file received
                                let target = PathBuf::from(local_path).join(&chunk.path);
                                if let Some(parent) = target.parent() {
                                    tokio::fs::create_dir_all(parent).await?;
                                }
                                tokio::fs::write(&target, &file_buf).await?;
                                file_buf.clear();

                                // Send ack
                                let ack = SyncMessage::Ack(Ack {
                                    path: chunk.path.clone(),
                                    success: true,
                                    error: None,
                                });
                                let framed = frame_message(&ack)?;
                                conn.write_all(&framed).await?;

                                let _ = self
                                    .event_tx
                                    .send(SyncEvent::FilePulled {
                                        path: chunk.path,
                                        device: remote_device_id.to_string(),
                                    })
                                    .await;
                                break;
                            }
                            // Shift remaining data in buf
                            let remaining = n - consumed;
                            if remaining > 0 {
                                buf.copy_within(consumed..n, 0);
                                // Need to read more - for simplicity, continue
                            }
                        }
                        _ => {
                            break;
                        }
                    }
                }
            }
        }

        // Read file requests from remote (they will request after receiving our index)
        // This is simplified - a real implementation would handle both directions

        let _ = self.event_tx.send(SyncEvent::Idle).await;
        Ok(())
    }

    /// Process an incoming index from a remote peer.
    /// Computes the diff and returns files that need to be pulled.
    pub async fn reconcile_index(
        &self,
        local_folder_id: i64,
        remote_index: &Index,
        _device_id: &str,
    ) -> Result<Vec<IndexEntry>> {
        let mut to_pull = Vec::new();

        for remote_entry in &remote_index.entries {
            let local_entry = self
                .storage
                .get_file_metadata(local_folder_id, &remote_entry.path)?;

            match local_entry {
                None => {
                    to_pull.push(remote_entry.clone());
                }
                Some(local) => {
                    if local.hash != remote_entry.hash {
                        if remote_entry.mtime > local.mtime {
                            to_pull.push(remote_entry.clone());
                        }
                    }
                }
            }
        }

        Ok(to_pull)
    }
}
