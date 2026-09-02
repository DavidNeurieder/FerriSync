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
    this.sharedFolders = const [],
  });

  /// Shared folders returned when a device is browsed in the Choose Devices
  /// dialog.
  final List<frb.RemoteSharedFolder> sharedFolders;

  /// (deviceId, folderGuid, localPath) triples this device paired to via the
  /// picker.
  final List<(String, String, String)> paired = [];

  /// Local folders recorded via the legacy addSyncFolder path.
  final List<(String, String)> added = [];

  @override
  Future<List<frb.RemoteSharedFolder>> remoteFoldersFor(Device device) async =>
      sharedFolders;

  @override
  Future<({String message, String? folderGuid})> syncRemoteFolder({
    required Device device,
    required frb.RemoteSharedFolder folder,
    String? localPath,
  }) async {
    paired.add((device.id, folder.folderGuid, localPath ?? ''));
    return (message: 'Paired to ${folder.name}', folderGuid: folder.folderGuid);
  }

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

/// SyncService whose browse of a peer's shared folders throws, so the dialog
/// must surface a reachability message rather than "no shared folders".
class UnreachableSyncService extends MockSyncService {
  UnreachableSyncService({super.testFolders = const [], super.testDevices = const []});

  @override
  Future<List<frb.RemoteSharedFolder>> remoteFoldersFor(Device device) async {
    throw Exception('connection refused');
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

/// Stubs the native directory picker to return [path] (non-null) or, when
/// [path] is null, to cancel so the flow falls back to manual entry.
void _mockNativeDirPicker(WidgetTester tester, String? path) {
  FilePicker.platform = FilePickerIO();
  const pickerChannel =
      MethodChannel('miguelruivo.flutter.plugins.filepicker', JSONMethodCodec());
  tester.binding.defaultBinaryMessenger
      .setMockMethodCallHandler(pickerChannel, (call) async {
    if (call.method == 'dir') return path;
    return null;
  });
  addTearDown(() => tester.binding.defaultBinaryMessenger
      .setMockMethodCallHandler(pickerChannel, null));
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

    testWidgets('picks a local folder first, then pairs a remote shared folder',
        (WidgetTester tester) async {
      _mockNativeDirPicker(tester, '/storage/picked');
      final service = AddingSyncService(
        testDevices: [
          Device(
            id: 'dev-1',
            name: 'Pixel 8',
            lastSeen: 100,
            presence: Presence.connected),
        ],
        sharedFolders: [
          const frb.RemoteSharedFolder(
            folderGuid: 'guid-1',
            name: 'Docs',
            mode: 'bidirectional',
            localPath: '/home/pixel/Docs',
          ),
        ],
      );
      await tester.pumpWidget(createTestApp(service));

      await tester.tap(find.byType(FloatingActionButton));
      await tester.pumpAndSettle();

      // Choosing the local folder is the first step, then the receiver lets
      // the user pick which of the peer's shared folders to sync it with.
      expect(find.text('Choose Remote Folder'), findsOneWidget);
      expect(find.textContaining('Local folder: /storage/picked'),
          findsOneWidget);
      await tester.tap(find.text('Pixel 8'));
      await tester.pumpAndSettle();

      // Browsing the selected device shows its discoverable shared folder.
      expect(
        find.byKey(const ValueKey('device_dev-1_share_guid-1')),
        findsOneWidget,
      );
      expect(find.text('Docs'), findsOneWidget);
      expect(service.paired, isEmpty);

      // Tapping a shared folder pairs it to the chosen local folder and
      // completes the flow.
      await tester.tap(find.byKey(const ValueKey('device_dev-1_share_guid-1')));
      await tester.pumpAndSettle();

      expect(service.paired, [('dev-1', 'guid-1', '/storage/picked')]);
      expect(find.text('Choose Remote Folder'), findsNothing);
    });

    testWidgets('when the native dir picker is unavailable, manual path entry '
        'leads to the remote folder chooser', (WidgetTester tester) async {
      // Native picker cancels, so the desktop fallback to manual entry fires.
      _mockNativeDirPicker(tester, null);
      final service = AddingSyncService(
        testDevices: [
          Device(
            id: 'dev-1',
            name: 'Pixel 8',
            lastSeen: 100,
            presence: Presence.connected),
        ],
        sharedFolders: [
          const frb.RemoteSharedFolder(
            folderGuid: 'guid-1',
            name: 'Docs',
            mode: 'bidirectional',
            localPath: '/home/pixel/Docs',
          ),
        ],
      );
      await tester.pumpWidget(createTestApp(service));

      await tester.tap(find.byType(FloatingActionButton));
      await tester.pumpAndSettle();

      expect(find.text('Enter folder path'), findsOneWidget);
      await tester.enterText(find.byType(TextField), '/manual/path');
      await tester.tap(find.text('Use this folder'));
      await tester.pumpAndSettle();

      expect(find.text('Choose Remote Folder'), findsOneWidget);
      await tester.tap(find.text('Pixel 8'));
      await tester.pumpAndSettle();
      await tester
          .tap(find.byKey(const ValueKey('device_dev-1_share_guid-1')));
      await tester.pumpAndSettle();

      expect(service.paired, [('dev-1', 'guid-1', '/manual/path')]);
    });

    testWidgets('a selected device with no shared folders shows an empty state',
        (WidgetTester tester) async {
      _mockNativeDirPicker(tester, '/storage/picked');
      final service = AddingSyncService(
        testDevices: [
          Device(
            id: 'dev-1',
            name: 'Pixel 8',
            lastSeen: 100,
            presence: Presence.connected),
        ],
        sharedFolders: const [],
      );
      await tester.pumpWidget(createTestApp(service));

      await tester.tap(find.byType(FloatingActionButton));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Pixel 8'));
      await tester.pumpAndSettle();

      // The peer answered but published nothing: an explicit "not published"
      // message (and a note that already-paired folders aren't listed here).
      expect(
        find.textContaining("hasn't published any shared folders"),
        findsOneWidget,
      );
      expect(service.paired, isEmpty);
    });

    testWidgets(
        'an unreachable peer shows a reachability message instead of '
        '"no shared folders"', (WidgetTester tester) async {
      _mockNativeDirPicker(tester, '/storage/picked');
      final service = UnreachableSyncService(
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
      await tester.tap(find.text('Pixel 8'));
      await tester.pumpAndSettle();

      expect(find.textContaining("Couldn't reach Pixel 8"), findsOneWidget);
    });

    testWidgets(
        'adding a shared folder does not crash when the permission_handler '
        'plugin is unavailable (Linux MissingPluginException regression)',
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

      _mockNativeDirPicker(tester, '/storage/picked');
      final service = AddingSyncService(
        testDevices: [
          Device(
            id: 'dev-1',
            name: 'Pixel 8',
            lastSeen: 100,
            presence: Presence.connected),
        ],
        sharedFolders: [
          const frb.RemoteSharedFolder(
            folderGuid: 'guid-1',
            name: 'Docs',
            mode: 'bidirectional',
            localPath: '/home/pixel/Docs',
          ),
        ],
      );
      await tester.pumpWidget(createTestApp(service));

      // The + button opens the flow directly (desktop skips the storage
      // permission gate) and pairing completes without throwing.
      await tester.tap(find.byType(FloatingActionButton));
      await tester.pumpAndSettle();
      expect(find.text('Choose Remote Folder'), findsOneWidget);

      await tester.tap(find.text('Pixel 8'));
      await tester.pumpAndSettle();
      await tester
          .tap(find.byKey(const ValueKey('device_dev-1_share_guid-1')));
      await tester.pumpAndSettle();

      expect(service.paired, [('dev-1', 'guid-1', '/storage/picked')]);
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