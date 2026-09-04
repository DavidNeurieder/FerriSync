import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import '../gen/api.dart' as frb;
import '../models/sync_models.dart';
import '../providers/sync_provider.dart';
import '../theme/ferri_theme.dart';
import '../utils/format_bytes.dart';
import '../utils/relative_time.dart';
import '../widgets/empty_state.dart';
import 'browse_shared_folders.dart';
import 'preview_screen.dart';

String _modeLabel(String mode) => switch (mode) {
      'push' || 'send_only' => 'Send only',
      'pull' || 'receive_only' => 'Receive only',
      _ => 'Two-way',
    };

/// Full-page device detail, the plan's "device-centric" hero screen: status,
/// the folders it syncs (each with its own health + a review affordance),
/// available remote folders, recent activity, and the actions you expect on a
/// device. Replaces the old bottom sheet with a proper route.
class DeviceDetailScreen extends ConsumerStatefulWidget {
  const DeviceDetailScreen({super.key, required this.device});

  final Device device;

  @override
  ConsumerState<DeviceDetailScreen> createState() => _DeviceDetailScreenState();
}

class _DeviceDetailScreenState extends ConsumerState<DeviceDetailScreen> {
  int _syncedBytes = 0;
  bool _loading = true;
  List<frb.RemoteSharedFolder> _remoteFolders = const [];
  bool _remoteLoading = true;
  bool _syncingRemote = false;
  List<frb.SessionEntry> _sessions = const [];
  bool _sessionsLoading = true;

  Device get _device => widget.device;

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    final service = ref.read(syncServiceProvider);
    int total = 0;
    try {
      final sessions = await service.sessionsForDevice(_device.id);
      for (final s in sessions) {
        total += s.pushedBytes.toInt() + s.pulledBytes.toInt();
      }
      if (mounted) {
        setState(() {
          _sessions = sessions.take(20).toList();
          _syncedBytes = total;
          _loading = false;
        });
      }
    } catch (_) {
      if (mounted) setState(() => _loading = false);
    }

    try {
      final folders = await service.remoteFoldersFor(_device);
      if (mounted) {
        setState(() {
          _remoteFolders = folders;
          _remoteLoading = false;
          _sessionsLoading = false;
        });
      }
    } catch (_) {
      if (mounted) {
        setState(() {
          _remoteLoading = false;
          _sessionsLoading = false;
        });
      }
    }
  }

  Future<void> _addRemoteFolder(frb.RemoteSharedFolder folder) async {
    if (_syncingRemote) return;
    setState(() => _syncingRemote = true);
    final service = ref.read(syncServiceProvider);
    final result = await service.pairToShare(device: _device, folder: folder);
    if (!mounted) return;
    setState(() => _syncingRemote = false);
    ScaffoldMessenger.of(context)
      ..hideCurrentSnackBar()
      ..showSnackBar(SnackBar(content: Text(result.message)));
    if (result.folderGuid != null) {
      await service.refresh();
    }
  }

  Future<void> _confirmRemove(SyncService service) async {
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: const Text('Remove device'),
        content: Text(
          'Remove ${_device.name}? '
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
    if (confirmed != true || !mounted) return;
    final msg = await service.removeDevice(_device.id);
    if (!mounted) return;
    ScaffoldMessenger.of(context)
      ..hideCurrentSnackBar()
      ..showSnackBar(SnackBar(content: Text(msg)));
    Navigator.of(context).pop();
  }

  Future<void> _promptRename(SyncService service) async {
    final ctrl = TextEditingController(text: _device.name);
    final name = await showDialog<String>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: Text('Rename ${_device.name}'),
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
    if (name == null || name.isEmpty || !mounted) return;
    final msg = await service.renameRemoteDevice(_device.id, name);
    if (!mounted) return;
    ScaffoldMessenger.of(context)
      ..hideCurrentSnackBar()
      ..showSnackBar(SnackBar(content: Text(msg)));
  }

  @override
  Widget build(BuildContext context) {
    final palette = context.ferri;
    final textTheme = Theme.of(context).textTheme;
    final service = ref.watch(syncServiceProvider);
    final folders = ref
        .watch(foldersProvider)
        .where((f) => f.deviceId == _device.id)
        .toList();

    return Scaffold(
      appBar: AppBar(title: const Text('Device')),
      body: RefreshIndicator(
        onRefresh: _load,
        child: ListView(
          padding: const EdgeInsets.all(FerriTokens.spaceL),
          children: [
            _Header(device: _device, palette: palette, textTheme: textTheme),
            const SizedBox(height: FerriTokens.spaceL),
            if (!_device.isOnline)
              _OfflineCard(palette: palette, textTheme: textTheme, device: _device),
            const SizedBox(height: FerriTokens.spaceL),
            Row(
              children: [
                Expanded(
                  child: OutlinedButton.icon(
                    key: const ValueKey('device_rename'),
                    onPressed: () => _promptRename(service),
                    icon: const Icon(Icons.edit_outlined, size: 16),
                    label: const Text('Rename'),
                  ),
                ),
                const SizedBox(width: FerriTokens.spaceM),
                Expanded(
                  child: OutlinedButton.icon(
                    key: const ValueKey('device_remove'),
                    onPressed: () => _confirmRemove(service),
                    style: OutlinedButton.styleFrom(
                      foregroundColor: palette.danger,
                    ),
                    icon: const Icon(Icons.person_remove_outlined, size: 16),
                    label: const Text('Remove'),
                  ),
                ),
              ],
            ),
            const SizedBox(height: FerriTokens.spaceXL),
            Row(
              children: [
                Expanded(
                  child: _StatValue(
                    icon: Icons.folder_outlined,
                    label: 'Folders',
                    value: '${folders.length}',
                    palette: palette,
                    textTheme: textTheme,
                  ),
                ),
                const SizedBox(width: FerriTokens.spaceM),
                Expanded(
                  child: _StatValue(
                    icon: Icons.storage_outlined,
                    label: 'Data synced',
                    value: _loading ? '…' : formatBytes(_syncedBytes),
                    palette: palette,
                    textTheme: textTheme,
                  ),
                ),
              ],
            ),
            const SizedBox(height: FerriTokens.spaceXL),
            Text(
              'SYNCED FOLDERS',
              style: textTheme.labelSmall!.copyWith(
                letterSpacing: 1.1,
                fontWeight: FontWeight.w700,
                color: palette.muted,
              ),
            ),
            const SizedBox(height: FerriTokens.spaceS),
            if (folders.isEmpty)
              const EmptyState(
                icon: Icons.folder_open,
                title: 'No shared folders',
                subtitle: 'Add a folder and share it with this device.',
              )
            else
              Card(
                child: Column(
                  children: [
                    for (final f in folders) ...[
                      _FolderRow(
                        folder: f,
                        peerName: _device.name,
                        onTap: () => Navigator.of(context).push(
                          MaterialPageRoute<void>(
                            builder: (_) => PreviewScreen(
                              folder: f,
                              deviceId: _device.id,
                              peerName: _device.name,
                            ),
                          ),
                        ),
                      ),
                      if (f != folders.last)
                        const Divider(height: 1, indent: 16, endIndent: 16),
                    ],
                  ],
                ),
              ),
            const SizedBox(height: FerriTokens.spaceXL),
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
                    style: textTheme.bodySmall!.copyWith(color: palette.muted),
                  ),
                  trailing: _syncingRemote
                      ? const SizedBox(
                          width: 16,
                          height: 16,
                          child: CircularProgressIndicator(strokeWidth: 2))
                      : const Icon(Icons.add_link, size: 20),
                  onTap: () => _addRemoteFolder(f),
                ),
            const SizedBox(height: FerriTokens.spaceXL),
            Text(
              'RECENT ACTIVITY',
              style: textTheme.labelSmall!.copyWith(
                letterSpacing: 1.1,
                fontWeight: FontWeight.w700,
                color: palette.muted,
              ),
            ),
            const SizedBox(height: FerriTokens.spaceS),
            if (_sessionsLoading)
              Text('…', style: textTheme.bodySmall!.copyWith(color: palette.muted))
            else if (_sessions.isEmpty)
              Text(
                'No sync sessions yet.',
                style: textTheme.bodySmall!.copyWith(color: palette.muted),
              )
            else
              Card(
                child: Column(
                  children: [
                    for (final s in _sessions) ...[
                      _SessionRow(session: s, palette: palette, textTheme: textTheme),
                      if (s != _sessions.last)
                        const Divider(height: 1, indent: 16, endIndent: 16),
                    ],
                  ],
                ),
              ),
            const SizedBox(height: FerriTokens.spaceL),
            FilledButton.tonalIcon(
              key: const ValueKey('browse_shared_folders'),
              onPressed: () => showModalBottomSheet<void>(
                context: context,
                showDragHandle: true,
                isScrollControlled: true,
                builder: (_) => BrowseSharedFoldersSheet(
                  device: _device,
                  service: service,
                ),
              ),
              icon: const Icon(Icons.folder_shared_outlined, size: 18),
              label: const Text('Browse shared folders'),
            ),
          ],
        ),
      ),
    );
  }
}

class _Header extends StatelessWidget {
  const _Header({
    required this.device,
    required this.palette,
    required this.textTheme,
  });

  final Device device;
  final FerriPalette palette;
  final TextTheme textTheme;

  @override
  Widget build(BuildContext context) {
    final online = device.presence != Presence.offline;
    final presenceText = switch (device.presence) {
      Presence.connected => 'Connected',
      Presence.recentlySeen => 'Recently seen',
      Presence.offline => device.lastSeen > 0
          ? 'Offline · last seen ${device.lastSeenFormatted}'
          : 'Offline',
    };
    return Row(
      children: [
        Container(
          width: 56,
          height: 56,
          decoration: BoxDecoration(
            color: palette.surfaceHigh,
            borderRadius: BorderRadius.circular(FerriTokens.radiusM),
          ),
          child: Icon(Icons.devices_other, color: palette.primary, size: 30),
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
                      device.name,
                      style: textTheme.titleLarge!
                          .copyWith(fontWeight: FontWeight.w700),
                      overflow: TextOverflow.ellipsis,
                    ),
                  ),
                  const SizedBox(width: FerriTokens.spaceS),
                  Icon(
                    Icons.verified_outlined,
                    size: 18,
                    color: online ? palette.success : palette.muted,
                  ),
                ],
              ),
              const SizedBox(height: 4),
              Row(
                children: [
                  Container(
                    width: 10,
                    height: 10,
                    decoration: BoxDecoration(
                      color: online ? palette.success : palette.muted,
                      shape: BoxShape.circle,
                    ),
                  ),
                  const SizedBox(width: 6),
                  Flexible(
                    child: Text(
                      presenceText,
                      style: textTheme.bodyMedium!.copyWith(
                        color: online ? palette.success : palette.muted,
                      ),
                      overflow: TextOverflow.ellipsis,
                    ),
                  ),
                ],
              ),
            ],
          ),
        ),
      ],
    );
  }
}

class _OfflineCard extends StatelessWidget {
  const _OfflineCard({
    required this.palette,
    required this.textTheme,
    required this.device,
  });

  final FerriPalette palette;
  final TextTheme textTheme;
  final Device device;

  @override
  Widget build(BuildContext context) {
    return Card(
      color: palette.surfaceHigh,
      child: Padding(
        padding: const EdgeInsets.all(FerriTokens.spaceL),
        child: Row(
          children: [
            Icon(Icons.cloud_off, color: palette.muted, size: 24),
            const SizedBox(width: FerriTokens.spaceM),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    'Offline',
                    style:
                        textTheme.titleSmall!.copyWith(fontWeight: FontWeight.w600),
                  ),
                  const SizedBox(height: 2),
                  Text(
                    "Your files are safe. Sync resumes automatically "
                    "when ${device.name} reconnects.",
                    style:
                        textTheme.bodySmall!.copyWith(color: palette.muted),
                  ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _StatValue extends StatelessWidget {
  const _StatValue({
    required this.icon,
    required this.label,
    required this.value,
    required this.palette,
    required this.textTheme,
  });

  final IconData icon;
  final String label;
  final String value;
  final FerriPalette palette;
  final TextTheme textTheme;

  @override
  Widget build(BuildContext context) {
    return Card(
      child: Padding(
        padding: const EdgeInsets.symmetric(
          vertical: FerriTokens.spaceM,
          horizontal: FerriTokens.spaceS,
        ),
        child: Column(
          children: [
            Icon(icon, size: 20, color: palette.muted),
            const SizedBox(height: FerriTokens.spaceXS),
            Text(
              value,
              style: textTheme.titleMedium!.copyWith(fontWeight: FontWeight.w700),
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
            ),
            const SizedBox(height: 2),
            Text(
              label,
              style: textTheme.bodySmall!.copyWith(color: palette.muted),
              maxLines: 1,
            ),
          ],
        ),
      ),
    );
  }
}

class _FolderRow extends StatelessWidget {
  const _FolderRow({
    required this.folder,
    required this.peerName,
    required this.onTap,
  });

  final SyncFolder folder;
  final String peerName;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final palette = context.ferri;
    final textTheme = Theme.of(context).textTheme;
    final label = folder.localPath
        .split(RegExp(r'[/\\]'))
        .where((s) => s.isNotEmpty)
        .last;
    final (icon, color, chipLabel) = _healthLook(folder.health, palette);
    return ListTile(
      onTap: onTap,
      dense: true,
      leading: Icon(icon, size: 20, color: color),
      title: Text(label, style: textTheme.bodyLarge),
      subtitle: Text(
        '$folder.name · $peerName',
        style: textTheme.bodySmall!.copyWith(color: palette.muted),
        overflow: TextOverflow.ellipsis,
      ),
      trailing: Text(
        chipLabel,
        style: textTheme.bodySmall!.copyWith(
          color: color,
          fontWeight: FontWeight.w600,
        ),
      ),
    );
  }

  (IconData, Color, String) _healthLook(FolderHealth h, FerriPalette palette) =>
      switch (h) {
        FolderHealth.healthy => (Icons.check_circle, palette.success, 'Synced'),
        FolderHealth.syncing => (Icons.sync, palette.syncing, 'Syncing'),
        FolderHealth.waiting => (Icons.hourglass_top, palette.warning, 'Waiting'),
        FolderHealth.offline => (Icons.cloud_off, palette.muted, 'Offline'),
        FolderHealth.error => (Icons.error, palette.danger, 'Error'),
        FolderHealth.conflict =>
          (Icons.warning_amber_rounded, palette.danger, 'Conflict'),
        FolderHealth.notConfigured =>
          (Icons.folder_shared_outlined, palette.muted, 'Unconfigured'),
      };
}

class _SessionRow extends StatelessWidget {
  const _SessionRow({
    required this.session,
    required this.palette,
    required this.textTheme,
  });

  final frb.SessionEntry session;
  final FerriPalette palette;
  final TextTheme textTheme;

  @override
  Widget build(BuildContext context) {
    final isPush = session.direction != 'pull';
    final folderName =
        session.folderPath.split(RegExp(r'[/\\]')).where((s) => s.isNotEmpty).lastOrNull ??
            '';
    final hadConflicts = session.conflictsCount > BigInt.zero;
    return ListTile(
      dense: true,
      leading: Icon(
        isPush ? Icons.arrow_upward : Icons.arrow_downward,
        size: 18,
        color: isPush ? palette.primary : palette.syncing,
      ),
      title: Text(folderName, style: textTheme.bodyMedium),
      subtitle: Text(
        '${relativeTime(session.ts)}'
        '${hadConflicts ? ' · conflict!' : ''}',
        style: textTheme.bodySmall!.copyWith(color: palette.muted),
      ),
      trailing: Text(
        '↑ ${session.pushedCount} ↓ ${session.pulledCount}',
        style: textTheme.bodySmall!.copyWith(
          color: hadConflicts ? palette.danger : palette.muted,
        ),
      ),
    );
  }
}
