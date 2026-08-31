use crate::commands::args::{SyncArgs, WatchArgs};

/// A parsed REPL input line.
#[derive(Debug, PartialEq)]
pub enum ReplCommand {
    Help,
    Status,
    Devices,
    Folders,
    Activity,
    Conflicts,
    Doctor,
    Sessions,
    Rename {
        name: String,
    },
    Discover {
        seconds: u64,
    },
    Pair {
        ip: String,
        port: u16,
    },
    Sync(SyncArgs),
    Unsync {
        folder: Option<String>,
        device: Option<String>,
        yes: bool,
    },
    Watch(WatchArgs),
    Watches,
    Unwatch {
        id: u32,
    },
    Serve {
        folder: String,
        port: u16,
    },
    Serves,
    Unserve {
        id: u32,
    },
    Pendings,
    Confirm {
        n: u32,
    },
    Deny {
        n: u32,
    },
    Yes,
    No,
    Exit,
}
