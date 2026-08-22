pub mod pair;
pub mod status;
pub mod sync;
pub mod watch;

use anyhow::Context;
use ferrisync_core::storage::Storage;
use std::net::SocketAddr;

pub const DEFAULT_PORT: u16 = 9847;

/// Make sure a device row exists so `sync_folders` can reference it.
pub fn ensure_device(storage: &Storage, device_id: &str) -> anyhow::Result<()> {
    storage.upsert_device(device_id, device_id, None, None)
}

/// Parse a device argument that is either an IP or an IP:port.
pub fn parse_device(device: &str, default_port: u16) -> anyhow::Result<SocketAddr> {
    if device.contains(':') {
        device
            .parse()
            .with_context(|| format!("invalid device address {device}"))
    } else {
        format!("{device}:{default_port}")
            .parse()
            .with_context(|| format!("invalid device address {device}"))
    }
}
