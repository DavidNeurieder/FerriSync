import 'package:ferrisync/models/sync_models.dart';
import 'package:ferrisync/providers/sync_provider.dart';
import 'package:ferrisync/screens/folders_screen.dart';
import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:ferrisync/gen/api.dart' as frb;

class MockSyncService extends SyncService {
  MockSyncService({
    this.testFolders = const [],
    this.testDevices = const [],
  });

  final List<SyncFolder> testFolders;
  final List<Device> testDevices;

  @override
  List<SyncFolder> get folders => testFolders;

  @override
  List<Device> get devices => testDevices;
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

class AddingSyncService extends MockSyncService {
  AddingSyncService({
    super.testFolders = const [],
    super.testDevices = const [],
  });

  final List<(String, String)> added = [];

  @override
  Future<int?> addSyncFolder(String localPath, String deviceId) async {
    added.add((localPath, deviceId));
    return 1;
  }

  @override
  Future<void> addSyncFolderWithPeers(
    String localPath,
    String name,
    List<({String deviceId, String? mode, String? remotePath})> peers,
  ) async {
    for (final p in peers) {
      added.add((localPath, p.deviceId));
    }
  }
}

/// SyncService exposing shared-folder state for the published-shares and
/// pairing-approval sections of the folders screen.
class SharedFoldersSyncService extends MockSyncService {
  SharedFoldersSyncService({
    this.testMySharedFolders = const [],
    this.testPendingFolderPairings = const [],
  });

  final List<frb.SharedFolder> testMySharedFolders;
  final List<frb.PendingFolderPairing> testPendingFolderPairings;

  @override
  List<frb.SharedFolder> get mySharedFolders => testMySharedFolders;

  @override
  List<frb.PendingFolderPairing> get pendingFolderPairings =>
      testPendingFolderPairings;

  @override
  String get deviceName => 'this-device';

  @override
  Future<String> setSharedDiscoverable(int shareId, bool discoverable) async {
    return 'updated';
  }

  @override
  Future<String> unshareFolder(int shareId) async {
    return 'stopped sharing';
  }

  @override
  Future<String> approveFolderPairing({
    required String deviceId,
    required String folderGuid,
    required String folderName,
    required String localPath,
  }) async {
    return 'approved $deviceId';
  }

  @override
  Future<String> denyFolderPairing(String deviceId, String folderGuid) async {
    return 'denied';
  }
}

Widget createTestApp(SyncService service) {
  return ProviderScope(
    overrides: [
      syncServiceProvider.overrideWith((ref) => service),
    ],
    child: const MaterialApp(home: FoldersScreen()),
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

      expect(find.text('No folders shared yet'), findsOneWidget);
      expect(find.text('Add a folder'), findsOneWidget);
    });

    testWidgets('renders folder cards with path and direction',
        (WidgetTester tester) async {
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
      expect(find.text('/storage/photos'), findsOneWidget);
      expect(find.text('Push only'), findsOneWidget);
    });

    testWidgets('shows device chip with online dot',
        (WidgetTester tester) async {
      await tester.pumpWidget(createTestApp(MockSyncService(
        testFolders: [
          SyncFolder(
            id: 2,
            localPath: '/storage/photos',
            deviceId: 'dev-1',
            direction: 'bidirectional',
            lastSyncAt: 500,
          ),
        ],
        testDevices: [
          Device(id: 'dev-1', name: 'Pixel 8', lastSeen: 100),
        ],
      )));

      expect(find.text('Pixel 8'), findsOneWidget);
    });

    testWidgets('flags a folder that never synced', (WidgetTester tester) async {
      await tester.pumpWidget(createTestApp(MockSyncService(
        testFolders: [
          SyncFolder(
            id: 3,
            localPath: '/docs',
            deviceId: 'dev-1',
            direction: 'bidirectional',
            lastSyncAt: 0,
          ),
        ],
      )));

      expect(find.text('Never synced'), findsOneWidget);
    });

    testWidgets('shows floating action button', (WidgetTester tester) async {
      await tester.pumpWidget(createTestApp(MockSyncService()));

      expect(find.byType(FloatingActionButton), findsOneWidget);
      expect(
        find.descendant(
          of: find.byType(FloatingActionButton),
          matching: find.byIcon(Icons.add),
        ),
        findsOneWidget,
      );
    });

    testWidgets('each folder exposes sync and remove actions',
        (WidgetTester tester) async {
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

      await tester.tap(find.byIcon(Icons.more_vert));
      await tester.pumpAndSettle();
      expect(find.text('Sync now'), findsOneWidget);
      expect(find.text('Remove'), findsOneWidget);
    });

    testWidgets('tapping sync now from the menu triggers sync for that folder',
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

      await tester.tap(find.byIcon(Icons.more_vert));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Sync now'));
      await tester.pumpAndSettle();

      expect(service.syncedFolders, hasLength(1));
      expect(service.syncedFolders.first.localPath, '/storage/docs');
      expect(find.textContaining('Sync complete'), findsOneWidget);
    });

    testWidgets('adding a folder shows the review step before syncing',
        (WidgetTester tester) async {
      FilePicker.platform = FilePickerIO();
      const pickerChannel = MethodChannel(
          'miguelruivo.flutter.plugins.filepicker', JSONMethodCodec());
      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
          .setMockMethodCallHandler(pickerChannel, (call) async {
        if (call.method == 'dir') return '/storage/picked';
        return null;
      });
      addTearDown(() => TestDefaultBinaryMessengerBinding.instance
          .defaultBinaryMessenger
          .setMockMethodCallHandler(pickerChannel, null));

      final service = AddingSyncService(
        testDevices: [
          Device(
            id: 'dev-1',
            name: 'Pixel 8',
            lastSeen: 100,
            presence: Presence.connected),
        ],
      );
      await tester.pumpWidget(createTestApp(service));

      await tester.tap(find.byType(FloatingActionButton));
      await tester.pumpAndSettle();

      // Device multi-select dialog appears first.
      expect(find.text('Pixel 8'), findsOneWidget);
      expect(find.text('Choose Devices'), findsOneWidget);
      await tester.tap(find.text('Pixel 8'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Continue'));
      await tester.pumpAndSettle();

      // Folder name prompt (defaults to the path label).
      await tester.tap(find.text('Save'));
      await tester.pumpAndSettle();

      // Review step: this is the last confirmation before anything changes.
      expect(find.text('Ready to sync'), findsOneWidget);
      expect(find.text('picked'), findsOneWidget);
      expect(find.text('This device'), findsOneWidget);
      expect(find.text('Remote devices'), findsOneWidget);
      expect(find.text('Pixel 8'), findsOneWidget);
      expect(find.text('Two-way'), findsOneWidget);
      expect(service.added, isEmpty);

      await tester.tap(find.text('Start syncing'));
      await tester.pumpAndSettle();

      expect(service.added, hasLength(1));
      expect(service.added.first, ('/storage/picked', 'dev-1'));
      expect(find.textContaining('Syncing'), findsOneWidget);
    });

    testWidgets(
        'when the native dir picker is unavailable, the + flow falls back to '
        'a manual path entry', (WidgetTester tester) async {
      FilePicker.platform = FilePickerIO();
      const pickerChannel = MethodChannel(
          'miguelruivo.flutter.plugins.filepicker', JSONMethodCodec());
      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
          .setMockMethodCallHandler(pickerChannel, (call) async {
        if (call.method == 'dir') return null; // no native helper / cancelled
        return null;
      });
      addTearDown(() => TestDefaultBinaryMessengerBinding.instance
          .defaultBinaryMessenger
          .setMockMethodCallHandler(pickerChannel, null));

      final service = AddingSyncService(
        testDevices: [
          Device(
            id: 'dev-1',
            name: 'Pixel 8',
            lastSeen: 100,
            presence: Presence.connected),
        ],
      );
      await tester.pumpWidget(createTestApp(service));

      await tester.tap(find.byType(FloatingActionButton));
      await tester.pumpAndSettle();

      // The manual path entry dialog is shown because the native picker
      // produced no directory.
      expect(find.text('Enter folder path'), findsOneWidget);
      await tester.enterText(
          find.byType(TextField), '/home/user/Documents');
      await tester.tap(find.text('Use this folder'));
      await tester.pumpAndSettle();

      expect(find.text('Choose Devices'), findsOneWidget);
      await tester.tap(find.text('Pixel 8'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Continue'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Save'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Start syncing'));
      await tester.pumpAndSettle();

      expect(service.added, hasLength(1));
      expect(service.added.first, ('/home/user/Documents', 'dev-1'));
    });

    testWidgets(
        'adding a folder does not crash when the permission_handler plugin is '
        'unavailable (Linux MissingPluginException regression)',
        (WidgetTester tester) async {
      const permChannel =
          MethodChannel('flutter.baseflow.com/permissions/methods');
      // Drop the granted-permission mock so any plugin call throws
      // MissingPluginException, reproducing the Linux crash.
      tester.binding.defaultBinaryMessenger
          .setMockMethodCallHandler(permChannel, null);
      addTearDown(() => tester.binding.defaultBinaryMessenger
          .setMockMethodCallHandler(permChannel, (call) async {
        if (call.method == 'checkPermissionStatus') return 1;
        return null;
      }));

      FilePicker.platform = FilePickerIO();
      const pickerChannel = MethodChannel(
          'miguelruivo.flutter.plugins.filepicker', JSONMethodCodec());
      tester.binding.defaultBinaryMessenger
          .setMockMethodCallHandler(pickerChannel, (call) async {
        if (call.method == 'dir') return '/storage/picked';
        return null;
      });
      addTearDown(() => tester.binding.defaultBinaryMessenger
          .setMockMethodCallHandler(pickerChannel, null));

      final service = AddingSyncService(
        testDevices: [
          Device(
            id: 'dev-1',
            name: 'Pixel 8',
            lastSeen: 100,
            presence: Presence.connected),
        ],
      );
      await tester.pumpWidget(createTestApp(service));

      // The + button must open the picker directly (desktop skips the storage
      // permission gate) and complete the flow without throwing.
      await tester.tap(find.byType(FloatingActionButton));
      await tester.pumpAndSettle();
      expect(find.text('Choose Devices'), findsOneWidget);

      await tester.tap(find.text('Pixel 8'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Continue'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Save'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Start syncing'));
      await tester.pumpAndSettle();

      expect(service.added, hasLength(1));
      expect(service.added.first, ('/storage/picked', 'dev-1'));
    });

    testWidgets('renders published shares with discoverable toggle and unshare',
        (WidgetTester tester) async {
      final service = SharedFoldersSyncService(
        testMySharedFolders: [
          const frb.SharedFolder(
            id: 3,
            folderGuid: 'guid-1',
            name: 'Docs',
            localPath: '/storage/docs',
            discoverable: true,
            enabled: true,
            permissions: 'read_write',
          ),
        ],
      );
      await tester.pumpWidget(createTestApp(service));

      expect(find.text('PUBLISHED SHARES (1)'), findsOneWidget);
      expect(find.text('Docs'), findsOneWidget);
      expect(find.textContaining('discoverable'), findsOneWidget);

      // Unshare action works.
      await tester.tap(find.byIcon(Icons.link_off));
      await tester.pumpAndSettle();
      expect(find.text('stopped sharing'), findsOneWidget);
    });

    testWidgets('renders folder pairing request with allow/deny actions',
        (WidgetTester tester) async {
      final service = SharedFoldersSyncService(
        testPendingFolderPairings: [
          const frb.PendingFolderPairing(
            deviceId: 'peer-1',
            deviceName: 'Pixel 9',
            folderGuid: 'guid-1',
            folderName: 'Docs',
          ),
        ],
      );
      await tester.pumpWidget(createTestApp(service));

      expect(find.textContaining('wants "Docs"'), findsOneWidget);
      expect(find.text('Allow'), findsOneWidget);
      expect(find.text('Deny'), findsOneWidget);

      await tester.tap(find.text('Deny'));
      await tester.pumpAndSettle();
      expect(find.text('denied'), findsOneWidget);
    });

    testWidgets('hidden pending pairings render nothing',
        (WidgetTester tester) async {
      await tester.pumpWidget(
          createTestApp(SharedFoldersSyncService()));

      expect(find.text('FOLDER PAIRING REQUESTS'), findsNothing);
      expect(find.text('PUBLISHED SHARES (0)'), findsOneWidget);
    });
  });
}