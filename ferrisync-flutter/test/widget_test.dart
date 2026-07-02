import 'package:ferrisync/main.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets('FerriSyncApp renders with navigation', (WidgetTester tester) async {
    await tester.pumpWidget(const ProviderScope(child: FerriSyncApp()));

    expect(find.text('FerriSync'), findsWidgets);
    expect(find.byType(NavigationBar), findsOneWidget);
    expect(find.text('Dashboard'), findsOneWidget);
    expect(find.text('Devices'), findsOneWidget);
    expect(find.text('Folders'), findsOneWidget);
    expect(find.text('Activity'), findsOneWidget);
  });

  testWidgets('default route shows Dashboard', (WidgetTester tester) async {
    await tester.pumpWidget(const ProviderScope(child: FerriSyncApp()));

    expect(find.text('Idle'), findsOneWidget);
    expect(find.text('Sync status'), findsOneWidget);
  });
}
