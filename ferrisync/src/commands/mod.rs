pub mod activity;
pub mod add;
pub mod args;
pub mod conflicts;
pub mod device;
pub mod devices;
pub mod doctor;
pub mod fmt;
pub mod folders;
pub mod folders_pair;
pub mod input;
pub mod pair;
pub mod remove;
pub mod rename;
pub mod reset;
pub mod serve;
pub mod share;
pub mod status;
pub mod sync;
pub mod watch;

use crate::app::ApplicationContext;
use crate::cli::{Commands, DevicesCommand, FoldersCommand, ShareCommand};

pub const DEFAULT_PORT: u16 = 9847;

pub use device::{ensure_device, resolve_device_key};

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
        Commands::Remove { device_id, yes } => {
            let (id, _name) =
                device::resolve_device_id(&ctx.storage, &device_id, &ctx.device_info.id)?;
            remove::run(ctx, &id, yes).await
        }
        Commands::Reset { yes } => reset::run(ctx, yes).await,
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
            FoldersCommand::Status { path } => folders::status(ctx, &path),
            FoldersCommand::AddDevice {
                path,
                device,
                remote_path,
                mode,
            } => folders::add_device(ctx, &path, &device, remote_path.as_deref(), &mode).await,
            FoldersCommand::RemoveDevice { path, device, yes } => {
                folders::remove_device(ctx, &path, &device, yes).await
            }
            FoldersCommand::Browse { ip, port } => folders_pair::browse(ctx, &ip, port).await,
            FoldersCommand::Request {
                ip,
                port,
                guid,
                path,
                name,
                seconds,
            } => {
                folders_pair::request(ctx, &ip, port, &guid, &path, name.as_deref(), seconds).await
            }
            FoldersCommand::Approve { device, guid } => folders_pair::approve(ctx, &device, &guid),
            FoldersCommand::Deny { device, guid } => folders_pair::deny(ctx, &device, &guid),
        },
        Commands::Add { path, name } => add::run(ctx, &path, name.as_deref()),
        Commands::Share { cmd } => match cmd {
            ShareCommand::List => share::list(ctx, json),
            ShareCommand::Add { path, name } => share::add(ctx, &path, name.as_deref()),
            ShareCommand::Discover { share_id, enabled } => {
                share::discover(ctx, share_id, enabled.unwrap_or(true))
            }
            ShareCommand::Off { share_id } => share::off(ctx, share_id),
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
