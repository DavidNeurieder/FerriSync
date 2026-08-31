use anyhow::anyhow;
use ferrisync_core::diagnostics::{self, CheckStatus, DiagnosticCheck};

use crate::app::ApplicationContext;

/// `ferrisync doctor [--json]` — run the on-device diagnostics.
pub async fn run(ctx: &ApplicationContext, json: bool) -> anyhow::Result<Vec<DiagnosticCheck>> {
    let checks = diagnostics::run_all(diagnostics::DiagnosticsInput {
        data_dir: &ctx.data_dir,
        crypto: &ctx.crypto,
        storage: &ctx.storage,
        own_id: &ctx.device_info.id,
        own_name: &ctx.device_info.name,
        serve_port: crate::commands::DEFAULT_PORT,
    })
    .await;

    if json {
        let out = serde_json::to_string_pretty(&checks)?;
        println!("{out}");
    } else {
        println!("FerriSync diagnostics for {}", ctx.device_info.name);
        println!("──────────────────────────────────────────────");
        for c in &checks {
            println!(
                "{status:<4} {name:<18} {message}",
                status = status_tag(c.status),
                name = c.name,
                message = c.message,
            );
        }
        println!("──────────────────────────────────────────────");
        let fails = checks.iter().filter(|c| c.status == CheckStatus::Fail);
        if fails.clone().count() == 0 {
            println!("All checks passed.");
        } else {
            println!("{} check(s) FAILED:", fails.clone().count());
            for c in fails {
                println!("  {:<4} {} — {}", status_tag(c.status), c.name, c.message);
                for hint in &c.hints {
                    println!("        • {hint}");
                }
            }
            println!("Run `ferrisync doctor --explain <check>` for details.");
        }
    }
    Ok(checks)
}

/// Print the actionable hints for one named check.
pub fn explain(checks: &[DiagnosticCheck], name: &str) -> anyhow::Result<()> {
    let check = checks
        .iter()
        .find(|c| c.name == name)
        .ok_or_else(|| anyhow!("unknown check '{name}'"))?;
    println!("{} — {}", check.name, check.message);
    if check.hints.is_empty() {
        println!("  (no further guidance)");
    }
    for hint in &check.hints {
        println!("  • {hint}");
    }
    Ok(())
}

fn status_tag(s: CheckStatus) -> &'static str {
    match s {
        CheckStatus::Pass => "OK",
        CheckStatus::Fail => "FAIL",
        CheckStatus::Warn => "WARN",
        CheckStatus::Info => "INFO",
    }
}