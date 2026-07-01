class Device {
  final String id;
  final String name;
  final int lastSeen;
  final bool isOnline;

  Device({
    required this.id,
    required this.name,
    required this.lastSeen,
    this.isOnline = false,
  });

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

class SyncFolder {
  final int id;
  final String localPath;
  final String deviceId;
  final String direction;
  final int lastSyncAt;

  SyncFolder({
    required this.id,
    required this.localPath,
    required this.deviceId,
    required this.direction,
    required this.lastSyncAt,
  });

  String get lastSyncFormatted {
    if (lastSyncAt == 0) return 'never';
    final dt = DateTime.fromMillisecondsSinceEpoch(lastSyncAt * 1000);
    return '${dt.hour}:${dt.minute.toString().padLeft(2, '0')}';
  }
}

enum SyncStatus { idle, syncing, error }

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
