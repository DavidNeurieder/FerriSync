use clap::{Parser, Subcommand};

use crate::commands::args::{SyncArgs, WatchArgs};
use crate::commands::DEFAULT_PORT;

#[derive(Parser)]
#[command(
    name = "ferrisync",
    version,
    about = "Decentralized folder sync",
    help_template = "\
{about}

USAGE
    {usage}

EVERYDAY
    status           Show synchronization status
    sync             Synchronize folders
    watch            Continuously synchronize a folder
    add              Publish a folder so it can be discovered & synced
    folders          List and manage sync folders
    share            Publish/manage shared folders others can request
    activity         Recent sync sessions and file changes
    conflicts        List unresolved conflicts
    conflict-resolve Resolve a conflict keeping one version

DEVICES
    devices          List and manage paired devices
    pair             Pair with a device by address
    rename           Change this device's network name
    remove           Remove a paired device

SERVER
    serve            Start the FerriSync server

MANAGEMENT
    reset            Restore the device to a fresh-install state

DIAGNOSTICS
    doctor           Run on-device diagnostics

Run `ferrisync <COMMAND> --help` for details.

OPTIONS
{options}
"
)]
pub struct Cli {
    /// Data directory (default: ~/.local/share/ferrisync)
    #[arg(long, default_value = "")]
    pub data_dir: String,

    /// Emit machine-readable JSON where a command supports it
    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Show pairing and sync status (presence + folder health by default)
    Status {
        /// Raw view: device ids and absolute timestamps
        #[arg(long)]
        verbose: bool,
    },
    /// One-shot folder sync (no args: sync all configured folders)
    Sync(SyncArgs),
    /// Continuously synchronize a folder with a device
    Watch(WatchArgs),
    /// Publish a folder so paired devices can discover and sync it
    Add {
        /// Local directory to make discoverable
        path: String,
        /// Optional share display name
        #[arg(long)]
        name: Option<String>,
    },
    /// List and manage sync folders (list is the default)
    Folders {
        #[command(subcommand)]
        cmd: Option<FoldersCommand>,
    },
    /// Publish and manage shared folders others can request
    Share {
        #[command(subcommand)]
        cmd: ShareCommand,
    },
    /// Recent sync sessions and file changes
    Activity {
        /// Number of entries per section (default: 15)
        #[arg(long, default_value_t = 15)]
        limit: u32,
    },
    /// List unresolved conflicts (add --folder <path> to filter)
    Conflicts {
        /// Only show conflicts in this folder
        #[arg(long)]
        folder: Option<String>,
    },
    /// Resolve a conflict keeping one version
    ConflictResolve {
        /// Real (winner) file path, as listed by `ferrisync conflicts`
        path: String,
        /// Which version to keep: this, other, or both
        #[arg(long)]
        keep: String,
    },

    /// Pair with a device by address
    Pair {
        /// IP address of the target device
        ip: String,
        /// Port (default: 9847)
        #[arg(long, default_value_t = DEFAULT_PORT)]
        port: u16,
    },
    /// List and manage paired devices (list is the default)
    Devices {
        #[command(subcommand)]
        cmd: Option<DevicesCommand>,
    },
    /// Change this device's network name (visible to peers)
    Rename {
        /// The new display name (max 64 characters)
        name: String,
    },
    /// Remove a paired device and all its associated data
    Remove {
        /// Paired device (name or id)
        #[arg(value_name = "NAME")]
        device_id: String,
        /// Skip the confirmation prompt
        #[arg(long)]
        yes: bool,
    },

    /// Restore the device to a fresh-install state (new identity, no devices
    /// or folders). User files are never touched.
    Reset {
        /// Skip the confirmation prompt
        #[arg(long)]
        yes: bool,
    },

    /// Listen for incoming sync connections
    Serve {
        /// Listen port (default: 9847)
        #[arg(long, default_value_t = DEFAULT_PORT)]
        port: u16,
        /// Accept pairing requests from unknown devices without confirmation
        #[arg(long)]
        auto_accept: bool,
        /// Local folder path to serve (default: all configured folders)
        folder: Option<String>,
    },

    /// Run on-device diagnostics
    Doctor {
        /// Print the actionable hints for one check (e.g. firewall)
        #[arg(long)]
        explain: Option<String>,
    },
}

/// `ferrisync devices` subcommands. Bare `devices` lists.
#[derive(Subcommand, Debug)]
pub enum DevicesCommand {
    /// List paired devices with presence and folder counts
    List,
    /// Scan the LAN for nearby FerriSync devices
    Discover {
        /// Seconds to listen (default: 4)
        #[arg(long, default_value_t = 4)]
        seconds: u32,
    },
    /// Pair with a device by IP, or browse interactively when no IP is given
    Pair {
        /// IP address of the target device
        ip: Option<String>,
        /// Port (default: 9847)
        #[arg(long, default_value_t = DEFAULT_PORT)]
        port: u16,
    },
    /// Rename a paired device (by name or id)
    Rename {
        /// Device to rename (name or id)
        device: String,
        /// New display name
        name: String,
    },
    /// Remove a paired device and all associated data
    Remove {
        /// Device to remove (name or id)
        device: String,
        /// Skip the confirmation prompt
        #[arg(long)]
        yes: bool,
    },
}

/// `ferrisync folders` subcommands. Bare `folders` lists.
#[derive(Subcommand, Debug)]
pub enum FoldersCommand {
    /// List sync folders with derived health
    List,
    /// Add a sync folder for a paired device
    Add {
        /// Local directory to sync
        path: String,
        /// Paired device (name or id) to sync against
        #[arg(long)]
        device: String,
    },
    /// Remove a sync folder (all history for it is deleted)
    Remove {
        /// Local directory to stop syncing
        path: String,
        /// Only remove the entry pointing at this device
        #[arg(long)]
        device: Option<String>,
        /// Skip the confirmation prompt
        #[arg(long)]
        yes: bool,
    },
    /// List every device a folder syncs with, per-pair mode + remote path
    Status {
        /// Local directory of the folder to inspect
        path: String,
    },
    /// Attach an additional paired device to an existing folder
    AddDevice {
        /// Local directory of the folder
        path: String,
        /// Paired device (name or id) to sync against
        #[arg(long)]
        device: String,
        /// Where this folder lives on the paired device (defaults to the local path)
        #[arg(long)]
        remote_path: Option<String>,
        /// Per-pair sync mode (bidirectional | send-only | receive-only)
        #[arg(long, default_value = "bidirectional")]
        mode: String,
    },
    /// Detach a paired device from a folder. Never deletes files — only the pairing.
    RemoveDevice {
        /// Local directory of the folder
        path: String,
        /// Paired device (name or id) to detach
        #[arg(long)]
        device: String,
        /// Skip the confirmation prompt
        #[arg(long)]
        yes: bool,
    },
    /// List a paired device's discoverable shared folders over TLS
    Browse {
        /// IP address of the paired device
        ip: String,
        /// Port of the paired device's server (default: 9847)
        #[arg(long, default_value_t = DEFAULT_PORT)]
        port: u16,
    },
    /// Request pairing to a remote shared folder and poll until approved
    Request {
        /// IP address of the paired device
        ip: String,
        /// Port of the paired device's server (default: 9847)
        #[arg(long, default_value_t = DEFAULT_PORT)]
        port: u16,
        /// The remote folder's stable guid (from `folders browse`)
        guid: String,
        /// Local directory to keep the peer's copy
        #[arg(long)]
        path: String,
        /// Optional share display name
        #[arg(long)]
        name: Option<String>,
        /// Seconds to keep polling for the owner's approval (default: 300)
        #[arg(long, default_value_t = 300)]
        seconds: u64,
    },
    /// Approve a folder-pairing request (only works while serving interactively)
    Approve {
        /// Requesting device (name or id)
        device: String,
        /// The shared folder's stable guid
        guid: String,
    },
    /// Deny a folder-pairing request (only works while serving interactively)
    Deny {
        /// Requesting device (name or id)
        device: String,
        /// The shared folder's stable guid
        guid: String,
    },
}

/// `ferrisync share` subcommands.
#[derive(Subcommand, Debug)]
pub enum ShareCommand {
    /// List this device's published shared folders
    List,
    /// Publish a local folder as a discoverable shared folder
    Add {
        /// Local directory to share
        path: String,
        /// Optional display name (defaults to the folder label)
        #[arg(long)]
        name: Option<String>,
    },
    /// Toggle whether a published share is visible to trusted peers
    Discover {
        /// Share id (run `share list`)
        share_id: i64,
        /// true to make it discoverable, false to hide it (default: true)
        #[arg(long, value_parser = clap::builder::BoolishValueParser::new())]
        enabled: Option<bool>,
    },
    /// Stop sharing a folder (existing peer pairs are kept)
    Off {
        /// Share id (run `share list`)
        share_id: i64,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn no_subcommand_is_none() {
        let cli = Cli::try_parse_from(["ferrisync", "--data-dir", "/tmp/x"]).unwrap();
        assert!(cli.command.is_none(), "no subcommand means the REPL runs");
    }

    #[test]
    fn subcommand_is_some() {
        for args in [
            vec!["status"],
            vec!["pair", "192.168.1.5"],
            vec!["sync"],
            vec!["serve", "/tmp/fold"],
            vec!["serve"],
            vec!["serve", "--auto-accept"],
            vec!["watch", "/tmp/fold", "--device", "192.168.1.5:9847"],
            vec!["rename", "Mr Desktop"],
            vec!["remove", "some-uuid"],
        ] {
            let cli = Cli::try_parse_from(
                ["ferrisync", "--data-dir", "/tmp/x"]
                    .iter()
                    .chain(args.iter()),
            )
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
            vec!["status", "--verbose"],
            vec!["watch", "/tmp/fold", "--device", "192.168.1.5:9847"],
            vec!["add", "/tmp/fold"],
            vec!["add", "/tmp/fold", "--name", "Docs"],
            vec!["serve", "/tmp/fold"],
            vec!["serve", "/tmp/fold", "--port", "9000", "--auto-accept"],
            vec!["serve"],
            vec!["serve", "--auto-accept"],
            vec!["rename", "Mr Desktop"],
            vec!["remove", "some-uuid"],
            vec!["remove", "some-uuid", "--yes"],
            vec!["reset"],
            vec!["reset", "--yes"],
            vec!["devices"],
            vec!["devices", "list"],
            vec!["devices", "discover"],
            vec!["devices", "discover", "--seconds", "7"],
            vec!["devices", "pair"],
            vec!["devices", "pair", "192.168.1.5"],
            vec!["devices", "pair", "192.168.1.5", "--port", "7000"],
            vec!["devices", "rename", "Pixel 9", "PixelX"],
            vec!["devices", "remove", "Pixel 9"],
            vec!["devices", "remove", "Pixel 9", "--yes"],
            vec!["folders"],
            vec!["folders", "list"],
            vec!["folders", "add", "/tmp/f", "--device", "Pixel 9"],
            vec!["folders", "remove", "/tmp/f"],
            vec![
                "folders", "remove", "/tmp/f", "--device", "Pixel 9", "--yes",
            ],
            vec!["folders", "status", "/tmp/f"],
            vec!["folders", "add-device", "/tmp/f", "--device", "Phone"],
            vec![
                "folders",
                "add-device",
                "/tmp/f",
                "--device",
                "Phone",
                "--remote-path",
                "/Documents",
                "--mode",
                "send-only",
            ],
            vec!["folders", "remove-device", "/tmp/f", "--device", "Phone"],
            vec![
                "folders",
                "remove-device",
                "/tmp/f",
                "--device",
                "Phone",
                "--yes",
            ],
            vec!["activity"],
            vec!["activity", "--limit", "5", "--json"],
            vec!["conflicts"],
            vec!["conflicts", "--folder", "/tmp/f"],
            vec!["conflict-resolve", "notes.txt", "--keep", "this"],
            vec!["doctor"],
            vec!["doctor", "--explain", "firewall"],
            vec!["doctor", "--json"],
            vec!["share", "list"],
            vec!["share", "add", "/tmp/shared"],
            vec!["share", "add", "/tmp/shared", "--name", "Docs"],
            vec!["share", "discover", "3"],
            vec!["share", "discover", "3", "--enabled", "false"],
            vec!["share", "off", "3"],
            vec!["folders", "browse", "192.168.1.5"],
            vec!["folders", "browse", "192.168.1.5", "--port", "9000"],
            vec![
                "folders",
                "request",
                "192.168.1.5",
                "folder-1",
                "--path",
                "/tmp/copy",
            ],
            vec![
                "folders",
                "request",
                "192.168.1.5",
                "folder-1",
                "--path",
                "/tmp/copy",
                "--name",
                "Docs",
                "--seconds",
                "30",
            ],
            vec!["folders", "approve", "dev-1", "folder-1"],
            vec!["folders", "deny", "dev-1", "folder-1"],
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
