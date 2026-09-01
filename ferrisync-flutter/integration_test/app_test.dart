import 'package:ferrisync/main.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

Future<void> pumpApp(WidgetTester tester) async {
  await tester.pumpWidget(const ProviderScope(child: FerriSyncApp()));
  await tester.pumpAndSettle();
}

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  group('App Shell', () {
    testWidgets('app launches and shows dashboard', (WidgetTester tester) async {
      await pumpApp(tester);

      expect(find.text('FerriSync'), findsWidgets);
      expect(find.text('Devices connected'), findsOneWidget);
      expect(find.byType(NavigationBar), findsOneWidget);
    });

    testWidgets('settings icon navigates to settings', (WidgetTester tester) async {
      await pumpApp(tester);

      await tester.tap(find.byIcon(Icons.settings));
      await tester.pumpAndSettle();

      expect(find.text('GENERAL'), findsOneWidget);
      expect(find.text('Display Name'), findsOneWidget);
    });
  });

  group('Dashboard', () {
    testWidgets('shows summary stats and device section',
        (WidgetTester tester) async {
      await pumpApp(tester);

      expect(find.text('Devices connected'), findsOneWidget);
      expect(find.text('Folders synced'), findsOneWidget);
      // "This device" card lives at the top of the dashboard.
      expect(find.textContaining('This device'), findsOneWidget);
    });

    testWidgets('shows empty paired devices message', (WidgetTester tester) async {
      await pumpApp(tester);

      expect(find.text('No devices paired'), findsOneWidget);
    });
  });

  group('Devices', () {
    testWidgets('shows empty state', (WidgetTester tester) async {
      await pumpApp(tester);

      await tester.tap(find.text('Devices'));
      await tester.pumpAndSettle();

      expect(find.text('No devices paired'), findsOneWidget);
      expect(find.byKey(const ValueKey('pair_a_device')), findsOneWidget);
      expect(find.byKey(const ValueKey('add_device_fab')), findsOneWidget);
    });

    testWidgets('opens add-device screen and returns', (WidgetTester tester) async {
      await pumpApp(tester);

      await tester.tap(find.text('Devices'));
      await tester.pumpAndSettle();

      await tester.tap(find.byKey(const ValueKey('add_device_fab')));
      await tester.pumpAndSettle();

      expect(find.text('Add device'), findsOneWidget);
      expect(find.text('Enter address manually'), findsOneWidget);

      await tester.pageBack();
      await tester.pumpAndSettle();

      expect(find.text('No devices paired'), findsOneWidget);
    });

    testWidgets('manual entry shows address fields and can be cancelled',
        (WidgetTester tester) async {
      await pumpApp(tester);

      await tester.tap(find.text('Devices'));
      await tester.pumpAndSettle();

      await tester.tap(find.byKey(const ValueKey('pair_a_device')));
      await tester.pumpAndSettle();

      await tester.tap(find.text('Enter address manually'));
      await tester.pumpAndSettle();

      expect(find.widgetWithText(TextField, 'IP address'), findsOneWidget);
      expect(find.widgetWithText(TextField, 'Port'), findsOneWidget);

      // Back out without pairing to keep the engine free.
      await tester.tap(find.text('Back'));
      await tester.pumpAndSettle();
      expect(find.text('Enter address manually'), findsOneWidget);
    });
  });

  group('Folders', () {
    testWidgets('shows empty state', (WidgetTester tester) async {
      await pumpApp(tester);

      await tester.tap(find.text('Folders'));
      await tester.pumpAndSettle();

      expect(find.text('No folders shared yet'), findsOneWidget);
      expect(find.byType(FloatingActionButton), findsOneWidget);
    });
  });

  group('Settings', () {
    testWidgets('shows device identity section', (WidgetTester tester) async {
      await pumpApp(tester);

      await tester.tap(find.byIcon(Icons.settings));
      await tester.pumpAndSettle();

      expect(find.byKey(const ValueKey('device_identity')), findsOneWidget);
      expect(find.text('Display Name'), findsOneWidget);
      expect(find.text('Device identity'), findsOneWidget);
    });

    testWidgets('shows sync settings section', (WidgetTester tester) async {
      await pumpApp(tester);

      await tester.tap(find.byIcon(Icons.settings));
      await tester.pumpAndSettle();

      expect(find.text('SYNC'), findsOneWidget);
      expect(find.text('Notifications'), findsOneWidget);
      expect(find.text('Show sync notifications'), findsOneWidget);
      expect(find.byType(SwitchListTile), findsOneWidget);
    });

    testWidgets('shows about section', (WidgetTester tester) async {
      await pumpApp(tester);

      await tester.tap(find.byIcon(Icons.settings));
      await tester.pumpAndSettle();
      await tester.drag(find.byType(ListView), const Offset(0, -600));
      await tester.pumpAndSettle();

      expect(find.text('ABOUT'), findsOneWidget);
      expect(find.text('0.1.0'), findsOneWidget);
      expect(find.text('License'), findsOneWidget);
      expect(find.text('AGPL-3.0'), findsOneWidget);
    });
  });

  group('Navigation', () {
    testWidgets('full round-trip navigation', (WidgetTester tester) async {
      await pumpApp(tester);

      expect(find.text('Devices connected'), findsOneWidget);

      await tester.tap(find.text('Devices'));
      await tester.pumpAndSettle();
      expect(find.text('No devices paired'), findsOneWidget);

      await tester.tap(find.text('Folders'));
      await tester.pumpAndSettle();
      expect(find.text('No folders shared yet'), findsOneWidget);

      await tester.tap(find.text('Home'));
      await tester.pumpAndSettle();
      expect(find.text('Devices connected'), findsOneWidget);
    });

    testWidgets('rapid navigation does not crash', (WidgetTester tester) async {
      await pumpApp(tester);

      for (final tab in ['Devices', 'Folders', 'Home']) {
        await tester.tap(find.text(tab));
        await tester.pump(const Duration(milliseconds: 100));
      }
      await tester.pumpAndSettle();

      expect(find.byType(NavigationBar), findsOneWidget);
    });

    testWidgets('settings back to dashboard via nav bar', (WidgetTester tester) async {
      await pumpApp(tester);

      await tester.tap(find.byIcon(Icons.settings));
      await tester.pumpAndSettle();
      expect(find.text('GENERAL'), findsOneWidget);

      await tester.tap(find.text('Home'));
      await tester.pumpAndSettle();
      expect(find.text('Devices connected'), findsOneWidget);
    });
  });
}