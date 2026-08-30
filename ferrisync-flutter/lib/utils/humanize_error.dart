/// Maps raw engine/exception strings to a short, human-readable headline so
/// the UI never shows stack-trace noise. Broadly worded on purpose: privacy
/// first, actionable second.
String humanizeError(Object? error, {String? fallback}) {
  final text = error?.toString() ?? '';
  final lc = text.toLowerCase();

  final known = [
    if (lc.contains('could not reach') ||
        lc.contains('timed out') ||
        lc.contains('connection refused') ||
        lc.contains('unreachable'))
      "Couldn't connect to the other device — make sure its app is open "
          'on the same local network.',
    if (lc.contains('tofu') || lc.contains('certificate') && lc.contains('regenerated'))
      "The other device's security key changed. Re-pair it to continue.",
    if (lc.contains('hash mismatch'))
      "A file didn't verify after transfer. Tap Retry to sync it again.",
    if (lc.contains('permission') || lc.contains('denied'))
      'The app was denied access it needs. Check its permissions.',
    if (lc.contains('no known address'))
      "That folder has no saved address for its peer — pair the device again.",
    if (lc.contains('stale device'))
      "That folder points at this device itself. Re-pair it with the real peer.",
    if (lc.contains('metadata.db'))
      'The engine database could not be read. Try restarting the app.',
  ].where((m) => m.isNotEmpty);

  return known.isEmpty
      ? (fallback ?? 'Something went wrong — tap Retry and we\'ll try again.')
      : known.first;
}