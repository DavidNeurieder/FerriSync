import 'package:ferrisync/gen/api.dart' as frb;
import 'package:ferrisync/models/sync_models.dart';
import 'package:ferrisync/providers/sync_provider.dart';
import 'package:ferrisync/screens/folder_detail_screen.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

class MockSyncService extends SyncService {
  MockSyncService({
    this.testDeviceName = 'My Phone',
    this.testHistory = const [],
    this.removeCalls = 0,
  });

  final String testDeviceName;
  final List<frb.FileHistoryEntry> testHistory;
  int removeCalls;

  @override
  String get deviceName => testDeviceName;

  @override
  Future<List<frb.FileHistoryEntry>> historyForFolder(int folderId) async =>
      testHistory;

  @override
  Future<String> syncFolderNow(SyncFolder folder) async =>
      'Sync complete with peer';

  @override
  Future<String> removeFolder(int folderId) async {
    removeCalls++;
    return 'Folder removed';
  }
}

Widget createTestApp(SyncService service, SyncFolder folder, {Device? device}) {
  return ProviderScope(
    overrides: [
      syncServiceProvider.overrideWith((ref) => service),
    ],
    child: MaterialApp(
      home: FolderDetailScreen(folder: folder),
    ),
  );
}

SyncFolder _folder({
  FolderHealth health = FolderHealth.healthy,
  String deviceName = 'Pixel 8',
  int lastSyncAt = 100,
  int conflicts = 0,
}) {
  return SyncFolder(
    id: 1,
    localPath: '/storage/Photos',
    deviceId: 'dev-1',
    direction: 'bidirectional',
    lastSyncAt: lastSyncAt,
    health: health,
    deviceName: deviceName,
    conflicts: conflicts,
  );
}

frb.FileHistoryEntry _entry(String path, String action, int ts) {
  return frb.FileHistoryEntry(
    path: path,
    action: action,
    deviceId: 'dev-1',
    recordedAt: ts,
    size: 1024,
  );
}

void main() {
  group('FolderDetailScreen', () {
    testWidgets('shows health, relationship, sync mode and stats',
        (WidgetTester tester) async {
      final service = MockSyncService();
      await tester.pumpWidget(createTestApp(service, _folder()));

      expect(find.text('Synced'), findsOneWidget); // health label
      expect(find.text('My Phone ↔ Pixel 8'), findsOneWidget);
      expect(find.text('Sync mode'), findsOneWidget);
      expect(find.text('Two-way'), findsWidgets); // relationship + peer mode
      expect(find.text('Last sync'), findsOneWidget);
      expect(find.text('Sync now'), findsOneWidget);
      expect(find.text('Browse files'), findsOneWidget);
      expect(find.text('Remove'), findsOneWidget);
    });

    testWidgets('conflict health expresses attention', (WidgetTester tester) async {
      final service = MockSyncService();
      await tester.pumpWidget(
        createTestApp(service, _folder(health: FolderHealth.conflict, conflicts: 3)),
      );

      expect(find.text('Needs attention'), findsOneWidget);
      expect(find.text('3'), findsOneWidget);
    });

    testWidgets('offline health shows offline label', (WidgetTester tester) async {
      final service = MockSyncService();
      await tester.pumpWidget(
        createTestApp(service, _folder(health: FolderHealth.offline)),
      );

      expect(find.text('Offline'), findsOneWidget);
    });

    testWidgets('shows shared-with devices and add-device affordance',
        (WidgetTester tester) async {
      final service = MockSyncService();
      final folder = _folder();
      await tester.pumpWidget(createTestApp(service, folder));
      await tester.pumpAndSettle();

      expect(find.text('SHARED WITH'), findsOneWidget);
      expect(find.byKey(const ValueKey('detail_add_device')), findsOneWidget);
      // With no devices paired, the relationship lists the primary peer.
      expect(find.text('dev-1'), findsWidgets);
    });

    testWidgets('lists recent changes from history', (WidgetTester tester) async {
      final service = MockSyncService(testHistory: [
        _entry('photos/a.jpg', 'push', 200),
        _entry('photos/b.jpg', 'pull', 150),
      ]);
      await tester.pumpWidget(createTestApp(service, _folder()));
      await tester.pumpAndSettle();

      await tester.scrollUntilVisible(find.text('RECENT CHANGES'), 200);
      await tester.pumpAndSettle();

      expect(find.text('RECENT CHANGES'), findsOneWidget);
      expect(find.text('a.jpg'), findsOneWidget);
      expect(find.text('b.jpg'), findsOneWidget);
    });

    testWidgets('sync now calls the engine', (WidgetTester tester) async {
      final service = MockSyncService();
      await tester.pumpWidget(createTestApp(service, _folder()));

      await tester.tap(find.byKey(const ValueKey('detail_sync_now')));
      await tester.pumpAndSettle();
      expect(find.textContaining('Sync complete'), findsOneWidget);
    });

    testWidgets('remove confirms before removing then pops', (WidgetTester tester) async {
      final service = MockSyncService();
      await tester.pumpWidget(createTestApp(service, _folder()));
      await tester.pumpAndSettle();

      await tester.ensureVisible(find.byKey(const ValueKey('detail_remove')));
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(const ValueKey('detail_remove')));
      await tester.pumpAndSettle();

      expect(find.text('Remove folder?'), findsOneWidget);
      await tester.tap(find.widgetWithText(FilledButton, 'Remove'));
      await tester.pumpAndSettle();

      expect(service.removeCalls, 1);
      expect(find.byType(FolderDetailScreen), findsNothing);
    });
  });
}