use anyhow::Result;
use ferrisync_core::crypto::CryptoProvider;
use ferrisync_core::persistence::InMemoryStateStore;
use ferrisync_core::storage::Storage;
use ferrisync_core::sync_engine::server::{self, ServeHandle};
use ferrisync_core::sync_engine::SyncEvent;
use ferrisync_core::SyncEngine;
use ferrisync_core::DeviceInfo;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use crate::app::ApplicationContext;
use crate::commands::watch::{folder_loop, get_or_create_folder};
use crate::commands::{parse_device, DEFAULT_PORT};

pub struct WatchHandle {
    folder: String,
    device: String,
    shutdown: tokio::sync::watch::Sender<bool>,
    task: tokio::task::JoinHandle<()>,
}

/// Everything the REPL owns across a session: its (renamable) identity and
/// the background watch/server handles. Foreground operations (`sync`,
/// `watch`) reuse the process-wide `ctx.engine`; only background servers
/// build their own engine, so per-folder `[serve:...]` logs and pairing
/// prompts never leak across folders.
pub struct ReplState {
    pub device_info: DeviceInfo,
    watches: BTreeMap<u32, WatchHandle>,
    servers: BTreeMap<u32, ServeHandle>,
    next_id: u32,
}

impl ReplState {
    pub fn new(ctx: &ApplicationContext) -> Self {
        ReplState {
            device_info: ctx.device_info.clone(),
            watches: BTreeMap::new(),
            servers: BTreeMap::new(),
            next_id: 1,
        }
    }

    fn next_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    // ---- background watches ----

    pub fn start_watch(
        &mut self,
        storage: &Storage,
        engine: &Arc<SyncEngine>,
        folder: String,
        device: String,
    ) {
        let addr = match parse_device(&device, DEFAULT_PORT) {
            Ok(a) => a,
            Err(e) => {
                eprintln!("error: {e:#}");
                return;
            }
        };
        let folder_id = match get_or_create_folder(storage, &folder, &device) {
            Ok(id) => id,
            Err(e) => {
                eprintln!("error: {e:#}");
                return;
            }
        };

        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        let id = self.next_id();
        let task_folder = folder.clone();
        let task_engine = engine.clone();
        let task = tokio::spawn(async move {
            if let Err(e) = folder_loop(task_folder.clone(), addr, folder_id, task_engine, shutdown_rx)
                .await
            {
                eprintln!("[watch:{task_folder}] error: {e:#}");
            }
        });

        self.watches.insert(
            id,
            WatchHandle {
                folder: folder.clone(),
                device: device.clone(),
                shutdown: shutdown_tx,
                task,
            },
        );
        println!("watch #{id} started: {folder} ↔ {device} (background)");
    }

    pub fn list_watches(&self) {
        if self.watches.is_empty() {
            println!("(no active background watches)");
            return;
        }
        for (id, w) in &self.watches {
            println!("  #{id}  {} ↔ {}", w.folder, w.device);
        }
    }

    pub async fn stop_watch(&mut self, id: u32) {
        match self.watches.remove(&id) {
            None => eprintln!("no such watch: #{id}"),
            Some(handle) => {
                let _ = handle.shutdown.send(true);
                await_shutdown(id, handle.task).await;
                println!("watch #{id} stopped");
            }
        }
    }

    async fn stop_all_watches(&mut self) {
        let taken = std::mem::take(&mut self.watches);
        for (id, w) in taken {
            let _ = w.shutdown.send(true);
            await_shutdown(id, w.task).await;
            println!("watch #{id} stopped");
        }
    }

    // ---- background servers ----

    pub async fn start_server(&mut self, ctx: &ApplicationContext, folder: String, port: u16) {
        let handle = match spawn_server(
            &folder,
            port,
            ctx.storage.clone(),
            ctx.crypto.clone(),
            self.device_info.clone(),
        )
        .await
        {
            Ok(handle) => handle,
            Err(e) => {
                eprintln!("error: {e:#}");
                return;
            }
        };

        let id = self.next_id();
        println!(
            "serve #{id} started: {} on 0.0.0.0:{} (background)\n  \
             unknown devices must be approved before they can pair — watch for \
             PAIRING REQUEST lines",
            handle.folder, handle.port
        );
        self.servers.insert(id, handle);
    }

    pub fn list_servers(&self) {
        if self.servers.is_empty() {
            println!("(no active background servers)");
            return;
        }
        for (id, s) in &self.servers {
            println!("  #{id}  {} on 0.0.0.0:{}", s.folder, s.port);
        }
    }

    pub fn has_background(&self) -> bool {
        !self.watches.is_empty() || !self.servers.is_empty()
    }

    pub async fn stop_server(&mut self, id: u32) {
        match self.servers.remove(&id) {
            None => eprintln!("no such server: #{id}"),
            Some(handle) => {
                handle.stop().await;
                println!("server #{id} stopped");
            }
        }
    }

    async fn stop_all_servers(&mut self) {
        let taken = std::mem::take(&mut self.servers);
        for (id, s) in taken {
            s.stop().await;
            println!("server #{id} stopped");
        }
    }

    /// Restart every running server under the given identity so mDNS
    /// advertisements and pairing responses carry the (new) name.
    pub async fn rename_restart_servers(&mut self, ctx: &ApplicationContext) {
        if self.servers.is_empty() {
            return;
        }
        let entries: Vec<(u32, u16, String)> = self
            .servers
            .iter()
            .map(|(id, s)| (*id, s.port, s.folder.clone()))
            .collect();
        let total = entries.len();

        for (_, handle) in std::mem::take(&mut self.servers) {
            let _ = handle.stop().await;
        }

        let mut restarted = 0;
        for (id, port, folder) in entries {
            match spawn_server(
                &folder,
                port,
                ctx.storage.clone(),
                ctx.crypto.clone(),
                self.device_info.clone(),
            )
            .await
            {
                Ok(handle) => {
                    self.servers.insert(id, handle);
                    println!("server #{id} restarted as '{}'", self.device_info.name);
                    restarted += 1;
                }
                Err(e) => eprintln!("error: server #{id} ({folder}) could not be restarted: {e:#}"),
            }
        }
        println!(
            "{restarted}/{total} server(s) now advertising as '{}'",
            self.device_info.name
        );
    }

    // ---- held pairing requests ----

    /// Held pairing requests across all servers: `(server_id, name, device_id)`,
    /// numbered 1..n in the order `pendings` displays them.
    fn collect_pending(&self) -> Vec<(u32, String, String)> {
        let mut out = Vec::new();
        for (sid, s) in &self.servers {
            match s.pending_pairings() {
                Ok(pending) => {
                    for (name, id) in pending {
                        out.push((*sid, name, id));
                    }
                }
                Err(e) => eprintln!("error: {e:#}"),
            }
        }
        out
    }

    pub fn list_pendings(&self) {
        print!("{}", format_pendings(&self.collect_pending()));
    }

    /// Answer the single held pairing request with y/n; refuses to guess when
    /// several requests are waiting.
    pub fn answer_latest(&mut self, approve: bool) {
        match self.collect_pending().len() {
            0 => println!("(no pairing requests waiting)"),
            1 => self.resolve_pending(1, approve),
            _ => {
                println!(
                    "multiple requests waiting — use 'confirm <n>' or 'deny <n>' (see 'pendings')"
                )
            }
        }
    }

    pub fn resolve_pending(&mut self, n: u32, approve: bool) {
        let all = self.collect_pending();
        if n == 0 || n as usize > all.len() {
            eprintln!("no such pairing request: {n} (see 'pendings')");
            return;
        }
        let (sid, name, id) = all[(n - 1) as usize].clone();
        let Some(server) = self.servers.get_mut(&sid) else {
            return;
        };
        let result = if approve {
            server
                .approve_pairing(&id, &name)
                .map(|_| {
                    format!(
                        "approved '{name}' — they can now pair\nattach folders with: sync <folder> --device {name}"
                    )
                })
        } else {
            server.deny_pairing(&id).map(|_| format!("denied '{name}'"))
        };
        match result {
            Ok(msg) => println!("{msg}"),
            Err(e) => eprintln!("error: {e:#}"),
        }
    }

    // ---- teardown ----

    pub async fn stop_all(&mut self) {
        self.stop_all_watches().await;
        self.stop_all_servers().await;
    }
}

/// Bring up a folder server plus its event-printing drain task. Built with
/// its own engine so events stay scoped to the one folder; sharing the
/// process-wide context engine here would broadcast every folder's events
/// into every drain task (and mislabel the folder tags).
async fn spawn_server(
    folder: &str,
    port: u16,
    storage: Arc<Storage>,
    crypto: Arc<CryptoProvider>,
    device_info: DeviceInfo,
) -> Result<ServeHandle> {
    let state_store = Arc::new(InMemoryStateStore::new());
    let engine = SyncEngine::new(storage, crypto, device_info, state_store);
    let (handle, mut events) = engine
        .serve_folder(folder.to_string(), port, server::PairPolicy::Confirm)
        .await?;

    // Drain sync events so the user sees activity from served folders.
    let task_folder = folder.to_string();
    tokio::spawn(async move {
        while let Some(event) = events.recv().await {
            match event {
                SyncEvent::PairRequested { name, .. } => {
                    print!("{}", pairing_notice_text(&task_folder, &name));
                    let _ = std::io::Write::flush(&mut std::io::stdout());
                }
                SyncEvent::DevicePaired { name, .. } => {
                    println!("[serve:{task_folder}] paired with {name}");
                }
                SyncEvent::FilePushed { path, device } => {
                    println!("[serve:{task_folder}] pushed {path} -> {device}");
                }
                SyncEvent::FilePulled { path, device } => {
                    println!("[serve:{task_folder}] pulled {path} <- {device}");
                }
                SyncEvent::Conflict { path, .. } => {
                    println!("[serve:{task_folder}] conflict on {path}");
                }
                _ => {}
            }
        }
    });

    Ok(handle)
}

fn format_pendings(all: &[(u32, String, String)]) -> String {
    if all.is_empty() {
        return "(no pairing requests waiting)\n".to_string();
    }
    let mut out = String::new();
    for (i, (_, name, id)) in all.iter().enumerate() {
        out.push_str(&format!("  {}  {name} ({id})\n", i + 1));
    }
    out
}

/// Notice printed when an unknown device is held for pairing approval.
/// Starts with a newline so it stays readable even if a REPL prompt or
/// serve log line was mid-output.
fn pairing_notice_text(folder: &str, name: &str) -> String {
    format!(
        "\n[serve:{folder}] PAIRING REQUEST — confirm connection with '{name}'?\n  \
         `y` allows, `n` denies (`pendings` lists held requests)\n"
    )
}

async fn await_shutdown(id: u32, task: tokio::task::JoinHandle<()>) {
    match tokio::time::timeout(Duration::from_secs(5), task).await {
        Ok(_) => {}
        Err(_) => {
            log::warn!("[watch:{id}] did not stop within 5s, aborting");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairing_notice_contains_all_guidance() {
        let text = pairing_notice_text("myfolder", "Pixel 7");
        assert!(
            text.starts_with('\n'),
            "notice should start on a fresh line"
        );
        assert!(text.contains("[serve:myfolder]"));
        assert!(text.contains("confirm connection with 'Pixel 7'?"));
        assert!(text.contains("`y` allows"));
        assert!(text.contains("`n` denies"));
        assert!(text.contains("`pendings`"));
        assert!(text.ends_with('\n'));
    }

    #[test]
    fn format_pendings_empty_and_numbered() {
        assert_eq!(format_pendings(&[]), "(no pairing requests waiting)\n");

        let entries = vec![
            (7u32, "phone-a".to_string(), "id-a".to_string()),
            (9u32, "phone-b".to_string(), "id-b".to_string()),
        ];
        let text = format_pendings(&entries);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].starts_with("  1  "));
        assert!(lines[0].contains("phone-a") && lines[0].contains("(id-a)"));
        assert!(lines[1].starts_with("  2  "));
        assert!(lines[1].contains("phone-b") && lines[1].contains("(id-b)"));
    }
}