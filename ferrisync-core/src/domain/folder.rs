use serde::{Deserialize, Serialize};
use std::fmt;

/// Newtype wrapper around a SQLite autoincrement folder row ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub struct FolderId(pub i64);

impl fmt::Display for FolderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<i64> for FolderId {
    fn from(id: i64) -> Self {
        Self(id)
    }
}

/// A locally-configured sync folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Folder {
    pub id: FolderId,
    pub local_path: String,
    pub device_id: String,
    pub direction: String,
    pub last_sync_at: Option<i64>,
}

/// Direction of file flow for a sync folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncDirection {
    Bidirectional,
    PushOnly,
    PullOnly,
}

impl SyncDirection {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Bidirectional => "bidirectional",
            Self::PushOnly => "push",
            Self::PullOnly => "pull",
        }
    }
}

impl fmt::Display for SyncDirection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for SyncDirection {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "bidirectional" => Ok(Self::Bidirectional),
            "push" => Ok(Self::PushOnly),
            "pull" => Ok(Self::PullOnly),
            _ => anyhow::bail!("unknown sync direction: {s}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_id_display() {
        let id = FolderId(42);
        assert_eq!(id.to_string(), "42");
    }

    #[test]
    fn direction_roundtrip() {
        for dir in [
            SyncDirection::Bidirectional,
            SyncDirection::PushOnly,
            SyncDirection::PullOnly,
        ] {
            let s = dir.to_string();
            let parsed: SyncDirection = s.parse().unwrap();
            assert_eq!(dir, parsed);
        }
    }

    #[test]
    fn direction_invalid() {
        assert!("invalid".parse::<SyncDirection>().is_err());
    }
}
