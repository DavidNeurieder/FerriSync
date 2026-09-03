import 'dart:io';

import 'package:flutter/material.dart';
import '../gen/api.dart' as frb;
import '../models/sync_models.dart';
import '../providers/sync_provider.dart';
import '../theme/ferri_theme.dart';

/// Bottom sheet that browses a paired device's discoverable shared folders
/// (over TLS, after trust) and lets the user request pairing to one. The peer
/// is reached at the IP/port the user enters; each published folder can be
/// requested with a local destination path.
class BrowseSharedFoldersSheet extends StatefulWidget {
  const BrowseSharedFoldersSheet({
    super.key,
    required this.device,
    required this.service,
  });

  final Device device;
  final SyncService service;

  @override
  State<BrowseSharedFoldersSheet> createState() => _BrowseSheetState();
}

class _BrowseSheetState extends State<BrowseSharedFoldersSheet> {
  final _ipCtrl = TextEditingController(text: '');
  final _portCtrl = TextEditingController(text: '9847');
  List<frb.RemoteSharedFolder> _folders = const [];
  bool _connected = false;
  bool _busy = false;
  String? _error;

  @override
  void dispose() {
    _ipCtrl.dispose();
    _portCtrl.dispose();
    super.dispose();
  }

  Future<void> _connect() async {
    final ip = _ipCtrl.text.trim();
    final port = int.tryParse(_portCtrl.text.trim()) ?? 9847;
    if (ip.isEmpty) {
      setState(() => _error = 'Enter the peer\'s IP address or host name.');
      return;
    }
    setState(() {
      _busy = true;
      _error = null;
    });
    final folders = await widget.service.browsePeerSharedFolders(ip, port);
    if (!mounted) return;
    setState(() {
      _folders = folders;
      _connected = true;
      _busy = false;
      if (folders.isEmpty) {
        _error = 'No discoverable shared folders at $ip:$port.';
      }
    });
  }

  Future<void> _request(BuildContext context, frb.RemoteSharedFolder f) async {
    final ctrl = TextEditingController(text: '');
    final path = await showDialog<String>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: Text('Pair to "${f.name}"?'),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            const Text('Where should this folder live on this device?'),
            const SizedBox(height: FerriTokens.spaceM),
            TextField(
              controller: ctrl,
              autofocus: true,
              decoration: const InputDecoration(labelText: 'Local folder path'),
            ),
          ],
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(ctx),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(ctx, ctrl.text.trim()),
            child: const Text('Request'),
          ),
        ],
      ),
    );
    if (path == null || path.isEmpty || !context.mounted) return;
    final result = await widget.service.requestFolderPairing(
      peerIp: _ipCtrl.text.trim(),
      peerPort: int.tryParse(_portCtrl.text.trim()) ?? 9847,
      peerDeviceId: widget.device.id,
      folderGuid: f.folderGuid,
      shareName: f.name,
      localPath: path,
      lifetimeMs: 60000,
    );
    if (!context.mounted) return;
    if (result.folderGuid == null) {
      ScaffoldMessenger.of(context)
        ..hideCurrentSnackBar()
        ..showSnackBar(SnackBar(content: Text(result.message)));
      return;
    }
    // Pair approved: close the sheet, then kick off the first sync so files
    // transfer right away (mirrors the manual "Sync now" path).
    Navigator.of(context).pop();
    final ip = _ipCtrl.text.trim();
    final port = int.tryParse(_portCtrl.text.trim()) ?? 9847;
    // Ensure the replica directory exists so the pull has somewhere to land.
    try {
      await Directory(path).create(recursive: true);
    } catch (_) {}
    await widget.service.syncFolder(path, ip, remotePort: port);
    await widget.service.refresh();
    if (!context.mounted) return;
    final message = switch (widget.service.status) {
      SyncStatus.error => widget.service.lastErrorMessage ?? 'Sync failed',
      _ => 'Paired to "${f.name}" and synced.',
    };
    ScaffoldMessenger.of(context)
      ..hideCurrentSnackBar()
      ..showSnackBar(SnackBar(content: Text(message)));
  }

  @override
  Widget build(BuildContext context) {
    final palette = context.ferri;
    final textTheme = Theme.of(context).textTheme;
    return SafeArea(
      child: Padding(
        padding: const EdgeInsets.fromLTRB(24, 4, 24, 16),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Row(
              children: [
                Icon(Icons.folder_shared_outlined, color: palette.primary),
                const SizedBox(width: FerriTokens.spaceM),
                Expanded(
                  child: Text(
                    'Browse shared folders · ${widget.device.name}',
                    style: textTheme.titleLarge!
                        .copyWith(fontWeight: FontWeight.w700),
                    overflow: TextOverflow.ellipsis,
                  ),
                ),
              ],
            ),
            const SizedBox(height: FerriTokens.spaceL),
            Row(
              children: [
                Expanded(
                  child: TextField(
                    controller: _ipCtrl,
                    enabled: !_connected,
                    keyboardType: TextInputType.url,
                    decoration: const InputDecoration(
                      labelText: 'Peer address',
                      hintText: '192.168.1.20',
                    ),
                  ),
                ),
                const SizedBox(width: FerriTokens.spaceM),
                SizedBox(
                  width: 84,
                  child: TextField(
                    controller: _portCtrl,
                    enabled: !_connected,
                    keyboardType: TextInputType.number,
                    decoration: const InputDecoration(labelText: 'Port'),
                  ),
                ),
                if (!_connected) ...[
                  const SizedBox(width: FerriTokens.spaceM),
                  FilledButton.icon(
                    key: const ValueKey('browse_connect'),
                    onPressed: _busy ? null : _connect,
                    icon: _busy
                        ? const SizedBox(
                            width: 16,
                            height: 16,
                            child: CircularProgressIndicator(strokeWidth: 2),
                          )
                        : const Icon(Icons.search),
                    label: const Text('Browse'),
                  ),
                ],
              ],
            ),
            if (_error != null) ...[
              const SizedBox(height: FerriTokens.spaceM),
              Text(_error!,
                  style: textTheme.bodySmall!.copyWith(color: palette.danger)),
            ],
            if (_connected) ...[
              const SizedBox(height: FerriTokens.spaceL),
              Text(
                'FOLDERS SHARED',
                style: textTheme.labelSmall!.copyWith(
                  letterSpacing: 1.1,
                  fontWeight: FontWeight.w700,
                  color: palette.muted,
                ),
              ),
              const SizedBox(height: FerriTokens.spaceS),
              if (_folders.isEmpty)
                Padding(
                  padding:
                      const EdgeInsets.symmetric(vertical: FerriTokens.spaceL),
                  child: Column(
                    children: [
                      Icon(Icons.folder_open, color: palette.muted, size: 32),
                      const SizedBox(height: FerriTokens.spaceM),
                      const Text('Nothing discoverable from that peer.'),
                    ],
                  ),
                )
              else
                Flexible(
                  child: ListView.builder(
                    shrinkWrap: true,
                    itemCount: _folders.length,
                    itemBuilder: (ctx, i) {
                      final f = _folders[i];
                      return ListTile(
                        key: ValueKey('browse_${f.folderGuid}'),
                        leading: const Icon(Icons.folder_outlined),
                        title: Text(f.name),
                        subtitle: Text(
                          '${f.folderGuid} · ${f.mode}',
                          style: textTheme.bodySmall!
                              .copyWith(color: palette.muted),
                        ),
                        trailing: const Icon(Icons.add_link),
                        onTap: () => _request(ctx, f),
                      );
                    },
                  ),
                ),
            ],
          ],
        ),
      ),
    );
  }
}
