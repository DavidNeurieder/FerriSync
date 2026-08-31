import 'package:ferrisync/gen/sync_engine/conflicts.dart' as frb_conflicts;
import 'package:ferrisync/models/sync_models.dart';
import 'package:ferrisync/providers/sync_provider.dart';
import 'package:flutter_test/flutter_test.dart';

frb_conflicts.ConflictEntry conflict(int folderId, String backupPath) {
  return frb_conflicts.ConflictEntry(
    folderId: folderId,
    path: 'path',
    backupPath: backupPath,
    loserLabel: 'local',
    winnerMtimeSecs: 0,
    winnerSize: BigInt.zero,
    loserMtimeSecs: 0,
    loserSize: BigInt.zero,
  );
}

Device offlineDevice(String id, {String name = 'Cloud'}) {
  return Device(
    id: id,
    name: name,
    lastSeen: DateTime.now().millisecondsSinceEpoch - 3600000,
    presence: Presence.offline,
  );
}

void main() {
  group('computeAttentionParts', () {
    test('returns nothing when notifications are disabled', () {
      final notified = <String>{};
      final parts = computeAttentionParts(
        conflicts: [conflict(1, 'a.bak')],
        devices: [offlineDevice('d1')],
        notified: notified,
        enabled: false,
      );
      expect(parts, isEmpty);
      expect(notified, isEmpty);
    });

    test('announces a new conflict once', () {
      final notified = <String>{};
      final first = computeAttentionParts(
        conflicts: [conflict(1, 'a.bak')],
        devices: const [],
        notified: notified,
        enabled: true,
      );
      expect(first, ['1 conflict needs your attention']);

      // Same conflict again does not re-notify.
      final second = computeAttentionParts(
        conflicts: [conflict(1, 'a.bak')],
        devices: const [],
        notified: notified,
        enabled: true,
      );
      expect(second, isEmpty);
    });

    test('announces offline devices once', () {
      final notified = <String>{};
      final first = computeAttentionParts(
        conflicts: const [],
        devices: [offlineDevice('d1')],
        notified: notified,
        enabled: true,
      );
      expect(first, ['1 device is offline']);

      final second = computeAttentionParts(
        conflicts: const [],
        devices: [offlineDevice('d1')],
        notified: notified,
        enabled: true,
      );
      expect(second, isEmpty);
    });

    test('combines conflicts and offline into one message', () {
      final notified = <String>{};
      final parts = computeAttentionParts(
        conflicts: [conflict(1, 'a.bak'), conflict(1, 'b.bak')],
        devices: [offlineDevice('d1'), offlineDevice('d2')],
        notified: notified,
        enabled: true,
      );
      expect(parts, [
        '2 conflicts need your attention',
        '2 devices are offline',
      ]);
    });

    test('ignores devices seen recently (not offline)', () {
      final notified = <String>{};
      final parts = computeAttentionParts(
        conflicts: const [],
        devices: [
          Device(
            id: 'd1',
            name: 'Online',
            lastSeen: 1,
            presence: Presence.connected,
          ),
        ],
        notified: notified,
        enabled: true,
      );
      expect(parts, isEmpty);
    });
  });
}
