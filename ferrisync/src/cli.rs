use clap::{Parser, Subcommand};

use crate::commands::DEFAULT_PORT;

#[derive(Parser)]
#[command(name = "ferrisync", version, about = "Decentralized folder sync")]
pub struct Cli {
    /// Data directory (default: ~/.local/share/ferrisync)
    #[arg(long, default_value = "")]
    pub data_dir: String,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Pair with a device by IP address
    Pair {
        /// IP address of the target device
        ip: String,
        /// Port (default: 9847)
        #[arg(long, default_value_t = DEFAULT_PORT)]
        port: u16,
    },
    /// One-shot folder sync (no args: sync all configured folders)
    Sync {
        /// Local folder path
        #[arg(requires = "device")]
        folder: Option<String>,
        /// Target device ID (ip[:port], paired device name, or uuid)
        #[arg(long, requires = "folder")]
        device: Option<String>,
        /// Keep retrying an unreachable peer for this many seconds
        #[arg(long, default_value_t = 0)]
        wait: u64,
    },
    /// Show pairing and sync status
    Status,
    /// Continuous foreground sync with live log
    Watch {
        /// Local folder path
        folder: String,
        /// Remote device address (IP:port)
        #[arg(long)]
        device: String,
    },
    /// Listen for incoming sync connections
    Serve {
        /// Listen port (default: 9847)
        #[arg(long, default_value_t = DEFAULT_PORT)]
        port: u16,
        /// Accept pairing requests from unknown devices without confirmation
        #[arg(long)]
        auto_accept: bool,
        /// Local folder path to serve
        folder: String,
    },
    /// Change this device's network name (visible to peers)
    Rename {
        /// The new display name (max 64 characters)
        name: String,
    },
    /// Remove a paired device and all its associated data
    Remove {
        /// Device ID (run `ferrisync status` to see paired IDs)
        device_id: String,
        /// Skip the confirmation prompt
        #[arg(long)]
        yes: bool,
    },
    /// Start the interactive shell (the no-command default is the
    /// full-screen TUI; this subcommand forces the REPL)
    Repl,
    /// Start the full-screen terminal UI
    Tui,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn no_subcommand_is_none() {
        let cli = Cli::try_parse_from(["ferrisync", "--data-dir", "/tmp/x"]).unwrap();
        assert!(cli.command.is_none(), "no subcommand means the TUI runs");
    }

    #[test]
    fn subcommand_is_some() {
        for args in [
            vec!["status"],
            vec!["pair", "192.168.1.5"],
            vec!["sync"],
            vec!["serve", "/tmp/fold"],
            vec!["watch", "/tmp/fold", "--device", "192.168.1.5:9847"],
            vec!["rename", "Mr Desktop"],
            vec!["remove", "some-uuid"],
            vec!["repl"],
            vec!["tui"],
        ] {
            let cli = Cli::try_parse_from(["ferrisync", "--data-dir", "/tmp/x"].iter().chain(args.iter()))
                .unwrap_or_else(|e| panic!("failed to parse {args:?}: {e}"));
            assert!(
                cli.command.is_some(),
                "{args:?} must select CLI mode, got {:?}",
                cli.command
            );
        }
    }

    #[test]
    fn all_command_shapes_parse() {
        for args in [
            vec!["pair", "192.168.1.5"],
            vec!["pair", "192.168.1.5", "--port", "7000"],
            vec!["sync"],
            vec!["sync", "/tmp/fold", "--device", "192.168.1.5:9847"],
            vec!["sync", "/tmp/fold", "--device", "peer-name", "--wait", "30"],
            vec!["status"],
            vec!["watch", "/tmp/fold", "--device", "192.168.1.5:9847"],
            vec!["serve", "/tmp/fold"],
            vec!["serve", "/tmp/fold", "--port", "9000", "--auto-accept"],
            vec!["rename", "Mr Desktop"],
            vec!["remove", "some-uuid"],
            vec!["remove", "some-uuid", "--yes"],
            vec!["repl"],
            vec!["tui"],
        ] {
            Cli::try_parse_from(["ferrisync"].iter().chain(args.iter()))
                .unwrap_or_else(|e| panic!("failed to parse {args:?}: {e}"));
        }
    }

    #[test]
    fn device_flag_requires_folder() {
        assert!(Cli::try_parse_from(["ferrisync", "sync", "--device", "x"]).is_err());
        assert!(Cli::try_parse_from(["ferrisync", "watch", "/tmp/f"]).is_err());
    }
}