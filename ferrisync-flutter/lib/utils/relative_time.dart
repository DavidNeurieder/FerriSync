/// Compact "5m ago" / "just now" formatting for unix-epoch timestamps.
String relativeTime(int? unixSec) {
  if (unixSec == null || unixSec == 0) return 'never';
  final dt = DateTime.fromMillisecondsSinceEpoch(unixSec * 1000);
  final diff = DateTime.now().difference(dt);
  if (diff.inSeconds < 60) return 'just now';
  if (diff.inMinutes < 60) return '${diff.inMinutes}m ago';
  if (diff.inHours < 24) return '${diff.inHours}h ago';
  if (diff.inDays < 7) return '${diff.inDays}d ago';
  return '${dt.year}-${dt.month.toString().padLeft(2, '0')}-${dt.day.toString().padLeft(2, '0')}';
}

/// Full local timestamp ("2026-08-30 17:42") for detail views.
String verboseTime(int? unixSec) {
  if (unixSec == null || unixSec == 0) return 'never';
  final dt = DateTime.fromMillisecondsSinceEpoch(unixSec * 1000);
  final hh = dt.hour.toString().padLeft(2, '0');
  final mm = dt.minute.toString().padLeft(2, '0');
  return '${dt.year}-${dt.month.toString().padLeft(2, '0')}-'
      '${dt.day.toString().padLeft(2, '0')} $hh:$mm';
}