import 'package:ferrisync/models/sync_models.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('Device', () {
    test('constructor sets fields correctly', () {
      final device = Device(id: 'abc-123', name: 'Pixel 8', lastSeen: 0);
      expect(device.id, 'abc-123');
      expect(device.name, 'Pixel 8');
      expect(device.lastSeen, 0);
      expect(device.isOnline, false);
    });

    test('isOnline defaults to false', () {
      final device = Device(id: '1', name: 'Test', lastSeen: 100);
      expect(device.isOnline, false);
    });

    test('isOnline can be set to true', () {
      final device = Device(id: '1', name: 'Test', lastSeen: 100, isOnline: true);
      expect(device.isOnline, true);
    });

    group('lastSeenFormatted', () {
      test('returns "never" when lastSeen is 0', () {
        final device = Device(id: '1', name: 'Test', lastSeen: 0);
        expect(device.lastSeenFormatted, 'never');
      });

      test('returns "just now" when less than 60 seconds ago', () {
        final now = DateTime.now().millisecondsSinceEpoch ~/ 1000;
        final device = Device(id: '1', name: 'Test', lastSeen: now - 5);
        expect(device.lastSeenFormatted, 'just now');
      });

      test('returns "Xm ago" when less than 60 minutes ago', () {
        final now = DateTime.now().millisecondsSinceEpoch ~/ 1000;
        final device = Device(id: '1', name: 'Test', lastSeen: now - 300);
        expect(device.lastSeenFormatted, '5m ago');
      });

      test('returns "Xh ago" when less than 24 hours ago', () {
        final now = DateTime.now().millisecondsSinceEpoch ~/ 1000;
        final device = Device(id: '1', name: 'Test', lastSeen: now - 7200);
        expect(device.lastSeenFormatted, '2h ago');
      });

      test('returns "Xd ago" when 24+ hours ago', () {
        final now = DateTime.now().millisecondsSinceEpoch ~/ 1000;
        final device = Device(id: '1', name: 'Test', lastSeen: now - 172800);
        expect(device.lastSeenFormatted, '2d ago');
      });
    });
  });

  group('SyncFolder', () {
    test('constructor sets fields correctly', () {
      final folder = SyncFolder(
        id: 1,
        localPath: '/home/user/sync',
        deviceId: 'dev-1',
        direction: 'bidirectional',
        lastSyncAt: 0,
      );
      expect(folder.id, 1);
      expect(folder.localPath, '/home/user/sync');
      expect(folder.deviceId, 'dev-1');
      expect(folder.direction, 'bidirectional');
      expect(folder.lastSyncAt, 0);
    });

    group('lastSyncFormatted', () {
      test('returns "never" when lastSyncAt is 0', () {
        final folder = SyncFolder(
          id: 1,
          localPath: '/path',
          deviceId: 'dev-1',
          direction: 'push',
          lastSyncAt: 0,
        );
        expect(folder.lastSyncFormatted, 'never');
      });

      test('returns formatted time when lastSyncAt is set', () {
        final epoch = DateTime(2024, 1, 15, 14, 30, 0).millisecondsSinceEpoch ~/ 1000;
        final folder = SyncFolder(
          id: 1,
          localPath: '/path',
          deviceId: 'dev-1',
          direction: 'push',
          lastSyncAt: epoch,
        );
        expect(folder.lastSyncFormatted, '14:30');
      });
    });
  });

  group('SyncStatus', () {
    test('has idle value', () {
      expect(SyncStatus.idle, isA<SyncStatus>());
    });

    test('has syncing value', () {
      expect(SyncStatus.syncing, isA<SyncStatus>());
    });

    test('has error value', () {
      expect(SyncStatus.error, isA<SyncStatus>());
    });

    test('all three values are distinct', () {
      expect(SyncStatus.idle, isNot(SyncStatus.syncing));
      expect(SyncStatus.syncing, isNot(SyncStatus.error));
      expect(SyncStatus.error, isNot(SyncStatus.idle));
    });
  });

  group('SyncEvent', () {
    test('constructor sets fields correctly', () {
      final event = SyncEvent(
        folderId: 'folder-1',
        status: SyncStatus.syncing,
        message: 'Syncing file',
      );
      expect(event.folderId, 'folder-1');
      expect(event.status, SyncStatus.syncing);
      expect(event.message, 'Syncing file');
    });

    test('message can be null', () {
      final event = SyncEvent(
        folderId: 'folder-1',
        status: SyncStatus.idle,
      );
      expect(event.message, isNull);
    });
  });
}
