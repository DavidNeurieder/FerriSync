//! Lightweight, on-device diagnostics for `ferrisync doctor`.
//!
//! Each check is a small, self-contained probe of one subsystem. Checks are
//! either *deterministic* (data dir, storage, identity, pairings, folder
//! presence — everything testable without touching the network) or *network*
//! (port bind, firewall self-connect, mDNS browse — annotated `#[ignore]` in
//! tests because they depend on the host).

use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::crypto::CryptoProvider;
use crate::storage::Storage;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    /// Everything is fine.
    Pass,
    /// A real problem needs fixing.
    Fail,
    /// Not necessarily broken, but worth a look.
    Warn,
    /// Just information.
    Info,
}

impl CheckStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            CheckStatus::Pass => "pass",
            CheckStatus::Fail => "fail",
            CheckStatus::Warn => "warn",
            CheckStatus::Info => "info",
        }
    }
}

/// One diagnostic finding. `name` is the stable identifier used by
/// `ferrisync doctor --explain <name>`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticCheck {
    pub name: String,
    pub status: CheckStatus,
    pub message: String,
    pub hints: Vec<String>,
}

/// Everything a doctor run needs. Kept as plain references so both the CLI
/// (which owns an `ApplicationContext`) and the Flutter bridge can drive it.
pub struct DiagnosticsInput<'a> {
    pub data_dir: &'a Path,
    pub crypto: &'a CryptoProvider,
    pub storage: &'a Storage,
    pub own_id: &'a str,
    pub own_name: &'a str,
    /// The port this device advertises/serves on (default 9847).
    pub serve_port: u16,
}

/// Run every check. Network probes (port, firewall, mDNS) run on the current
/// tokio runtime; deterministic checks are all synchronous.
pub async fn run_all(input: DiagnosticsInput<'_>) -> Vec<DiagnosticCheck> {
    let mut checks = Vec::new();
    checks.push(check_data_dir(input.data_dir));
    checks.push(check_storage(input.storage));
    checks.push(check_identity(input.crypto, input.own_id, input.own_name).await);
    checks.push(check_pairings(input.storage));
    checks.push(check_folders(input.storage));
    checks.push(check_network_interface());
    checks.push(check_port_bind(input.serve_port).await);
    checks.push(check_firewall(input.serve_port).await);
    checks.push(check_mdns(input.own_id, input.serve_port).await);
    checks
}

// ── deterministic checks ──

fn check_data_dir(data_dir: &Path) -> DiagnosticCheck {
    let probe = data_dir.join(".doctor-write-test");
    let ok = std::fs::create_dir_all(data_dir).is_ok()
        && std::fs::write(&probe, "ok").is_ok()
        && std::fs::remove_file(&probe).is_ok();
    DiagnosticCheck {
        name: "data_dir".into(),
        status: if ok {
            CheckStatus::Pass
        } else {
            CheckStatus::Fail
        },
        message: format!(
            "data directory {} is {}",
            data_dir.display(),
            if ok { "writable" } else { "NOT writable" }
        ),
        hints: if ok {
            vec![]
        } else {
            vec![
                format!("Create or fix permissions on {}", data_dir.display()),
                "Ensure the directory is owned by your user and on a filesystem that permits writes.".into(),
            ]
        },
    }
}

fn check_storage(storage: &Storage) -> DiagnosticCheck {
    match storage.list_sync_folders() {
        Ok(folders) => DiagnosticCheck {
            name: "storage".into(),
            status: CheckStatus::Pass,
            message: format!(
                "metadata database is readable ({} sync folders)",
                folders.len()
            ),
            hints: vec![],
        },
        Err(e) => DiagnosticCheck {
            name: "storage".into(),
            status: CheckStatus::Fail,
            message: format!("metadata database error: {e:#}"),
            hints: vec![
                "The metadata.db may be corrupt or held by another process.".into(),
                "If needed, ensure only one FerriSync frontend is running, then re-run doctor."
                    .into(),
            ],
        },
    }
}

async fn check_identity(crypto: &CryptoProvider, own_id: &str, own_name: &str) -> DiagnosticCheck {
    // Verifies the key parses and matches the cert, and that the device id
    // still derives from the certificate (so persisted pairings stay valid).
    let cert_config = crypto.client_config().await;
    let cert = crypto.certificate().await;
    let derived = crate::crypto::cert_to_device_id(cert.as_ref());
    let ok = cert_config.is_ok() && derived == own_id && !own_id.is_empty();
    DiagnosticCheck {
        name: "identity".into(),
        status: if ok {
            CheckStatus::Pass
        } else {
            CheckStatus::Fail
        },
        message: format!(
            "device identity {} ({})",
            own_name,
            if ok { "is valid" } else { "is INVALID" }
        ),
        hints: if ok {
            vec![]
        } else {
            vec![
                "The identity certificate/key are missing, unreadable or mismatched.".into(),
                format!("Device id: {own_id} (derived from certificate: {derived})"),
                "Re-run pairing if the certificate was ever replaced.".into(),
            ]
        },
    }
}

fn check_pairings(storage: &Storage) -> DiagnosticCheck {
    let devices = match storage.list_devices() {
        Ok(d) => d,
        Err(e) => {
            return DiagnosticCheck {
                name: "pairings".into(),
                status: CheckStatus::Fail,
                message: format!("cannot list devices: {e:#}"),
                hints: vec![],
            }
        }
    };

    let mut bad = Vec::new();
    for (id, _name, _last) in &devices {
        if let Some(addr) = storage.device_last_addr(id).unwrap_or(None) {
            if addr.parse::<std::net::SocketAddr>().is_err() {
                bad.push(format!("{id}: {addr}"));
            }
        }
    }

    let status = if bad.is_empty() {
        CheckStatus::Pass
    } else {
        CheckStatus::Warn
    };
    DiagnosticCheck {
        name: "pairings".into(),
        status,
        message: format!(
            "{} paired device{}, {} malformed address{}",
            devices.len(),
            if devices.len() == 1 { "" } else { "s" },
            bad.len(),
            if bad.len() == 1 { "" } else { "es" },
        ),
        hints: if bad.is_empty() {
            vec![]
        } else {
            let mut hints = vec!["Malformed stored addresses:".to_string()];
            hints.extend(bad);
            hints.push("Re-pair the affected device to refresh its address.".into());
            hints
        },
    }
}

fn check_folders(storage: &Storage) -> DiagnosticCheck {
    let folders = match storage.list_sync_folders() {
        Ok(f) => f,
        Err(e) => {
            return DiagnosticCheck {
                name: "folders".into(),
                status: CheckStatus::Fail,
                message: format!("cannot list sync folders: {e:#}"),
                hints: vec![],
            }
        }
    };

    let mut missing = Vec::new();
    for (_id, path, _dev, _dir, _last) in &folders {
        if !Path::new(path).is_dir() {
            missing.push(path.clone());
        }
    }

    let status = if missing.is_empty() {
        CheckStatus::Pass
    } else {
        CheckStatus::Warn
    };
    DiagnosticCheck {
        name: "folders".into(),
        status,
        message: format!(
            "{} sync folder{}, {} missing from disk",
            folders.len(),
            if folders.len() == 1 { "" } else { "s" },
            missing.len(),
        ),
        hints: if missing.is_empty() {
            vec![]
        } else {
            let mut hints = vec!["Configured folders that no longer exist:".to_string()];
            hints.extend(missing);
            hints.push(
                "Remove them with `ferrisync folders remove <path>`, or recreate the folder."
                    .into(),
            );
            hints
        },
    }
}

// ── network checks ──

fn check_network_interface() -> DiagnosticCheck {
    let ip = crate::discovery::local_ip();
    let ok = !ip.is_loopback();
    DiagnosticCheck {
        name: "network_interface".into(),
        status: if ok {
            CheckStatus::Pass
        } else {
            CheckStatus::Warn
        },
        message: if ok {
            format!("LAN interface detected: {ip}")
        } else {
            "could not determine a LAN IP — device may be isolated or offline".into()
        },
        hints: if ok {
            vec![]
        } else {
            vec![
                "Make sure the machine is connected to a network.".into(),
                "If it is, a VPN or container may be hiding the real interface.".into(),
            ]
        },
    }
}

async fn check_port_bind(serve_port: u16) -> DiagnosticCheck {
    let source = format!("0.0.0.0:{serve_port}");
    match tokio::net::TcpListener::bind(&source).await {
        Ok(_listener) => DiagnosticCheck {
            name: "serve_port".into(),
            status: CheckStatus::Pass,
            message: format!("port {serve_port} is available for serving"),
            hints: vec![],
        },
        Err(e) => {
            if e.kind() == std::io::ErrorKind::AddrInUse {
                DiagnosticCheck {
                    name: "serve_port".into(),
                    status: CheckStatus::Info,
                    message: format!("port {serve_port} is already in use (expected while a frontend is running)"),
                    hints: vec![],
                }
            } else {
                DiagnosticCheck {
                    name: "serve_port".into(),
                    status: CheckStatus::Warn,
                    message: format!("cannot bind {serve_port}: {e}"),
                    hints: vec![
                        "Another process may hold the port.".into(),
                        format!("If you changed the listen port, pass it as --port to serve/sync."),
                    ],
                }
            }
        }
    }
}

async fn check_firewall(serve_port: u16) -> DiagnosticCheck {
    let ip = crate::discovery::local_ip();
    if ip.is_loopback() {
        return DiagnosticCheck {
            name: "firewall".into(),
            status: CheckStatus::Info,
            message: "skipped — no LAN IP to probe".into(),
            hints: vec![],
        };
    }

    // Bind an ephemeral port, then connect back to our own LAN IP. A packet
    // allowed by the firewall reaches us; a dropped one tells us inbound
    // ferrisync traffic is blocked.
    let listener = tokio::net::TcpListener::bind("0.0.0.0:0").await;
    let Ok(listener) = listener else {
        return DiagnosticCheck {
            name: "firewall".into(),
            status: CheckStatus::Warn,
            message: "could not bind a probe port".into(),
            hints: vec!["A restrictive profile may be blocking ephemeral binds.".into()],
        };
    };
    let port = listener.local_addr().unwrap().port();
    let _ = listener;
    let target = format!("{ip}:{port}");
    let timeout = tokio::time::Duration::from_millis(1500);

    let result = tokio::time::timeout(timeout, tokio::net::TcpStream::connect(&target)).await;

    match result {
        Ok(Ok(_)) => DiagnosticCheck {
            name: "firewall".into(),
            status: CheckStatus::Pass,
            message: "inbound connections to this machine work".into(),
            hints: vec![],
        },
        Ok(Err(e)) => DiagnosticCheck {
            name: "firewall".into(),
            status: CheckStatus::Warn,
            message: format!("self-connect on {target} failed: {e}"),
            hints: vec![
                "A firewall is likely dropping inbound ferrisync traffic.".into(),
                format!("Allow the listen port, e.g.: sudo ufw allow {serve_port}/tcp"),
                "Or forward the port if this device is behind NAT.".into(),
            ],
        },
        Err(_) => DiagnosticCheck {
            name: "firewall".into(),
            status: CheckStatus::Warn,
            message: format!("self-connect on {target} timed out"),
            hints: vec![
                "Inbound ferrisync traffic looks blocked.".into(),
                format!("Allow the listen port, e.g.: sudo ufw allow {serve_port}/tcp"),
            ],
        },
    }
}

async fn check_mdns(own_id: &str, serve_port: u16) -> DiagnosticCheck {
    let device_info = crate::DeviceInfo {
        id: own_id.to_string(),
        name: "doctor-probe".into(),
        cert_fingerprint: vec![],
    };
    let Ok(svc) = crate::discovery::DiscoveryService::new(device_info, serve_port) else {
        return DiagnosticCheck {
            name: "mdns".into(),
            status: CheckStatus::Warn,
            message: "could not start the mDNS daemon".into(),
            hints: vec![
                "mDNS (port 5353/udp) may be blocked by this network or a firewall.".into(),
                "Pair with `ferrisync devices pair <ip>` to skip discovery.".into(),
            ],
        };
    };
    let Ok(mut rx) = svc.browse() else {
        return DiagnosticCheck {
            name: "mdns".into(),
            status: CheckStatus::Warn,
            message: "could not browse for peers".into(),
            hints: vec![],
        };
    };

    let mut count = 0usize;
    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_millis(1500);
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(deadline - tokio::time::Instant::now(), rx.recv()).await {
            Ok(Some(_)) => count += 1,
            _ => break,
        }
    }
    svc.shutdown();

    DiagnosticCheck {
        name: "mdns".into(),
        status: CheckStatus::Info,
        message: format!(
            "mDNS discovery is working ({} peer{} found)",
            count,
            if count == 1 { "" } else { "s" }
        ),
        hints: if count == 0 {
            vec![
                "No peers advertising. This is normal unless FerriSync is open on another machine."
                    .into(),
            ]
        } else {
            vec![]
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture() -> (tempfile::TempDir, Storage) {
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::open(&dir.path().join("metadata.db")).unwrap();
        storage
            .upsert_device("peer", "Pixel 9", None, Some("192.168.1.5:9847".into()))
            .unwrap();
        (dir, storage)
    }

    #[test]
    fn data_dir_check_passes_when_writable() {
        let (dir, _storage) = fixture();
        let check = check_data_dir(dir.path());
        assert_eq!(check.status, CheckStatus::Pass);
    }

    #[test]
    fn data_dir_check_fails_when_unwritable() {
        let path = PathBuf::from("/proc/ferrisync-doctor-should-not-exist");
        let check = check_data_dir(&path);
        assert_eq!(check.status, CheckStatus::Fail);
    }

    #[test]
    fn storage_check_reports_folder_count() {
        let (_dir, storage) = fixture();
        storage
            .add_sync_folder("/tmp/a", "peer", "bidirectional")
            .unwrap();
        let check = check_storage(&storage);
        assert_eq!(check.status, CheckStatus::Pass);
        assert!(check.message.contains("1 sync folder"));
    }

    #[test]
    fn pairings_flags_malformed_addresses() {
        let (_dir, storage) = fixture();
        storage
            .upsert_device("peer", "Pixel 9", None, Some("not-an-addr".into()))
            .unwrap();
        let check = check_pairings(&storage);
        assert!(matches!(check.status, CheckStatus::Warn));
        let combined = format!("{} {:?}", check.message, check.hints);
        assert!(combined.contains("not-an-addr"));

        let (_, storage2) = fixture();
        let check2 = check_pairings(&storage2);
        assert_eq!(check2.status, CheckStatus::Pass);
    }

    #[test]
    fn folders_flags_missing_directories() {
        let (dir, storage) = fixture();
        let ghost = dir.path().join("gone");
        storage
            .add_sync_folder(ghost.to_str().unwrap(), "peer", "bidirectional")
            .unwrap();
        let check = check_folders(&storage);
        assert!(matches!(check.status, CheckStatus::Warn));
        assert!(check.message.contains("1 sync folder"));
    }
}
