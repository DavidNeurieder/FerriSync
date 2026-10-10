import 'package:ferrisync/gen/api.dart' as frb;
import 'package:ferrisync/models/sync_models.dart';
import 'package:ferrisync/providers/sync_provider.dart';
import 'package:flutter_test/flutter_test.dart';

/// `SyncService` with every engine-touching call stubbed, so the address
/// resolution fallback behind `remoteFoldersFor` can be observed in isolation.
class AddressSyncService extends SyncService {
  AddressSyncService({this.stored, this.discovered = const []});

  final String? stored;
  final List<frb.DiscoveredDevice> discovered;
  final browsed = <({String host, int port})>[];

  @override
  Future<String?> storedDeviceAddress(String deviceId) async => stored;

  @override
  Future<List<frb.DiscoveredDevice>> discoverDevices(
          {int timeoutSecs = 3}) async =>
      discovered;

  @override
  Future<List<frb.RemoteSharedFolder>> browsePeerSharedFolders(
      String peerIp, int peerPort) async {
    browsed.add((host: peerIp, port: peerPort));
    // Only the live listener (port 4242) answers; the dead :9847 guess refuses.
    if (peerPort == 4242) {
      return const [
        frb.RemoteSharedFolder(
          folderGuid: 'guid-1',
          name: 'shared-notes',
          mode: 'both',
          localPath: '/home/linux/notes',
        ),
      ];
    }
    return const [];
  }
}

void main() {
  final device = Device(id: 'linux-id', name: 'Linux', lastSeen: 0);

  test('remote folders fall back to the advertised address when the stored '
      'one is dead (responder-side :9847 guess)', () async {
    final service = AddressSyncService(
      stored: '192.168.1.5:9847',
      discovered: const [
        frb.DiscoveredDevice(
          id: 'linux-id',
          name: 'linux',
          ip: '192.168.1.5',
          port: 4242,
        ),
      ],
    );

    final folders = await service.remoteFoldersFor(device);

    expect(folders.map((f) => f.name), ['shared-notes']);
    expect(service.browsed, [
      (host: '192.168.1.5', port: 9847),
      (host: '192.168.1.5', port: 4242),
    ]);
  });

  test('remote folders use the stored address when it answers', () async {
    final service = AddressSyncService(
      stored: '192.168.1.5:4242',
      discovered: const [
        frb.DiscoveredDevice(
          id: 'linux-id',
          name: 'linux',
          ip: '192.168.1.5',
          port: 4242,
        ),
      ],
    );

    final folders = await service.remoteFoldersFor(device);

    expect(folders.map((f) => f.name), ['shared-notes']);
    // The stored address already worked; the matching advertisement is a
    // duplicate and is not dialled twice.
    expect(service.browsed, [(host: '192.168.1.5', port: 4242)]);
  });
}
