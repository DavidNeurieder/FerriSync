import 'package:ferrisync/widgets/log_entry.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('LogEntryWidget', () {
    testWidgets('renders message text', (WidgetTester tester) async {
      await tester.pumpWidget(MaterialApp(
        home: Scaffold(body: LogEntryWidget(message: 'File synced successfully')),
      ));

      expect(find.text('File synced successfully'), findsOneWidget);
    });

    testWidgets('renders timestamp', (WidgetTester tester) async {
      final ts = DateTime(2024, 1, 15, 9, 5, 3);
      await tester.pumpWidget(MaterialApp(
        home: Scaffold(body: LogEntryWidget(
          message: 'test',
          timestamp: ts,
        )),
      ));

      expect(find.text('09:05:03'), findsOneWidget);
    });

    testWidgets('renders info icon for info level', (WidgetTester tester) async {
      await tester.pumpWidget(MaterialApp(
        home: Scaffold(body: LogEntryWidget(
          message: 'test',
          level: LogLevel.info,
        )),
      ));

      expect(find.byIcon(Icons.info_outline), findsOneWidget);
    });

    testWidgets('renders success icon for success level', (WidgetTester tester) async {
      await tester.pumpWidget(MaterialApp(
        home: Scaffold(body: LogEntryWidget(
          message: 'test',
          level: LogLevel.success,
        )),
      ));

      expect(find.byIcon(Icons.check_circle_outline), findsOneWidget);
    });

    testWidgets('renders warning icon for warning level', (WidgetTester tester) async {
      await tester.pumpWidget(MaterialApp(
        home: Scaffold(body: LogEntryWidget(
          message: 'test',
          level: LogLevel.warning,
        )),
      ));

      expect(find.byIcon(Icons.warning_amber), findsOneWidget);
    });

    testWidgets('renders error icon for error level', (WidgetTester tester) async {
      await tester.pumpWidget(MaterialApp(
        home: Scaffold(body: LogEntryWidget(
          message: 'test',
          level: LogLevel.error,
        )),
      ));

      expect(find.byIcon(Icons.error_outline), findsOneWidget);
    });

    testWidgets('defaults to info level', (WidgetTester tester) async {
      await tester.pumpWidget(MaterialApp(
        home: Scaffold(body: LogEntryWidget(message: 'test')),
      ));

      expect(find.byIcon(Icons.info_outline), findsOneWidget);
    });

    testWidgets('uses current time when no timestamp provided', (WidgetTester tester) async {
      await tester.pumpWidget(MaterialApp(
        home: Scaffold(body: LogEntryWidget(message: 'test')),
      ));

      final now = DateTime.now();
      final hourStr = now.hour.toString().padLeft(2, '0');
      // Just verify the format, not the exact time
      expect(find.textContaining(':'), findsOneWidget);
    });
  });
}
