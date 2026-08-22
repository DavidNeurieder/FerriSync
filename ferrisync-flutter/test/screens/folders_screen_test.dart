import 'package:ferrisync/models/sync_models.dart';
import 'package:ferrisync/providers/sync_provider.dart';
import 'package:ferrisync/screens/folders_screen.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

class MockSyncService extends SyncService {
  MockSyncService({
    this.testFolders = const [],
  });

  final List<SyncFolder> testFolders;

  @override
  List<SyncFolder> get folders => testFolders;
}

class RecordingSyncService extends MockSyncService {
  RecordingSyncService({required super.testFolders});

  final List<SyncFolder> syncedFolders = [];

  @override
  Future<String> syncFolderNow(SyncFolder folder) async {
    syncedFolders.add(folder);
    return 'Sync complete';
  }
}

Widget createTestApp(SyncService service) {
  return ProviderScope(
    overrides: [
      syncServiceProvider.overrideWith((ref) => service),
    ],
    child: MaterialApp(home: FoldersScreen()),
  );
}

void main() {
  setUpAll(() {
    // Grant storage permission for tests exercising the sync-now flow.
    TestWidgetsFlutterBinding.ensureInitialized();
    const channel = MethodChannel('flutter.baseflow.com/permissions/methods');
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(channel, (call) async {
      switch (call.method) {
        case 'checkPermissionStatus':
          return 1; // PermissionStatus.granted
        case 'requestPermissions':
          return <int, int>{};
        default:
          return null;
      }
    });
  });

  group('FoldersScreen', () {
    testWidgets('shows empty state when no folders', (WidgetTester tester) async {
      await tester.pumpWidget(createTestApp(MockSyncService()));

      expect(find.text('No sync folders configured'), findsOneWidget);
    });

    testWidgets('renders folder list', (WidgetTester tester) async {
      await tester.pumpWidget(createTestApp(MockSyncService(
        testFolders: [
          SyncFolder(
            id: 1,
            localPath: '/storage/photos',
            deviceId: 'dev-1',
            direction: 'push',
            lastSyncAt: 500,
          ),
        ],
      )));

      expect(find.text('photos'), findsOneWidget);
    });

    testWidgets('shows floating action button', (WidgetTester tester) async {
      await tester.pumpWidget(createTestApp(MockSyncService()));

      expect(find.byType(FloatingActionButton), findsOneWidget);
      expect(find.byIcon(Icons.add), findsOneWidget);
    });

    testWidgets('each folder has a toggle switch', (WidgetTester tester) async {
      await tester.pumpWidget(createTestApp(MockSyncService(
        testFolders: [
          SyncFolder(
            id: 1,
            localPath: '/docs',
            deviceId: 'dev-1',
            direction: 'bidirectional',
            lastSyncAt: 100,
          ),
        ],
      )));

      expect(find.byType(Switch), findsOneWidget);
    });

    testWidgets('each folder has a sync now button', (WidgetTester tester) async {
      await tester.pumpWidget(createTestApp(MockSyncService(
        testFolders: [
          SyncFolder(
            id: 7,
            localPath: '/docs',
            deviceId: 'dev-1',
            direction: 'bidirectional',
            lastSyncAt: 0,
          ),
        ],
      )));

      expect(find.byKey(const ValueKey('sync_now_7')), findsOneWidget);
      expect(find.textContaining('Last sync: never'), findsOneWidget);
    });

    testWidgets('tapping sync now triggers sync for that folder',
        (WidgetTester tester) async {
      final service = RecordingSyncService(testFolders: [
        SyncFolder(
          id: 7,
          localPath: '/storage/docs',
          deviceId: 'dev-1',
          direction: 'bidirectional',
          lastSyncAt: 0,
        ),
      ]);
      await tester.pumpWidget(createTestApp(service));

      await tester.tap(find.byKey(const ValueKey('sync_now_7')));
      await tester.pumpAndSettle();

      expect(service.syncedFolders, hasLength(1));
      expect(service.syncedFolders.first.localPath, '/storage/docs');
      expect(find.textContaining('Sync complete'), findsOneWidget);
    });
  });
}
