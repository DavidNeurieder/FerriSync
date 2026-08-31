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
