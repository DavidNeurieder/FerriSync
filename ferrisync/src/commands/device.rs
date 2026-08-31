use anyhow::Context;
use ferrisync_core::storage::Storage;
use std::net::SocketAddr;

use super::DEFAULT_PORT;

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

/// Resolve a `--device` argument into the storage row key plus, when known,
/// its current address. Accepts:
/// - `ip[:port]` — legacy ip-keyed rows (key is the argument itself),
/// - a paired device's uuid (exact match),
/// - a unique case-insensitive prefix of a paired device's name.
///
/// Our own device row never matches: folders pointing at ourselves are
/// served locally, not sync targets.
pub fn resolve_device_key(
    storage: &Storage,
    device: &str,
    own_device_id: &str,
) -> anyhow::Result<(String, Option<SocketAddr>)> {
    if device.contains(':') || device.parse::<std::net::IpAddr>().is_ok() {
        let addr = parse_device(device, DEFAULT_PORT)?;
        return Ok((device.to_string(), Some(addr)));
    }

    let (id, _name) = resolve_device_id(storage, device, own_device_id)?;
    let addr = storage.device_last_addr(&id)?.and_then(|a| a.parse().ok());
    Ok((id, addr))
}

/// Resolve a `--device` argument for `watch`, requiring a reachable address.
/// Like `resolve_device_key`, this accepts a paired name, UUID, or ip[:port],
/// and mirrors `sync`'s legacy-ip-row handling so the folder row can be keyed
/// by the resolved device. Returns the resolved storage key and address.
pub fn resolve_watch_target(
    storage: &Storage,
    device: &str,
    own_device_id: &str,
) -> anyhow::Result<(String, SocketAddr)> {
    let (row_device, resolved) = resolve_device_key(storage, device, own_device_id)?;
    if row_device == device {
        ensure_device(storage, &row_device)?;
    }
    let addr = resolved.ok_or_else(|| {
        anyhow::anyhow!(
            "{device} is not reachable yet — run `ferrisync devices pair <ip>`, \
             or open FerriSync on it so its address is recorded"
        )
    })?;
    Ok((row_device, addr))
}

/// Resolve a device argument (exact id, or unique case-insensitive prefix of
/// a display name) into its `(id, name)`. Never matches our own device row.
pub fn resolve_device_id(
    storage: &Storage,
    device: &str,
    own_device_id: &str,
) -> anyhow::Result<(String, String)> {
    let paired: Vec<(String, String)> = storage
        .list_devices()?
        .into_iter()
        .filter(|(id, _, _)| id != own_device_id)
        .map(|(id, name, _)| (id, name))
        .collect();

    if let Some((id, name)) = paired.iter().find(|(id, _)| id == device) {
        return Ok((id.clone(), name.clone()));
    }

    let needle = device.to_lowercase();
    let matches: Vec<&(String, String)> = paired
        .iter()
        .filter(|(_, name)| name.to_lowercase().starts_with(&needle))
        .collect();
    match matches.as_slice() {
        [one] => Ok((one.0.clone(), one.1.clone())),
        [] => {
            anyhow::bail!("unknown device {device:?} — pair it first or use an ip[:port] address")
        }
        many => {
            let names: Vec<&str> = many.iter().map(|(_, n)| n.as_str()).collect();
            anyhow::bail!("ambiguous device {device:?} — matches {}", names.join(", "))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (tempfile::TempDir, Storage, String) {
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::open(&dir.path().join("metadata.db")).unwrap();
        storage
            .upsert_device("self-uuid", "cachyos-x8664", None, None)
            .unwrap();
        storage
            .upsert_device("uuid-phone", "localhost", None, Some("192.168.178.70:9847"))
            .unwrap();
        storage
            .upsert_device("uuid-laptop", "laptop", None, None)
            .unwrap();
        (dir, storage, "self-uuid".to_string())
    }

    #[test]
    fn ip_arguments_are_passed_through() {
        let (_dir, storage, own) = fixture();
        let (key, addr) = resolve_device_key(&storage, "10.1.2.3:7000", &own).unwrap();
        assert_eq!(key, "10.1.2.3:7000");
        assert_eq!(addr, "10.1.2.3:7000".parse().ok());
    }

    #[test]
    fn bare_ip_gets_default_port() {
        let (_dir, storage, own) = fixture();
        let (_, addr) = resolve_device_key(&storage, "10.1.2.3", &own).unwrap();
        assert_eq!(addr, "10.1.2.3:9847".parse().ok());
    }

    #[test]
    fn exact_uuid_uses_recorded_address() {
        let (_dir, storage, own) = fixture();
        let (key, addr) = resolve_device_key(&storage, "uuid-phone", &own).unwrap();
        assert_eq!(key, "uuid-phone");
        assert_eq!(addr, "192.168.178.70:9847".parse().ok());
    }

    #[test]
    fn unique_name_prefix_resolves_case_insensitively() {
        let (_dir, storage, own) = fixture();
        let (key, addr) = resolve_device_key(&storage, "LOCA", &own).unwrap();
        assert_eq!(key, "uuid-phone");
        assert_eq!(addr, "192.168.178.70:9847".parse().ok());
    }

    #[test]
    fn ambiguous_prefix_lists_candidates() {
        let (_dir, storage, own) = fixture();
        storage
            .upsert_device("uuid-c2", "localbox", None, None)
            .unwrap();
        let err = resolve_device_key(&storage, "loc", &own).unwrap_err();
        assert!(format!("{err:#}").contains("ambiguous"), "{err:#}");
    }

    #[test]
    fn unknown_name_is_rejected() {
        let (_dir, storage, own) = fixture();
        assert!(resolve_device_key(&storage, "nope", &own).is_err());
    }

    #[test]
    fn own_uuid_never_matches() {
        let (_dir, storage, own) = fixture();
        assert!(resolve_device_key(&storage, "self-uuid", &own).is_err());
    }

    #[test]
    fn watch_by_name_uses_recorded_address() {
        let (_dir, storage, own) = fixture();
        storage
            .upsert_device("uuid-desktop", "Desktop", None, Some("192.168.1.20:9847"))
            .unwrap();
        let (key, addr) = resolve_watch_target(&storage, "desk", &own).unwrap();
        assert_eq!(key, "uuid-desktop");
        assert_eq!(addr, "192.168.1.20:9847".parse().ok().unwrap());
    }

    #[test]
    fn watch_by_uuid_uses_recorded_address() {
        let (_dir, storage, own) = fixture();
        let (key, addr) = resolve_watch_target(&storage, "uuid-phone", &own).unwrap();
        assert_eq!(key, "uuid-phone");
        assert_eq!(addr, "192.168.178.70:9847".parse().ok().unwrap());
    }

    #[test]
    fn watch_by_ip_passes_through() {
        let (_dir, storage, own) = fixture();
        let (key, addr) = resolve_watch_target(&storage, "10.1.2.3", &own).unwrap();
        assert_eq!(key, "10.1.2.3");
        assert_eq!(addr, "10.1.2.3:9847".parse().ok().unwrap());
    }

    #[test]
    fn watch_unknown_device_is_rejected() {
        let (_dir, storage, own) = fixture();
        assert!(resolve_watch_target(&storage, "nope", &own).is_err());
    }

    #[test]
    fn watch_name_without_address_is_rejected() {
        let (_dir, storage, own) = fixture();
        let err = resolve_watch_target(&storage, "laptop", &own).unwrap_err();
        assert!(
            format!("{err:#}").contains("not reachable"),
            "unexpected error: {err:#}"
        );
    }
}
