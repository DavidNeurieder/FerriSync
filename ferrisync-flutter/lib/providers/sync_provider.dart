import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:path_provider/path_provider.dart';
import '../models/sync_models.dart';

// After running flutter_rust_bridge_codegen, replace the mock API calls
// with generated bindings from `ferrisync_core.api`.

class SyncService {
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

    // TODO: replace with FRB-generated initEngine call:
    // final state = await initEngine(dataDir: dataDir);
    // _deviceId = deviceId(state: state);
    // _deviceName = deviceName(state: state);

    _deviceId = '00000000-0000-0000-0000-000000000000';
    _deviceName = 'Flutter Device';
  }

  Future<void> refresh() async {
    // TODO: replace with FRB calls:
    // final deviceList = await listDevices(state: _state);
    // _devices = deviceList.map((d) => Device(id: d.id, name: d.name, lastSeen: d.lastSeen)).toList();
  }

  Future<String> pairWithDevice(String ip, int port) async {
    // TODO: replace with FRB call:
    // return await pairWithDevice(state: _state, ip: ip, port: port);
    await Future.delayed(const Duration(seconds: 1));
    return 'Paired with device at $ip:$port';
  }

  Future<void> syncFolder(String path, String deviceId) async {
    _status = SyncStatus.syncing;
    // TODO: replace with FRB call:
    // await syncFolder(state: _state, folderId: id, ...);
    await Future.delayed(const Duration(seconds: 2));
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
