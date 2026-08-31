mod app;
mod cli;
mod commands;
mod repl;

use clap::Parser;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let cli = cli::Cli::parse();
    let mut ctx = app::ApplicationContext::new(cli.data_dir).await?;

    match cli.command {
        Some(command) => match commands::run(command, &ctx, cli.json).await {
            Ok(()) => Ok(()),
            Err(e) => {
                eprintln!("error: {}", commands::fmt::friendly_error(&e));
                Err(e)
            }
        },
        None => match repl::run(&mut ctx).await {
            Ok(()) => Ok(()),
            Err(e) => {
                eprintln!("error: {}", commands::fmt::friendly_error(&e));
                Err(e)
            }
        },
    }
}
