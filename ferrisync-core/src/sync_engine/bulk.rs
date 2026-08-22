//! Bulk sync: run one-shot sync sessions for every configured sync folder.

use crate::crypto::CryptoProvider;
use crate::storage::Storage;
use crate::sync_engine::session::{self, SyncResult};
use crate::sync_engine::SyncEvent;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::mpsc;

pub const DEFAULT_PORT: u16 = 9847;

/// Result of one folder inside a bulk sync.
pub struct FolderOutcome {
    pub path: String,
    pub device_id: String,
    /// `None` when no address could be resolved (folder was skipped).
    pub addr: Option<SocketAddr>,
    /// `None` when the folder was skipped, otherwise the session result.
    pub result: Option<anyhow::Result<SyncResult>>,
}

/// Resolve the remote address for a folder's device.
///
/// Prefers the address recorded at pairing time (`last_addr`), falling back
/// to parsing the device id itself — legacy CLI rows use "ip[:port]" as the
/// device id.
pub fn resolve_addr(device_id: &str, last_addr: Option<&str>) -> Option<SocketAddr> {
    if let Some(addr) = last_addr {
        if let Ok(parsed) = addr.parse::<SocketAddr>() {
            return Some(parsed);
        }
    }
    if device_id.contains(':') {
        device_id.parse().ok()
    } else {
        format!("{device_id}:{DEFAULT_PORT}").parse().ok()
    }
}

/// Sync every configured sync folder sequentially. A failing folder does not
/// abort the remaining ones; per-folder results are returned in order.
pub async fn sync_all_folders(
    crypto: Arc<CryptoProvider>,
    storage: Arc<Storage>,
    event_tx: mpsc::Sender<SyncEvent>,
) -> anyhow::Result<Vec<FolderOutcome>> {
    let mut outcomes = Vec::new();
    for (id, path, device_id, _direction, _last_sync) in storage.list_sync_folders()? {
        let last = storage.device_last_addr(&device_id)?;
        let addr = resolve_addr(&device_id, last.as_deref());
        if addr.is_none() {
            outcomes.push(FolderOutcome {
                path,
                device_id,
                addr: None,
                result: None,
            });
            continue;
        }
        let addr = addr.expect("checked above");
        let result = session::run_sync_session(
            crypto.clone(),
            storage.clone(),
            &path,
            addr,
            id,
            &device_id,
            event_tx.clone(),
        )
        .await;
        outcomes.push(FolderOutcome {
            path,
            device_id,
            addr: Some(addr),
            result: Some(result),
        });
    }
    Ok(outcomes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn last_addr_wins() {
        assert_eq!(
            resolve_addr("uuid-123", Some("10.0.0.9:7000")),
            "10.0.0.9:7000".parse().ok()
        );
    }

    #[test]
    fn falls_back_to_device_id_as_ip() {
        assert_eq!(
            resolve_addr("192.168.1.5", None),
            "192.168.1.5:9847".parse().ok()
        );
        assert_eq!(
            resolve_addr("192.168.1.5:7000", None),
            "192.168.1.5:7000".parse().ok()
        );
    }

    #[test]
    fn unresolvable_device_yields_none() {
        assert_eq!(
            resolve_addr("b50b3a4f-282d-48fb-b311-8c9e8651cb4c", None),
            None
        );
        assert_eq!(resolve_addr("uuid", Some("not-an-addr")), None);
    }

    #[tokio::test]
    async fn unknown_devices_are_skipped_not_failed() {
        let dir = tempfile::tempdir().unwrap();
        let crypto = Arc::new(CryptoProvider::generate().unwrap());
        let storage = Arc::new(Storage::open(&dir.path().join("metadata.db")).unwrap());
        storage
            .upsert_device("uuid-no-addr", "phone", None, None)
            .unwrap();
        storage
            .add_sync_folder("/does/not/matter", "uuid-no-addr", "bidirectional")
            .unwrap();

        let (event_tx, _event_rx) = mpsc::channel(256);
        let outcomes = sync_all_folders(crypto, storage.clone(), event_tx)
            .await
            .unwrap();

        assert_eq!(outcomes.len(), 1);
        assert!(outcomes[0].addr.is_none());
        assert!(outcomes[0].result.is_none());
    }
}
