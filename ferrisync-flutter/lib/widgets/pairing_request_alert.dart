import 'dart:collection';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import '../gen/api.dart' as frb;
import '../providers/sync_provider.dart';

/// Globally-mounted, invisible listener that turns incoming pairing requests
/// into a modal approval dialog on every platform (Linux desktops and Android
/// phones have no pull-to-refresh). Each distinct request is announced at most
/// once; a request that disappears (approved/denied) can re-announce if the
/// peer tries again later. Existing requests found at startup are silently
/// primed so a restart never replays stale popups.
class PairingRequestAlert extends ConsumerStatefulWidget {
  const PairingRequestAlert({super.key});

  @override
  ConsumerState<PairingRequestAlert> createState() =>
      _PairingRequestAlertState();
}

sealed class _Announcement {
  const _Announcement();
}

class _DeviceAnnouncement extends _Announcement {
  const _DeviceAnnouncement(this.name, this.id);

  final String name;
  final String id;
}

class _FolderAnnouncement extends _Announcement {
  const _FolderAnnouncement(this.request);

  final frb.PendingFolderPairing request;
}

class _PairingRequestAlertState extends ConsumerState<PairingRequestAlert> {
  final Set<String> _announcedDevices = <String>{};
  final Set<String> _announcedFolders = <String>{};
  final Queue<_Announcement> _pending = Queue<_Announcement>();
  bool _showing = false;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) return;
      _prime(ref.read(syncServiceProvider));
    });
  }

  /// Records already-visible requests without popups so a restart doesn't
  /// replay them as new.
  void _prime(SyncService service) {
    for (final (_, id) in service.pendingPairings) {
      _announcedDevices.add(id);
    }
    for (final p in service.pendingFolderPairings) {
      _announcedFolders.add(_folderKey(p.deviceId, p.folderGuid));
    }
  }

  String _folderKey(String deviceId, String folderGuid) =>
      '$deviceId#$folderGuid';

  @override
  Widget build(BuildContext context) {
    ref.listen<SyncService>(syncServiceProvider, (_, service) {
      _announce(service);
    });
    // Render nothing; this widget exists only to own the dialog lifecycle.
    return const SizedBox.shrink();
  }

  void _announce(SyncService service) {
    final deviceIds = service.pendingPairings.map((e) => e.$2).toSet();
    _announcedDevices.removeWhere((id) => !deviceIds.contains(id));
    for (final (name, id) in service.pendingPairings) {
      if (_announcedDevices.add(id)) {
        _pending.add(_DeviceAnnouncement(name, id));
      }
    }

    final folderKeys = service.pendingFolderPairings
        .map((p) => _folderKey(p.deviceId, p.folderGuid))
        .toSet();
    _announcedFolders.removeWhere((k) => !folderKeys.contains(k));
    for (final p in service.pendingFolderPairings) {
      if (_announcedFolders.add(_folderKey(p.deviceId, p.folderGuid))) {
        _pending.add(_FolderAnnouncement(p));
      }
    }

    _drain();
  }

  void _drain() {
    if (_showing || _pending.isEmpty || !mounted) return;
    _showing = true;
    final next = _pending.removeFirst();
    WidgetsBinding.instance.addPostFrameCallback((_) async {
      if (!mounted) {
        _showing = false;
        return;
      }
      await _showDialog(next);
      _showing = false;
      if (mounted) _drain();
    });
  }

  Future<void> _showDialog(_Announcement announcement) async {
    final service = ref.read(syncServiceProvider);
    if (announcement is _DeviceAnnouncement) {
      final allow = await _prompt(_dialog(
        icon: Icons.person_add_alt_1,
        title: 'Pairing request',
        content: '${announcement.name} wants to pair with this device.',
      ));
      if (allow == null || !mounted) return;
      final message = allow
          ? await service.approvePairing(
              announcement.id, announcement.name)
          : await service.denyPairing(announcement.id);
      _snack(message);
      return;
    }

    final p = (announcement as _FolderAnnouncement).request;
    final allow = await _prompt(_dialog(
      icon: Icons.folder_shared,
      title: 'Folder pairing request',
      content:
          '${p.deviceName} wants to pair to your shared folder '
          '"${p.folderName}".',
    ));
    if (allow == null || !mounted) return;
    final message = allow
        ? await service.approveFolderPairing(
            deviceId: p.deviceId,
            folderGuid: p.folderGuid,
            folderName: p.folderName,
            localPath:
                service.mySharedFolders
                        .where((s) => s.folderGuid == p.folderGuid)
                        .firstOrNull
                        ?.localPath ??
                    '',
          )
        : await service.denyFolderPairing(p.deviceId, p.folderGuid);
    _snack(message);
  }

  Widget _dialog({
    required IconData icon,
    required String title,
    required String content,
  }) {
    return AlertDialog(
      icon: Icon(icon),
      title: Text(title),
      content: Text(content),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(false),
          child: const Text('Deny'),
        ),
        FilledButton(
          onPressed: () => Navigator.of(context).pop(true),
          child: const Text('Allow'),
        ),
      ],
    );
  }

  Future<bool?> _prompt(Widget dialog) =>
      showDialog<bool>(context: context, builder: (_) => dialog);

  void _snack(String message) {
    if (!mounted) return;
    ScaffoldMessenger.of(context)
      ..hideCurrentSnackBar()
      ..showSnackBar(SnackBar(content: Text(message)));
  }
}