use anyhow::Result;
use std::sync::Arc;

use crate::domain::{DeviceId, FolderId};
use crate::persistence::StateStore;

/// Permission level for a device accessing a folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Permission {
    /// No access.
    Denied,
    /// Read-only access (can pull but not push).
    ReadOnly,
    /// Full read/write access.
    ReadWrite,
}

/// Checks and manages per-folder device authorization.
///
/// Currently all paired devices have ReadWrite access to all folders.
/// This module provides the hook for future per-folder ACLs without
/// changing callers.
pub struct FolderAuthorizer {
    #[allow(dead_code)]
    store: Arc<dyn StateStore>,
}

impl FolderAuthorizer {
    pub fn new(store: Arc<dyn StateStore>) -> Self {
        Self { store }
    }

    /// Check if a device is authorized to access a folder.
    ///
    /// Current policy: any known (paired) device has ReadWrite access.
    /// Unknown devices are Denied.
    pub async fn check_permission(
        &self,
        device: &DeviceId,
        _folder: FolderId,
    ) -> Result<Permission> {
        // Current policy: paired = full access
        let is_known = self.store.get_device(device).await?.is_some();
        Ok(if is_known {
            Permission::ReadWrite
        } else {
            Permission::Denied
        })
    }

    /// Check if a device can read from a folder.
    pub async fn can_read(&self, device: &DeviceId, folder: FolderId) -> Result<bool> {
        Ok(self.check_permission(device, folder).await? >= Permission::ReadOnly)
    }

    /// Check if a device can write to a folder.
    pub async fn can_write(&self, device: &DeviceId, folder: FolderId) -> Result<bool> {
        Ok(self.check_permission(device, folder).await? >= Permission::ReadWrite)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::InMemoryStateStore;

    #[tokio::test]
    async fn unknown_device_denied() {
        let store = Arc::new(InMemoryStateStore::new());
        let auth = FolderAuthorizer::new(store);
        let perm = auth
            .check_permission(&DeviceId("unknown".into()), FolderId(1))
            .await
            .unwrap();
        assert_eq!(perm, Permission::Denied);
        assert!(!auth
            .can_read(&DeviceId("unknown".into()), FolderId(1))
            .await
            .unwrap());
        assert!(!auth
            .can_write(&DeviceId("unknown".into()), FolderId(1))
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn paired_device_full_access() {
        let store = Arc::new(InMemoryStateStore::new());
        store
            .upsert_device(&crate::domain::Device {
                id: DeviceId("dev-1".into()),
                name: "Test".into(),
                fingerprint: None,
                last_seen: None,
                last_addr: None,
            })
            .await
            .unwrap();
        let auth = FolderAuthorizer::new(store);
        let perm = auth
            .check_permission(&DeviceId("dev-1".into()), FolderId(1))
            .await
            .unwrap();
        assert_eq!(perm, Permission::ReadWrite);
        assert!(auth
            .can_read(&DeviceId("dev-1".into()), FolderId(1))
            .await
            .unwrap());
        assert!(auth
            .can_write(&DeviceId("dev-1".into()), FolderId(1))
            .await
            .unwrap());
    }
}
