mod app;
mod cli;
mod commands;
mod repl;

use clap::Parser;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let cli = cli::Cli::parse();
    let ctx = app::ApplicationContext::new(cli.data_dir).await?;

    match cli.command {
        Some(command) => commands::run(command, &ctx).await,
        None => repl::run(&ctx).await,
    }
}