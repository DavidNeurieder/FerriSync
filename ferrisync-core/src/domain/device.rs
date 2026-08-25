use serde::{Deserialize, Serialize};
use std::fmt;

/// Newtype wrapper around a device UUID string.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub struct DeviceId(pub String);

impl fmt::Display for DeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for DeviceId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<String> for DeviceId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for DeviceId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// BLAKE3 hash of a TLS certificate DER encoding.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CertificateFingerprint(pub Vec<u8>);

impl CertificateFingerprint {
    /// Compute the fingerprint from raw certificate DER bytes.
    pub fn from_der(der: &[u8]) -> Self {
        Self(blake3::hash(der).as_bytes().to_vec())
    }

    /// Raw bytes of the blake3 hash.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Display for CertificateFingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for b in &self.0 {
            write!(f, "{:02x}", b)?;
        }
        Ok(())
    }
}

/// A paired device in the system.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Device {
    pub id: DeviceId,
    pub name: String,
    pub fingerprint: Option<CertificateFingerprint>,
    pub last_seen: Option<i64>,
    pub last_addr: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_id_display() {
        let id = DeviceId("abc-123".into());
        assert_eq!(id.to_string(), "abc-123");
        assert_eq!(id.as_ref(), "abc-123");
    }

    #[test]
    fn device_id_from_string() {
        let id = DeviceId::from("test".to_string());
        assert_eq!(id.0, "test");
    }

    #[test]
    fn device_id_from_str() {
        let id: DeviceId = "test".into();
        assert_eq!(id.0, "test");
    }

    #[test]
    fn fingerprint_from_der() {
        let der = b"some certificate data";
        let fp = CertificateFingerprint::from_der(der);
        assert_eq!(fp.0.len(), 32); // blake3 output is 32 bytes
    }

    #[test]
    fn fingerprint_display() {
        let fp = CertificateFingerprint(vec![0u8, 1, 2, 255]);
        assert_eq!(fp.to_string(), "000102ff");
    }

    #[test]
    fn device_roundtrip_serde() {
        let device = Device {
            id: DeviceId("dev-1".into()),
            name: "Laptop".into(),
            fingerprint: Some(CertificateFingerprint(vec![42; 32])),
            last_seen: Some(1000),
            last_addr: Some("192.168.1.5:9847".into()),
        };
        let json = serde_json::to_string(&device).unwrap();
        let back: Device = serde_json::from_str(&json).unwrap();
        assert_eq!(device, back);
    }
}
