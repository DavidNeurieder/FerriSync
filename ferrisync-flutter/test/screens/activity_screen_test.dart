import 'package:ferrisync/screens/activity_screen.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

Widget createTestApp() {
  return const ProviderScope(
    child: MaterialApp(home: Scaffold(body: ActivityScreen())),
  );
}

void main() {
  group('ActivityScreen', () {
    testWidgets('renders activity events', (WidgetTester tester) async {
      await tester.pumpWidget(createTestApp());

      expect(find.text('Device paired: Pixel 8 (a1b2c3d4) — 2 min ago'), findsOneWidget);
      expect(find.text('Synced: photos/IMG_001.jpg → Pixel 8 — 5 min ago'), findsOneWidget);
      expect(find.text('Synced: docs/report.pdf ← Laptop — 12 min ago'), findsOneWidget);
      expect(find.text('Conflict resolved: notes.txt — 1h ago'), findsOneWidget);
    });

    testWidgets('shows info icons for each event', (WidgetTester tester) async {
      await tester.pumpWidget(createTestApp());

      expect(find.byIcon(Icons.info_outline), findsNWidgets(4));
    });

    testWidgets('uses ListView', (WidgetTester tester) async {
      await tester.pumpWidget(createTestApp());

      expect(find.byType(ListView), findsOneWidget);
    });
  });
}
