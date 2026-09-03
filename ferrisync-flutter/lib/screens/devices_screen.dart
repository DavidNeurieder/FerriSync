import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import '../gen/api.dart' as frb;
import '../models/sync_models.dart';
import '../providers/sync_provider.dart';
import '../theme/ferri_theme.dart';
import '../utils/format_bytes.dart';
import '../widgets/empty_state.dart';
import 'add_device_screen.dart';
import 'browse_shared_folders.dart';

String _modeLabel(String mode) => switch (mode) {
      'push' || 'send_only' => 'Send only',
      'pull' || 'receive_only' => 'Receive only',
      _ => 'Two-way',
    };

class DevicesScreen extends ConsumerWidget {
  const DevicesScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final service = ref.watch(syncServiceProvider);
    final devices = ref.watch(devicesProvider);
    final folders = ref.watch(foldersProvider);
    final pending = service.pendingPairings;

    final folderCountByDevice = <String, int>{};
    for (final f in folders) {
      folderCountByDevice[f.deviceId] =
          (folderCountByDevice[f.deviceId] ?? 0) + 1;
    }

    return Scaffold(
      body: RefreshIndicator(
        onRefresh: service.refresh,
        child: ListView(
          physics: const AlwaysScrollableScrollPhysics(),
          padding: const EdgeInsets.all(FerriTokens.spaceL),
          children: [
            if (pending.isNotEmpty) ...[
              SectionHeader(title: 'PAIRING REQUESTS (${pending.length})'),
              const SizedBox(height: FerriTokens.spaceS),
              for (final (name, id) in pending) ...[
                _PairingRequestCard(
                  name: name,
                  id: id,
                  onDeny: () async {
                    final msg = await service.denyPairing(id);
                    if (context.mounted) _showSnack(context, msg);
                  },
                  onAllow: () async {
                    final msg = await service.approvePairing(id, name);
                    if (context.mounted) _showSnack(context, msg);
                  },
                ),
                const SizedBox(height: FerriTokens.spaceS),
              ],
              const SizedBox(height: FerriTokens.spaceL),
            ],
            SectionHeader(title: 'PAIRED DEVICES (${devices.length})'),
            const SizedBox(height: FerriTokens.spaceS),
            if (devices.isEmpty)
              EmptyState(
                icon: Icons.hub_outlined,
                title: 'No devices paired',
                subtitle:
                    'Pair a device on your local network to share folders.',
                action: FilledButton.icon(
                  key: const ValueKey('pair_a_device'),
                  onPressed: () => openAddDevice(context, ref),
                  icon: const Icon(Icons.add_link),
                  label: const Text('Pair a device'),
                ),
              )
            else
              Card(
                child: Column(
                  children: [
                    for (final d in devices) ...[
                      _DeviceCard(
                        device: d,
                        folderCount: folderCountByDevice[d.id] ?? 0,
                        onTap: () =>
                            _showDeviceDetail(context, service, d, folders),
                        onRemove: () =>
                            _confirmRemove(context, service, d),
                        onRename: () => _promptRename(context, service, d),
                      ),
                      if (d != devices.last)
                        const Divider(height: 1, indent: 16, endIndent: 16),
                    ],
                  ],
                ),
              ),
          ],
        ),
      ),
      floatingActionButton: FloatingActionButton.extended(
        key: const ValueKey('add_device_fab'),
        onPressed: () => openAddDevice(context, ref),
        icon: const Icon(Icons.add),
        label: const Text('Add device'),
      ),
    );
  }

  void _showSnack(BuildContext context, String message) {
    ScaffoldMessenger.of(context)
      ..hideCurrentSnackBar()
      ..showSnackBar(SnackBar(content: Text(message)));
  }

  Future<void> _confirmRemove(
      BuildContext context, SyncService service, Device d) async {
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: const Text('Remove device'),
        content: Text(
          'Remove ${d.name}? '
          'This deletes all associated folders and history.',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(ctx, false),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(ctx, true),
            child: const Text('Remove'),
          ),
        ],
      ),
    );
    if (confirmed != true || !context.mounted) return;
    final msg = await service.removeDevice(d.id);
    if (context.mounted) _showSnack(context, msg);
  }

  Future<void> _promptRename(
      BuildContext context, SyncService service, Device d) async {
    final ctrl = TextEditingController(text: d.name);
    final name = await showDialog<String>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: Text('Rename ${d.name}'),
        content: TextField(
          controller: ctrl,
          autofocus: true,
          decoration: const InputDecoration(labelText: 'Device name'),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(ctx),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(ctx, ctrl.text.trim()),
            child: const Text('Rename'),
          ),
        ],
      ),
    );
    if (name == null || name.isEmpty || !context.mounted) return;
    final msg = await service.renameRemoteDevice(d.id, name);
    if (context.mounted) _showSnack(context, msg);
  }

  void _showDeviceDetail(BuildContext context, SyncService service, Device d,
      List<SyncFolder> folders) {
    showModalBottomSheet<void>(
      context: context,
      showDragHandle: true,
      isScrollControlled: true,
      builder: (ctx) => _DeviceDetailSheet(
        device: d,
        folders: folders.where((f) => f.deviceId == d.id).toList(),
        service: service,
        onRename: () {
          Navigator.of(ctx).pop();
          _promptRename(context, service, d);
        },
        onRemove: () {
          Navigator.of(ctx).pop();
          _confirmRemove(context, service, d);
        },
        onBrowseShared: () {
          showModalBottomSheet<void>(
            context: context,
            showDragHandle: true,
            isScrollControlled: true,
            builder: (_) => BrowseSharedFoldersSheet(device: d, service: service),
          );
        },
      ),
    );
  }
}

class _PairingRequestCard extends StatelessWidget {
  const _PairingRequestCard({
    required this.name,
    required this.id,
    required this.onDeny,
    required this.onAllow,
  });

  final String name;
  final String id;
  final VoidCallback onDeny;
  final VoidCallback onAllow;

  @override
  Widget build(BuildContext context) {
    final palette = context.ferri;
    return Card(
      color: palette.surfaceHigh,
      child: Padding(
        padding: const EdgeInsets.all(FerriTokens.spaceL),
        child: Row(
          children: [
            Icon(Icons.person_add_alt_1, size: 28, color: palette.warning),
            const SizedBox(width: FerriTokens.spaceM),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    name,
                    style: Theme.of(context).textTheme.titleSmall,
                  ),
                  const SizedBox(height: 2),
                  Text(
                    'wants to pair with this device',
                    style: Theme.of(context)
                        .textTheme
                        .bodySmall!
                        .copyWith(color: palette.muted),
                  ),
                ],
              ),
            ),
            TextButton(
              onPressed: onDeny,
              style: TextButton.styleFrom(foregroundColor: palette.danger),
              child: const Text('Deny'),
            ),
            FilledButton(
              onPressed: onAllow,
              style: FilledButton.styleFrom(
                backgroundColor: palette.success,
                foregroundColor: Colors.black87,
              ),
              child: const Text('Allow'),
            ),
          ],
        ),
      ),
    );
  }
}

class _DeviceCard extends StatelessWidget {
  const _DeviceCard({
    required this.device,
    required this.folderCount,
    required this.onTap,
    required this.onRemove,
    required this.onRename,
  });

  final Device device;
  final int folderCount;
  final VoidCallback onTap;
  final VoidCallback onRemove;
  final VoidCallback onRename;

  IconData _iconFor(String name) {
    final n = name.toLowerCase();
    if (n.contains('laptop') || n.contains('desktop') || n.contains('pc')) {
      return Icons.laptop_mac;
    }
    if (n.contains('phone') ||
        n.contains('pixel') ||
        n.contains('sm-g') ||
        n.contains('motorola')) {
      return Icons.smartphone;
    }
    if (n.contains('server') || n.contains('nas') || n.contains('cloud')) {
      return Icons.dns;
    }
    return Icons.devices_other;
  }

  @override
  Widget build(BuildContext context) {
    final palette = context.ferri;
    final textTheme = Theme.of(context).textTheme;
    return InkWell(
      onTap: onTap,
      borderRadius: BorderRadius.circular(FerriTokens.radiusM),
      child: Padding(
        padding: const EdgeInsets.symmetric(
          horizontal: FerriTokens.spaceM,
          vertical: FerriTokens.spaceM,
        ),
        child: Row(
          children: [
          Stack(
            alignment: Alignment.bottomRight,
            children: [
              Container(
                width: 40,
                height: 40,
                decoration: BoxDecoration(
                  color: palette.surfaceHigh,
                  borderRadius: BorderRadius.circular(FerriTokens.radiusS),
                ),
                child: Icon(_iconFor(device.name),
                    color: palette.primary, size: 22),
              ),
              Container(
                width: 12,
                height: 12,
                decoration: BoxDecoration(
                  color: _presenceColor(device.presence, palette),
                  shape: BoxShape.circle,
                  border: Border.all(color: palette.surface, width: 2),
                ),
              ),
            ],
          ),
          const SizedBox(width: FerriTokens.spaceM),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  device.name,
                  style: textTheme.titleMedium
                      ?.copyWith(fontWeight: FontWeight.w600),
                  overflow: TextOverflow.ellipsis,
                ),
                const SizedBox(height: 2),
                Text(
                  '$folderCount folder${folderCount == 1 ? '' : 's'} · '
                  '${_presenceText(device)}',
                  style: textTheme.bodySmall!.copyWith(color: palette.muted),
                ),
              ],
            ),
          ),
          PopupMenuButton<String>(
            tooltip: 'Device actions',
            onSelected: (action) {
              if (action == 'rename') onRename();
              if (action == 'remove') onRemove();
            },
            itemBuilder: (_) => [
              const PopupMenuItem(value: 'rename', child: Text('Rename')),
              const PopupMenuItem(value: 'remove', child: Text('Remove')),
            ],
            icon: Icon(Icons.more_vert, color: palette.muted),
          ),
        ],
        ),
      ),
    );
  }
}

/// Presence vocabulary used across device rows and the detail sheet.
String _presenceText(Device d) => switch (d.presence) {
      Presence.connected => 'Connected',
      Presence.recentlySeen => 'Recently seen',
      Presence.offline =>
        d.lastSeen > 0 ? 'Offline · last seen ${d.lastSeenFormatted}' : 'Offline',
    };

Color _presenceColor(Presence p, FerriPalette palette) => switch (p) {
      Presence.connected => palette.success,
      Presence.recentlySeen => palette.warning,
      Presence.offline => palette.muted,
    };

/// Bottom sheet detail for a paired device: status, trusted badge, shared
/// folders and total data moved (from recorded outgoing sessions).
class _DeviceDetailSheet extends StatefulWidget {
  const _DeviceDetailSheet({
    required this.device,
    required this.folders,
    required this.service,
    required this.onRename,
    required this.onRemove,
    required this.onBrowseShared,
  });

  final Device device;
  final List<SyncFolder> folders;
  final SyncService service;
  final VoidCallback onRename;
  final VoidCallback onRemove;
  final VoidCallback onBrowseShared;

  @override
  State<_DeviceDetailSheet> createState() => _DeviceDetailSheetState();
}

class _DeviceDetailSheetState extends State<_DeviceDetailSheet> {
  int _syncedBytes = 0;
  bool _loading = true;

  /// Remote folders this peer makes available to sync (auto-discovered via its
  /// last-known address, so no address/path has to be typed).
  List<frb.RemoteSharedFolder> _remoteFolders = const [];
  bool _remoteLoading = true;
  bool _syncingRemote = false;

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    try {
      final sessions = await widget.service.sessionsForDevice(widget.device.id);
      if (!mounted) return;
      var total = 0;
      for (final s in sessions) {
        total += s.pushedBytes.toInt() + s.pulledBytes.toInt();
      }
      setState(() {
        _syncedBytes = total;
        _loading = false;
      });
    } catch (_) {
      if (mounted) setState(() => _loading = false);
    }

    try {
      final folders = await widget.service.remoteFoldersFor(widget.device);
      if (!mounted) return;
      setState(() {
        _remoteFolders = folders;
        _remoteLoading = false;
      });
    } catch (_) {
      if (mounted) setState(() => _remoteLoading = false);
    }
  }

  Future<void> _addRemoteFolder(frb.RemoteSharedFolder folder) async {
    if (_syncingRemote) return;
    setState(() => _syncingRemote = true);
    final result = await widget.service
        .syncRemoteFolder(device: widget.device, folder: folder);
    if (!mounted) return;
    setState(() => _syncingRemote = false);
    ScaffoldMessenger.of(context)
      ..hideCurrentSnackBar()
      ..showSnackBar(SnackBar(content: Text(result.message)));
    if (result.folderGuid != null) {
      await widget.service.refresh();
    }
  }

  @override
  Widget build(BuildContext context) {
    final palette = context.ferri;
    final textTheme = Theme.of(context).textTheme;
    final d = widget.device;
    return SafeArea(
      child: SingleChildScrollView(
        padding: const EdgeInsets.fromLTRB(24, 4, 24, 24),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Container(
                  width: 44,
                  height: 44,
                  decoration: BoxDecoration(
                    color: palette.surfaceHigh,
                    borderRadius: BorderRadius.circular(FerriTokens.radiusS),
                  ),
                  child: Icon(Icons.devices_other,
                      color: palette.primary, size: 24),
                ),
                const SizedBox(width: FerriTokens.spaceM),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Row(
                        children: [
                          Flexible(
                            child: Text(
                              d.name,
                              style: textTheme.titleLarge!
                                  .copyWith(fontWeight: FontWeight.w700),
                              overflow: TextOverflow.ellipsis,
                            ),
                          ),
                          const SizedBox(width: FerriTokens.spaceS),
                          Icon(
                            Icons.verified_outlined,
                            size: 16,
                            color: d.isOnline
                                ? palette.success
                                : palette.muted,
                          ),
                        ],
                      ),
                      const SizedBox(height: 2),
                      Text(
                        _presenceText(d),
                        style:
                            textTheme.bodySmall!.copyWith(color: palette.muted),
                      ),
                    ],
                  ),
                ),
              ],
            ),
            const SizedBox(height: FerriTokens.spaceL),
            ExpansionTile(
              initiallyExpanded: false,
              shape: const Border(),
              title: Text(
                'Details',
                style: textTheme.labelSmall!.copyWith(
                  letterSpacing: 1.1,
                  fontWeight: FontWeight.w700,
                  color: palette.muted,
                ),
              ),
              children: [
                _DetailTextRow(label: 'Presence', value: _presenceText(d)),
                _DetailTextRow(
                  label: 'Last seen',
                  value: d.lastSeen == 0 ? 'never' : d.lastSeenFormatted,
                ),
              ],
            ),
            const SizedBox(height: FerriTokens.spaceL),
            Text(
              'SYNCED WITH THIS DEVICE',
              style: textTheme.labelSmall!.copyWith(
                letterSpacing: 1.1,
                fontWeight: FontWeight.w700,
                color: palette.muted,
              ),
            ),
            const SizedBox(height: 2),
            Text(
              _loading
                  ? '…'
                  : _syncedBytes == 0
                      ? 'nothing yet'
                      : formatBytes(_syncedBytes),
              style: textTheme.titleMedium!.copyWith(fontWeight: FontWeight.w600),
            ),
            const SizedBox(height: FerriTokens.spaceL),
            Text(
              widget.folders.isEmpty
                  ? 'No shared folders'
                  : 'Shared folders (${widget.folders.length})',
              style: textTheme.labelSmall!.copyWith(
                letterSpacing: 1.1,
                fontWeight: FontWeight.w700,
                color: palette.muted,
              ),
            ),
            if (widget.folders.isNotEmpty) ...[
              const SizedBox(height: FerriTokens.spaceS),
              for (final f in widget.folders)
                Padding(
                  padding: const EdgeInsets.only(bottom: 4),
                  child: Row(
                    children: [
                      Icon(Icons.folder_outlined, size: 16, color: palette.muted),
                      const SizedBox(width: 8),
                      Expanded(
                        child: Text(
                          f.localPath
                              .split(RegExp(r'[/\\]'))
                              .where((s) => s.isNotEmpty)
                              .last,
                          style: textTheme.bodyMedium,
                          overflow: TextOverflow.ellipsis,
                        ),
                      ),
                    ],
                  ),
                ),
            ],
            const SizedBox(height: FerriTokens.spaceL),
            Text(
              'AVAILABLE TO SYNC',
              style: textTheme.labelSmall!.copyWith(
                letterSpacing: 1.1,
                fontWeight: FontWeight.w700,
                color: palette.muted,
              ),
            ),
            const SizedBox(height: 2),
            if (_remoteLoading)
              Text('…', style: textTheme.bodySmall!.copyWith(color: palette.muted))
            else if (_remoteFolders.isEmpty)
              Text(
                'Nothing discoverable from this device right now.',
                style: textTheme.bodySmall!.copyWith(color: palette.muted),
              )
            else
              for (final f in _remoteFolders)
                ListTile(
                  key: ValueKey('remote_path_${f.folderGuid}'),
                  dense: true,
                  contentPadding: EdgeInsets.zero,
                  leading: Icon(Icons.folder_shared_outlined,
                      size: 20, color: palette.primary),
                  title: Text(
                    f.name,
                    style: textTheme.bodyMedium,
                    overflow: TextOverflow.ellipsis,
                  ),
                  subtitle: Text(
                    _modeLabel(f.mode),
                    style:
                        textTheme.bodySmall!.copyWith(color: palette.muted),
                  ),
                  trailing: _syncingRemote
                      ? const SizedBox(
                          width: 16,
                          height: 16,
                          child: CircularProgressIndicator(strokeWidth: 2))
                      : const Icon(Icons.add_link, size: 20),
                  onTap: () => _addRemoteFolder(f),
                ),
            const SizedBox(height: FerriTokens.spaceL),
            FilledButton.tonalIcon(
              key: const ValueKey('browse_shared_folders'),
              onPressed: widget.onBrowseShared,
              icon: const Icon(Icons.folder_shared_outlined, size: 18),
              label: const Text('Browse shared folders'),
            ),
            const SizedBox(height: FerriTokens.spaceM),
            Row(
              children: [
                Expanded(
                  child: OutlinedButton.icon(
                    onPressed: widget.onRename,
                    icon: const Icon(Icons.edit_outlined, size: 16),
                    label: const Text('Rename'),
                  ),
                ),
                const SizedBox(width: FerriTokens.spaceM),
                Expanded(
                  child: OutlinedButton.icon(
                    onPressed: widget.onRemove,
                    style: OutlinedButton.styleFrom(
                      foregroundColor: palette.danger,
                    ),
                    icon: const Icon(Icons.person_remove_outlined, size: 16),
                    label: const Text('Remove'),
                  ),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}

/// One label/value row used inside the expandable "Details" section of the
/// device detail sheet.
class _DetailTextRow extends StatelessWidget {
  const _DetailTextRow({
    required this.label,
    required this.value,
  });

  final String label;
  final String value;

  @override
  Widget build(BuildContext context) {
    final textTheme = Theme.of(context).textTheme;
    final palette = context.ferri;
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 4),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          SizedBox(
            width: 110,
            child: Text(
              label,
              style: textTheme.bodySmall!.copyWith(color: palette.muted),
            ),
          ),
          Expanded(
            child: Text(value, style: textTheme.bodySmall),
          ),
        ],
      ),
    );
  }
}
