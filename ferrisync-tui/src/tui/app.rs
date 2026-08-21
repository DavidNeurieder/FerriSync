use ferrisync_core::storage::Storage;
use ferrisync_core::sync_engine::pairing::PairingManager;
use ferrisync_core::sync_engine::SyncEngine;
use ferrisync_core::DeviceInfo;
use std::path::PathBuf;
use std::sync::Arc;

pub struct App {
    #[allow(dead_code)]
    pub engine: Arc<SyncEngine>,
    pub pairing: PairingManager,
    pub storage: Arc<Storage>,
    pub device_info: DeviceInfo,
    pub data_dir: PathBuf,

    pub active_tab: usize,
    pub confirm_quit: bool,
    pub status_message: String,

    pub devices: Vec<(String, String, Option<i64>)>,
    pub folders: Vec<(i64, String, String, String, Option<i64>)>,
    pub log_entries: Vec<String>,

    pub pairing_ip: String,
    pub pairing_port: u16,
    #[allow(dead_code)]
    pub input_mode: bool,
}

impl App {
    pub fn new(
        engine: Arc<SyncEngine>,
        pairing: PairingManager,
        storage: Arc<Storage>,
        device_info: DeviceInfo,
        data_dir: PathBuf,
    ) -> Self {
        Self {
            engine,
            pairing,
            storage,
            device_info,
            data_dir,
            active_tab: 0,
            confirm_quit: false,
            status_message: "Ready".to_string(),
            devices: Vec::new(),
            folders: Vec::new(),
            log_entries: Vec::new(),
            pairing_ip: String::new(),
            pairing_port: 9847,
            input_mode: false,
        }
    }

    pub fn set_tab(&mut self, tab: usize) {
        self.active_tab = tab;
        self.confirm_quit = false;
    }

    pub async fn refresh(&mut self) {
        self.devices = self.storage.list_devices().unwrap_or_default();
        self.folders = self.storage.list_sync_folders().unwrap_or_default();
    }

    pub async fn handle_enter(&mut self) {
        if self.active_tab == 1 {
            // Devices tab - initiate pairing
            let ip = self.pairing_ip.clone();
            if !ip.is_empty() {
                let msg = format!("Pairing with {ip}:{}...", self.pairing_port);
                self.log_entries.push(msg.clone());
                self.status_message = msg;

                let addr: std::net::SocketAddr =
                    format!("{}:{}", ip, self.pairing_port).parse().unwrap();
                match self.pairing.pair_with(addr).await {
                    Ok(peer) => {
                        let msg = format!("Paired with {} ({})", peer.name, peer.id);
                        self.log_entries.push(msg.clone());
                        self.status_message = msg;
                    }
                    Err(e) => {
                        let msg = format!("Pairing failed: {e}");
                        self.log_entries.push(msg.clone());
                        self.status_message = msg;
                    }
                }
                self.pairing_ip.clear();
            }
        }
    }
}
