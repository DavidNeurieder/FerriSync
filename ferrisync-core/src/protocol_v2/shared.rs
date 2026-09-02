use serde::{Deserialize, Serialize};

/// Shared-folder discovery/pairing protocol messages, exchanged over the
/// already-authenticated TLS channel (never via mDNS). Following the
/// security model, a peer may only list a device's shared folders *after*
/// device trust has been established by TLS + pinned certificate.
///
/// # String payloads
///
/// - `SharedFolderInfo.folder_guid` is the logical sync-space identity
///   (`sync_folders.folder_guid`), shared across both replicas, independent of
///   the local filesystem path.
/// - `RemoteFolderPair.mode` is the requested grant ("both" this phase;
///   read-only/write permissions are reserved but not yet enforced).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedFolderInfo {
    /// Logical identity of the shared folder (a `sync_folders.folder_guid`).
    pub folder_guid: String,
    /// Human-readable name the owner chose for the share.
    pub name: String,
    /// Requested/offered sync mode: "both" (permissions not enforced yet).
    pub mode: String,
}

impl SharedFolderInfo {
    pub fn new(folder_guid: &str, name: &str, mode: &str) -> Self {
        Self {
            folder_guid: folder_guid.to_string(),
            name: name.to_string(),
            mode: mode.to_string(),
        }
    }
}

/// A folder-pairing relationship (the replica grant), returned on approval.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteFolderPair {
    /// Logical identity of the paired shared folder (sync space).
    pub folder_guid: String,
    /// Folder name the owner exposes (mirror of `SharedFolderInfo.name`).
    pub name: String,
    /// Granted mode. This phase always "bidirectional".
    pub mode: String,
    /// Absolute destination on this peer where the requester keeps its copy.
    /// None = the peer's own registered/default folder path.
    pub remote_path: Option<String>,
}

/// A folder-level pairing request held on the owner, awaiting approval.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteFolderPairRequest {
    /// The requesting (remote) device's id.
    pub device_id: String,
    /// The requesting device's display name (for the owner's approval UI).
    pub device_name: String,
    /// The share being requested.
    pub folder_guid: String,
    /// Human-readable folder name.
    pub name: String,
    /// Requested mode ("both"/"bidirectional").
    pub mode: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_folder_info_roundtrip() {
        let info = SharedFolderInfo::new("f-abc", "Documents", "both");
        let json = serde_json::to_string(&info).unwrap();
        let back: SharedFolderInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(back.folder_guid, "f-abc");
        assert_eq!(back.name, "Documents");
        assert_eq!(back.mode, "both");
    }

    #[test]
    fn remote_folder_pair_roundtrip() {
        let pair = RemoteFolderPair {
            folder_guid: "f-xyz".into(),
            name: "Documents".into(),
            mode: "bidirectional".into(),
            remote_path: Some("/mirror".into()),
        };
        let json = serde_json::to_string(&pair).unwrap();
        let back: RemoteFolderPair = serde_json::from_str(&json).unwrap();
        assert_eq!(back.folder_guid, "f-xyz");
        assert_eq!(back.remote_path.as_deref(), Some("/mirror"));
    }
}