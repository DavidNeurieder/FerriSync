import 'package:ferrisync/widgets/progress_bar.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('SyncProgressBar', () {
    testWidgets('renders label text', (WidgetTester tester) async {
      await tester.pumpWidget(MaterialApp(
        home: Scaffold(body: SyncProgressBar(label: 'Syncing photos...')),
      ));

      expect(find.text('Syncing photos...'), findsOneWidget);
    });

    testWidgets('uses default label', (WidgetTester tester) async {
      await tester.pumpWidget(MaterialApp(
        home: Scaffold(body: SyncProgressBar()),
      ));

      expect(find.text('Syncing...'), findsOneWidget);
    });

    testWidgets('shows indeterminate progress when totalFiles is 0', (WidgetTester tester) async {
      await tester.pumpWidget(MaterialApp(
        home: Scaffold(body: SyncProgressBar(filesSynced: 0, totalFiles: 0)),
      ));

      final progress = tester.widget<LinearProgressIndicator>(find.byType(LinearProgressIndicator));
      expect(progress.value, isNull);
    });

    testWidgets('shows determinate progress when totalFiles > 0', (WidgetTester tester) async {
      await tester.pumpWidget(MaterialApp(
        home: Scaffold(body: SyncProgressBar(filesSynced: 3, totalFiles: 10)),
      ));

      final progress = tester.widget<LinearProgressIndicator>(find.byType(LinearProgressIndicator));
      expect(progress.value, 0.3);
    });

    testWidgets('shows files synced count text', (WidgetTester tester) async {
      await tester.pumpWidget(MaterialApp(
        home: Scaffold(body: SyncProgressBar(filesSynced: 3, totalFiles: 10)),
      ));

      expect(find.text('3 / 10 files'), findsOneWidget);
    });

    testWidgets('hides file count when totalFiles is 0', (WidgetTester tester) async {
      await tester.pumpWidget(MaterialApp(
        home: Scaffold(body: SyncProgressBar(filesSynced: 0, totalFiles: 0)),
      ));

      expect(find.text('0 / 0 files'), findsNothing);
    });

    testWidgets('uses Card wrapper', (WidgetTester tester) async {
      await tester.pumpWidget(MaterialApp(
        home: Scaffold(body: SyncProgressBar()),
      ));

      expect(find.byType(Card), findsOneWidget);
    });
  });
}
