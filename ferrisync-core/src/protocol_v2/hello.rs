use serde::{Deserialize, Serialize};

use crate::DeviceInfo;

/// Protocol version for forward/backward compatibility.
pub const PROTOCOL_VERSION: u32 = 3;

/// Maximum number of folders a single Hello can advertise.
pub const MAX_HELLO_FOLDERS: usize = 256;

/// Maximum number of trusted devices a single Hello can advertise.
pub const MAX_HELLO_DEVICES: usize = 256;

/// Maximum number of folder-pair replicas a single Hello can advertise.
pub const MAX_HELLO_FOLDER_PAIRS: usize = MAX_HELLO_FOLDERS;

/// A trusted device advertised in the Hello message.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HelloDevice {
    pub id: String,
    pub name: String,
    pub last_seen: i64,
}

/// The initial handshake message exchanged between sync peers.
///
/// Both sides send a Hello immediately after TLS handshake completes.
/// Hello carries the device identity, protocol version, and the set
/// of folders the device is offering.
///
/// # Validation rules
///
/// - `protocol_version` must be ≤ `PROTOCOL_VERSION` (we accept older)
/// - `device_id` must be non-empty and ≤ 128 bytes
/// - `folders` must have ≤ `MAX_HELLO_FOLDERS` entries
/// - Each folder entry must have a non-empty name
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hello {
    pub protocol_version: u32,
    pub device_id: String,
    pub device_name: String,
    pub cert_fingerprint: Vec<u8>,
    pub folders: Vec<HelloFolder>,
    #[serde(default)]
    pub trusted_devices: Vec<HelloDevice>,
    #[serde(default)]
    pub revoked_devices: Vec<String>,
    /// Folder replicas this device has paired with the Hello recipient.
    ///
    /// Propagating these on the authenticated sync handshake is what makes a
    /// locally paired folder appear on the peer device too. Older peers omit
    /// the field, so it must remain default-decodable.
    #[serde(default)]
    pub folder_pairs: Vec<HelloFolderPair>,
}

/// One folder↔peer relationship advertised to the peer it belongs to.
///
/// The receiving side uses [`Self::folder_guid`] to find or create its replica
/// of the same logical folder. [`Self::peer_device_id`] prevents a pair meant
/// for a third device from being implanted by a peer that happens to relay it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HelloFolderPair {
    /// Stable logical folder id shared by both replicas.
    pub folder_guid: String,
    /// Human-readable folder name.
    pub name: String,
    /// Sync mode on the advertising device.
    pub mode: String,
    /// The advertising device's local path (the peer's *remote* path).
    pub local_path: String,
    /// Device this pair is intended for.
    pub peer_device_id: String,
    /// Where the recipient keeps its copy, when the advertiser knows it.
    #[serde(default)]
    pub peer_path: Option<String>,
}

/// A folder advertised in the Hello message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HelloFolder {
    /// Unique folder identifier (database ID as string).
    pub id: String,
    /// Human-readable folder name.
    pub name: String,
    /// Sync direction: "push", "pull", or "both".
    pub direction: String,
}

/// Errors that can occur during Hello validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HelloError {
    VersionMismatch { remote: u32, max: u32 },
    EmptyDeviceId,
    DeviceIdTooLong { len: usize, max: usize },
    TooManyFolders { count: usize, max: usize },
    EmptyFolderName { index: usize },
    TooManyFolderPairs { count: usize, max: usize },
    EmptyFolderPairGuid { index: usize },
}

impl std::fmt::Display for HelloError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::VersionMismatch { remote, max } => {
                write!(f, "protocol version {remote} unsupported (max {max})")
            }
            Self::EmptyDeviceId => write!(f, "device_id is empty"),
            Self::DeviceIdTooLong { len, max } => {
                write!(f, "device_id too long: {len} bytes (max {max})")
            }
            Self::TooManyFolders { count, max } => {
                write!(f, "too many folders: {count} (max {max})")
            }
            Self::EmptyFolderName { index } => {
                write!(f, "folder at index {index} has empty name")
            }
            Self::TooManyFolderPairs { count, max } => {
                write!(f, "too many folder pairs: {count} (max {max})")
            }
            Self::EmptyFolderPairGuid { index } => {
                write!(f, "folder pair at index {index} has empty guid")
            }
        }
    }
}

impl std::error::Error for HelloError {}

impl Hello {
    /// Validate a received Hello message.
    ///
    /// Returns Ok(()) if the Hello is acceptable, or Err with the
    /// specific validation failure.
    pub fn validate(&self) -> Result<(), HelloError> {
        if self.protocol_version > PROTOCOL_VERSION {
            return Err(HelloError::VersionMismatch {
                remote: self.protocol_version,
                max: PROTOCOL_VERSION,
            });
        }

        if self.device_id.is_empty() {
            return Err(HelloError::EmptyDeviceId);
        }

        if self.device_id.len() > 128 {
            return Err(HelloError::DeviceIdTooLong {
                len: self.device_id.len(),
                max: 128,
            });
        }

        if self.folders.len() > MAX_HELLO_FOLDERS {
            return Err(HelloError::TooManyFolders {
                count: self.folders.len(),
                max: MAX_HELLO_FOLDERS,
            });
        }

        for (i, folder) in self.folders.iter().enumerate() {
            if folder.name.is_empty() {
                return Err(HelloError::EmptyFolderName { index: i });
            }
        }

        if self.trusted_devices.len() > MAX_HELLO_DEVICES {
            return Err(HelloError::TooManyFolders {
                count: self.trusted_devices.len(),
                max: MAX_HELLO_DEVICES,
            });
        }

        for device in &self.trusted_devices {
            if device.id.is_empty() {
                return Err(HelloError::EmptyDeviceId);
            }
        }

        if self.folder_pairs.len() > MAX_HELLO_FOLDER_PAIRS {
            return Err(HelloError::TooManyFolderPairs {
                count: self.folder_pairs.len(),
                max: MAX_HELLO_FOLDER_PAIRS,
            });
        }
        for (i, pair) in self.folder_pairs.iter().enumerate() {
            if pair.folder_guid.is_empty() {
                return Err(HelloError::EmptyFolderPairGuid { index: i });
            }
        }

        Ok(())
    }

    /// Attach the folder pairs to propagate with this Hello.
    pub fn with_folder_pairs(mut self, pairs: Vec<HelloFolderPair>) -> Self {
        self.folder_pairs = pairs;
        self
    }

    /// Build a Hello message from local device info.
    pub async fn from_device(
        device: &DeviceInfo,
        crypto: &crate::crypto::CryptoProvider,
        folders: Vec<HelloFolder>,
        trusted_devices: Vec<HelloDevice>,
        revoked_devices: Vec<String>,
    ) -> Self {
        Hello {
            protocol_version: PROTOCOL_VERSION,
            device_id: device.id.clone(),
            device_name: device.name.clone(),
            cert_fingerprint: crypto.fingerprint().await,
            folders,
            trusted_devices,
            revoked_devices,
            folder_pairs: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_hello() -> Hello {
        Hello {
            protocol_version: 1,
            device_id: "dev-1".into(),
            device_name: "Test Device".into(),
            cert_fingerprint: vec![0, 1, 2, 3],
            folders: vec![HelloFolder {
                id: "1".into(),
                name: "Documents".into(),
                direction: "both".into(),
            }],
            trusted_devices: vec![],
            revoked_devices: vec![],
            folder_pairs: vec![],
        }
    }

    #[test]
    fn valid_hello_passes_validation() {
        assert!(valid_hello().validate().is_ok());
    }

    #[test]
    fn missing_trusted_devices_defaults_to_empty() {
        // Older peers (protocol v3) omit the field; deserialization must still
        // succeed and default to an empty directory.
        let json = r#"{
            "protocol_version": 3,
            "device_id": "dev-1",
            "device_name": "Test Device",
            "cert_fingerprint": [0,1,2,3],
            "folders": [{"id":"1","name":"Documents","direction":"both"}]
        }"#;
        let hello: Hello = serde_json::from_str(json).unwrap();
        assert!(hello.trusted_devices.is_empty());
        // Folder-pair propagation is likewise additive: old peers omit it.
        assert!(hello.folder_pairs.is_empty());
        assert!(hello.validate().is_ok());
    }

    #[test]
    fn folder_pairs_roundtrip_without_peer_path() {
        let mut h = valid_hello();
        h.folder_pairs = vec![HelloFolderPair {
            folder_guid: "f-guid".into(),
            name: "Projects".into(),
            mode: "bidirectional".into(),
            local_path: "/home/me/Projects".into(),
            peer_device_id: "dev-2".into(),
            peer_path: None,
        }];
        assert!(h.validate().is_ok());
        let json = serde_json::to_string(&h).unwrap();
        // Older peers must be able to read a Hello that lacks `peer_path`.
        let without_peer_path = json.replace(",\"peer_path\":null", "");
        let back: Hello = serde_json::from_str(&without_peer_path).unwrap();
        assert_eq!(back.folder_pairs, h.folder_pairs);
    }

    #[test]
    fn invalid_folder_pairs_rejected() {
        let mut h = valid_hello();
        h.folder_pairs = vec![HelloFolderPair {
            folder_guid: String::new(),
            name: "Projects".into(),
            mode: "bidirectional".into(),
            local_path: "/home/me/Projects".into(),
            peer_device_id: "dev-2".into(),
            peer_path: None,
        }];
        assert_eq!(
            h.validate(),
            Err(HelloError::EmptyFolderPairGuid { index: 0 })
        );

        let mut h = valid_hello();
        h.folder_pairs = (0..MAX_HELLO_FOLDER_PAIRS + 1)
            .map(|i| HelloFolderPair {
                folder_guid: format!("f-{i}"),
                name: "Projects".into(),
                mode: "bidirectional".into(),
                local_path: "/home/me/Projects".into(),
                peer_device_id: "dev-2".into(),
                peer_path: None,
            })
            .collect();
        assert!(matches!(
            h.validate(),
            Err(HelloError::TooManyFolderPairs { .. })
        ));
    }

    #[test]
    fn version_too_high_rejected() {
        let mut h = valid_hello();
        h.protocol_version = 999;
        assert!(matches!(
            h.validate(),
            Err(HelloError::VersionMismatch { .. })
        ));
    }

    #[test]
    fn empty_device_id_rejected() {
        let mut h = valid_hello();
        h.device_id.clear();
        assert_eq!(h.validate(), Err(HelloError::EmptyDeviceId));
    }

    #[test]
    fn device_id_too_long_rejected() {
        let mut h = valid_hello();
        h.device_id = "x".repeat(129);
        assert!(matches!(
            h.validate(),
            Err(HelloError::DeviceIdTooLong { .. })
        ));
    }

    #[test]
    fn too_many_folders_rejected() {
        let mut h = valid_hello();
        h.folders = (0..MAX_HELLO_FOLDERS + 1)
            .map(|i| HelloFolder {
                id: i.to_string(),
                name: format!("Folder {i}"),
                direction: "both".into(),
            })
            .collect();
        assert!(matches!(
            h.validate(),
            Err(HelloError::TooManyFolders { .. })
        ));
    }

    #[test]
    fn empty_folder_name_rejected() {
        let mut h = valid_hello();
        h.folders.push(HelloFolder {
            id: "99".into(),
            name: String::new(),
            direction: "both".into(),
        });
        assert!(matches!(
            h.validate(),
            Err(HelloError::EmptyFolderName { index: 1 })
        ));
    }

    #[test]
    fn older_protocol_version_accepted() {
        let mut h = valid_hello();
        h.protocol_version = 0;
        assert!(h.validate().is_ok());
    }

    #[test]
    fn hello_serde_roundtrip() {
        let h = valid_hello();
        let json = serde_json::to_string(&h).unwrap();
        let back: Hello = serde_json::from_str(&json).unwrap();
        assert_eq!(h.protocol_version, back.protocol_version);
        assert_eq!(h.device_id, back.device_id);
        assert_eq!(h.folders.len(), back.folders.len());
    }
}
