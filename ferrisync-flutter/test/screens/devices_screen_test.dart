import 'package:ferrisync/gen/api.dart' as frb;
import 'package:ferrisync/models/sync_models.dart';
import 'package:ferrisync/providers/sync_provider.dart';
import 'package:ferrisync/screens/devices_screen.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

class MockSyncService extends SyncService {
  MockSyncService({
    this.testDevices = const [],
    this.testFolders = const [],
    this.testPending = const [],
    this.testSessions = const [],
  });

  final List<Device> testDevices;
  final List<SyncFolder> testFolders;
  final List<(String, String)> testPending;
  final List<frb.SessionEntry> testSessions;

  @override
  List<Device> get devices => testDevices;

  @override
  List<SyncFolder> get folders => testFolders;

  @override
  List<(String, String)> get pendingPairings => testPending;

  @override
  Future<List<frb.SessionEntry>> sessionsForDevice(String deviceId) async =>
      deviceId == '1' ? testSessions : [];

  @override
  Future<void> refresh() async {}
}

Widget createTestApp(SyncService service) {
  return ProviderScope(
    overrides: [
      syncServiceProvider.overrideWith((ref) => service),
    ],
    child: const MaterialApp(home: DevicesScreen()),
  );
}

void main() {
  group('DevicesScreen', () {
    testWidgets('shows empty state when no devices', (WidgetTester tester) async {
      await tester.pumpWidget(createTestApp(MockSyncService()));

      expect(find.text('No devices paired'), findsOneWidget);
      expect(find.text('Pair a device'), findsOneWidget);
    });

    testWidgets('renders device list', (WidgetTester tester) async {
      await tester.pumpWidget(createTestApp(MockSyncService(
        testDevices: [
          Device(id: '1', name: 'Pixel 8', lastSeen: 100, isOnline: true),
          Device(id: '2', name: 'Laptop', lastSeen: 500),
        ],
      )));

      expect(find.text('Pixel 8'), findsOneWidget);
      expect(find.text('Laptop'), findsOneWidget);
    });

    testWidgets('shows folder count and online status', (WidgetTester tester) async {
      await tester.pumpWidget(createTestApp(MockSyncService(
        testDevices: [
          Device(id: '1', name: 'Pixel 8', lastSeen: 100, isOnline: true),
          Device(id: '2', name: 'Laptop', lastSeen: 500),
        ],
        testFolders: [
          SyncFolder(
            id: 1,
            localPath: '/photos',
            deviceId: '1',
            direction: 'bidirectional',
            lastSyncAt: 0,
          ),
        ],
      )));

      expect(find.textContaining('1 folder'), findsOneWidget);
      expect(find.textContaining('Online'), findsOneWidget);
    });

    testWidgets('shows pairing request card', (WidgetTester tester) async {
      await tester.pumpWidget(createTestApp(MockSyncService(
        testPending: [('Pixel 8', 'dev-x')],
      )));

      expect(find.text('Pixel 8'), findsOneWidget);
      expect(find.text('Allow'), findsOneWidget);
      expect(find.text('Deny'), findsOneWidget);
    });

    testWidgets('shows floating action buttons', (WidgetTester tester) async {
      await tester.pumpWidget(createTestApp(MockSyncService()));

      expect(find.byType(FloatingActionButton), findsNWidgets(2));
      expect(find.byIcon(Icons.search), findsOneWidget);
      expect(find.byIcon(Icons.add), findsOneWidget);
    });

    testWidgets('each device exposes a remove action', (WidgetTester tester) async {
      await tester.pumpWidget(createTestApp(MockSyncService(
        testDevices: [
          Device(id: '1', name: 'Pixel 8', lastSeen: 100),
        ],
      )));

      expect(find.byIcon(Icons.more_vert), findsOneWidget);

      await tester.tap(find.byIcon(Icons.more_vert));
      await tester.pumpAndSettle();
      expect(find.text('Remove'), findsOneWidget);
    });

    testWidgets('tapping a device opens its detail sheet with synced bytes',
        (WidgetTester tester) async {
      final service = MockSyncService(
        testDevices: [
          Device(id: '1', name: 'Pixel 8', lastSeen: 100, isOnline: true),
        ],
        testFolders: [
          SyncFolder(
            id: 1,
            localPath: '/storage/photos',
            deviceId: '1',
            direction: 'bidirectional',
            lastSyncAt: 100,
          ),
        ],
        testSessions: [
          frb.SessionEntry(
            ts: 123,
            direction: 'push',
            peerDevice: 'Pixel 8',
            addr: '',
            folderPath: '/storage/photos',
            pushedCount: BigInt.zero,
            pulledCount: BigInt.zero,
            conflictsCount: BigInt.zero,
            pushedBytes: BigInt.from(20 * 1024 * 1024),
            pulledBytes: BigInt.zero,
          ),
        ],
      );
      await tester.pumpWidget(createTestApp(service));
      await tester.pumpAndSettle();

      await tester.tap(find.text('Pixel 8'));
      await tester.pumpAndSettle();

      expect(find.text('TRUSTED DEVICE'), findsOneWidget);
      expect(find.text('20.0 MB'), findsOneWidget);
      expect(find.text('photos'), findsOneWidget);
      expect(find.text('Rename'), findsOneWidget);
      expect(find.text('Remove'), findsOneWidget);
    });
  });
}