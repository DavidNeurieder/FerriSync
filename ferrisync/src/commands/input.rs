/// Read one line from stdin and interpret it as an approval decision.
/// Anything other than y/yes counts as "no" (including EOF).
pub(crate) async fn read_yes_no() -> bool {
    use tokio::io::AsyncBufReadExt;
    let mut line = String::new();
    let _ = tokio::io::BufReader::new(tokio::io::stdin())
        .read_line(&mut line)
        .await;
    matches!(line.trim().to_lowercase().as_str(), "y" | "yes")
}