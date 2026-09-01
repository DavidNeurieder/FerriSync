use anyhow::Result;
use std::sync::Arc;

use crate::domain::{CertificateFingerprint, Device, DeviceId};
use crate::persistence::StateStore;

/// Verifies peer identity using TOFU (Trust On First Use) certificate pinning.
///
/// The flow is:
/// 1. Peer presents a TLS certificate
/// 2. Compute blake3 fingerprint of the DER-encoded certificate
/// 3. Look up the fingerprint in the device store
/// 4. If found → return the associated DeviceId (authenticated)
/// 5. If not found → return None (unknown peer)
///
/// First-use trust is handled separately by [`trust_first_use`].
pub struct IdentityVerifier {
    store: Arc<dyn StateStore>,
}

impl IdentityVerifier {
    pub fn new(store: Arc<dyn StateStore>) -> Self {
        Self { store }
    }

    /// Verify a peer's TLS certificate against stored device identities.
    ///
    /// Returns `Ok(Some(device_id))` if the certificate matches a known device.
    /// Returns `Ok(None)` if the certificate is not from any known device.
    pub async fn verify_peer(&self, cert_der: &[u8]) -> Result<Option<DeviceId>> {
        let fingerprint = CertificateFingerprint::from_der(cert_der);
        self.store
            .get_device_by_cert_fingerprint(&fingerprint)
            .await
    }

    /// Register a new device's certificate on first connection (TOFU).
    ///
    /// This stores the certificate for future verification. If the device
    /// already exists, the certificate is updated only if different.
    pub async fn trust_first_use(&self, device_id: &DeviceId, cert_der: &[u8]) -> Result<()> {
        self.store.set_device_cert(device_id, cert_der).await
    }

    /// Verify that a presented certificate matches the expected device.
    ///
    /// This is the full TOFU verification flow:
    /// 1. Check if we have a stored cert for this device
    /// 2. If yes, verify the presented cert matches
    /// 3. If no, store it (first-use trust)
    ///
    /// Returns `Ok(true)` if verification passed or cert was stored.
    /// Returns `Ok(false)` if the cert doesn't match (identity changed).
    /// Returns `Err` on storage failures.
    pub async fn verify_or_trust(&self, device_id: &DeviceId, cert_der: &[u8]) -> Result<bool> {
        if let Some(stored_der) = self.store.get_device_cert(device_id).await? {
            Ok(cert_der == stored_der)
        } else {
            self.trust_first_use(device_id, cert_der).await?;
            Ok(true)
        }
    }

    /// Get a device's stored certificate.
    pub async fn get_device_cert(&self, device_id: &DeviceId) -> Result<Option<Vec<u8>>> {
        self.store.get_device_cert(device_id).await
    }

    /// Get the device record.
    pub async fn get_device(&self, device_id: &DeviceId) -> Result<Option<Device>> {
        self.store.get_device(device_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::InMemoryStateStore;

    fn setup() -> (Arc<InMemoryStateStore>, IdentityVerifier) {
        let store = Arc::new(InMemoryStateStore::new());
        let verifier = IdentityVerifier::new(store.clone());
        (store, verifier)
    }

    #[tokio::test]
    async fn verify_unknown_returns_none() {
        let (_, verifier) = setup();
        let result = verifier.verify_peer(b"unknown cert").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn trust_and_verify() {
        let (_, verifier) = setup();
        let device_id = DeviceId("dev-1".into());
        let cert = b"device certificate";

        // First use — stores the cert
        verifier.trust_first_use(&device_id, cert).await.unwrap();

        // Now verify — should match
        let result = verifier.verify_peer(cert).await.unwrap();
        assert_eq!(result, Some(device_id));
    }

    #[tokio::test]
    async fn verify_rejects_wrong_cert() {
        let (store, verifier) = setup();
        let device_id = DeviceId("dev-1".into());

        // Register with cert A
        store
            .upsert_device(&Device {
                id: device_id.clone(),
                name: "Test".into(),
                fingerprint: Some(CertificateFingerprint::from_der(b"cert A")),
                last_seen: None,
                last_addr: None,
            })
            .await
            .unwrap();

        // Verify with cert B — should not match
        let result = verifier.verify_peer(b"cert B").await.unwrap();
        assert!(result.is_none());

        // Verify with cert A — should match
        let result = verifier.verify_peer(b"cert A").await.unwrap();
        assert_eq!(result, Some(device_id));
    }

    #[tokio::test]
    async fn verify_or_trust_first_time() {
        let (_, verifier) = setup();
        let device_id = DeviceId("dev-1".into());

        let ok = verifier
            .verify_or_trust(&device_id, b"my cert")
            .await
            .unwrap();
        assert!(ok);

        // Now it should verify
        let result = verifier.verify_peer(b"my cert").await.unwrap();
        assert_eq!(result, Some(device_id));
    }

    #[tokio::test]
    async fn verify_or_trust_rejects_changed_cert() {
        let (store, verifier) = setup();
        let device_id = DeviceId("dev-1".into());

        // Store original cert
        store
            .upsert_device(&Device {
                id: device_id.clone(),
                name: "Test".into(),
                fingerprint: Some(CertificateFingerprint::from_der(b"original")),
                last_seen: None,
                last_addr: None,
            })
            .await
            .unwrap();

        // Different cert should fail
        let ok = verifier
            .verify_or_trust(&device_id, b"different")
            .await
            .unwrap();
        assert!(!ok);
    }
}
