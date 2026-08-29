pub mod args;
pub mod device;
pub mod input;
pub mod pair;
pub mod remove;
pub mod rename;
pub mod serve;
pub mod status;
pub mod sync;
pub mod watch;

use crate::app::ApplicationContext;
use crate::cli::Commands;

pub const DEFAULT_PORT: u16 = 9847;

pub use device::{ensure_device, parse_device, resolve_device_key};

/// Dispatch a parsed CLI subcommand through the shared application context.
pub async fn run(command: Commands, ctx: &ApplicationContext) -> anyhow::Result<()> {
    match command {
        Commands::Pair { ip, port } => pair::run(ctx, &ip, port).await,
        Commands::Sync(args) => sync::run(ctx, &args).await,
        Commands::Status => {
            let status = status::run(ctx)?;
            print!("{}", status::format(&status));
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
    }
}