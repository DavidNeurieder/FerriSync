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