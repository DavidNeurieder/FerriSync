import 'dart:io';

import 'package:ferrisync/main.dart';
import 'package:ferrisync/providers/sync_provider.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

/// UI-driven folder lifecycle: add folder, sync from the tile's overflow
/// menu, and verify the transfer actually moved bytes both ways.
///
/// Contract with test_android_flutter_sync.sh:
///   - the host serve directory is pre-seeded with `from_host.txt`
///     containing exactly `host_content` (runner does this before launch)
///   - this suite creates a temp dir with `from_app.txt` = `app_content`;
///     after the run the runner asserts that file exists in the host
///     serve dir (push direction is verified outside the app sandbox)
///
/// The folder itself is added via SyncService.addSyncFolder because the
/// production flow uses the native SAF file picker, which cannot be
/// driven by flutter_test on-device (documented seam).
const hostIp = String.fromEnvironment('FERRISYNC_HOST',
    defaultValue: '127.0.0.1');
const hostPort =
    int.fromEnvironment('FERRISYNC_PORT', defaultValue: 9847);

Future<ProviderContainer> pumpApp(WidgetTester tester) async {
  final container = ProviderContainer();
  final service = container.read(syncServiceProvider);
  await service.init();
  // The engine is fresh after each install, so get past the first-launch
  // wizard and land on the shell's dashboard before driving the tabs.
  await service.completeOnboarding();
  await tester.pumpWidget(
    UncontrolledProviderScope(
      container: container,
      child: const FerriSyncApp(),
    ),
  );
  await tester.pumpAndSettle();
  return container;
}

Future<bool> waitUntil(bool Function() condition, WidgetTester tester,
    {Duration timeout = const Duration(seconds: 45)}) async {
  final deadline = DateTime.now().add(timeout);
  while (DateTime.now().isBefore(deadline)) {
    await tester.pump(const Duration(milliseconds: 250));
    if (condition()) return true;
  }
  return false;
}

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  // The storage-permission gate must not block the UI flow under test;
  // report it as already granted.
  TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
      .setMockMethodCallHandler(
    const MethodChannel('flutter.baseflow.com/permissions/methods'),
    (call) async {
      switch (call.method) {
        case 'checkPermissionStatus':
          return 1; // PermissionStatus.granted
        case 'requestPermissions':
          return <int, int>{};
        default:
          return null;
      }
    },
  );

  testWidgets('folder sync now from tile overflow menu transfers both ways',
      (WidgetTester tester) async {
    final container = await pumpApp(tester);
    final service = container.read(syncServiceProvider);

    // Precondition: at least one paired device (pair via API if needed —
    // pairing_ui_test.dart covers the dialog path).
    await service.refresh();
    if (service.devices.isEmpty) {
      await service.pairWithDevice(hostIp, hostPort);
      await service.refresh();
    }
    expect(service.devices, isNotEmpty,
        reason: 'a paired device is required for folder sync');

    final deviceId = service.devices.first.id;

    final dir = await Directory.systemTemp.createTemp('ferrisync_ui_folder_');
    addTearDown(() async {
      try {
        await dir.delete(recursive: true);
      } catch (_) {}
    });
    await File('${dir.path}/from_app.txt').writeAsString('app_content');

    // Seam: skip native SAF picker.
    await service.addSyncFolder(dir.path, deviceId);

    // Let the background sync/auto-refresh settle and clear any transient
    // snackbar so it cannot intercept the nav-bar tap below.
    await tester.pumpAndSettle();
    await tester.pump(const Duration(seconds: 5));
    await tester.pumpAndSettle();

    // Navigate to the Folders tab. Retry because an in-flight background
    // refresh can briefly leave the shell on Home.
    final tileTitle = dir.path.split('/').last;
    bool onFolders = false;
    for (var attempt = 0; attempt < 4 && !onFolders; attempt++) {
      await tester.tap(find.text('Folders').last);
      await tester.pumpAndSettle();
      onFolders =
          find.textContaining('SHARED FOLDERS').evaluate().isNotEmpty;
    }
    expect(onFolders, isTrue, reason: 'should land on the Folders list');

    final renderedTexts = find
        .byType(Text)
        .evaluate()
        .map((e) => ((e.widget as Text).data ?? '<rich>'))
        .toList();
    expect(find.text(tileTitle), findsOneWidget,
        reason: 'folder tile "$tileTitle" not rendered; '
            'service.folders=${service.folders.map((f) => f.localPath).toList()}, '
            'deviceId=$deviceId, renderedTexts=$renderedTexts');

    // The tile exposes sync through its overflow menu.
    final folderId =
        service.folders.firstWhere((f) => f.localPath == dir.path).id;
    final tileKey = find.byKey(ValueKey('folder_tile_$folderId'));
    expect(tileKey, findsOneWidget,
        reason: 'folder card (folder_tile_$folderId) not rendered');
    // The overflow button can sit below the fold on small windows; scroll it
    // into view first so the tap actually hits it.
    final overflow = find.descendant(
        of: tileKey, matching: find.byIcon(Icons.more_vert));
    expect(overflow, findsOneWidget);
    await tester.ensureVisible(overflow);
    await tester.pumpAndSettle();
    await tester.tap(overflow);
    await tester.pumpAndSettle();
    await tester.tap(find.text('Sync now'));
    await tester.pumpAndSettle();

    final done = await waitUntil(
      () => find.textContaining('Sync complete').evaluate().isNotEmpty ||
          find.textContaining('Sync failed').evaluate().isNotEmpty,
      tester,
    );
    expect(done, true, reason: 'sync now should finish with a snackbar');
    expect(find.textContaining('Sync failed'), findsNothing);

    // Pulled content arrived with correct bytes.
    final pulled = File('${dir.path}/from_host.txt');
    expect(pulled.existsSync(), true,
        reason: 'host-seeded file should be pulled into the folder');
    expect(pulled.readAsStringSync().trim(), 'host_content');

    // Local file still present.
    expect(File('${dir.path}/from_app.txt').readAsStringSync().trim(),
        'app_content');
  });
}
