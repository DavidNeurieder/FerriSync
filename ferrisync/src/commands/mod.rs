pub mod pair;
pub mod remove;
pub mod rename;
pub mod serve;
pub mod status;
pub mod sync;
pub mod watch;

use anyhow::Context;
use ferrisync_core::storage::Storage;
use std::net::SocketAddr;

use crate::app::ApplicationContext;
use crate::cli::Commands;
use crate::{repl, tui};

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

    let paired: Vec<(String, String)> = storage
        .list_devices()?
        .into_iter()
        .filter(|(id, _, _)| id != own_device_id)
        .map(|(id, name, _)| (id, name))
        .collect();

    if let Some((id, _)) = paired.iter().find(|(id, _)| id == device) {
        let addr = storage.device_last_addr(id)?.and_then(|a| a.parse().ok());
        return Ok((id.clone(), addr));
    }

    let needle = device.to_lowercase();
    let matches: Vec<&(String, String)> = paired
        .iter()
        .filter(|(_, name)| name.to_lowercase().starts_with(&needle))
        .collect();
    match matches.as_slice() {
        [one] => {
            let addr = storage
                .device_last_addr(&one.0)?
                .and_then(|a| a.parse().ok());
            Ok((one.0.clone(), addr))
        }
        [] => {
            anyhow::bail!("unknown device {device:?} — pair it first or use an ip[:port] address")
        }
        many => {
            let names: Vec<&str> = many.iter().map(|(_, n)| n.as_str()).collect();
            anyhow::bail!("ambiguous device {device:?} — matches {}", names.join(", "))
        }
    }
}

/// Read one line from stdin and interpret it as an approval decision.
/// Anything other than y/yes counts as "no" (including EOF).
pub(crate) async fn read_yes_no() -> bool {
    use tokio::io::AsyncBufReadExt;
    let mut line = String::new();
    let _ = tokio::io::BufReader::new(tokio::io::stdin())
        .read_line(&mut line)
        .await;
    matches!(line.trim().to_lowercase().as_str(), "y" | "yes")
}

/// Dispatch a parsed CLI subcommand through the shared application context.
pub async fn run(command: Commands, ctx: ApplicationContext) -> anyhow::Result<()> {
    let ApplicationContext {
        data_dir,
        crypto,
        storage,
        device_info,
        engine,
        pairing,
    } = ctx;

    match command {
        Commands::Pair { ip, port } => pair::run(ip, port, &pairing).await,
        Commands::Sync {
            folder,
            device,
            wait,
        } => {
            sync::run_dispatch(folder, device, wait, storage, crypto, &device_info.id).await
        }
        Commands::Status => status::run(storage, device_info).await,
        Commands::Watch { folder, device } => watch::run(folder, device, storage, crypto).await,
        Commands::Serve {
            port,
            auto_accept,
            folder,
        } => serve::run(folder, port, auto_accept, engine).await,
        Commands::Rename { name } => rename::run(&name, &data_dir).await,
        Commands::Remove { device_id, yes } => remove::run(&device_id, yes, &storage).await,
        Commands::Repl => repl::run(pairing, storage, crypto, device_info, &data_dir).await,
        Commands::Tui => tui::run_tui(engine, pairing, storage, device_info, &data_dir).await,
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
}