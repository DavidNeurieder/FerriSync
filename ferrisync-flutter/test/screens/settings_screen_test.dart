import 'package:ferrisync/models/sync_models.dart';
import 'package:ferrisync/providers/sync_provider.dart';
import 'package:ferrisync/screens/settings_screen.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

class MockSyncService extends SyncService {
  MockSyncService({
    this.testDeviceId = '',
    this.testDeviceName = '',
  });

  final String testDeviceId;
  final String testDeviceName;

  @override
  String get deviceId => testDeviceId;

  @override
  String get deviceName => testDeviceName;
}

Widget createTestApp(SyncService service) {
  return ProviderScope(
    overrides: [
      syncServiceProvider.overrideWithValue(service),
    ],
    child: MaterialApp(home: SettingsScreen()),
  );
}

void main() {
  group('SettingsScreen', () {
    testWidgets('renders device section with name and ID', (WidgetTester tester) async {
      await tester.pumpWidget(createTestApp(MockSyncService(
        testDeviceId: 'dev-abc-123',
        testDeviceName: 'My Phone',
      )));

      expect(find.text('My Phone'), findsOneWidget);
      expect(find.text('dev-abc-123'), findsOneWidget);
      expect(find.text('Device'), findsOneWidget);
    });

    testWidgets('renders sync section with notifications toggle', (WidgetTester tester) async {
      await tester.pumpWidget(createTestApp(MockSyncService()));

      expect(find.text('Notifications'), findsOneWidget);
      expect(find.text('Show sync notifications'), findsOneWidget);
      expect(find.byType(SwitchListTile), findsOneWidget);
    });

    testWidgets('renders about section with version and license', (WidgetTester tester) async {
      await tester.pumpWidget(createTestApp(MockSyncService()));

      expect(find.text('Version'), findsOneWidget);
      expect(find.text('License'), findsOneWidget);
      expect(find.text('0.1.0'), findsOneWidget);
      expect(find.text('AGPL-3.0'), findsOneWidget);
    });

    testWidgets('shows edit icon for device name', (WidgetTester tester) async {
      await tester.pumpWidget(createTestApp(MockSyncService()));

      expect(find.byIcon(Icons.edit), findsOneWidget);
    });

    testWidgets('renders all three cards', (WidgetTester tester) async {
      await tester.pumpWidget(createTestApp(MockSyncService()));

      expect(find.byType(Card), findsNWidgets(3));
    });
  });
}
