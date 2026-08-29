use anyhow::Result;
use std::path::Path;

pub async fn run(name: &str, data_dir: &Path) -> Result<()> {
    match ferrisync_core::sanitize_device_name(name) {
        Ok(clean) => {
            ferrisync_core::persist_device_name(data_dir, &clean);
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