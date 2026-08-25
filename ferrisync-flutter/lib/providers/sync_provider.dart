import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:path_provider/path_provider.dart';
import '../gen/api.dart' as frb;
import '../gen/frb_generated.dart';
import '../gen/sync_engine/session.dart' as frb_session;
import '../models/sync_models.dart';
import '../services/android_foreground.dart';

class SyncService extends ChangeNotifier {
  frb.ApiState? _state;
  String _deviceId = '';
  String _deviceName = '';
  List<Device> _devices = [];
  List<SyncFolder> _folders = [];
  SyncStatus _status = SyncStatus.idle;
  frb_session.SyncResult? _lastResult;
  // False until init() actually runs: a service nobody asked to initialize
  // must not render a perpetual "starting" spinner (and wedge pumpAndSettle).
  bool _initializing = false;
  String? _initError;

  /// How long to wait for the Rust engine before giving up and surfacing an
  /// in-app error instead of hanging on the splash screen forever.
  static const Duration _initTimeout = Duration(seconds: 20);

  String get deviceId => _deviceId;
  String get deviceName => _deviceName;
  List<Device> get devices => _devices;
  List<SyncFolder> get folders => _folders;
  SyncStatus get status => _status;
  bool get initializing => _initializing;

  /// Non-null when engine startup failed or timed out; the app stays usable
  /// (with degraded data) so the message is visible.
  String? get initError => _initError;

  Future<void> init() async {
    _initializing = true;
    _initError = null;
    notifyListeners();
    final dir = await getApplicationSupportDirectory();
    final dataDir = '${dir.path}/ferrisync';

    try {
      // Callers other than main() (e.g. integration tests) may not have
      // initialized the bridge yet; RustLib.init throws if called twice.
      // ignore: avoid_print
      if (!RustLib.instance.initialized) {
        await RustLib.init();
      }
      // ignore: avoid_print
      final state = await frb
          .initEngine(dataDir: dataDir)
          .timeout(_initTimeout);
      // ignore: avoid_print
      _state = state;
      _deviceId = await frb.deviceId(state: state);
      _deviceName = await frb.deviceName(state: state);
    } on TimeoutException {
      _deviceId = '00000000-0000-0000-0000-000000000000';
      _deviceName = 'Flutter Device';
      _initError = 'engine did not start within ${_initTimeout.inSeconds}s — '
          'try clearing the app\'s storage and reopening';
    } catch (e) {
      _deviceId = '00000000-0000-0000-0000-000000000000';
      _deviceName = 'Flutter Device';
      _initError = '$e';
    } finally {
      _initializing = false;
      notifyListeners();
    }
  }

  Future<void> refresh() async {
    final state = _state;
    if (state == null) return;

    final deviceList = await frb.listDevices(state: state);
    _devices = deviceList
        .map((d) => Device(id: d.id, name: d.name, lastSeen: d.lastSeen))
        .toList();

    final folderList = await frb.listSyncFolders(state: state);
    _folders = folderList
        .map((f) => SyncFolder(
              id: f.id,
              localPath: f.localPath,
              deviceId: f.deviceId,
              direction: f.direction,
              lastSyncAt: f.lastSyncAt,
            ))
        .toList();
    notifyListeners();
  }

  /// Rename this device. Persists in the Rust layer and restarts any running
  /// folder servers so peers see the new name immediately.
  Future<void> setDeviceName(String name) async {
    final state = _state;
    if (state == null) throw StateError('Sync engine not initialized');
    _deviceName = await frb.setDeviceName(state: state, name: name);
    notifyListeners();
  }

  Future<String> pairWithDevice(String ip, int port) async {
    final state = _state;
    if (state == null) {
      await Future.delayed(const Duration(seconds: 1));
      return 'Paired with device at $ip:$port';
    }
    return await frb.pairWithDevice(state: state, ip: ip, port: port);
  }

  Future<List<frb.DiscoveredDevice>> discoverDevices({int timeoutSecs = 3}) async {
    return await frb.discoverDevices(timeoutSecs: BigInt.from(timeoutSecs));
  }

  Future<void> addSyncFolder(String localPath, String deviceId) async {
    final state = _state;
    if (state == null) return;
    final folderId = await frb.addSyncFolder(
      state: state,
      localPath: localPath,
      deviceId: deviceId,
      direction: 'bidirectional',
    );
    // Serve the new folder so the peer can push to us later.
    await startFolderServer(folderId.toInt(), localPath);    await refresh();
  }

  /// Start a listener for a folder, walking up from port 9847 if taken.
  Future<void> startFolderServer(int folderId, String localPath) async {
    final state = _state;
    if (state == null) return;
    for (var port = 9847; port < 9867; port++) {
      try {
        await frb.startServer(
          state: state,
          port: port,
          folderId: folderId,
          localPath: localPath,
        );
        // Keep the process unfrozen while we are reachable for peers.
        await AndroidForeground.startServing();
        return;
      } on Object catch (_) {
        if (port == 9866) rethrow;
      }
    }
  }

  Future<void> syncFolder(
    String path,
    String remoteIp, {
    int remotePort = 9847,
    String? deviceId,
  }) async {
    _status = SyncStatus.syncing;
    _lastResult = null;
    notifyListeners();

    final state = _state;
    if (state == null) {
      await Future.delayed(const Duration(seconds: 2));
      _status = SyncStatus.idle;
      notifyListeners();
      return;
    }

    // Find the folder by path
    final folders = await frb.listSyncFolders(state: state);
    final folder = folders.where((f) => f.localPath == path).firstOrNull;
    if (folder == null) {
      _status = SyncStatus.error;
      notifyListeners();
      return;
    }

    final did = deviceId ?? folder.deviceId;
    _lastResult = await frb.syncFolder(
      state: state,
      folderId: folder.id,
      localPath: path,
      remoteIp: remoteIp,
      remotePort: remotePort,
      deviceId: did,
    );

    // Poll for remaining events
    final events = await frb.pollSyncEvents(state: state);
    for (final event in events) {
      event.when(
        syncing: (_) => _status = SyncStatus.syncing,
        idle: () => _status = SyncStatus.idle,
        error: (_) => _status = SyncStatus.error,
        pairRequested: (_, __) {},
        devicePaired: (_, __) {},
        filePulled: (_, __) {},
        filePushed: (_, __) {},
        conflict: (_, __, ___) {},
      );
    }

    await refresh();
    _status = SyncStatus.idle;
    notifyListeners();
  }

  /// Sync a folder right now against its paired device's last known address.
  /// Returns a user-facing result message.
  Future<String> syncFolderNow(SyncFolder folder) async {
    final state = _state;
    if (state == null) return 'Engine not ready';

    final addr =
        await frb.deviceLastAddr(state: state, deviceId: folder.deviceId);
    if (addr == null) {
      return 'No known address for device — pair again';
    }

    final parsed = _parseHostPort(addr);
    if (parsed == null) {
      return 'Invalid address stored for device: $addr';
    }
    final (:host, :port) = parsed;

    try {
      await syncFolder(folder.localPath, host, remotePort: port);
      final res = _lastResult;
      return _status == SyncStatus.error
          ? 'Sync failed'
          : res == null
              ? 'Sync complete with $host:$port'
              : 'Sync complete with $host:$port '
                  '(Pushed ${res.pushed.length}, Pulled ${res.pulled.length})';
    } catch (e) {
      _status = SyncStatus.error;
      notifyListeners();
      return 'Sync failed: $e';
    }
  }

  static ({String host, int port})? _parseHostPort(String addr) {
    final bracket = RegExp(r'^\[(.+)\]:(\d+)$').firstMatch(addr);
    if (bracket != null) {
      return (host: bracket.group(1)!, port: int.parse(bracket.group(2)!));
    }
    final idx = addr.lastIndexOf(':');
    if (idx <= 0 || idx >= addr.length - 1) return null;
    final port = int.tryParse(addr.substring(idx + 1));
    if (port == null) return null;
    return (host: addr.substring(0, idx), port: port);
  }
}

final syncServiceProvider = ChangeNotifierProvider<SyncService>((ref) {
  return SyncService();
});

final deviceIdProvider = Provider<String>((ref) {
  return ref.watch(syncServiceProvider).deviceId;
});

final deviceNameProvider = Provider<String>((ref) {
  return ref.watch(syncServiceProvider).deviceName;
});

final devicesProvider = Provider<List<Device>>((ref) {
  return ref.watch(syncServiceProvider).devices;
});

final foldersProvider = Provider<List<SyncFolder>>((ref) {
  return ref.watch(syncServiceProvider).folders;
});

final syncStatusProvider = Provider<SyncStatus>((ref) {
  return ref.watch(syncServiceProvider).status;
});
