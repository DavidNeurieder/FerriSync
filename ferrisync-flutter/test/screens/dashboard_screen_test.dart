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
  });

  final String testDeviceId;
  final String testDeviceName;
  final List<Device> testDevices;
  final List<SyncFolder> testFolders;
  final SyncStatus testStatus;
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
  Future<void> refresh() async {
    refreshCalled = true;
  }
}

Widget createTestApp(SyncService service) {
  return ProviderScope(
    overrides: [
      syncServiceProvider.overrideWith((ref) => service),
    ],
    child: MaterialApp(home: DashboardScreen()),
  );
}

void main() {
  group('DashboardScreen', () {
    testWidgets('shows idle status by default', (WidgetTester tester) async {
      await tester.pumpWidget(createTestApp(TestSyncService(
        testDeviceId: 'dev-1',
        testDeviceName: 'My Phone',
      )));

      expect(find.text('Idle'), findsOneWidget);
      expect(find.text('Sync status'), findsOneWidget);
    });

    testWidgets('shows syncing status', (WidgetTester tester) async {
      await tester.pumpWidget(createTestApp(TestSyncService(
        testStatus: SyncStatus.syncing,
      )));

      expect(find.text('Syncing...'), findsOneWidget);
      expect(find.byIcon(Icons.sync), findsOneWidget);
    });

    testWidgets('shows error status', (WidgetTester tester) async {
      await tester.pumpWidget(createTestApp(TestSyncService(
        testStatus: SyncStatus.error,
      )));

      expect(find.text('Error'), findsOneWidget);
      expect(find.byIcon(Icons.error), findsOneWidget);
    });

    testWidgets('displays device ID and name', (WidgetTester tester) async {
      await tester.pumpWidget(createTestApp(TestSyncService(
        testDeviceId: 'abc-123',
        testDeviceName: 'My Phone',
      )));

      expect(find.text('abc-123'), findsOneWidget);
      expect(find.text('My Phone'), findsOneWidget);
    });

    testWidgets('shows empty devices message', (WidgetTester tester) async {
      await tester.pumpWidget(createTestApp(TestSyncService()));

      expect(find.text('No devices paired.'), findsOneWidget);
    });

    testWidgets('shows paired device count', (WidgetTester tester) async {
      await tester.pumpWidget(createTestApp(TestSyncService(
        testDevices: [
          Device(id: '1', name: 'Laptop', lastSeen: 100),
        ],
      )));

      expect(find.textContaining('Paired Devices'), findsOneWidget);
      expect(find.text('Laptop'), findsOneWidget);
    });

    testWidgets('shows multiple paired devices', (WidgetTester tester) async {
      await tester.pumpWidget(createTestApp(TestSyncService(
        testDevices: [
          Device(id: '1', name: 'Laptop', lastSeen: 100),
          Device(id: '2', name: 'Server', lastSeen: 200),
        ],
      )));

      expect(find.text('Laptop'), findsOneWidget);
      expect(find.text('Server'), findsOneWidget);
    });

    testWidgets('has RefreshIndicator', (WidgetTester tester) async {
      await tester.pumpWidget(createTestApp(TestSyncService()));

      expect(find.byType(RefreshIndicator), findsOneWidget);
    });

    testWidgets('pull to refresh calls service.refresh', (WidgetTester tester) async {
      final service = TestSyncService();

      await tester.pumpWidget(createTestApp(service));

      await tester.fling(find.byType(RefreshIndicator), const Offset(0, 300), 1000);
      await tester.pump();
      await tester.pump(const Duration(seconds: 1));

      expect(service.refreshCalled, true);
    });
  });
}
