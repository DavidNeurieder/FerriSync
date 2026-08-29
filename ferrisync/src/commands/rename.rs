use anyhow::Result;

use crate::app::ApplicationContext;

pub async fn run(ctx: &ApplicationContext, name: &str) -> Result<()> {
    match ferrisync_core::sanitize_device_name(name) {
        Ok(clean) => {
            ferrisync_core::persist_device_name(&ctx.data_dir, &clean);
            println!("Renamed to '{clean}'.");
            println!(
                "Already-running 'serve' processes keep the old name until restarted."
            );
            Ok(())
        }
        Err(e) => {
            anyhow::bail!("{e:#}");
        }
    }
}