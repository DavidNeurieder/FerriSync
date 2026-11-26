import 'package:ferrisync/gen/api.dart' as frb;
import 'package:ferrisync/providers/sync_provider.dart';
import 'package:flutter_test/flutter_test.dart';

frb.SharedFolder local(String name) => frb.SharedFolder(
      id: name.hashCode,
      folderGuid: 'local-$name',
      name: name,
      localPath: '/local/$name',
      discoverable: true,
      enabled: true,
      permissions: 'readwrite',
    );

frb.RemoteSharedFolder remote(String name) => frb.RemoteSharedFolder(
      folderGuid: 'remote-$name',
      name: name,
      mode: 'bidirectional',
      localPath: '/peer/$name',
    );

void main() {
  group('planFolderPair', () {
    test('uses the sole share on each side without asking', () {
      final plan = planFolderPair([local('Documents')], [remote('Photos')]);

      expect(plan.kind, FolderPairPlanKind.auto);
      expect(plan.local?.name, 'Documents');
      expect(plan.remote?.name, 'Photos');
    });

    test('adopts a sole remote share into a new folder when local has none', () {
      final plan = planFolderPair([], [remote('Photos')]);

      expect(plan.kind, FolderPairPlanKind.auto);
      expect(plan.local, isNull);
      expect(plan.remote?.name, 'Photos');
    });

    test('asks when the local side has several shares', () {
      final plan = planFolderPair(
        [local('Documents'), local('Photos')],
        [remote('Peer')],
      );

      expect(plan.kind, FolderPairPlanKind.needsChoice);
      expect(plan.localChoices, hasLength(2));
      expect(plan.remoteChoices, hasLength(1));
    });

    test('asks when the remote side has several shares', () {
      final plan = planFolderPair(
        [local('Documents')],
        [remote('Peer A'), remote('Peer B')],
      );

      expect(plan.kind, FolderPairPlanKind.needsChoice);
      expect(plan.localChoices, hasLength(1));
      expect(plan.remoteChoices, hasLength(2));
    });

    test('offers our sole share when the peer has none', () {
      final plan = planFolderPair([local('Documents')], []);

      expect(plan.kind, FolderPairPlanKind.autoLocal);
      expect(plan.local?.name, 'Documents');
      expect(plan.remote, isNull);
      expect(plan.shouldPair, isTrue);
    });

    test('asks which of our shares to offer when the peer has none', () {
      final plan = planFolderPair(
        [local('Documents'), local('Photos')],
        [],
      );

      expect(plan.kind, FolderPairPlanKind.needsChoice);
      expect(plan.localChoices, hasLength(2));
      expect(plan.remoteChoices, isEmpty);
    });

    test('skips folder pairing when neither side has shares', () {
      final plan = planFolderPair([], []);

      expect(plan.kind, FolderPairPlanKind.skip);
      expect(plan.shouldPair, isFalse);
    });

    test('adopted replica path uses the share name, not its remote path', () {
      expect(
        SyncService.deriveAdoptedReplicaPath(
          'auto-share',
          '/tmp/test_linux_flutter_serve',
        ),
        endsWith('/auto-share'),
      );
    });

    test('an explicit choice wins over ambiguity', () {
      final chosen = remote('Peer B');
      final plan = planFolderPair(
        [local('Documents'), local('Photos')],
        [remote('Peer A'), chosen],
        explicitChoice: (local: null, remote: chosen),
      );

      expect(plan.kind, FolderPairPlanKind.explicitChoice);
      expect(plan.local, isNull);
      expect(plan.remote, chosen);
    });

    test('an explicit local-only choice offers that share', () {
      final chosen = local('Photos');
      final plan = planFolderPair(
        [local('Documents'), chosen],
        [],
        explicitChoice: (local: chosen, remote: null),
      );

      expect(plan.kind, FolderPairPlanKind.explicitChoice);
      expect(plan.local, chosen);
      expect(plan.remote, isNull);
    });

    test('folder signature changes when a peer is attached', () {
      frb.FolderEntry folderWithPeers(List<frb.FolderPeer> peers) =>
          frb.FolderEntry(
            id: 1,
            localPath: '/tmp/shared',
            name: 'shared',
            deviceId: 'self',
            direction: 'bidirectional',
            peers: peers,
            lastSyncAt: 0,
          );

      final before = SyncService.folderPairSignature([folderWithPeers([])]);
      final after = SyncService.folderPairSignature([
        folderWithPeers([
          const frb.FolderPeer(
            deviceId: 'peer',
            mode: 'bidirectional',
            enabled: true,
          ),
        ]),
      ]);

      expect(after, isNot(before));
      expect(
        SyncService.folderPairSignature([folderWithPeers([])]), before,
        reason: 'the same rows must not trigger a refresh again',
      );
    });
  });
}
