use anyhow::anyhow;
use ferrisync_core::diagnostics::{self, CheckStatus, DiagnosticCheck};
use serde::Serialize;

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
        let report = DoctorReport::from_checks(&checks);
        let out = serde_json::to_string_pretty(&report)?;
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

/// Structured, machine-readable result of a full doctor run.
#[derive(Serialize)]
struct DoctorReport {
    healthy: bool,
    summary: CheckSummary,
    checks: Vec<DiagnosticCheck>,
}

#[derive(Serialize)]
struct CheckSummary {
    total: usize,
    passed: usize,
    warnings: usize,
    failures: usize,
}

impl DoctorReport {
    fn from_checks(checks: &[DiagnosticCheck]) -> Self {
        let mut summary = CheckSummary {
            total: checks.len(),
            passed: 0,
            warnings: 0,
            failures: 0,
        };
        for c in checks {
            match c.status {
                CheckStatus::Pass => summary.passed += 1,
                CheckStatus::Warn => summary.warnings += 1,
                CheckStatus::Fail => summary.failures += 1,
                CheckStatus::Info => {}
            }
        }
        DoctorReport {
            healthy: summary.failures == 0,
            summary,
            checks: checks.to_vec(),
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn check(name: &str, status: CheckStatus, hints: &[&str]) -> DiagnosticCheck {
        DiagnosticCheck {
            name: name.into(),
            status,
            message: format!("{name} message"),
            hints: hints.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn render(checks: &[DiagnosticCheck]) -> serde_json::Value {
        let report = DoctorReport::from_checks(checks);
        serde_json::to_value(&report).unwrap()
    }

    #[test]
    fn json_reports_healthy_when_nothing_fails() {
        let v = render(&[
            check("identity", CheckStatus::Pass, &[]),
            check("storage", CheckStatus::Warn, &["check disk space"]),
        ]);
        assert_eq!(v["healthy"], true, "{v}");
        assert_eq!(v["summary"]["total"], 2, "{v}");
        assert_eq!(v["summary"]["warnings"], 1, "{v}");
    }

    #[test]
    fn json_reports_unhealthy_when_any_check_fails() {
        let v = render(&[
            check("storage", CheckStatus::Pass, &[]),
            check("network", CheckStatus::Fail, &["Check firewall rules"]),
        ]);
        assert_eq!(v["healthy"], false, "{v}");
        assert_eq!(v["summary"]["failures"], 1, "{v}");
        // A failing check carries its remediation hints.
        assert_eq!(v["checks"][1]["status"], "fail", "{v}");
        assert_eq!(v["checks"][1]["hints"][0], "Check firewall rules", "{v}");
    }

    #[test]
    fn json_status_uses_lowercase_vocabulary() {
        let v = render(&[
            check("identity", CheckStatus::Pass, &[]),
            check("port", CheckStatus::Fail, &[]),
            check("mdns", CheckStatus::Warn, &[]),
            check("data_dir", CheckStatus::Info, &[]),
        ]);
        let statuses: Vec<&str> = v["checks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["status"].as_str().unwrap())
            .collect();
        assert_eq!(statuses, ["pass", "fail", "warn", "info"], "{v}");
    }
}
