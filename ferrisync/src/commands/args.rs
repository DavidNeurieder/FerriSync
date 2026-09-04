use clap::Args;

/// One-shot folder sync (no args: sync all configured folders)
#[derive(Debug, Clone, PartialEq, Args)]
pub struct SyncArgs {
    /// Local folder path
    #[arg(requires = "device")]
    pub folder: Option<String>,
    /// Target device ID (ip[:port], paired device name, or uuid)
    #[arg(long, requires = "folder")]
    pub device: Option<String>,
    /// Keep retrying an unreachable peer for this many seconds
    #[arg(long, default_value_t = 0)]
    pub wait: u64,
    /// Show what a sync would do (per reconciled pair) without transferring
    #[arg(long)]
    pub dry_run: bool,
}

/// Continuous foreground sync with live log
#[derive(Debug, Clone, PartialEq, Args)]
pub struct WatchArgs {
    /// Local folder path
    pub folder: String,
    /// Target device. Can be: paired device name or device UUID
    #[arg(long)]
    pub device: String,
}
