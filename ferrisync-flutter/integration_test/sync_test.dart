import 'dart:io';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:ferrisync/gen/frb_generated.dart';
import 'package:ferrisync/gen/api.dart';

/// Integration test that exercises the full sync pipeline on-device.
///
/// File naming convention shared with test_android_flutter_sync.sh:
///   - from_local.txt   — created by this test  (pushed to remote)
///   - from_remote.txt  — created by serve dir   (pulled locally)
///   - conflict.txt     — created by both sides with different content
///
/// The test verifies that after sync the local directory contains files
/// from both sides with the correct content.
void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  late ApiState state;
  late Directory localDir;
  late String peerDeviceId;
  const remoteIp = '127.0.0.1';
  const remotePort = 9847;

  setUpAll(() async {
    await RustLib.init();
  });

  setUp(() async {
    localDir = await Directory.systemTemp.createTemp('ferrisync_local_');

    // Write files that only exist on the local (emulator) side
    await File('${localDir.path}/from_local.txt').writeAsString('local_content');

    // Write conflict file with deliberately old mtime so the host's
    // version (created by the shell script before test) is newer.
    final conflict = File('${localDir.path}/conflict.txt');
    await conflict.writeAsString('local_version');
    await conflict.setLastModified(DateTime(2020, 1, 1));

    state = await initEngine(dataDir: localDir.path);

    // Under the hardened security model a device must be paired (its TLS
    // certificate pinned with the host) before it may sync. Pair with the
    // host, then address it by the cert-derived id pairing returns.
    final pairDesc = await pairWithDevice(
        state: state, ip: remoteIp, port: remotePort);
    final open = pairDesc.lastIndexOf('(');
    if (open < 0 || !pairDesc.endsWith(')')) {
      fail('unexpected pairWithDevice result: $pairDesc');
    }
    peerDeviceId = pairDesc.substring(open + 1, pairDesc.length - 1);
  });

  tearDown(() async {
    try {
      await localDir.delete(recursive: true);
    } catch (_) {}
  });

  testWidgets('sync folder with remote serve process',
      (WidgetTester tester) async {
    final folderId = await addSyncFolder(
      state: state,
      localPath: localDir.path,
      deviceId: peerDeviceId,
      direction: 'bidirectional',
    );
    expect(folderId, greaterThan(0));

    final result = await syncFolder(
      state: state,
      folderId: folderId,
      localPath: localDir.path,
      remoteIp: remoteIp,
      remotePort: remotePort,
      deviceId: peerDeviceId,
      dryRun: false,
    );

    // ── Assertions ──

    // Local files were pushed
    expect(result.pushed, contains('from_local.txt'),
        reason: 'local-only file should be pushed');

    // Remote files were pulled
    expect(result.pulled, contains('from_remote.txt'),
        reason: 'remote-only file should be pulled');

    // from_remote.txt arrived locally with correct content
    final pulled = File('${localDir.path}/from_remote.txt');
    expect(pulled.existsSync(), true,
        reason: 'from_remote.txt should have been pulled');
    expect(pulled.readAsStringSync().trim(), 'remote_content');

    // Local file is still present
    final localStill = File('${localDir.path}/from_local.txt');
    expect(localStill.existsSync(), true,
        reason: 'from_local.txt should still exist locally');
    expect(localStill.readAsStringSync().trim(), 'local_content');

    // Conflict: remote had "remote_version" with newer mtime, so it wins.
    // Local should have the remote content and a .ferrisync-conflict-* backup
    // of the losing local version.
    final conflict = File('${localDir.path}/conflict.txt');
    expect(conflict.existsSync(), true);
    expect(conflict.readAsStringSync().trim(), 'remote_version',
        reason: 'newer (remote) content should win');

    final backupName = localDir
        .listSync()
        .whereType<File>()
        .map((f) => f.uri.pathSegments.last)
        .where((name) => name.startsWith('conflict.txt.ferrisync-conflict-'))
        .toList();
    expect(backupName, isNotEmpty,
        reason: 'a .ferrisync-conflict-* backup should exist');
    expect(File('${localDir.path}/${backupName.first}')
        .readAsStringSync()
        .trim(),
        'local_version',
        reason: 'backup should hold the losing local content');

    // Conflict was reported
    expect(result.conflicts, isNotEmpty,
        reason: 'conflict should be reported');
  });
}
