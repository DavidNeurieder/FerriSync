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
        let service_info = ServiceInfo::new(
            SERVICE_TYPE,
            &self.device_info.name,
            &format!("{}.local.", self.device_info.name),
            &self.device_info.id.to_string(),
            self.port,
            None,
        )?;

        self.mdns.register(service_info)?;
        Ok(())
    }

    /// Browse for other devices, returning a channel of discovered peers.
    pub fn browse(&self) -> Result<mpsc::Receiver<DiscoveredPeer>> {
        let receiver = self.mdns.browse(SERVICE_TYPE)?;
        let (tx, rx) = mpsc::channel(64);

        tokio::task::spawn_blocking(move || {
            while let Ok(event) = receiver.recv() {
                match event {
                    ServiceEvent::ServiceResolved(info) => {
                        let addresses: Vec<SocketAddr> = info
                            .get_addresses()
                            .iter()
                            .filter_map(|addr| {
                                let ip: IpAddr = (*addr).to_owned();
                                let port = info.get_port();
                                Some(SocketAddr::new(ip, port))
                            })
                            .collect();

                        let device_id = info
                            .get_hostname()
                            .trim_end_matches(".local.")
                            .to_string();

                        let mut properties = HashMap::new();
                        for prop in info.get_properties().iter() {
                            if let Some(val) = prop.val() {
                                if let Ok(s) = std::str::from_utf8(val) {
                                    properties.insert(prop.key().to_string(), s.to_string());
                                }
                            }
                        }

                        let peer = DiscoveredPeer {
                            id: device_id.clone(),
                            name: info.get_fullname().to_string(),
                            addresses,
                            properties,
                        };

                        let _ = tx.blocking_send(peer);
                    }
                    _ => {}
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
