import 'package:ferrisync/models/sync_models.dart';
import 'package:ferrisync/widgets/folder_tile.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('FolderTile', () {
    final folder = SyncFolder(
      id: 1,
      localPath: '/home/user/sync/photos',
      deviceId: 'dev-1',
      direction: 'push',
      lastSyncAt: 1000000,
    );

    testWidgets('renders folder name (last path segment)', (WidgetTester tester) async {
      await tester.pumpWidget(MaterialApp(
        home: Scaffold(body: FolderTile(folder: folder)),
      ));

      expect(find.text('photos'), findsOneWidget);
    });

    testWidgets('renders full local path', (WidgetTester tester) async {
      await tester.pumpWidget(MaterialApp(
        home: Scaffold(body: FolderTile(folder: folder)),
      ));

      expect(find.textContaining('/home/user/sync/photos'), findsOneWidget);
    });

    testWidgets('renders direction and last sync', (WidgetTester tester) async {
      await tester.pumpWidget(MaterialApp(
        home: Scaffold(body: FolderTile(folder: folder)),
      ));

      expect(find.textContaining('push'), findsOneWidget);
    });

    testWidgets('shows enabled switch by default', (WidgetTester tester) async {
      await tester.pumpWidget(MaterialApp(
        home: Scaffold(body: FolderTile(folder: folder)),
      ));

      final switchWidget = tester.widget<Switch>(find.byType(Switch));
      expect(switchWidget.value, true);
    });

    testWidgets('shows disabled switch when enabled is false', (WidgetTester tester) async {
      await tester.pumpWidget(MaterialApp(
        home: Scaffold(body: FolderTile(folder: folder, enabled: false)),
      ));

      final switchWidget = tester.widget<Switch>(find.byType(Switch));
      expect(switchWidget.value, false);
    });

    testWidgets('calls onToggle when switch is toggled', (WidgetTester tester) async {
      bool toggled = false;
      await tester.pumpWidget(MaterialApp(
        home: Scaffold(body: FolderTile(
          folder: folder,
          onToggle: () => toggled = true,
        )),
      ));

      await tester.tap(find.byType(Switch));
      expect(toggled, true);
    });

    testWidgets('renders folder icon', (WidgetTester tester) async {
      await tester.pumpWidget(MaterialApp(
        home: Scaffold(body: FolderTile(folder: folder)),
      ));

      expect(find.byIcon(Icons.folder), findsOneWidget);
    });
  });
}
