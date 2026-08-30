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
  });

  final List<Device> testDevices;
  final List<SyncFolder> testFolders;
  final List<(String, String)> testPending;

  @override
  List<Device> get devices => testDevices;

  @override
  List<SyncFolder> get folders => testFolders;

  @override
  List<(String, String)> get pendingPairings => testPending;

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
  });
}