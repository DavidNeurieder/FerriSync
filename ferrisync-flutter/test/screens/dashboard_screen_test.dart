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
    child: const MaterialApp(home: DashboardScreen()),
  );
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
  });
}