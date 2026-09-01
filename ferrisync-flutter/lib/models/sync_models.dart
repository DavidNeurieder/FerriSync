/// How recently a paired device was last heard from. Mirrors the shared
/// semantic state in ferrisync-core so every frontend speaks one vocabulary.
enum Presence {
  connected,
  recentlySeen,
  offline,
}

class Device {
  final String id;
  final String name;
  final int lastSeen;
  final Presence presence;

  Device({
    required this.id,
    required this.name,
    required this.lastSeen,
    this.presence = Presence.offline,
  });

  /// A "connected" device is effectively online for the user.
  bool get isOnline => presence != Presence.offline;

  String get lastSeenFormatted {
    if (lastSeen == 0) return 'never';
    final dt = DateTime.fromMillisecondsSinceEpoch(lastSeen * 1000);
    final diff = DateTime.now().difference(dt);
    if (diff.inSeconds < 60) return 'just now';
    if (diff.inMinutes < 60) return '${diff.inMinutes}m ago';
    if (diff.inHours < 24) return '${diff.inHours}h ago';
    return '${diff.inDays}d ago';
  }
}

/// What a configured folder currently means for the user. Mirrors the shared
/// model in ferrisync-core.
enum FolderHealth {
  healthy,
  syncing,
  waiting,
  offline,
  error,
  conflict,
  notConfigured;

  /// Whether this health should surface in an "attention" section.
  bool get needsAttention => switch (this) {
        FolderHealth.conflict ||
        FolderHealth.error ||
        FolderHealth.offline =>
          true,
        _ => false,
      };
}

class SyncFolder {
  final int id;
  final String localPath;
  /// User-facing display name (derived from the path by default).
  final String name;
  /// Primary peer device id (first non-self pair).
  final String deviceId;
  /// Primary peer's mode.
  final String direction;
  /// Every paired device and its per-device mode.
  final List<FolderPeer> peers;
  final int lastSyncAt;
  final FolderHealth health;
  /// Display name of the peer device, when known.
  final String? deviceName;
  /// Number of unresolved conflict backups in this folder.
  final int conflicts;

  SyncFolder({
    required this.id,
    required this.localPath,
    required this.deviceId,
    required this.direction,
    required this.lastSyncAt,
    String? name,
    List<FolderPeer>? peers,
    this.health = FolderHealth.notConfigured,
    this.deviceName,
    this.conflicts = 0,
  })  : name = name ?? _pathLabel(localPath),
        peers = peers ??
            [
              FolderPeer(
                deviceId: deviceId,
                mode: direction,
              )
            ];

  /// Convenience: whether the folder syncs with more than one device.
  bool get isMultiDevice => peers.length > 1;

  /// A peer by device id, or null.
  FolderPeer? peerFor(String deviceId) {
    for (final p in peers) {
      if (p.deviceId == deviceId) return p;
    }
    return null;
  }

  String get lastSyncFormatted {
    if (lastSyncAt == 0) return 'never';
    final dt = DateTime.fromMillisecondsSinceEpoch(lastSyncAt * 1000);
    return '${dt.hour}:${dt.minute.toString().padLeft(2, '0')}';
  }

  static String _pathLabel(String path) {
    final parts = path
        .split(RegExp(r'[/\\]'))
        .where((s) => s.isNotEmpty)
        .toList();
    return parts.isEmpty ? path : parts.last;
  }
}

/// One folder ↔ device relationship with its own sync mode.
class FolderPeer {
  final String deviceId;
  final String mode;
  final String? remotePath;
  final bool enabled;

  const FolderPeer({
    required this.deviceId,
    required this.mode,
    this.remotePath,
    this.enabled = true,
  });
}

enum SyncStatus { idle, syncing, error }

/// High-level "is everything OK?" answer shown by the dashboard hero.
enum FerriStatus { healthy, syncing, attention, offline }

/// One actionable thing that needs the user's attention.
enum AttentionKind { conflictFiles, offlineDevice, folderHealth }

class AttentionItem {
  final AttentionKind kind;
  final String label;

  const AttentionItem({required this.kind, required this.label});
}

class SyncEvent {
  final String folderId;
  final SyncStatus status;
  final String? message;

  SyncEvent({
    required this.folderId,
    required this.status,
    this.message,
  });
}

/// A dry-run plan for a folder vs. one peer: what a real sync would transfer.
class SyncPreview {
  final int wouldPush;
  final int wouldPull;
  final int wouldConflict;
  final int pushBytes;
  final int pullBytes;

  const SyncPreview({
    required this.wouldPush,
    required this.wouldPull,
    required this.wouldConflict,
    required this.pushBytes,
    required this.pullBytes,
  });

  /// Build a preview from a native dry-run `SyncResult`.
  factory SyncPreview.fromResult(dynamic result) {
    return SyncPreview(
      wouldPush: result.pushed.length,
      wouldPull: result.pulled.length,
      wouldConflict: result.conflicts.length,
      pushBytes: (result.pushedBytes as BigInt).toInt(),
      pullBytes: (result.pulledBytes as BigInt).toInt(),
    );
  }

  bool get isEmpty => wouldPush + wouldPull + wouldConflict == 0;
}
