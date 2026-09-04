use crate::DeviceInfo;
use anyhow::Result;
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use tokio::sync::mpsc;

const SERVICE_TYPE: &str = "_ferrisync._tcp.local.";

/// Discovers other FerriSync devices on the LAN via mDNS.
pub struct DiscoveryService {
    mdns: Arc<ServiceDaemon>,
    device_info: DeviceInfo,
    port: u16,
}

impl DiscoveryService {
    pub fn new(device_info: DeviceInfo, port: u16) -> Result<Self> {
        let mdns = Arc::new(ServiceDaemon::new()?);
        Ok(Self {
            mdns,
            device_info,
            port,
        })
    }

    /// Start advertising this device on the network.
    pub fn advertise(&self) -> Result<()> {
        let txt: &[(&str, &str)] = &[("id", self.device_info.id.as_str())];
        let service_info = ServiceInfo::new(
            SERVICE_TYPE,
            &self.device_info.name,
            &format!("{}.local.", self.device_info.name),
            local_ip(),
            self.port,
            txt,
        )?;

        self.mdns.register(service_info)?;
        Ok(())
    }

    /// Stop advertising/browsing and shut down the mDNS daemon.
    pub fn shutdown(&self) {
        if let Ok(rx) = self.mdns.shutdown() {
            // Consume the status reply so the daemon doesn't log a send error.
            let _ = rx.recv_timeout(std::time::Duration::from_millis(500));
        }
    }

    /// Probe the LAN for another instance that is already advertising this
    /// device's name on a different address/port — the classic "two ferrisync
    /// processes on one host" split-brain that yields duplicate device and
    /// folder entries. Returns the competing fullnames, or empty when this is
    /// the only announcing instance.
    ///
    /// This uses a short-lived, independent mDNS daemon so the check does not
    /// disturb the live advertisement (or its conflict/tie-break handling).
    /// Our own announcement is excluded by address+port comparison.
    pub fn detect_duplicate_on_lan(&self, window_ms: u64) -> Result<Vec<String>> {
        let mdns = ServiceDaemon::new()?;
        let rx = mdns.browse(SERVICE_TYPE)?;

        let our = format!("{}:{}", local_ip(), self.port);

        let start = std::time::Instant::now();
        let mut by_addr: std::collections::HashMap<String, String> = Default::default();
        let deadline = std::time::Duration::from_millis(window_ms);
        while start.elapsed() < deadline {
            if let Ok(event) = rx.recv_timeout(deadline - start.elapsed()) {
                if let ServiceEvent::ServiceResolved(info) = event {
                    let name = instance_name(&info.get_fullname()).to_string();
                    if !name.eq_ignore_ascii_case(&self.device_info.name) {
                        continue;
                    }
                    for addr in info.get_addresses().iter() {
                        let key = format!("{addr}:{}", info.get_port());
                        if key == our {
                            // This is our own announcement.
                            continue;
                        }
                        by_addr
                            .entry(key)
                            .or_insert_with(|| info.get_fullname().to_string());
                    }
                }
            }
        }
        let _ = mdns.shutdown();
        Ok(by_addr.into_values().collect())
    }

    /// Browse for other devices, returning a channel of discovered peers.
    pub fn browse(&self) -> Result<mpsc::Receiver<DiscoveredPeer>> {
        let receiver = self.mdns.browse(SERVICE_TYPE)?;
        let (tx, rx) = mpsc::channel(64);

        tokio::task::spawn_blocking(move || {
            while let Ok(event) = receiver.recv() {
                if let ServiceEvent::ServiceResolved(info) = event {
                    let addresses: Vec<SocketAddr> = info
                        .get_addresses()
                        .iter()
                        .map(|addr| SocketAddr::new(*addr, info.get_port()))
                        .collect();

                    let mut properties = HashMap::new();
                    for prop in info.get_properties().iter() {
                        if let Some(val) = prop.val() {
                            if let Ok(s) = std::str::from_utf8(val) {
                                properties.insert(prop.key().to_string(), s.to_string());
                            }
                        }
                    }

                    // Prefer the advertised device ID; fall back to the hostname.
                    let device_id = properties.get("id").cloned().unwrap_or_else(|| {
                        info.get_hostname().trim_end_matches(".local.").to_string()
                    });

                    let peer = DiscoveredPeer {
                        id: device_id,
                        name: info.get_fullname().to_string(),
                        addresses,
                        properties,
                    };

                    let _ = tx.blocking_send(peer);
                }
            }
        });

        Ok(rx)
    }
}

#[derive(Debug, Clone)]
pub struct DiscoveredPeer {
    pub id: String,
    pub name: String,
    pub addresses: Vec<SocketAddr>,
    pub properties: HashMap<String, String>,
}

/// Extract the service instance label (the leading label) from an mDNS
/// fullname such as `repl_cli._ferrisync._tcp.local.`. Falls back to the
/// whole name if it is not in the expected dotted form.
fn instance_name(fullname: &str) -> &str {
    fullname.split('.').next().unwrap_or(fullname)
}

/// Probe the LAN for a second instance announcing the same device name and
/// return a human-readable warning when one is found, else `None`. Best-effort:
/// any discovery failure yields `None`. Runs a short blocking probe
/// (`window_ms`, default 1200) so callers wanting a non-blocking prompt should
/// run it off the hot path.
pub fn duplicate_announce_warning(device_info: &crate::DeviceInfo, port: u16) -> Option<String> {
    let svc = DiscoveryService::new(device_info.clone(), port).ok()?;
    let dupes = svc.detect_duplicate_on_lan(1200).ok()?;
    if dupes.is_empty() {
        return None;
    }
    Some(format!(
        "warning: another instance is already advertising '{}' on this LAN ({}). \
         This can duplicate `status`/`discover` entries. Stop the other ferrisync \
         process or rename this device.",
        device_info.name,
        dupes.join(", ")
    ))
}

/// Best-effort local LAN address for mDNS advertisement (and diagnostics).
/// Uses a UDP "connect" (route lookup only — no packet is sent); falls back
/// to localhost.
pub fn local_ip() -> IpAddr {
    let any = std::net::SocketAddr::new(IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED), 0);
    let probe =
        std::net::SocketAddr::new(IpAddr::V4(std::net::Ipv4Addr::new(10, 254, 254, 254)), 9847);

    std::net::UdpSocket::bind(any)
        .and_then(|sock| sock.connect(probe).map(|()| sock))
        .and_then(|sock| sock.local_addr())
        .map(|addr| addr.ip())
        .unwrap_or(IpAddr::V4(std::net::Ipv4Addr::LOCALHOST))
}

#[cfg(test)]
mod tests {
    use super::instance_name;

    #[test]
    fn extracts_instance_label_from_fullname() {
        assert_eq!(instance_name("repl_cli._ferrisync._tcp.local."), "repl_cli");
        assert_eq!(
            instance_name("repl_cli (2)._ferrisync._tcp.local."),
            "repl_cli (2)"
        );
        assert_eq!(instance_name("no-dots"), "no-dots");
    }
}
