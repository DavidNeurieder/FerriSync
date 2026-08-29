import 'dart:io';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:ferrisync/gen/frb_generated.dart';
import 'package:ferrisync/gen/api.dart';

/// Incremental-sync integration test.
///
/// Contract with test_android_flutter_sync.sh:
///   - serve dir is seeded with base.txt = 'v1' before this suite runs
///   - after the suite, the script verifies on the HOST that the second
///     sync pushed our edits: base.txt == 'v2-from-app' and
///     app_new.txt == 'made-by-app'
///
/// Round 1 pulls the seed, round 2 pushes local modifications, proving
/// that changes made between sessions propagate through the real Rust
/// core running on-device.
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
    localDir = await Directory.systemTemp.createTemp('ferrisync_incr_');
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

  testWidgets('incremental changes propagate on second sync',
      (WidgetTester tester) async {
    final folderId = await addSyncFolder(
      state: state,
      localPath: localDir.path,
      deviceId: peerDeviceId,
      direction: 'bidirectional',
    );
    expect(folderId, greaterThan(0));

    // ── Round 1: pull the seeded remote file ──
    final r1 = await syncFolder(
      state: state,
      folderId: folderId,
      localPath: localDir.path,
      remoteIp: remoteIp,
      remotePort: remotePort,
      deviceId: peerDeviceId,
    );
    expect(r1.pulled, contains('base.txt'),
        reason: 'seeded remote file should be pulled');
    expect(
        File('${localDir.path}/base.txt').readAsStringSync().trim(), 'v1');

    await Future<void>.delayed(const Duration(milliseconds: 50));

    // ── Local changes between sessions ──
    await File('${localDir.path}/base.txt').writeAsString('v2-from-app');
    await File('${localDir.path}/app_new.txt').writeAsString('made-by-app');

    // ── Round 2: push the modifications ──
    final r2 = await syncFolder(
      state: state,
      folderId: folderId,
      localPath: localDir.path,
      remoteIp: remoteIp,
      remotePort: remotePort,
      deviceId: peerDeviceId,
    );
    expect(r2.pushed, contains('base.txt'),
        reason: 'modified file should be re-pushed');
    expect(r2.pushed, contains('app_new.txt'),
        reason: 'newly added file should be pushed');

    expect(
        File('${localDir.path}/base.txt').readAsStringSync().trim(),
        'v2-from-app');
    expect(
        File('${localDir.path}/app_new.txt').readAsStringSync().trim(),
        'made-by-app');
  });
}
