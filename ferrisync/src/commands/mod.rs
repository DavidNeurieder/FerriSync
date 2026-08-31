pub mod activity;
pub mod args;
pub mod conflicts;
pub mod device;
pub mod devices;
pub mod doctor;
pub mod fmt;
pub mod folders;
pub mod input;
pub mod pair;
pub mod remove;
pub mod rename;
pub mod serve;
pub mod status;
pub mod sync;
pub mod watch;

use crate::app::ApplicationContext;
use crate::cli::{Commands, DevicesCommand, FoldersCommand};

pub const DEFAULT_PORT: u16 = 9847;

pub use device::{ensure_device, parse_device, resolve_device_key};

/// Dispatch a parsed CLI subcommand through the shared application context.
pub async fn run(command: Commands, ctx: &ApplicationContext, json: bool) -> anyhow::Result<()> {
    match command {
        Commands::Pair { ip, port } => pair::run(ctx, &ip, port).await,
        Commands::Sync(args) => sync::run(ctx, &args).await,
        Commands::Status { verbose } => {
            let status = status::run(ctx)?;
            if json {
                print!("{}", status::format_json(&status));
            } else {
                print!("{}", status::format_human(&status, verbose));
            }
            Ok(())
        }
        Commands::Watch(args) => watch::run(ctx, &args).await,
        Commands::Serve {
            port,
            auto_accept,
            folder,
        } => serve::run(ctx, &folder, port, auto_accept).await,
        Commands::Rename { name } => rename::run(ctx, &name).await,
        Commands::Remove { device_id, yes } => remove::run(ctx, &device_id, yes).await,
        Commands::Devices { cmd } => match cmd.unwrap_or(DevicesCommand::List) {
            DevicesCommand::List => devices::list(ctx, json),
            DevicesCommand::Discover { seconds } => devices::discover(ctx, seconds).await,
            DevicesCommand::Pair { ip, port } => devices::pair(ctx, ip, port).await,
            DevicesCommand::Rename { device, name } => devices::rename(ctx, &device, &name),
            DevicesCommand::Remove { device, yes } => devices::remove(ctx, &device, yes).await,
        },
        Commands::Folders { cmd } => match cmd.unwrap_or(FoldersCommand::List) {
            FoldersCommand::List => folders::list(ctx, json),
            FoldersCommand::Add { path, device } => folders::add(ctx, &path, &device).await,
            FoldersCommand::Remove { path, device, yes } => {
                folders::remove(ctx, &path, device.as_deref(), yes).await
            }
        },
        Commands::Activity { limit } => activity::run(ctx, limit, json),
        Commands::Conflicts { folder } => conflicts::list(ctx, folder.as_deref()),
        Commands::ConflictResolve { path, keep } => conflicts::resolve(ctx, &path, &keep).await,
        Commands::Doctor { explain } => {
            let checks = doctor::run(ctx, json).await?;
            if let Some(name) = explain {
                doctor::explain(&checks, &name)?;
            }
            Ok(())
        }
    }
}