//! `ferrisync add <path>` — make a folder available to be discovered and
//! synced.
//!
//! Publishing a folder is a one-shot storage operation: it registers the folder
//! as a configured sync folder and publishes it as a discoverable share. No
//! server is spawned here — the folder is served automatically whenever a
//! FerriSync process (app or REPL) is running, because it now appears in
//! `list_sync_folders`, which both `init_engine` and the REPL's
//! `auto_serve_existing` serve on launch.

use anyhow::Result;

use crate::app::ApplicationContext;

/// `ferrisync add <path> [--name <name>]` — publish a folder so paired devices
/// can discover and request to sync it. Idempotent: re-adding the same path is
/// a no-op ("Already shared").
pub fn run(ctx: &ApplicationContext, path: &str, name: Option<&str>) -> Result<()> {
    if !std::path::Path::new(path).is_dir() {
        anyhow::bail!("'{path}' is not a directory");
    }
    super::share::add(ctx, path, name)?;
    Ok(())
}
