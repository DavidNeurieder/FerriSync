use rustyline::error::ReadlineError;
use rustyline::history::DefaultHistory;
use rustyline::Editor;
use std::path::PathBuf;

mod commands;
mod completion;
mod parser;
mod runner;
mod state;

use crate::app::ApplicationContext;

pub use commands::ReplCommand;

pub const COMMANDS: &[&str] = &[
    "help", "status", "sessions", "discover", "pair", "sync", "unsync", "watch", "watches",
    "unwatch", "serve", "serves", "unserve", "pendings", "confirm", "deny", "rename", "exit",
    "quit",
];

pub async fn run(ctx: &ApplicationContext) -> anyhow::Result<()> {
    let history_path: PathBuf = ctx.data_dir.join("repl_history");

    let mut rl = Editor::<completion::ReplHelper, DefaultHistory>::new()?;
    rl.set_helper(Some(completion::ReplHelper));
    let _ = rl.load_history(&history_path);

    let mut state = state::ReplState::new(ctx);

    println!(
        "FerriSync {} — interactive shell",
        env!("CARGO_PKG_VERSION")
    );
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

fn print_help() {
    println!(
        "Commands:
  help                          Show this help
  status                        Show paired devices and sync folders
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