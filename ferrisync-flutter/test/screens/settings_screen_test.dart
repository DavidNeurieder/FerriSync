import 'package:ferrisync/providers/sync_provider.dart';
import 'package:ferrisync/screens/diagnostics_screen.dart';
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

class MockRecordingService extends MockSyncService {
  MockRecordingService({super.testDeviceName});

  String? removed;
  String? reset;

  @override
  Future<String> removeAllDevices() async {
    removed = 'triggered';
    return 'Removed 2 device(s)';
  }

  @override
  Future<String> factoryReset() async {
    reset = 'triggered';
    return 'Device reset to a fresh install. A new device id was generated.';
  }
}
Widget createTestApp(SyncService service) {
  return ProviderScope(
    overrides: [
      syncServiceProvider.overrideWith((ref) => service),
    ],
    child: const MaterialApp(home: Scaffold(body: SettingsScreen())),
  );
}

/// Pump the settings screen on a tall surface so every section is mounted.
Future<void> pumpSettings(WidgetTester tester, SyncService service) async {
  await tester.binding.setSurfaceSize(const Size(800, 1400));
  addTearDown(() => tester.binding.setSurfaceSize(null));
  await tester.pumpWidget(createTestApp(service));
}

void main() {
  group('SettingsScreen', () {
    testWidgets('renders general section with device name', (WidgetTester tester) async {
      await pumpSettings(tester, MockSyncService(
        testDeviceId: 'dev-abc-123',
        testDeviceName: 'My Phone',
      ));

      expect(find.text('My Phone'), findsOneWidget);
      expect(find.text('GENERAL'), findsOneWidget);
    });

    testWidgets('device identity with copy affordance lives under Advanced',
        (WidgetTester tester) async {
      await pumpSettings(tester, MockSyncService(testDeviceId: 'dev-abc-123'));

      expect(find.text('ADVANCED'), findsOneWidget);
      expect(find.byKey(const ValueKey('device_identity')), findsOneWidget);
      expect(find.text('dev-abc-123'), findsOneWidget);
      expect(find.byIcon(Icons.copy), findsOneWidget);
    });

    testWidgets('diagnostics tile lives under Advanced and opens the screen',
        (WidgetTester tester) async {
      await pumpSettings(tester, MockSyncService());

      expect(find.text('ADVANCED'), findsOneWidget);
      expect(find.byKey(const ValueKey('diagnostics')), findsOneWidget);

      await tester.ensureVisible(find.byKey(const ValueKey('diagnostics')));
      await tester.tap(find.byKey(const ValueKey('diagnostics')));
      await tester.pumpAndSettle();

      expect(find.byType(DiagnosticsScreen), findsOneWidget);
      expect(find.text('Diagnostics'), findsOneWidget);
    });

    testWidgets('renders sync section with notifications toggle', (WidgetTester tester) async {
      await pumpSettings(tester, MockSyncService());

      expect(find.text('Notifications'), findsOneWidget);
      expect(find.text('Show sync notifications'), findsOneWidget);
      expect(find.byType(SwitchListTile), findsOneWidget);
    });

    testWidgets('renders about section with version and license', (WidgetTester tester) async {
      await pumpSettings(tester, MockSyncService());

      expect(find.text('Version'), findsOneWidget);
      expect(find.text('License'), findsOneWidget);
      expect(find.text('0.1.0'), findsOneWidget);
      expect(find.text('AGPL-3.0'), findsOneWidget);
    });

    testWidgets('shows edit icon for device name', (WidgetTester tester) async {
      await pumpSettings(tester, MockSyncService());

      expect(find.byIcon(Icons.edit), findsOneWidget);
    });

    testWidgets('renders all section cards', (WidgetTester tester) async {
      await pumpSettings(tester, MockSyncService());

      expect(find.byType(Card), findsNWidgets(6));
    });

    testWidgets('theme tile defaults to dark placement', (WidgetTester tester) async {
      await pumpSettings(tester, MockSyncService());

      expect(find.text('Theme'), findsOneWidget);
      expect(find.text('Dark'), findsOneWidget);
    });

    testWidgets('theme picker offers and applies the System option',
        (WidgetTester tester) async {
      await pumpSettings(tester, MockSyncService());

      await tester.tap(find.text('Theme'));
      await tester.pumpAndSettle();
      expect(find.text('System'), findsOneWidget);

      await tester.tap(find.text('System'));
      await tester.pumpAndSettle();

      expect(find.text('System'), findsOneWidget);
      expect(find.text('Dark'), findsNothing);
    });

    testWidgets('security section lists trusted devices',
        (WidgetTester tester) async {
      await pumpSettings(tester, MockSyncService());

      expect(find.text('SECURITY'), findsOneWidget);
      expect(find.text('Trusted devices'), findsOneWidget);
      expect(find.text('No devices paired yet'), findsOneWidget);
    });

    testWidgets('danger zone lists remove-all and factory reset',
        (WidgetTester tester) async {
      await pumpSettings(tester, MockSyncService());

      expect(find.text('DANGER ZONE'), findsOneWidget);
      expect(
        find.byKey(const ValueKey('remove_all_devices')),
        findsOneWidget,
      );
      expect(find.byKey(const ValueKey('factory_reset')), findsOneWidget);
    });

    testWidgets('canceling remove-all does not invoke the engine',
        (WidgetTester tester) async {
      final service = MockRecordingService();
      await pumpSettings(tester, service);

      await tester.tap(find.byKey(const ValueKey('remove_all_devices')));
      await tester.pumpAndSettle();
      expect(find.text('Remove all devices?'), findsOneWidget);

      await tester.tap(find.text('Cancel'));
      await tester.pumpAndSettle();
      expect(service.removed, isNull);
    });

    testWidgets('factory reset shows a confirm dialog and runs on confirm',
        (WidgetTester tester) async {
      final service = MockRecordingService();
      await pumpSettings(tester, service);

      await tester.tap(find.byKey(const ValueKey('factory_reset')));
      await tester.pumpAndSettle();
      expect(find.text('Factory reset?'), findsOneWidget);

      await tester.tap(find.widgetWithText(FilledButton, 'Factory reset'));
      await tester.pumpAndSettle();
      expect(service.reset, 'triggered');
      expect(
        find.text('Device reset to a fresh install. A new device id was generated.'),
        findsOneWidget,
      );
    });

    testWidgets('canceling factory reset does not invoke the engine',
        (WidgetTester tester) async {
      final service = MockRecordingService();
      await pumpSettings(tester, service);

      await tester.tap(find.byKey(const ValueKey('factory_reset')));
      await tester.pumpAndSettle();
      expect(find.text('Factory reset?'), findsOneWidget);

      await tester.tap(find.text('Cancel'));
      await tester.pumpAndSettle();
      expect(service.reset, isNull);
    });
  });
}