use serde::{Deserialize, Serialize};

/// Protocol version for forward/backward compatibility.
pub const PROTOCOL_VERSION: u32 = 1;

/// Maximum number of folders a single Hello can advertise.
pub const MAX_HELLO_FOLDERS: usize = 256;

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

        Ok(())
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
        }
    }

    #[test]
    fn valid_hello_passes_validation() {
        assert!(valid_hello().validate().is_ok());
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
        assert!(matches!(h.validate(), Err(HelloError::DeviceIdTooLong { .. })));
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
        assert!(matches!(h.validate(), Err(HelloError::TooManyFolders { .. })));
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
