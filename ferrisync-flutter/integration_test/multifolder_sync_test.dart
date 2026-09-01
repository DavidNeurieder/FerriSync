import 'dart:io';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:ferrisync/gen/frb_generated.dart';
import 'package:ferrisync/gen/api.dart';

/// Multi-folder sync test: one device maintains two sync folders against the
/// same host, and the second folder exercises per-pair `remote_path`
/// relocation so the host serves it from a sub-directory rather than the
/// folder's own basename.
///
/// Contract with test_linux_flutter_sync.sh and test_android_flutter_sync.sh:
///   - the host serve dir ($SERVE_DIR) is seeded with:
///       from_host_root.txt   = 'host_root_content'   (folder A pulls)
///     and its sub-dir $SERVE_DIR/second seeded with:
///       from_host_second.txt = 'host_second_content' (folder B pulls)
///   - each script passes FERRISYNC_REMOTE_PATH=$SERVE_DIR/second via
///     --dart-define, because remote_path is the absolute destination on the
///     responder host (the engine reconciles into that created dir).
///   - folder A registers no remote path  -> host serves it from $SERVE_DIR
///   - folder B registers remotePath=$FERRISYNC_REMOTE_PATH
///   - the app creates from_app_A.txt / from_app_B.txt in its two local dirs;
///     after the run the runner asserts those files exist at $SERVE_DIR and
///     $SERVE_DIR/second respectively (push direction is confirmed host-side).
void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  // Absolute destination on the responder host for folder B. Supplied by the
  // runner scripts (they know $SERVE_DIR); defaults to a Linux-friendly path
  // for a manual `-d linux` run against a locally seeded serve root.
  const hostRemotePath = String.fromEnvironment(
      'FERRISYNC_REMOTE_PATH',
      defaultValue: '/tmp/ferrisync_multifolder_host/second');
  late ApiState state;
  late Directory dataDir;
  late Directory folderA;
  late Directory folderB;
  late String peerDeviceId;
  const remoteIp = '127.0.0.1';
  const remotePort = 9847;

  setUpAll(() async {
    await RustLib.init();
  });

  setUp(() async {
    // The engine keeps its own identity/state in a dedicated dir, separate
    // from the two folder dirs, so those engine files never shadow the folder
    // contents that the multi-folder assertions inspect.
    dataDir = await Directory.systemTemp.createTemp('ferrisync_multifolder_data_');
    folderA = await Directory.systemTemp.createTemp('ferrisync_multifolder_a_');
    folderB = await Directory.systemTemp.createTemp('ferrisync_multifolder_b_');

    await File('${folderA.path}/from_app_A.txt').writeAsString('app_A_content');
    await File('${folderB.path}/from_app_B.txt').writeAsString('app_B_content');

    state = await initEngine(dataDir: dataDir.path);

    // Pair with the host; address it by the cert-derived id pairing returns.
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
      await dataDir.delete(recursive: true);
    } catch (_) {}
    try {
      await folderA.delete(recursive: true);
    } catch (_) {}
    try {
      await folderB.delete(recursive: true);
    } catch (_) {}
  });

  testWidgets('two folders sync to same host, folder B via remote subpath',
      (WidgetTester tester) async {
    // ── Folder A: no remote path -> host serves from its root dir. ──
    final folderAId = await addSyncFolderWithPeers(
      state: state,
      localPath: folderA.path,
      name: 'folder_a',
      peers: [
        FolderPeerRequest(
          deviceId: peerDeviceId,
          mode: 'bidirectional',
        ),
      ],
    );
    expect(folderAId, greaterThan(0));

    // ── Folder B: remotePath 'second' -> host serves $SERVE_DIR/second. ──
    final folderBId = await addSyncFolderWithPeers(
      state: state,
      localPath: folderB.path,
      name: 'folder_b',
      peers: [
        FolderPeerRequest(
          deviceId: peerDeviceId,
          mode: 'bidirectional',
          remotePath: hostRemotePath,
        ),
      ],
    );
    expect(folderBId, greaterThan(0));

    // Both folders share the paired device (multi-folder graph on one row).
    final listed = await listSyncFolders(state: state);
    expect(listed.length, 2, reason: 'two distinct folder rows should exist');
    final a = listed.firstWhere((f) => f.id == folderAId);
    final b = listed.firstWhere((f) => f.id == folderBId);
    expect(a.guid, isNotEmpty, reason: 'folder A has a stable guid');
    expect(b.guid, isNot(a.guid), reason: 'folders have distinct guids');

    // ── Sync folder A against the host root. ──
    final ra = await syncFolder(
      state: state,
      folderId: folderAId,
      localPath: folderA.path,
      remoteIp: remoteIp,
      remotePort: remotePort,
      deviceId: peerDeviceId,
      dryRun: false,
    );
    expect(ra.pushed, contains('from_app_A.txt'),
        reason: 'folder A local file pushed to host root');
    expect(ra.pulled, contains('from_host_root.txt'),
        reason: 'folder A pulled host root seed');

    final pulledA = File('${folderA.path}/from_host_root.txt');
    expect(pulledA.readAsStringSync().trim(), 'host_root_content');
    expect(File('${folderA.path}/from_app_A.txt').readAsStringSync().trim(),
        'app_A_content');

    // ── Sync folder B against the host sub-dir (remote_path relocation). ──
    final rb = await syncFolder(
      state: state,
      folderId: folderBId,
      localPath: folderB.path,
      remoteIp: remoteIp,
      remotePort: remotePort,
      deviceId: peerDeviceId,
      dryRun: false,
    );
    expect(rb.pushed, contains('from_app_B.txt'),
        reason: 'folder B local file pushed to host sub-dir');
    expect(rb.pulled, contains('from_host_second.txt'),
        reason: 'folder B pulled host sub-dir seed');

    final pulledB = File('${folderB.path}/from_host_second.txt');
    expect(pulledB.readAsStringSync().trim(), 'host_second_content');
    expect(File('${folderB.path}/from_app_B.txt').readAsStringSync().trim(),
        'app_B_content');

    // Root and sub-dir are independent: folder A must NOT see folder B's seed
    // (and vice versa), proving the per-pair remote_path relocated the serving
    // root instead of sharing one namespace.
    expect(File('${folderA.path}/from_host_second.txt').existsSync(), false,
        reason: 'folder A should not receive the sub-dir seed');
    expect(File('${folderB.path}/from_host_root.txt').existsSync(), false,
        reason: 'folder B should not receive the root seed');

    // Per-folder device relationships are surfaced.
    final devicesB = await listFolderDevices(state: state, folderId: folderBId);
    final peer = devicesB.firstWhere((d) => d.deviceId == peerDeviceId);
    expect(peer.remotePath, hostRemotePath,
        reason: 'folder B device relationship stores its remote path');
  });
}