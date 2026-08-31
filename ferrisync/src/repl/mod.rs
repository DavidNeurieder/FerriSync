use rustyline::error::ReadlineError;
use rustyline::history::DefaultHistory;
use rustyline::Editor;
use std::io::IsTerminal;
use std::path::PathBuf;

mod commands;
mod completion;
mod parser;
mod runner;
mod state;

use crate::app::ApplicationContext;

pub use commands::ReplCommand;

pub const COMMANDS: &[&str] = &[
    "help",
    "status",
    "devices",
    "folders",
    "activity",
    "conflicts",
    "doctor",
    "sessions",
    "discover",
    "pair",
    "sync",
    "unsync",
    "watch",
    "watches",
    "unwatch",
    "serve",
    "serves",
    "unserve",
    "pendings",
    "confirm",
    "deny",
    "rename",
    "exit",
    "quit",
];

pub async fn run(ctx: &mut ApplicationContext) -> anyhow::Result<()> {
    let history_path: PathBuf = ctx.data_dir.join("repl_history");

    let mut rl = Editor::<completion::ReplHelper, DefaultHistory>::new()?;
    rl.set_helper(Some(completion::ReplHelper));
    let _ = rl.load_history(&history_path);

    if is_first_run(ctx) && std::io::stdin().is_terminal() {
        first_run_welcome(ctx);
    }

    let mut state = state::ReplState::new(ctx);

    println!(
        "FerriSync {} — interactive shell",
        env!("CARGO_PKG_VERSION")
    );
    print_dashboard(ctx);
    println!("Type 'help' for commands, 'exit' or Ctrl-D to quit.");

    loop {
        let readline = rl.readline("ferrisync> ");
        match readline {
            Ok(line) => {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    let _ = rl.add_history_entry(trimmed);
                }
                match parser::parse_line(trimmed) {
                    Ok(None) => {}
                    Ok(Some(ReplCommand::Exit)) => break,
                    Ok(Some(ReplCommand::Help)) => print_help(),
                    Ok(Some(command)) => runner::dispatch(&mut state, ctx, command).await,
                    Err(e) => eprintln!("error: {e:#}"),
                }
            }
            Err(ReadlineError::Interrupted) => {
                println!("(to quit, press Ctrl-D or type 'exit')");
            }
            Err(ReadlineError::Eof) => {
                println!();
                break;
            }
            Err(e) => {
                eprintln!("readline error: {e}");
                break;
            }
        }
    }

    state.stop_all().await;
    let _ = rl.save_history(&history_path);
    Ok(())
}

/// Folder-centric "how is everything?" startup view.
fn print_dashboard(ctx: &ApplicationContext) {
    match crate::commands::status::run(ctx) {
        Ok(status) => print!("{}", crate::commands::status::dashboard(&status)),
        Err(e) => eprintln!("(could not read status: {e:#})"),
    }
}

/// True on a brand-new data dir: no persisted device name and no devices or
/// folders yet. `device.name` is the marker (it is only written on rename).
fn is_first_run(ctx: &ApplicationContext) -> bool {
    let has_name = ctx.data_dir.join("device.name").exists();
    let devices_empty = ctx
        .storage
        .list_devices()
        .map(|v| v.is_empty())
        .unwrap_or(false);
    let folders_empty = ctx
        .storage
        .list_sync_folders()
        .map(|v| v.is_empty())
        .unwrap_or(false);
    first_run_condition(has_name, devices_empty, folders_empty)
}

/// Pure first-run decision (kept separate for easy testing).
fn first_run_condition(has_name: bool, devices_empty: bool, folders_empty: bool) -> bool {
    !has_name && devices_empty && folders_empty
}

/// Welcome + device-name prompt shown only on the first run (interactive).
fn first_run_welcome(ctx: &mut ApplicationContext) {
    println!("Welcome to FerriSync");
    println!("Private file sync between your devices — over your LAN, no cloud, no account.\n");
    let default = ctx.device_info.name.clone();
    let mut line = String::new();
    eprint!("Device name [{}]: ", default);
    if std::io::stdin()
        .read_line(&mut line)
        .map(|_| line.trim().is_empty())
        .unwrap_or(true)
    {
        // Empty input (or EOF) keeps the hostname default.
        return;
    }
    let input = line.trim().to_string();
    match ferrisync_core::api::sanitize_device_name(&input) {
        Ok(clean) => {
            ferrisync_core::config::persist_device_name(&ctx.data_dir, &clean);
            ctx.device_info.name = clean.clone();
            ctx.pairing.set_name(&clean);
            println!("This device is '{}'.\n", clean);
        }
        Err(e) => eprintln!("error: {e:#}"),
    }
}

fn print_help() {
    println!(
        "Commands:
  help                          Show this help
  status                        Show paired devices and sync folders (presence + health)
  devices                       List paired devices with presence
  folders                       List sync folders with health
  activity                      Recent sync sessions and file changes
  conflicts                     List unresolved conflicts
  doctor                        Run on-device diagnostics
  sessions                      Show recent sync sessions (both directions)
  discover [seconds]            Scan the LAN for FerriSync devices (default 3s)
  pair <ip> [--port <port>]     Pair with a device (default port {})
  sync                          Sync ALL configured folders
  sync <folder> --device <ip[:port]|name|uuid> [--wait secs]
                                One-shot folder sync
  unsync                        Show what a full reset would remove
  unsync --yes                  Clear ALL folders, devices, and sync metadata
  unsync <folder> [--device <id>]
                                Remove sync entries for a folder
  watch <folder> --device <ip[:port]>
                                Sync on every change (runs in background)
   watches                       List background watches
   unwatch <id>                  Stop a background watch
   serve <folder> [--port <port>]
                                 Host the folder for pairing + sync (background)
   serves                        List background servers
   unserve <id>                  Stop a background server
   pendings                      List devices waiting for pairing approval
   confirm <n>                   Approve a held pairing request
   deny <n>                      Deny a held pairing request
   rename <name>                 Change this device's network name
   y / n                         Answer the single held pairing request
   exit                          Leave the shell (also: quit, Ctrl-D)",
        crate::commands::DEFAULT_PORT
    );
}

#[cfg(test)]
mod tests {
    use super::first_run_condition;

    #[test]
    fn brand_new_dir_is_first_run() {
        assert!(first_run_condition(false, true, true));
    }

    #[test]
    fn existing_device_name_is_not_first_run() {
        assert!(!first_run_condition(true, true, true));
    }

    #[test]
    fn existing_devices_or_folders_is_not_first_run() {
        assert!(!first_run_condition(false, false, true));
        assert!(!first_run_condition(false, true, false));
    }
}
