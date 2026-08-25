import 'dart:async';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:path_provider/path_provider.dart';
import '../gen/api.dart' as frb;
import '../gen/frb_generated.dart';
import '../gen/sync_engine/session.dart' as frb_session;
import '../models/sync_models.dart';
import '../services/android_foreground.dart';
import '../services/notification_service.dart';

class SyncService extends ChangeNotifier {
  SyncService({NotificationsApi? notifications})
      : _notifications = notifications ?? NotificationsService();

  final NotificationsApi _notifications;
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
  bool _notificationsEnabled = false;
  List<(String, String)> _pendingPairings = [];

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

  /// Whether sync-completion notifications should be posted. Reflects the
  /// persisted preference AND the OS notification permission.
  bool get notificationsEnabled => _notificationsEnabled;

  /// Pairing requests waiting for user approval: `(device_name, device_id)`.
  List<(String, String)> get pendingPairings => _pendingPairings;

  Future<void> init() async {
    _initializing = true;
    _initError = null;
    notifyListeners();
    final dir = await getApplicationSupportDirectory();
    final dataDir = '${dir.path}/ferrisync';

    // Notification preference is independent of the engine; load it even if
    // engine startup later fails.
    _notificationsEnabled =
        await _notifications.getPref() && await _notifications.areEnabled();

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

  /// Toggle sync-completion notifications. Enabling runs the Android runtime
  /// permission request first; the preference is only persisted when granted.
  /// Returns whether notifications are enabled afterwards.
  Future<bool> setNotificationsEnabled(bool enabled) async {
    if (!enabled) {
      _notificationsEnabled = false;
      await _notifications.setPref(false);
      notifyListeners();
      return false;
    }
    final granted = await _notifications.requestPermission();
    _notificationsEnabled = granted;
    await _notifications.setPref(granted);
    notifyListeners();
    return granted;
  }

  /// Poll the Rust layer for pairing requests waiting for approval.
  Future<void> pollPendingPairings() async {
    final state = _state;
    if (state == null) return;
    try {
      final pairs = await frb.pendingPairings(state: state);
      _pendingPairings = pairs;
      notifyListeners();
    } catch (_) {}
  }

  /// Approve a pending pairing request. The remote device is written to the
  /// paired-devices table so its next sync attempt is accepted silently.
  Future<String> approvePairing(
      String deviceId, String deviceName) async {
    final state = _state;
    if (state == null) return 'Engine not ready';
    try {
      await frb.approvePendingPairing(
          state: state, deviceId: deviceId, deviceName: deviceName);
      await pollPendingPairings();
      await refresh();
      return 'Paired with $deviceName';
    } catch (e) {
      return 'Approve failed: $e';
    }
  }

  /// Deny a pending pairing request. The device is remembered for this
  /// session so repeated requests are silently rejected.
  Future<String> denyPairing(String deviceId) async {
    final state = _state;
    if (state == null) return 'Engine not ready';
    try {
      await frb.denyPendingPairing(state: state, deviceId: deviceId);
      await pollPendingPairings();
      return 'Pairing denied';
    } catch (e) {
      return 'Deny failed: $e';
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

    // Surface any pairing requests waiting for approval.
    try {
      _pendingPairings = await frb.pendingPairings(state: state);
    } catch (_) {}

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

  /// Remove a paired device and all its associated data (folders,
  /// metadata, history, sessions). Returns a user-facing result message.
  Future<String> removeDevice(String deviceId) async {
    final state = _state;
    if (state == null) return 'Engine not ready';
    try {
      final c = await frb.removeDevice(state: state, deviceId: deviceId);
      await refresh();
      if (c.deviceRemoved == BigInt.zero) return 'Device not found';
      final parts = <String>[];
      if (c.foldersRemoved > BigInt.zero) {
        parts.add('${c.foldersRemoved} folder(s)');
      }
      if (c.sessionsRemoved > BigInt.zero) {
        parts.add('${c.sessionsRemoved} session(s)');
      }
      if (c.historyRemoved > BigInt.zero) {
        parts.add('${c.historyRemoved} history entry/entries');
      }
      return parts.isEmpty
          ? 'Device removed'
          : 'Device removed — ${parts.join(', ')} deleted';
    } catch (e) {
      return 'Failed to remove device: $e';
    }
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
        pairRequested: (_, __) {
          // A remote device is trying to pair — refresh the pending list
          // so the UI can surface an approval dialog.
          unawaited(pollPendingPairings());
        },
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
      final message = _status == SyncStatus.error
          ? null
          : res == null
              ? 'Sync complete with $host:$port'
              : 'Sync complete with $host:$port '
                  '(Pushed ${res.pushed.length}, Pulled ${res.pulled.length})';

      // Post-sync notification (best-effort; gated on the user's toggle).
      if (_notificationsEnabled && message != null) {
        final folderName =
            folder.localPath.split(Platform.pathSeparator).where((s) => s.isNotEmpty).last;
        await _notifications.show(
          title: 'FerriSync — $folderName',
          body: message,
        );
      }
      return message ?? 'Sync failed';
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
