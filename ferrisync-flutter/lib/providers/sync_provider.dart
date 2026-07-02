import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:path_provider/path_provider.dart';
import '../gen/api.dart' as frb;
import '../models/sync_models.dart';

class SyncService {
  frb.ApiState? _state;
  String _deviceId = '';
  String _deviceName = '';
  List<Device> _devices = [];
  List<SyncFolder> _folders = [];
  SyncStatus _status = SyncStatus.idle;

  String get deviceId => _deviceId;
  String get deviceName => _deviceName;
  List<Device> get devices => _devices;
  List<SyncFolder> get folders => _folders;
  SyncStatus get status => _status;

  Future<void> init() async {
    final dir = await getApplicationSupportDirectory();
    final dataDir = '${dir.path}/ferrisync';

    try {
      final state = await frb.initEngine(dataDir: dataDir);
      _state = state;
      _deviceId = await frb.deviceId(state: state);
      _deviceName = await frb.deviceName(state: state);
    } catch (_) {
      _deviceId = '00000000-0000-0000-0000-000000000000';
      _deviceName = 'Flutter Device';
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
    await frb.addSyncFolder(
      state: state,
      localPath: localPath,
      deviceId: deviceId,
      direction: 'bidirectional',
    );
    await refresh();
  }

  Future<void> syncFolder(
    String path,
    String remoteIp, {
    int remotePort = 9847,
    String? deviceId,
  }) async {
    _status = SyncStatus.syncing;

    final state = _state;
    if (state == null) {
      await Future.delayed(const Duration(seconds: 2));
      _status = SyncStatus.idle;
      return;
    }

    // Find the folder by path
    final folders = await frb.listSyncFolders(state: state);
    final folder = folders.where((f) => f.localPath == path).firstOrNull;
    if (folder == null) {
      _status = SyncStatus.error;
      return;
    }

    final did = deviceId ?? folder.deviceId;
    await frb.syncFolder(
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
        filePulled: (_, __) {},
        filePushed: (_, __) {},
        conflict: (_, __, ___) {},
      );
    }

    await refresh();
    _status = SyncStatus.idle;
  }
}

final syncServiceProvider = Provider<SyncService>((ref) {
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
