import 'package:ferrisync/models/sync_models.dart';
import 'package:ferrisync/providers/sync_provider.dart';
import 'package:ferrisync/screens/devices_screen.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

class MockSyncService extends SyncService {
  MockSyncService({
    this.testDevices = const [],
  });

  final List<Device> testDevices;

  @override
  List<Device> get devices => testDevices;

  @override
  Future<void> refresh() async {}
}

Widget createTestApp(SyncService service) {
  return ProviderScope(
    overrides: [
      syncServiceProvider.overrideWithValue(service),
    ],
    child: MaterialApp(home: DevicesScreen()),
  );
}

void main() {
  group('DevicesScreen', () {
    testWidgets('shows empty state when no devices', (WidgetTester tester) async {
      await tester.pumpWidget(createTestApp(MockSyncService()));

      expect(find.text('No devices paired'), findsOneWidget);
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

    testWidgets('shows floating action button', (WidgetTester tester) async {
      await tester.pumpWidget(createTestApp(MockSyncService()));

      expect(find.byType(FloatingActionButton), findsOneWidget);
      expect(find.byIcon(Icons.add), findsOneWidget);
    });

    testWidgets('each device has a delete button', (WidgetTester tester) async {
      await tester.pumpWidget(createTestApp(MockSyncService(
        testDevices: [
          Device(id: '1', name: 'Pixel 8', lastSeen: 100),
        ],
      )));

      expect(find.byIcon(Icons.delete_outline), findsOneWidget);
    });
  });
}
