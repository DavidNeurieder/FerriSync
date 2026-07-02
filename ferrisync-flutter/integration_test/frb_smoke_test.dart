import 'dart:io';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:ferrisync/gen/frb_generated.dart';
import 'package:ferrisync/gen/api.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  group('FRB Smoke Test', () {
    setUpAll(() async {
      await RustLib.init();
    });

    testWidgets('initEngine returns valid state', (WidgetTester tester) async {
      final dir = await tester.runAsync(() => Directory.systemTemp.createTemp('ferrisync_test_'));
      final state = await initEngine(dataDir: dir.path);
      expect(state.dataDir, dir.path);
      expect(state.deviceInfo.id, isNotEmpty);
      await dir.delete(recursive: true);
    });

    testWidgets('deviceName returns a non-empty string', (WidgetTester tester) async {
      final dir = await tester.runAsync(() => Directory.systemTemp.createTemp('ferrisync_test_'));
      final state = await initEngine(dataDir: dir.path);
      final name = await deviceName(state: state);
      expect(name, isNotEmpty);
      await dir.delete(recursive: true);
    });

    testWidgets('listDevices returns empty for fresh engine', (WidgetTester tester) async {
      final dir = await tester.runAsync(() => Directory.systemTemp.createTemp('ferrisync_test_'));
      final state = await initEngine(dataDir: dir.path);
      final devices = await listDevices(state: state);
      expect(devices, isEmpty);
      await dir.delete(recursive: true);
    });

    testWidgets('listSyncFolders returns empty for fresh engine', (WidgetTester tester) async {
      final dir = await tester.runAsync(() => Directory.systemTemp.createTemp('ferrisync_test_'));
      final state = await initEngine(dataDir: dir.path);
      final folders = await listSyncFolders(state: state);
      expect(folders, isEmpty);
      await dir.delete(recursive: true);
    });

    testWidgets('addSyncFolder creates a folder entry', (WidgetTester tester) async {
      final dir = await tester.runAsync(() => Directory.systemTemp.createTemp('ferrisync_test_'));
      final state = await initEngine(dataDir: dir.path);
      final folderId = await addSyncFolder(
        state: state,
        localPath: dir.path,
        deviceId: 'test-device',
        direction: 'bidirectional',
      );
      expect(folderId, greaterThan(0));

      final folders = await listSyncFolders(state: state);
      expect(folders, hasLength(1));
      expect(folders.first.localPath, dir.path);
      await dir.delete(recursive: true);
    });
  });
}
