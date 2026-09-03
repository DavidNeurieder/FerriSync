import 'package:ferrisync/main.dart';
import 'package:ferrisync/providers/sync_provider.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'dart:io';

/// UI + sync-provider folder-pair test between the Linux Flutter app and a
/// REPL/CLI `ferrisync` peer.
///
/// The peer is the OWNER: a real CLI process that `add`s (publishes) a folder
/// and `serve`s it interactively under a PTY so it can approve pairing. This
/// suite is the REQUESTER: it pairs with the peer over the LAN, browses the
/// peer's published shared folder, and requests a folder pair. Drives the same
/// FRB paths the Folders screen's "Choose remote folder" flow uses.
///
/// Contract with scripts/test_linux_flutter_repl_folder_pair.sh:
///   - the peer listens on FERRISYNC_PORT and will approve the device-pairing
///     and folder-pairing prompts raised by the requests this suite sends.
const hostIp = String.fromEnvironment('FERRISYNC_HOST',
    defaultValue: '127.0.0.1');
const hostPort = int.fromEnvironment('FERRISYNC_PORT', defaultValue: 19897);
const peerDir = String.fromEnvironment('FERRISYNC_PEER_DIR', defaultValue: '');

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
    {Duration timeout = const Duration(seconds: 60)}) async {
  final deadline = DateTime.now().add(timeout);
  while (DateTime.now().isBefore(deadline)) {
    await tester.pump(const Duration(milliseconds: 250));
    if (condition()) return true;
  }
  return false;
}

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('pairs to a REPL/CLI peer and requests its published folder',
      (WidgetTester tester) async {
    final container = await pumpApp(tester);
    final service = container.read(syncServiceProvider);

    // Pair with the REPL/CLI peer. The peer's interactive serve approves the
    // device-pairing request (handled on the harness side).
    await service.refresh();
    if (service.devices.isEmpty) {
      await service.pairWithDevice(hostIp, hostPort);
      await service.refresh();
    }
    final appeared = await waitUntil(
      () => service.devices.any((d) => d.id.isNotEmpty),
      tester,
      timeout: const Duration(seconds: 30),
    );
    expect(appeared, isTrue,
        reason: 'the REPL/CLI peer should appear as a paired device after '
            'pairing; devices=${service.devices}');

    // The peer published a folder named "PeerShared" via `add`; browse to
    // discover it over TLS.
    final remote =
        await service.browsePeerSharedFolders(hostIp, hostPort);
    final target = remote
        .where((f) => f.name == 'PeerShared' || f.folderGuid.isNotEmpty)
        .toList();
    expect(target, isNotEmpty,
        reason: 'the peer\'s `add`-published folder must be browseable; '
            'listed=${remote.map((f) => f.name).toList()}');

    // Request a folder pair. The REPL/CLI owner approves, so we expect a
    // non-null folderGuid and an acknowledgment message. The replica directory
    // the app registers must exist, mirroring the real add-folder flow.
    const replicaLocalPath = '/tmp/ferrisync_repl_peer_replica';
    await Directory(replicaLocalPath).create(recursive: true);
    addTearDown(() async {
      try {
        await Directory(replicaLocalPath).delete(recursive: true);
      } catch (_) {}
    });

    // Drop a file on the OWNER's copy before pairing so the first sync has
    // something to pull into our replica. The harness hands us the owner's
    // served directory via FERRISYNC_PEER_DIR.
    const ownerFile = 'landshot.txt';
    if (peerDir.isNotEmpty) {
      final seed = File('$peerDir/$ownerFile');
      if (!seed.existsSync()) seed.writeAsStringSync('seeded before pairing');
    }

    final result = await service.requestFolderPairing(
      peerIp: hostIp,
      peerPort: hostPort,
      peerDeviceId: service.devices.first.id,
      folderGuid: target.first.folderGuid,
      shareName: target.first.name,
      localPath: replicaLocalPath,
      lifetimeMs: 45000,
    );
    expect(result.folderGuid, isNotNull,
        reason: 'folder pair to the REPL/CLI peer should be approved; '
            'message=${result.message}');
    expect(result.message, contains('Approved'),
        reason: 'expected an approval message, got: ${result.message}');

    // The approved folder must remember WHICH remote folder it is paired with
    // (the owner's shared path) so the Folders card can show it, not just the
    // device name.
    await service.refresh();
    final folder = service.folders.firstWhere(
        (f) => f.localPath == replicaLocalPath);
    final peer = folder.peerFor(service.devices.first.id);
    if (peerDir.isNotEmpty) {
      expect(peer?.remotePath, peerDir,
          reason: 'the requester should record the owner\'s shared folder path '
              'as that peer\'s remotePath, got: ${peer?.remotePath}');
    } else {
      expect(peer?.remotePath, isNotEmpty,
          reason: 'the requester should have a remotePath for the paired peer');
    }

    // Approval registers the replica; now run a sync and assert the owner's
    // seeded file actually lands in our local copy.
    await service.syncFolder(replicaLocalPath, hostIp, remotePort: hostPort);
    final didSync = service.status;
    final syncError = service.lastErrorMessage;
    final landed = await waitUntil(
      () => File('$replicaLocalPath/$ownerFile').existsSync(),
      tester,
      timeout: const Duration(seconds: 60),
    );
    expect(landed, isTrue,
        reason: 'the paired folder should sync its contents; '
            'status=$didSync error=$syncError; missing '
            '$replicaLocalPath/$ownerFile');
    expect(File('$replicaLocalPath/$ownerFile').readAsStringSync(),
        'seeded before pairing');
  });
}