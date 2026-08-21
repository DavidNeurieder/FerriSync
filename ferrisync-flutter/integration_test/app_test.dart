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
      expect(find.text('Idle'), findsOneWidget);
      expect(find.byType(NavigationBar), findsOneWidget);
    });

    testWidgets('settings icon navigates to settings', (WidgetTester tester) async {
      await pumpApp(tester);

      await tester.tap(find.byIcon(Icons.settings));
      await tester.pumpAndSettle();

      expect(find.text('Version'), findsOneWidget);
      expect(find.text('AGPL-3.0'), findsOneWidget);
    });
  });

  group('Dashboard', () {
    testWidgets('displays status card', (WidgetTester tester) async {
      await pumpApp(tester);

      expect(find.text('Idle'), findsOneWidget);
      expect(find.text('Sync status'), findsOneWidget);
      expect(find.byIcon(Icons.check_circle), findsOneWidget);
    });

    testWidgets('displays device info section', (WidgetTester tester) async {
      await pumpApp(tester);

      expect(find.text('Device'), findsOneWidget);
      expect(find.text('ID'), findsOneWidget);
      expect(find.text('Name'), findsOneWidget);
    });

    testWidgets('shows empty paired devices message', (WidgetTester tester) async {
      await pumpApp(tester);

      expect(find.text('No devices paired.'), findsOneWidget);
    });
  });

  group('Devices', () {
    testWidgets('shows empty state', (WidgetTester tester) async {
      await pumpApp(tester);

      await tester.tap(find.text('Devices'));
      await tester.pumpAndSettle();

      expect(find.text('No devices paired'), findsOneWidget);
      expect(find.byKey(const ValueKey('pair_fab')), findsOneWidget);
      expect(find.byKey(const ValueKey('scan_fab')), findsOneWidget);
    });

    testWidgets('pair dialog opens and can be cancelled', (WidgetTester tester) async {
      await pumpApp(tester);

      await tester.tap(find.text('Devices'));
      await tester.pumpAndSettle();

      await tester.tap(find.byKey(const ValueKey('pair_fab')));
      await tester.pumpAndSettle();

      expect(find.text('Pair Device'), findsOneWidget);
      expect(find.text('IP Address'), findsOneWidget);
      expect(find.text('Port'), findsOneWidget);
      expect(find.text('Cancel'), findsOneWidget);
      expect(find.text('Pair'), findsOneWidget);

      await tester.tap(find.text('Cancel'));
      await tester.pumpAndSettle();

      expect(find.text('Pair Device'), findsNothing);
    });

    testWidgets('pair dialog accepts IP input and pairs', (WidgetTester tester) async {
      await pumpApp(tester);

      await tester.tap(find.text('Devices'));
      await tester.pumpAndSettle();

      await tester.tap(find.byKey(const ValueKey('pair_fab')));
      await tester.pumpAndSettle();

      final ipField = find.widgetWithText(TextField, 'IP Address');
      await tester.enterText(ipField, '192.168.1.50');
      await tester.pumpAndSettle();

      await tester.tap(find.text('Pair'));
      await tester.pumpAndSettle();

      expect(find.text('Pair Device'), findsNothing);
      expect(find.text('No devices paired'), findsOneWidget);
    });
  });

  group('Folders', () {
    testWidgets('shows empty state', (WidgetTester tester) async {
      await pumpApp(tester);

      await tester.tap(find.text('Folders'));
      await tester.pumpAndSettle();

      expect(find.text('No sync folders configured'), findsOneWidget);
      expect(find.byType(FloatingActionButton), findsOneWidget);
    });
  });

  group('Activity', () {
    testWidgets('renders event list', (WidgetTester tester) async {
      await pumpApp(tester);

      await tester.tap(find.text('Activity'));
      await tester.pumpAndSettle();

      expect(find.text('Device paired: Pixel 8 (a1b2c3d4) — 2 min ago'), findsOneWidget);
      expect(find.text('Synced: photos/IMG_001.jpg → Pixel 8 — 5 min ago'), findsOneWidget);
      expect(find.text('Synced: docs/report.pdf ← Laptop — 12 min ago'), findsOneWidget);
      expect(find.text('Conflict resolved: notes.txt — 1h ago'), findsOneWidget);
      expect(find.byIcon(Icons.info_outline), findsNWidgets(4));
    });

    testWidgets('event list is scrollable', (WidgetTester tester) async {
      await pumpApp(tester);

      await tester.tap(find.text('Activity'));
      await tester.pumpAndSettle();

      await tester.drag(find.byType(ListView), const Offset(0, -200));
      await tester.pumpAndSettle();

      expect(find.text('Device paired: Pixel 8 (a1b2c3d4) — 2 min ago'), findsOneWidget);
    });
  });

  group('Settings', () {
    testWidgets('shows device identity section', (WidgetTester tester) async {
      await pumpApp(tester);

      await tester.tap(find.byIcon(Icons.settings));
      await tester.pumpAndSettle();

      expect(find.text('Device'), findsOneWidget);
      expect(find.text('Display Name'), findsOneWidget);
      expect(find.text('Device ID'), findsOneWidget);
    });

    testWidgets('shows sync settings section', (WidgetTester tester) async {
      await pumpApp(tester);

      await tester.tap(find.byIcon(Icons.settings));
      await tester.pumpAndSettle();

      expect(find.text('Sync'), findsOneWidget);
      expect(find.text('Notifications'), findsOneWidget);
      expect(find.text('Show sync notifications'), findsOneWidget);
      expect(find.byType(SwitchListTile), findsOneWidget);
    });

    testWidgets('shows about section', (WidgetTester tester) async {
      await pumpApp(tester);

      await tester.tap(find.byIcon(Icons.settings));
      await tester.pumpAndSettle();

      expect(find.text('About'), findsOneWidget);
      expect(find.text('Version'), findsOneWidget);
      expect(find.text('0.1.0'), findsOneWidget);
      expect(find.text('License'), findsOneWidget);
      expect(find.text('AGPL-3.0'), findsOneWidget);
    });
  });

  group('Navigation', () {
    testWidgets('full round-trip navigation', (WidgetTester tester) async {
      await pumpApp(tester);

      expect(find.text('Idle'), findsOneWidget);

      await tester.tap(find.text('Devices'));
      await tester.pumpAndSettle();
      expect(find.text('No devices paired'), findsOneWidget);

      await tester.tap(find.text('Folders'));
      await tester.pumpAndSettle();
      expect(find.text('No sync folders configured'), findsOneWidget);

      await tester.tap(find.text('Activity'));
      await tester.pumpAndSettle();
      expect(find.byIcon(Icons.info_outline), findsWidgets);

      await tester.tap(find.text('Dashboard'));
      await tester.pumpAndSettle();
      expect(find.text('Idle'), findsOneWidget);
    });

    testWidgets('rapid navigation does not crash', (WidgetTester tester) async {
      await pumpApp(tester);

      for (final tab in ['Devices', 'Folders', 'Activity', 'Dashboard']) {
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
      expect(find.text('Version'), findsOneWidget);

      await tester.tap(find.text('Dashboard'));
      await tester.pumpAndSettle();
      expect(find.text('Idle'), findsOneWidget);
    });
  });
}
