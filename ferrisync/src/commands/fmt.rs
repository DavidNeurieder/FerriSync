//! Shared human-facing formatting helpers for CLI output.

/// "2h ago" style relative time, or "never" when absent.
pub fn relative(secs: Option<i64>) -> String {
    let Some(ts) = secs else {
        return "never".into();
    };
    let diff = chrono::Utc::now().timestamp().saturating_sub(ts);
    if diff < 60 {
        "just now".into()
    } else if diff < 3600 {
        format!("{}m ago", diff / 60)
    } else if diff < 86_400 {
        format!("{}h ago", diff / 3600)
    } else {
        format!("{}d ago", diff / 86_400)
    }
}

/// ISO-ish timestamp (`2026-08-31 09:14`) for verbose listings.
pub fn iso(secs: Option<i64>) -> String {
    match secs {
        Some(ts) => chrono::DateTime::from_timestamp(ts, 0)
            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
            .unwrap_or_else(|| ts.to_string()),
        None => "never".into(),
    }
}

/// Pretty-printed "123.4 MB" style size (used by activity summaries).
pub fn bytes_human(bytes: f64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", value as u64, UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// Turn a terse sync error into a WHAT/WHY/NEXT style hint.
pub fn friendly_error(e: &anyhow::Error) -> String {
    let s = format!("{e:#}");
    let lower = s.to_lowercase();
    let (hint, next) = if lower.contains("could not reach") || lower.contains("connect/tls") {
        (
            "peer app may be closed, or a firewall/port is blocking it",
            "run `ferrisync doctor`, or sync an ip[:port] explicitly",
        )
    } else if lower.contains("timed out") {
        (
            "the peer did not respond in time",
            "try again, or run `ferrisync doctor` to check the network",
        )
    } else if lower.contains("refused") {
        (
            "the peer is not serving this folder",
            "make sure `serve` is running on it for this folder",
        )
    } else {
        ("", "")
    };
    if hint.is_empty() {
        s
    } else {
        format!("{s} — {hint}. Next: {next}.")
    }
}

#[cfg(test)]
mod tests {
    use super::friendly_error;

    #[test]
    fn reach_error_gets_actionable_hint() {
        let e = anyhow::anyhow!("could not reach 192.168.1.5:9847");
        let out = friendly_error(&e);
        assert!(out.contains("could not reach"), "{out}");
        assert!(out.contains("firewall"), "{out}");
        assert!(out.contains("Next:"), "{out}");
    }

    #[test]
    fn unrelated_error_returns_verbatim() {
        let e = anyhow::anyhow!("boom: something unrelated");
        let out = friendly_error(&e);
        assert_eq!(out, "boom: something unrelated");
    }
}
