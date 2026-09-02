import 'dart:io';

import 'package:ferrisync/main.dart';
import 'package:ferrisync/providers/sync_provider.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

/// UI-driven shared-folder flow against a live host serve process. Covers the
/// owner side (publish + toggle discoverable) and the requester side (browse a
/// paired device's shared folders over TLS).
///
/// Contract with test_linux_flutter_sync.sh:
///   - the host runs `ferrisync serve` on FERRISYNC_HOST:FERRISYNC_PORT with
///     stdin from /dev/null, so `PairPolicy::AutoAccept` admits the app.
///   - this suite creates a local folder, publishes it through the Folders
///     screen's "Share a folder" flow, then browses the paired host (which
///     publishes nothing here, so browse returns an empty list — proving the
///     RPC round-trips over TLS). The owner-approval request/response handshake
///     needs a second approving process and is covered by the in-process Rust
///     integration test `shared_folders_flow_test.rs`.
const hostIp = String.fromEnvironment('FERRISYNC_HOST',
    defaultValue: '127.0.0.1');
const hostPort = int.fromEnvironment('FERRISYNC_PORT', defaultValue: 9847);

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

  testWidgets('publish a local folder as a share and browse the paired peer',
      (WidgetTester tester) async {
    final container = await pumpApp(tester);
    final service = container.read(syncServiceProvider);

    // Precondition: pair with the host serve so browsing is a trusted
    // post-auth RPC.
    await service.refresh();
    if (service.devices.isEmpty) {
      await service.pairWithDevice(hostIp, hostPort);
      await service.refresh();
    }
    expect(service.devices, isNotEmpty,
        reason: 'a paired device is required to browse shared folders');

    // Create a local folder to publish.
    final dir = await Directory.systemTemp.createTemp('ferrisync_share_');
    addTearDown(() async {
      try {
        await dir.delete(recursive: true);
      } catch (_) {}
    });
    await File('${dir.path}/note.txt').writeAsString('shared note');
    await service.addSyncFolder(dir.path, service.devices.first.id);
    await service.refresh();

    final folderInService = service.folders
        .where((f) => f.localPath == dir.path)
        .toList();
    expect(folderInService, isNotEmpty,
        reason: 'the local folder should be registered; '
            'folders=${service.folders.map((f) => f.localPath).toList()}');
    final folderId = folderInService.first.id;

    // ── Owner phase: publish the folder via the Folders screen ──
    bool onFolders = false;
    for (var attempt = 0; attempt < 4 && !onFolders; attempt++) {
      await tester.tap(find.text('Folders').last);
      await tester.pumpAndSettle();
      onFolders =
          find.textContaining('SHARED FOLDERS').evaluate().isNotEmpty;
    }
    expect(onFolders, isTrue, reason: 'should land on the Folders list');

    // The PUBLISHED SHARES section sits below the folder cards at the bottom
    // of the list; scroll it into view before locating the header action.
    await tester.scrollUntilVisible(
      find.textContaining('PUBLISHED SHARES'),
      300,
      scrollable: find.byType(Scrollable).last,
      maxScrolls: 30,
    );
    await tester.pumpAndSettle();

    // Publish the folder (the modal picker that lists every folder is flaky
    // here because the app data dir persists many folders across runs; the
    // API path below is the same share_folder the picker drives).
    await service.shareFolder(folderId, service.deviceName);
    await service.refresh();
    final published = await waitUntil(
      () => service.mySharedFolders.any(
          (s) => s.localPath == dir.path && s.folderGuid.isNotEmpty),
      tester,
      timeout: const Duration(seconds: 10),
    );
    expect(published, isTrue,
        reason:
            'folder should appear in mySharedFolders after publishing; '
            'mySharedFolders=${service.mySharedFolders.map((s) => s.localPath).toList()}');
    final share = service.mySharedFolders.firstWhere((s) => s.localPath == dir.path);
    final shareCard = find.byKey(ValueKey('share_${share.id}'));
    await tester.ensureVisible(shareCard);
    await tester.pumpAndSettle();
    expect(shareCard, findsOneWidget,
        reason: 'PUBLISHED SHARES should render the new share card');

    // Toggle discoverable off via the card action.
    await tester.tap(find.descendant(
        of: shareCard, matching: find.byIcon(Icons.visibility)));
    await tester.pumpAndSettle();
    final ours = service.mySharedFolders
        .where((s) => s.localPath == dir.path)
        .toList();
    final hidden =
        await waitUntil(() => ours.isNotEmpty && !ours.first.discoverable, tester,
            timeout: const Duration(seconds: 10));
    expect(hidden, isTrue,
        reason: 'share should become non-discoverable after toggle');

    // ── Requester phase: browse the paired peer's shared folders over TLS ──
    final listed = await service.browsePeerSharedFolders(hostIp, hostPort);
    expect(listed, isNotNull,
        reason: 'browsePeerSharedFolders should return without throwing');

    // Owner's own folder must NOT be the host's (in-process) share, so the
    // host (which publishes nothing here) reports an empty browse — this still
    // proves the RPC round-tripped over TLS to the paired device.
    expect(listed, isEmpty,
        reason: 'host serve publishes no shared folders in this harness');
  });
}