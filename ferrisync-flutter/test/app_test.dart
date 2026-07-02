import 'package:ferrisync/main.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets('full app renders navigation bar', (WidgetTester tester) async {
    await tester.pumpWidget(const ProviderScope(child: FerriSyncApp()));

    expect(find.byType(MaterialApp), findsOneWidget);
    expect(find.byType(NavigationBar), findsOneWidget);
    expect(find.byType(NavigationDestination), findsNWidgets(4));

    expect(find.text('Dashboard'), findsOneWidget);
    expect(find.text('Devices'), findsOneWidget);
    expect(find.text('Folders'), findsOneWidget);
    expect(find.text('Activity'), findsOneWidget);
  });

  testWidgets('default route is Dashboard', (WidgetTester tester) async {
    await tester.pumpWidget(const ProviderScope(child: FerriSyncApp()));

    expect(find.text('Sync status'), findsOneWidget);
    expect(find.text('Idle'), findsOneWidget);
  });

  testWidgets('navigation to Devices screen', (WidgetTester tester) async {
    await tester.pumpWidget(const ProviderScope(child: FerriSyncApp()));

    await tester.tap(find.text('Devices'));
    await tester.pumpAndSettle();

    expect(find.text('No devices paired'), findsOneWidget);
  });

  testWidgets('navigation to Folders screen', (WidgetTester tester) async {
    await tester.pumpWidget(const ProviderScope(child: FerriSyncApp()));

    await tester.tap(find.text('Folders'));
    await tester.pumpAndSettle();

    expect(find.text('No sync folders configured'), findsOneWidget);
  });

  testWidgets('navigation to Activity screen', (WidgetTester tester) async {
    await tester.pumpWidget(const ProviderScope(child: FerriSyncApp()));

    await tester.tap(find.text('Activity'));
    await tester.pumpAndSettle();

    expect(find.byIcon(Icons.info_outline), findsWidgets);
  });

  testWidgets('settings icon opens settings screen', (WidgetTester tester) async {
    await tester.pumpWidget(const ProviderScope(child: FerriSyncApp()));

    await tester.tap(find.byIcon(Icons.settings));
    await tester.pumpAndSettle();

    expect(find.text('Version'), findsOneWidget);
    expect(find.text('License'), findsOneWidget);
  });
}
