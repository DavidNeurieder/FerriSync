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

    test('skips folder pairing when the peer has no shares', () {
      final plan = planFolderPair([local('Documents'), local('Photos')], []);

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
  });
}
