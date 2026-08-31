import 'package:ferrisync/main.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets('full app renders navigation bar', (WidgetTester tester) async {
    await tester.pumpWidget(const ProviderScope(child: FerriSyncApp()));

    expect(find.byType(MaterialApp), findsOneWidget);
    expect(find.byType(NavigationBar), findsOneWidget);
    expect(find.byType(NavigationDestination), findsNWidgets(3));

    expect(find.text('Home'), findsOneWidget);
    expect(find.text('Devices'), findsOneWidget);
    expect(find.text('Folders'), findsOneWidget);
  });

  testWidgets('default route is Dashboard', (WidgetTester tester) async {
    await tester.pumpWidget(const ProviderScope(child: FerriSyncApp()));

    expect(find.text('Devices connected'), findsOneWidget);
    expect(find.textContaining('This device'), findsOneWidget);
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

    expect(find.text('No folders shared yet'), findsOneWidget);
  });

  testWidgets('settings icon opens settings screen', (WidgetTester tester) async {
    await tester.pumpWidget(const ProviderScope(child: FerriSyncApp()));

    await tester.tap(find.byIcon(Icons.settings));
    await tester.pumpAndSettle();

    expect(find.text('GENERAL'), findsOneWidget);
    expect(find.text('Display Name'), findsOneWidget);
  });
}