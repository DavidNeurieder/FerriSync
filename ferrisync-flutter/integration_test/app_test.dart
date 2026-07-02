import 'package:ferrisync/main.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  group('FerriSync app integration', () {
    testWidgets('app launches and shows dashboard', (WidgetTester tester) async {
      await tester.pumpWidget(const ProviderScope(child: FerriSyncApp()));
      await tester.pumpAndSettle();

      expect(find.text('FerriSync'), findsWidgets);
      expect(find.text('Idle'), findsOneWidget);
      expect(find.byType(NavigationBar), findsOneWidget);
    });

    testWidgets('can navigate to all screens', (WidgetTester tester) async {
      await tester.pumpWidget(const ProviderScope(child: FerriSyncApp()));
      await tester.pumpAndSettle();

      expect(find.text('Sync status'), findsOneWidget);

      await tester.tap(find.text('Devices'));
      await tester.pumpAndSettle();
      expect(find.text('No devices paired'), findsOneWidget);
      expect(find.byType(FloatingActionButton), findsOneWidget);

      await tester.tap(find.text('Folders'));
      await tester.pumpAndSettle();
      expect(find.text('No sync folders configured'), findsOneWidget);
      expect(find.byType(FloatingActionButton), findsOneWidget);

      await tester.tap(find.text('Activity'));
      await tester.pumpAndSettle();
      expect(find.byIcon(Icons.info_outline), findsWidgets);

      await tester.tap(find.text('Dashboard'));
      await tester.pumpAndSettle();
      expect(find.text('Idle'), findsOneWidget);
    });

    testWidgets('settings screen opens from app bar', (WidgetTester tester) async {
      await tester.pumpWidget(const ProviderScope(child: FerriSyncApp()));
      await tester.pumpAndSettle();

      await tester.tap(find.byIcon(Icons.settings));
      await tester.pumpAndSettle();

      expect(find.text('Version'), findsOneWidget);
      expect(find.text('AGPL-3.0'), findsOneWidget);
      expect(find.byType(SwitchListTile), findsOneWidget);
    });

    testWidgets('dark theme toggle works', (WidgetTester tester) async {
      await tester.pumpWidget(const ProviderScope(child: FerriSyncApp()));

      final materialApp = tester.widget<MaterialApp>(find.byType(MaterialApp));
      expect(materialApp.darkTheme, isNotNull);
      expect(materialApp.theme, isNotNull);
    });
  });
}
