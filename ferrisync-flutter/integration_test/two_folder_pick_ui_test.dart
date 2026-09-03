import 'package:ferrisync/main.dart';
import 'package:ferrisync/providers/sync_provider.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

/// Reproduces the reported defect: an owner with TWO published shared folders
/// must let the app choose the CORRECT one. The app must browse and render both
/// folders (distinct keys/labels) so the user can pick the right one — not be
/// forced to one of them.
///
/// The owner is a real REPL/CLI `ferrisync` that `add`s two folders (FolderA,
/// FolderB) and serves them interactively under a PTY to approve pairing.
const hostIp = String.fromEnvironment('FERRISYNC_HOST',
    defaultValue: '127.0.0.1');
const hostPort = int.fromEnvironment('FERRISYNC_PORT', defaultValue: 19899);

Future<bool> waitUntil(bool Function() condition, WidgetTester tester,
    {Duration timeout = const Duration(seconds: 30)}) async {
  final deadline = DateTime.now().add(timeout);
  while (DateTime.now().isBefore(deadline)) {
    await tester.pump(const Duration(milliseconds: 250));
    if (condition()) return true;
  }
  return false;
}

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('app can choose between two shared folders on one device',
      (WidgetTester tester) async {
    final container = ProviderContainer();
    final service = container.read(syncServiceProvider);

    await service.init();
    await service.completeOnboarding();
    await tester.pumpWidget(
      UncontrolledProviderScope(
        container: container,
        child: const FerriSyncApp(),
      ),
    );
    await tester.pumpAndSettle();

    // Pair with the owner.
    await service.refresh();
    if (service.devices.isEmpty) {
      await service.pairWithDevice(hostIp, hostPort);
      await service.refresh();
    }
    final deadline = DateTime.now().add(const Duration(seconds: 30));
    while (service.devices.isEmpty && DateTime.now().isBefore(deadline)) {
      await tester.pump(const Duration(milliseconds: 250));
    }
    expect(service.devices, isNotEmpty,
        reason: 'owner should appear as a paired device');

    // Browse the owner's shared folders: BOTH must be discoverable.
    final remote = await service.browsePeerSharedFolders(hostIp, hostPort);
    final names = remote.map((f) => f.name).toList();
    expect(names, containsAll(['FolderA', 'FolderB']),
        reason: 'browse must return BOTH published folders, got: $names');

    // Now drive the real device detail UI: open the owner's device panel, which
    // lists the remote folders "AVAILABLE TO SYNC". BOTH must render with
    // distinct keys so the user can pick the correct one.
    await tester.tap(find.text('Devices'));
    await tester.pumpAndSettle();
    await tester.tap(find.text(service.devices.first.name));
    await tester.pumpAndSettle();

    // Both folders must be present and distinguishable by guid-derived key.
    for (final f in remote) {
      final key = ValueKey('remote_path_${f.folderGuid}');
      final found = await waitUntil(() => find.byKey(key).evaluate().isNotEmpty, tester);
      expect(found, isTrue,
          reason: 'device detail must render shared folder ${f.name} '
              '(key remote_path_${f.folderGuid})');
      expect(find.text(f.name), findsOneWidget);
    }
    expect(remote.length, greaterThanOrEqualTo(2),
        reason: 'setup must publish at least two folders');

    container.dispose();
  });
}