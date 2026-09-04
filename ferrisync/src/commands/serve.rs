use anyhow::{Context, Result};
use ferrisync_core::crypto::CryptoProvider;
use ferrisync_core::persistence::InMemoryStateStore;
use ferrisync_core::storage::Storage;
use ferrisync_core::sync_engine::server::ServeHandle;
use ferrisync_core::sync_engine::SyncEvent;
use ferrisync_core::{DeviceInfo, PairPolicy, SyncEngine};
use std::io::IsTerminal;
use std::sync::Arc;

use crate::app::ApplicationContext;

use super::input::read_yes_no;

pub async fn run(
    ctx: &ApplicationContext,
    folder: Option<&str>,
    port: u16,
    auto_accept: bool,
) -> Result<()> {
    match folder {
        Some(folder) => serve_one(ctx, folder, port, auto_accept).await,
        None => serve_all(ctx, port, auto_accept).await,
    }
}

/// Foreground, single-folder serve: bind one server, prompt for pairings inline,
/// and block until Ctrl+C. Used by `ferrisync serve <folder>`.
async fn serve_one(
    ctx: &ApplicationContext,
    folder: &str,
    port: u16,
    auto_accept: bool,
) -> Result<()> {
    let interactive = std::io::stdin().is_terminal() && !auto_accept;
    let policy = if interactive {
        PairPolicy::Confirm
    } else {
        PairPolicy::AutoAccept
    };
    let (server, mut events) = ctx
        .engine
        .serve_folder(folder.to_string(), port, policy)
        .await?;
    println!("Serving folder \"{folder}\" on 0.0.0.0:{}", server.port);
    println!("Advertising on mDNS as _ferrisync._tcp");
    if interactive {
        println!("Unknown devices need your confirmation before pairing.");
    } else {
        println!("Auto-accepting pairing requests (use --auto-accept or run without a TTY).");
        eprintln!(
            "WARNING: --auto-accept trusts ANY device on the network and gives it \
             read/write access to this folder. Only use it on trusted networks."
        );
    }
    println!("Serve mode active. Press Ctrl+C to stop.");

    tokio::select! {
        _ = async {
            while let Some(event) = events.recv().await {
                match event {
                    SyncEvent::PairRequested { name, id } => {
                        use std::io::Write as _;
                        println!();
                        print!("Confirm connection with '{name}' ({id})? [y/N] ");
                        let _ = std::io::stdout().flush();
                        let answer = read_yes_no().await;
                        if answer {
                            if let Err(e) = server.approve_pairing(&id, &name) {
                                println!("approve failed: {e:#}");
                            }
                            println!("\nApproved '{name}'. They can now pair.");
                        } else {
                            if let Err(e) = server.deny_pairing(&id) {
                                println!("deny failed: {e:#}");
                            }
                            println!("\nDenied '{name}'.");
                        }
                    }
                    SyncEvent::DevicePaired { name, .. } => {
                        println!("[serve] paired with {name}");
                    }
                    SyncEvent::FolderPairRequested { name, id, folder: _folder_guid } => {
                        if !interactive {
                            println!("[serve] folder-pairing request from {name} for '{folder}' (auto-accept mode ignores; use shared-folder pairing from the app)");
                            continue;
                        }
                        use std::io::Write as _;
                        println!();
                        print!(
                            "Confirm pairing folder '{folder}' with '{name}'? [y/N] "
                        );
                        let _ = std::io::stdout().flush();
                        let answer = read_yes_no().await;
                        if answer {
                            // The owner's copy of the shared folder is the served
                            // `folder`; register the peer pair onto it. The peer's
                            // remote path is unknown here (the requester records
                            // its own), so pass None to keep the existing value.
                            if let Err(e) = server.approve_folder_pairing(
                                &id,
                                &_folder_guid,
                                &name,
                                folder,
                                None,
                            ) {
                                println!("approve failed: {e:#}");
                            }
                            println!("\nApproved '{name}' for '{folder}'.");
                        } else {
                            if let Err(e) = server.deny_folder_pairing(&id, &_folder_guid) {
                                println!("deny failed: {e:#}");
                            }
                            println!("\nDenied '{name}' for '{folder}'.");
                        }
                    }
                    SyncEvent::FilePushed { path, device } => {
                        println!("[serve] pushed {path} -> {device}");
                    }
                    SyncEvent::FilePulled { path, device } => {
                        println!("[serve] pulled {path} <- {device}");
                    }
                    SyncEvent::Conflict { path, .. } => {
                        println!("[serve] conflict on {path}");
                    }
                    _ => {}
                }
            }
        } => {}
        _ = tokio::signal::ctrl_c() => {}
    }

    server.stop().await;
    println!("Shutting down.");
    Ok(())
}

/// Background, "serve everything" mode: host every configured folder so peers
/// can pair with and sync any of them, then block until Ctrl+C. Used by a bare
/// `ferrisync serve` (no folder argument).
async fn serve_all(ctx: &ApplicationContext, port: u16, auto_accept: bool) -> Result<()> {
    if port != super::DEFAULT_PORT {
        anyhow::bail!("--port only applies when serving a single folder");
    }
    let policy = if auto_accept {
        PairPolicy::AutoAccept
    } else {
        PairPolicy::Confirm
    };

    let handles = serve_all_configured(
        ctx.storage.clone(),
        ctx.crypto.clone(),
        ctx.device_info.clone(),
        port,
        policy,
        auto_accept,
    )
    .await?;
    if handles.is_empty() {
        anyhow::bail!(
            "no folders are configured to serve — use `ferrisync serve <folder>` \
             or `ferrisync add <folder>` first"
        );
    }

    println!(
        "Serve mode active for {} folder(s). Press Ctrl+C to stop.",
        handles.len()
    );
    if !auto_accept {
        eprintln!(
            "note: unknown-device pairings are held and printed. Re-run with \
             --auto-accept to trust any device automatically (only on trusted networks)."
        );
    }
    tokio::signal::ctrl_c().await.context("awaiting Ctrl+C")?;
    for h in handles {
        h.stop().await;
    }
    println!("Shutting down.");
    Ok(())
}

/// Spawn one background server hosting `folder`. Builds its own engine (so each
/// served folder is independent), drains sync events to stdout, and returns the
/// live handle.
pub async fn spawn_folder_server(
    storage: Arc<Storage>,
    crypto: Arc<CryptoProvider>,
    device_info: DeviceInfo,
    folder: String,
    port: u16,
    policy: PairPolicy,
) -> Result<ServeHandle> {
    let state_store = Arc::new(InMemoryStateStore::new());
    let engine = SyncEngine::new(storage, crypto, device_info, state_store);
    let (handle, mut events) = engine.serve_folder(folder.clone(), port, policy).await?;

    let task_folder = folder.clone();
    tokio::spawn(async move {
        while let Some(event) = events.recv().await {
            match event {
                SyncEvent::PairRequested { name, .. } => {
                    use std::io::Write as _;
                    println!(
                        "\n[serve:{task_folder}] PAIRING REQUEST — confirm connection with \
                         '{name}'?\n  `y` allows, `n` denies (`pendings` lists held requests)\n"
                    );
                    let _ = std::io::stdout().flush();
                }
                SyncEvent::DevicePaired { name, .. } => {
                    println!("[serve:{task_folder}] paired with {name}");
                }
                SyncEvent::FolderPairRequested {
                    name,
                    folder: _guid,
                    ..
                } => {
                    println!("[serve:{task_folder}] folder-pairing request from {name}");
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

/// Host every configured folder (deduplicated by id or path) as independent
/// background servers, starting at `start_port` and incrementing per folder.
/// A folder whose path is gone or whose port is taken is logged and skipped,
/// never fatal. Returns the handles of the folders that were started.
pub async fn serve_all_configured(
    storage: Arc<Storage>,
    crypto: Arc<CryptoProvider>,
    device_info: DeviceInfo,
    start_port: u16,
    policy: PairPolicy,
    auto_accept: bool,
) -> Result<Vec<ServeHandle>> {
    let rows = storage.list_sync_folders()?;
    let mut seen: Vec<(i64, String)> = Vec::new();
    let mut handles: Vec<ServeHandle> = Vec::new();
    let mut i = 0usize;
    for (folder_id, local_path, _device, _dir, _last) in rows {
        if seen
            .iter()
            .any(|(fid, path)| *fid == folder_id || *path == local_path)
        {
            continue;
        }
        seen.push((folder_id, local_path.clone()));
        let port = start_port.saturating_add(i as u16);
        i += 1;
        match spawn_folder_server(
            storage.clone(),
            crypto.clone(),
            device_info.clone(),
            local_path.clone(),
            port,
            policy,
        )
        .await
        {
            Ok(handle) => {
                println!("Serving \"{}\" on 0.0.0.0:{}", handle.folder, handle.port);
                handles.push(handle);
            }
            Err(e) => {
                log::warn!("failed to serve {local_path} on port {port}: {e:#}");
            }
        }
    }

    if !auto_accept && !handles.is_empty() {
        println!("Advertising on mDNS as _ferrisync._tcp");
        println!("Unknown devices need approval before pairing; watch for PAIRING REQUEST lines.");
    }

    Ok(handles)
}
