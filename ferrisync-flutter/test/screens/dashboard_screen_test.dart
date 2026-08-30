import 'package:ferrisync/gen/api.dart' as frb;
import 'package:ferrisync/models/sync_models.dart';
import 'package:ferrisync/providers/sync_provider.dart';
import 'package:ferrisync/screens/dashboard_screen.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

class TestSyncService extends SyncService {
  TestSyncService({
    this.testDeviceId = '',
    this.testDeviceName = '',
    this.testDevices = const [],
    this.testFolders = const [],
    this.testStatus = SyncStatus.idle,
    this.testHistory = const [],
    this.testSessions = const [],
    this.testRecentConflicts = 0,
    this.testError,
  });

  final String testDeviceId;
  final String testDeviceName;
  final List<Device> testDevices;
  final List<SyncFolder> testFolders;
  final SyncStatus testStatus;
  final List<frb.FileHistoryEntry> testHistory;
  final List<frb.SessionEntry> testSessions;
  final int testRecentConflicts;
  final String? testError;
  bool refreshCalled = false;

  @override
  String get deviceId => testDeviceId;

  @override
  String get deviceName => testDeviceName;

  @override
  List<Device> get devices => testDevices;

  @override
  List<SyncFolder> get folders => testFolders;

  @override
  SyncStatus get status => testStatus;

  @override
  List<frb.FileHistoryEntry> get history => testHistory;

  @override
  List<frb.SessionEntry> get sessions => testSessions;

  @override
  int get recentConflicts => testRecentConflicts;

  @override
  String? get lastErrorMessage => testError;

  @override
  List<AttentionItem> get attentionItems {
    final items = <AttentionItem>[];
    if (testRecentConflicts > 0) {
      items.add(AttentionItem(
        kind: AttentionKind.conflictFiles,
        label: testRecentConflicts == 1
            ? '1 file has a conflict'
            : '$testRecentConflicts files have conflicts',
      ));
    }
    for (final d in testDevices.where((d) => !d.isOnline && d.lastSeen > 0)) {
      items.add(AttentionItem(
        kind: AttentionKind.offlineDevice,
        label: '${d.name} is offline',
      ));
    }
    return items;
  }

  @override
  Future<void> refresh() async {
    refreshCalled = true;
  }
}

Widget createTestApp(SyncService service) {
  return ProviderScope(
    overrides: [
      syncServiceProvider.overrideWith((ref) => service),
    ],
    child: const MaterialApp(home: DashboardScreen()),
  );
}

class LiveProgressTestService extends TestSyncService {
  LiveProgressTestService({
    this.filesDone = 3,
    this.filesTotal = 5,
    this.bytesDone = 10 * 1024 * 1024,
    this.bytesTotal = 50 * 1024 * 1024,
    this.progress = 0.6,
    super.testStatus = SyncStatus.syncing,
  });

  final int filesDone;
  final int filesTotal;
  final int bytesDone;
  final int bytesTotal;
  final double progress;

  @override
  String? get syncingFolderLabel => 'photos';

  @override
  bool get hasLiveProgress => filesTotal > 0;

  @override
  int get syncFilesDone => filesDone;

  @override
  int get syncFilesTotal => filesTotal;

  @override
  int get syncBytesDone => bytesDone;

  @override
  int get syncBytesTotal => bytesTotal;

  @override
  double? get syncProgressValue => progress;
}

Future<void> pumpDashboard(WidgetTester tester, SyncService service) async {
  // Tall viewport so lazily-built lower sections render during assertions.
  await tester.binding.setSurfaceSize(const Size(800, 2400));
  addTearDown(() => tester.binding.setSurfaceSize(null));
  await tester.pumpWidget(createTestApp(service));
}

void main() {
  group('DashboardScreen', () {
    testWidgets('shows idle hero status by default', (WidgetTester tester) async {
      await pumpDashboard(
        tester,
        TestSyncService(testDeviceId: 'dev-1', testDeviceName: 'My Phone'),
      );

      expect(find.text('Everything is in sync'), findsOneWidget);
      expect(find.text('In sync'), findsOneWidget);
    });

    testWidgets('shows syncing hero status', (WidgetTester tester) async {
      await pumpDashboard(
        tester,
        TestSyncService(testStatus: SyncStatus.syncing),
      );

      expect(find.text('Syncing folders…'), findsOneWidget);
      expect(find.byIcon(Icons.sync), findsWidgets);
    });

    testWidgets('shows live byte progress in the syncing hero',
        (WidgetTester tester) async {
      await pumpDashboard(
        tester,
        LiveProgressTestService(testStatus: SyncStatus.syncing),
      );

      expect(find.text('Syncing photos'), findsOneWidget);
      expect(find.text('3 / 5 files · 60% · 40.0 MB remaining'),
          findsOneWidget);
      final bar = tester.widget<LinearProgressIndicator>(
          find.byType(LinearProgressIndicator));
      expect(bar.value, closeTo(0.6, 0.001));
      expect(find.text('Preparing files…'), findsNothing);
    });

    testWidgets('shows attention hero status on error',
        (WidgetTester tester) async {
      await pumpDashboard(
        tester,
        TestSyncService(testStatus: SyncStatus.error),
      );

      expect(find.text('Needs attention'), findsWidgets);
      expect(find.byIcon(Icons.error), findsWidgets);
    });

    testWidgets('displays this-device ID and name', (WidgetTester tester) async {
      await pumpDashboard(
        tester,
        TestSyncService(testDeviceId: 'abc-123', testDeviceName: 'My Phone'),
      );

      expect(find.text('abc-123'), findsOneWidget);
      expect(find.text('My Phone'), findsOneWidget);
    });

    testWidgets('shows empty devices message', (WidgetTester tester) async {
      await pumpDashboard(tester, TestSyncService());

      expect(find.text('No devices paired'), findsOneWidget);
      expect(find.text('Pair a device'), findsOneWidget);
    });

    testWidgets('shows paired device count and device name',
        (WidgetTester tester) async {
      await pumpDashboard(
        tester,
        TestSyncService(
          testDevices: [
            Device(id: '1', name: 'Laptop', lastSeen: 100, isOnline: true),
          ],
        ),
      );

      expect(find.textContaining('YOUR DEVICES'), findsOneWidget);
      expect(find.text('Laptop'), findsOneWidget);
      expect(find.text('1'), findsOneWidget);
      expect(find.text('Devices connected'), findsOneWidget);
    });

    testWidgets('shows multiple paired devices', (WidgetTester tester) async {
      await pumpDashboard(
        tester,
        TestSyncService(
          testDevices: [
            Device(id: '1', name: 'Laptop', lastSeen: 100),
            Device(id: '2', name: 'Server', lastSeen: 200),
          ],
        ),
      );

      expect(find.text('Laptop'), findsOneWidget);
      expect(find.text('Server'), findsOneWidget);
    });

    testWidgets('shows empty activity state', (WidgetTester tester) async {
      await pumpDashboard(tester, TestSyncService());

      expect(find.text('No syncs yet'), findsOneWidget);
    });

    testWidgets('has RefreshIndicator', (WidgetTester tester) async {
      await pumpDashboard(tester, TestSyncService());

      expect(find.byType(RefreshIndicator), findsOneWidget);
    });

    testWidgets('pull to refresh calls service.refresh', (WidgetTester tester) async {
      final service = TestSyncService();

      await pumpDashboard(tester, service);

      final list = find.byType(ListView);
      final start = tester.getTopLeft(list) + const Offset(200, 60);
      final gesture = await tester.startGesture(start);
      await gesture.moveBy(const Offset(0, 800));
      await tester.pump();
      await gesture.up();
      await tester.pump(const Duration(seconds: 1));
      await tester.pump(const Duration(seconds: 1));

      expect(service.refreshCalled, true);
    });

    testWidgets('turns hero to attention and lists conflicts when present',
        (WidgetTester tester) async {
      await pumpDashboard(tester, TestSyncService(testRecentConflicts: 2));

      expect(find.text('Needs your attention'), findsOneWidget);
      expect(find.text('Review conflicts'), findsOneWidget);
      expect(find.text('NEEDS ATTENTION'), findsOneWidget);
      expect(find.text('2 files have conflicts'), findsOneWidget);
    });

    testWidgets('offline device shows offline hero and attention entry',
        (WidgetTester tester) async {
      await pumpDashboard(
        tester,
        TestSyncService(
          testDevices: [
            Device(id: '1', name: 'Laptop', lastSeen: 1234567),
          ],
        ),
      );

      expect(find.text('Laptop is offline'), findsWidgets);
      expect(
        find.text(
            'Your files are safe. Sync resumes automatically when it reconnects.'),
        findsOneWidget,
      );
    });

    testWidgets('healthy hero shows last-sync subcopy and add-folder action',
        (WidgetTester tester) async {
      final nowSec = DateTime.now().millisecondsSinceEpoch ~/ 1000;
      await pumpDashboard(
        tester,
        TestSyncService(testFolders: [
          SyncFolder(
            id: 1,
            localPath: '/Photos',
            deviceId: '1',
            direction: 'bidirectional',
            lastSyncAt: nowSec - 120,
          ),
        ]),
      );

      expect(find.text('Everything is in sync'), findsOneWidget);
      expect(find.textContaining('Last sync'), findsWidgets);
      expect(find.text('Add a folder'), findsNothing);
    });

    testWidgets('no-folder healthy hero offers an add-folder action',
        (WidgetTester tester) async {
      await pumpDashboard(tester, TestSyncService());

      expect(find.text('Everything is in sync'), findsOneWidget);
      expect(find.text('Add a folder'), findsOneWidget);
    });

    testWidgets('error hero humanizes the failure with a retry action',
        (WidgetTester tester) async {
      await pumpDashboard(
        tester,
        TestSyncService(
          testStatus: SyncStatus.error,
          testError: 'error could not reach 10.0.0.5',
        ),
      );

      expect(find.text('Needs attention'), findsWidgets);
      expect(find.text('Try again'), findsOneWidget);
      expect(
        find.textContaining("Couldn't connect to the other device"),
        findsOneWidget,
      );
    });

    testWidgets('tapping a session row reveals its details',
        (WidgetTester tester) async {
      await pumpDashboard(
        tester,
        TestSyncService(
          testSessions: [
            frb.SessionEntry(
              ts: DateTime.now().millisecondsSinceEpoch ~/ 1000,
              direction: 'push',
              peerDevice: 'peer-7',
              addr: '10.0.0.7:9847',
              folderPath: '/Photos',
              pushedCount: BigInt.from(4),
              pulledCount: BigInt.zero,
              conflictsCount: BigInt.zero,
              pushedBytes: BigInt.from(4096),
              pulledBytes: BigInt.zero,
            ),
          ],
        ),
      );

      await tester.tap(find.text('Photos'));
      await tester.pumpAndSettle();

      expect(find.text('Photos sync'), findsOneWidget);
      expect(find.text('Sent to peer-7'), findsOneWidget);
      expect(find.text('4'), findsWidgets);
    });

    testWidgets('tapping a conflict history entry offers review',
        (WidgetTester tester) async {
      final nowSec = DateTime.now().millisecondsSinceEpoch ~/ 1000;
      await pumpDashboard(
        tester,
        TestSyncService(
          testRecentConflicts: 1,
          testHistory: [
            frb.FileHistoryEntry(
              path: 'IMG_1.jpg',
              deviceId: 'peer-7',
              action: 'conflict',
              size: 4096,
              recordedAt: nowSec,
            ),
          ],
        ),
      );

      await tester.tap(find.text('IMG_1.jpg'));
      await tester.pumpAndSettle();

      expect(find.text('Resolve conflicts'), findsOneWidget);
      expect(find.text('4.0 KB'), findsWidgets);
    });
  });
}