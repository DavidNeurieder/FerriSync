use anyhow::Result;

use crate::app::ApplicationContext;

use super::input::read_yes_no;

pub async fn run(ctx: &ApplicationContext, yes: bool) -> Result<()> {
    if !yes {
        println!(
            "Factory reset restores this device to a fresh-install state:\n\
             \x20 - deletes the local identity (a new device id is generated on next start)\n\
             \x20 - unpairs every device\n\
             \x20 - removes all folders, shares, history and metadata\n\
             \x20 - keeps your local files untouched"
        );
        print!("Continue? [y/N] ");
        use std::io::Write as _;
        let _ = std::io::stdout().flush();
        if !read_yes_no().await {
            println!("Aborted.");
            return Ok(());
        }
    }

    ctx.reset().await?;
    println!(
        "Device reset to a fresh install. A new device id will be generated on the next start."
    );
    Ok(())
}
