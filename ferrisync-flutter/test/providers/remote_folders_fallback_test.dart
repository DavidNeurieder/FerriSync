import 'package:ferrisync/gen/api.dart' as frb;
import 'package:ferrisync/models/sync_models.dart';
import 'package:ferrisync/providers/sync_provider.dart';
import 'package:flutter_test/flutter_test.dart';

PeerRequestFailure _transport(String message) =>
    PeerRequestFailure(kind: frb.PeerRequestErrorKind.transport, message: message,
        stage: null, discovered: false);

PeerRequestFailure _timeout(String message) =>
    PeerRequestFailure(kind: frb.PeerRequestErrorKind.timeout, message: message,
        stage: null, discovered: false);

PeerRequestFailure _internal(String message) =>
    PeerRequestFailure(kind: frb.PeerRequestErrorKind.internal, message: message,
        stage: null, discovered: false);

const _share = frb.RemoteSharedFolder(
  folderGuid: 'guid-1',
  name: 'shared-notes',
  mode: 'both',
  localPath: '/home/linux/notes',
);

/// A `SyncService` with engine-touching calls stubbed so the shared
/// endpoint-resolution policy can be observed in isolation. Only ports mapped
/// in [replies]/[pairReplies] answer; everything else is an unreachable
/// transport failure.
class ResolverSyncService extends SyncService {
  ResolverSyncService({
    required this.replies,
    this.pairReplies = const {},
    this.stored,
    this.discovered = const [],
    this.persist = true,
  });

  /// Port → browse outcome.
  final Map<int, PeerOpResult<List<frb.RemoteSharedFolder>>> replies;

  /// Port → folder-pair outcome.
  final Map<int, PeerOpResult<frb.FolderPairRequestOutcome>> pairReplies;

  final String? stored;
  final List<frb.DiscoveredDevice> discovered;

  /// Whether a `rememberPeerEndpoint` write "lands". false simulates a
  /// concurrent writer having moved the address on (CAS denial).
  final bool persist;

  final List<({String host, int port})> browsed = [];
  final List<({String host, int port})> paired = [];
  final List<
      ({
        String deviceId,
        String host,
        int port,
        String? expectedPrevious,
      })> remembered = [];
  int discoverCalls = 0;

  PeerOpResult<List<frb.RemoteSharedFolder>> outcomeFor(int port) =>
      replies[port] ?? PeerOpResult.failed(_transport('could not reach :$port'));

  @override
  Future<String?> storedDeviceAddress(String deviceId) async => stored;

  @override
  Future<List<frb.DiscoveredDevice>> discoverDevices({int timeoutSecs = 3}) async {
    discoverCalls += 1;
    return discovered;
  }

  @override
  Future<PeerOpResult<List<frb.RemoteSharedFolder>>> browsePeerFoldersOutcome(
      String host, int port) async {
    browsed.add((host: host, port: port));
    return outcomeFor(port);
  }

  @override
  Future<PeerOpResult<frb.FolderPairRequestOutcome>> invokeFolderPairing({
    required String peerIp,
    required int peerPort,
    required String peerDeviceId,
    required String folderGuid,
    required String shareName,
    required String localPath,
    required int lifetimeMs,
  }) async {
    paired.add((host: peerIp, port: peerPort));
    return pairReplies[peerPort] ??
        PeerOpResult.failed(_transport('could not reach $peerIp:$peerPort'));
  }

  @override
  Future<bool> rememberPeerEndpoint({
    required String deviceId,
    required String host,
    required int port,
    String? expectedPrevious,
  }) async {
    remembered.add((
      deviceId: deviceId,
      host: host,
      port: port,
      expectedPrevious: expectedPrevious,
    ));
    return persist;
  }
}

void main() {
  final device = Device(id: 'linux-id', name: 'Linux', lastSeen: 0);
  const advertised = frb.DiscoveredDevice(
    id: 'linux-id',
    name: 'linux',
    ip: '192.168.1.5',
    port: 4242,
  );
  const staleGuess = '192.168.1.5:9847';

  group('remote folder browsing', () {
    test('stored endpoint works: one request, no discovery', () async {
      final service = ResolverSyncService(
        replies: {4242: const PeerOpResult.reached([_share])},
        stored: '192.168.1.5:4242',
        discovered: [advertised],
      );

      final folders = await service.remoteFoldersFor(device);

      expect(folders.map((f) => f.name), ['shared-notes']);
      expect(service.browsed, [(host: '192.168.1.5', port: 4242)]);
      expect(service.discoverCalls, 0, reason: 'no discovery on the happy path');
      expect(service.remembered, isEmpty);
    });

    test('stale stored port falls back to the advertised port and persists',
        () async {
      final service = ResolverSyncService(
        replies: {
          9847: PeerOpResult.failed(_transport('dead guess')),
          4242: const PeerOpResult.reached([_share]),
        },
        stored: staleGuess,
        discovered: [advertised],
      );

      final folders = await service.remoteFoldersFor(device);

      expect(folders.map((f) => f.name), ['shared-notes']);
      expect(service.browsed, [
        (host: '192.168.1.5', port: 9847),
        (host: '192.168.1.5', port: 4242),
      ]);
      expect(service.discoverCalls, 1, reason: 'one discovery, one retry');
      expect(service.remembered, [
        (
          deviceId: 'linux-id',
          host: '192.168.1.5',
          port: 4242,
          expectedPrevious: staleGuess,
        )
      ]);
    });

    test('healthy peer with zero folders returns an empty list without retry',
        () async {
      final service = ResolverSyncService(
        replies: {4242: const PeerOpResult.reached(<frb.RemoteSharedFolder>[])},
        stored: '192.168.1.5:4242',
        discovered: [advertised],
      );

      final folders = await service.remoteFoldersFor(device);

      expect(folders, isEmpty);
      expect(service.browsed, [(host: '192.168.1.5', port: 4242)]);
      expect(service.discoverCalls, 0,
          reason: 'an empty but successful reply is not a failure');
    });

    test('stored endpoint times out: discovered endpoint is tried', () async {
      final service = ResolverSyncService(
        replies: {
          9847: PeerOpResult.failed(_timeout('connect timed out')),
          4242: const PeerOpResult.reached([_share]),
        },
        stored: staleGuess,
        discovered: [advertised],
      );

      final folders = await service.remoteFoldersFor(device);

      expect(folders.map((f) => f.name), ['shared-notes']);
      expect(service.browsed, [
        (host: '192.168.1.5', port: 9847),
        (host: '192.168.1.5', port: 4242),
      ]);
    });

    test('discovery returns no candidate: single attempt, empty result',
        () async {
      final service = ResolverSyncService(
        replies: {9847: PeerOpResult.failed(_transport('dead guess'))},
        stored: staleGuess,
        discovered: [],
      );

      final folders = await service.remoteFoldersFor(device);

      expect(folders, isEmpty);
      expect(service.browsed, [(host: '192.168.1.5', port: 9847)]);
      expect(service.discoverCalls, 1);
    });

    test('discovery confirming the same endpoint is not retried', () async {
      final service = ResolverSyncService(
        replies: {4242: PeerOpResult.failed(_transport('still dead'))},
        stored: '192.168.1.5:4242',
        discovered: [advertised],
      );

      final folders = await service.remoteFoldersFor(device);

      expect(folders, isEmpty);
      expect(service.browsed, [(host: '192.168.1.5', port: 4242)]);
    });

    test('malformed stored address recovers via discovery', () async {
      final service = ResolverSyncService(
        replies: {4242: const PeerOpResult.reached([_share])},
        stored: 'definitely-not-an-address',
        discovered: [advertised],
      );

      final folders = await service.remoteFoldersFor(device);

      expect(folders.map((f) => f.name), ['shared-notes']);
      expect(service.browsed, [(host: '192.168.1.5', port: 4242)]);
      expect(service.remembered, [
        (
          deviceId: 'linux-id',
          host: '192.168.1.5',
          port: 4242,
          expectedPrevious: 'definitely-not-an-address',
        )
      ]);
    });

    test('discovered endpoint also fails: refines the failure and retries once',
        () async {
      final service = ResolverSyncService(
        replies: {
          9847: PeerOpResult.failed(_transport('stored endpoint dead')),
          4242: PeerOpResult.failed(_timeout('advertised endpoint slow')),
        },
        stored: staleGuess,
        discovered: [advertised],
      );

      final folders = await service.remoteFoldersFor(device);

      expect(folders, isEmpty);
      expect(service.browsed, [
        (host: '192.168.1.5', port: 9847),
        (host: '192.168.1.5', port: 4242),
      ]);
      // The fallback was attempted at most once.
      expect(service.discoverCalls, 1);
    });

    test('concurrent newer endpoint is not overwritten (CAS respected)',
        () async {
      final service = ResolverSyncService(
        replies: {
          9847: PeerOpResult.failed(_transport('dead guess')),
          4242: const PeerOpResult.reached([_share]),
        },
        stored: staleGuess,
        discovered: [advertised],
        // The compare-and-swap loses: another writer already moved the row on.
        persist: false,
      );

      final folders = await service.remoteFoldersFor(device);

      expect(folders.map((f) => f.name), ['shared-notes']);
      // The client handed the storage layer the value it based its decision on;
      // Rust refuses to overwrite when that value is no longer current.
      expect(service.remembered, [
        (
          deviceId: 'linux-id',
          host: '192.168.1.5',
          port: 4242,
          expectedPrevious: staleGuess,
        )
      ]);
    });
  });

  group('folder pairing', () {
    const approved = frb.FolderPairRequestOutcome.approved(
      folderGuid: 'guid-1',
      name: 'shared-notes',
    );

    test('stale stored endpoint falls back to the advertised port', () async {
      final service = ResolverSyncService(
        replies: const {},
        pairReplies: {
          9847: PeerOpResult.failed(_transport('dead guess')),
          4242: const PeerOpResult.reached(approved),
        },
        stored: staleGuess,
        discovered: [advertised],
      );

      final result = await service.pairToShare(device: device, folder: _share);

      expect(result.folderGuid, 'guid-1');
      expect(result.message, contains('Approved'));
      expect(service.paired, [
        (host: '192.168.1.5', port: 9847),
        (host: '192.168.1.5', port: 4242),
      ]);
      expect(service.remembered, [
        (
          deviceId: 'linux-id',
          host: '192.168.1.5',
          port: 4242,
          expectedPrevious: staleGuess,
        )
      ]);
    });

    test('peer rejection is not retried', () async {
      final service = ResolverSyncService(
        replies: const {},
        pairReplies: {
          9847: const PeerOpResult.reached(
              frb.FolderPairRequestOutcome.rejected('no thanks')),
        },
        stored: staleGuess,
        discovered: [advertised],
      );

      final result = await service.pairToShare(device: device, folder: _share);

      expect(result.folderGuid, isNull);
      expect(result.message, contains('rejected'));
      expect(service.paired, [(host: '192.168.1.5', port: 9847)]);
      expect(service.discoverCalls, 0,
          reason: 'a peer decision must not trigger discovery');
    });

    test('unrecognized internal error is not retried', () async {
      final service = ResolverSyncService(
        replies: const {},
        pairReplies: {
          9847: PeerOpResult.failed(_internal('unexpected protocol reply')),
        },
        stored: staleGuess,
        discovered: [advertised],
      );

      final result = await service.pairToShare(device: device, folder: _share);

      expect(result.folderGuid, isNull);
      expect(result.message, contains('Pairing failed'));
      expect(service.paired, [(host: '192.168.1.5', port: 9847)]);
      expect(service.discoverCalls, 0);
    });

    test('stored endpoint timing out retries on the advertised port', () async {
      final service = ResolverSyncService(
        replies: const {},
        pairReplies: {
          9847: PeerOpResult.failed(_timeout('stored endpoint slow')),
          4242: const PeerOpResult.reached(approved),
        },
        stored: staleGuess,
        discovered: [advertised],
      );

      final result = await service.pairToShare(device: device, folder: _share);

      expect(result.folderGuid, 'guid-1');
      expect(service.paired, [
        (host: '192.168.1.5', port: 9847),
        (host: '192.168.1.5', port: 4242),
      ]);
    });

    test('no stored address and no discovery: actionable failure', () async {
      final service = ResolverSyncService(replies: const {}, discovered: []);

      final result = await service.pairToShare(device: device, folder: _share);

      expect(result.folderGuid, isNull);
      expect(result.message, contains('Could not reach'));
    });

    test('stored endpoint works: single request, no discovery', () async {
      final service = ResolverSyncService(
        replies: const {},
        pairReplies: {4242: const PeerOpResult.reached(approved)},
        stored: '192.168.1.5:4242',
        discovered: [advertised],
      );

      final result = await service.pairToShare(device: device, folder: _share);

      expect(result.folderGuid, 'guid-1');
      expect(service.paired, [(host: '192.168.1.5', port: 4242)]);
      expect(service.discoverCalls, 0);
      expect(service.remembered, isEmpty);
    });

    test('pending approval is a reachable outcome, not a failure', () async {
      final service = ResolverSyncService(
        replies: const {},
        pairReplies: {
          4242: const PeerOpResult.reached(frb.FolderPairRequestOutcome.pending()),
        },
        stored: '192.168.1.5:4242',
        discovered: [advertised],
      );

      final result = await service.pairToShare(device: device, folder: _share);

      expect(result.folderGuid, isNull);
      expect(result.message, contains('approve'));
      expect(service.paired, [(host: '192.168.1.5', port: 4242)]);
      expect(service.discoverCalls, 0);
    });
  });
}