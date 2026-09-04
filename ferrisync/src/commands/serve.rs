use anyhow::Result;
use ferrisync_core::{PairPolicy, SyncEvent};
use std::io::IsTerminal;

use crate::app::ApplicationContext;

use super::input::read_yes_no;

pub async fn run(
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
                                &folder,
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
