import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import '../gen/api.dart' as frb;
import '../models/sync_models.dart';
import '../providers/sync_provider.dart';
import '../theme/ferri_theme.dart';
import '../utils/format_bytes.dart';
import '../utils/relative_time.dart';
import '../widgets/empty_state.dart';
import '../widgets/add_folder_device_flow.dart';
import 'folder_content_screen.dart';

/// Canonical relationship view for one shared folder: its health, who it is
/// shared with, last sync, size, recent changes, and the actions you expect
/// on a folder (sync now / browse / remove).
class FolderDetailScreen extends ConsumerStatefulWidget {
  const FolderDetailScreen({super.key, required this.folder});

  final SyncFolder folder;

  @override
  ConsumerState<FolderDetailScreen> createState() => _FolderDetailScreenState();
}

class _FolderDetailScreenState extends ConsumerState<FolderDetailScreen> {
  Future<List<frb.FileHistoryEntry>>? _recent;

  SyncFolder get _folder => widget.folder;

  String get _label => _folder.localPath
      .split(RegExp(r'[/\\]'))
      .where((s) => s.isNotEmpty)
      .last;

  @override
  void initState() {
    super.initState();
    _recent = _loadRecent();
  }

  Future<List<frb.FileHistoryEntry>> _loadRecent() async {
    final service = ref.read(syncServiceProvider);
    final entries = await service.historyForFolder(_folder.id);
    final seen = <String>{}; // latest per file path wins
    final latest = <frb.FileHistoryEntry>[];
    for (final e in entries) {
      if (seen.contains(e.path)) continue;
      seen.add(e.path);
      latest.add(e);
    }
    latest.sort((a, b) => b.recordedAt.compareTo(a.recordedAt));
    return latest.take(20).toList();
  }

  void _snack(String message) {
    ScaffoldMessenger.of(context)
      ..hideCurrentSnackBar()
      ..showSnackBar(SnackBar(content: Text(message)));
  }

  Future<void> _syncNow(SyncService service) async {
    _snack('Syncing $_label…');
    final message = await service.syncFolderNow(_folder);
    _snack(message);
  }

  Future<void> _remove(SyncService service) async {
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: const Text('Remove folder?'),
        content: Text(
          'This stops syncing ${_folder.localPath} and deletes its local '
          'metadata and history. Files on disk are kept.',
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
    final message = await service.removeFolder(_folder.id);
    if (!mounted) return;
    _snack(message);
    Navigator.of(context).pop();
  }

  @override
  Widget build(BuildContext context) {
    final palette = context.ferri;
    final textTheme = Theme.of(context).textTheme;
    final service = ref.watch(syncServiceProvider);
    final devices = ref.watch(devicesProvider);
    final myName = ref.watch(deviceNameProvider);
    final health = _folder.health;

    final deviceById = {for (final d in devices) d.id: d};
    final peerName =
        _folder.deviceId == service.deviceId
            ? 'this device'
            : deviceById[_folder.deviceId]?.name ??
                  _folder.deviceName ??
                  _folder.deviceId;

    final size = ref.watch(folderSizeProvider(_folder.id));
    final states = ref.watch(folderFileStatesProvider(_folder));

    final (healthIcon, healthColor, healthLabel) =
        _healthLook(health, palette);
    final lastSync = _folder.lastSyncAt;

    final recent = _recent;

    return Scaffold(
      appBar: AppBar(title: Text(_label)),
      body: RefreshIndicator(
        onRefresh: () async {
          setState(() {
            _recent = _loadRecent();
          });
          await service.refresh();
        },
        child: ListView(
          padding: const EdgeInsets.all(FerriTokens.spaceL),
          children: [
            Card(
              child: Padding(
                padding: const EdgeInsets.all(FerriTokens.spaceL),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Row(
                      children: [
                        Container(
                          width: 44,
                          height: 44,
                          decoration: BoxDecoration(
                            color: healthColor.withValues(alpha: 0.12),
                            borderRadius:
                                BorderRadius.circular(FerriTokens.radiusS),
                          ),
                          child: Icon(healthIcon, color: healthColor),
                        ),
                        const SizedBox(width: FerriTokens.spaceM),
                        Expanded(
                          child: Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              Text(
                                healthLabel,
                                style: textTheme.titleMedium!.copyWith(
                                  fontWeight: FontWeight.w700,
                                  color: healthColor,
                                ),
                              ),
                              const SizedBox(height: 2),
                              Text(
                                _folder.localPath,
                                style: textTheme.bodySmall!
                                    .copyWith(color: palette.muted),
                              ),
                            ],
                          ),
                        ),
                      ],
                    ),
                    const SizedBox(height: FerriTokens.spaceL),
                    _DetailRow(
                      label: 'Relationship',
                      value: '$myName ↔ $peerName',
                    ),
                    _DetailRow(
                      label: 'Last sync',
                      value: lastSync == 0
                          ? 'Never synced yet'
                          : relativeTime(lastSync),
                    ),
                    _DetailRow(
                      label: 'Sync mode',
                      value: _directionLabel(_folder.direction),
                    ),
                  ],
                ),
              ),
            ),
            const SizedBox(height: FerriTokens.spaceM),
            _SharedWithCard(
              folderId: _folder.id,
              localPath: _folder.localPath,
              peers: _folder.peers,
              deviceById: deviceById,
              myDeviceId: service.deviceId,
            ),
            const SizedBox(height: FerriTokens.spaceM),
            Row(
              children: [
                Expanded(
                  child: _StatCard(
                    icon: Icons.folder_outlined,
                    label: 'Files',
                    value: states.value == null ? '—' : '${states.value!.length}',
                  ),
                ),
                const SizedBox(width: FerriTokens.spaceM),
                Expanded(
                  child: _StatCard(
                    icon: Icons.storage_outlined,
                    label: 'Size',
                    value: size.value ?? '—',
                  ),
                ),
                const SizedBox(width: FerriTokens.spaceM),
                Expanded(
                  child: _StatCard(
                    icon: Icons.confirmation_number_outlined,
                    label: 'Conflicts',
                    value: '${_folder.conflicts}',
                  ),
                ),
              ],
            ),
            const SizedBox(height: FerriTokens.spaceM),
            Wrap(
              spacing: FerriTokens.spaceS,
              runSpacing: FerriTokens.spaceS,
              children: [
                FilledButton.icon(
                  key: const ValueKey('detail_sync_now'),
                  onPressed: () => _syncNow(service),
                  icon: const Icon(Icons.sync, size: 16),
                  label: const Text('Sync now'),
                ),
                OutlinedButton.icon(
                  key: const ValueKey('detail_browse_files'),
                  onPressed: () {
                    Navigator.of(context).push(
                      MaterialPageRoute<void>(
                        builder: (_) => FolderContentScreen(
                          folder: _folder,
                          states: states.value ?? const {},
                        ),
                      ),
                    );
                  },
                  icon: const Icon(Icons.folder_open, size: 16),
                  label: const Text('Browse files'),
                ),
                OutlinedButton.icon(
                  key: const ValueKey('detail_remove'),
                  onPressed: () => _remove(service),
                  icon: const Icon(Icons.delete_outline, size: 16),
                  label: const Text('Remove'),
                ),
              ],
            ),
            const SizedBox(height: FerriTokens.spaceXL),
            const SectionHeader(title: 'RECENT CHANGES'),
            const SizedBox(height: FerriTokens.spaceS),
            FutureBuilder<List<frb.FileHistoryEntry>>(
              future: recent,
              builder: (context, snapshot) {
                final entries = snapshot.data;
                if (entries == null || entries.isEmpty) {
                  return Card(
                    child: Padding(
                      padding: const EdgeInsets.all(FerriTokens.spaceL),
                      child: Text(
                        entries == null
                            ? 'Loading…'
                            : 'No recorded changes yet.',
                        style: textTheme.bodyMedium!
                            .copyWith(color: palette.muted),
                      ),
                    ),
                  );
                }
                return Card(
                  child: Column(
                    children: [
                      for (final e in entries) ...[
                        _HistoryRow(entry: e),
                        if (e != entries.last)
                          const Divider(height: 1, indent: 16, endIndent: 16),
                      ],
                    ],
                  ),
                );
              },
            ),
          ],
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
        FolderHealth.error => (Icons.error, palette.danger, 'Sync error'),
        FolderHealth.conflict =>
          (Icons.warning_amber_rounded, palette.danger, 'Needs attention'),
        FolderHealth.notConfigured =>
          (Icons.folder_shared_outlined, palette.muted, 'Not configured'),
      };

  String _directionLabel(String direction) => switch (direction) {
        'push' => 'Push only',
        'pull' => 'Pull only',
        _ => 'Two-way',
      };
}

class _DetailRow extends StatelessWidget {
  const _DetailRow({required this.label, required this.value});

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
            width: 120,
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

class _StatCard extends StatelessWidget {
  const _StatCard({
    required this.icon,
    required this.label,
    required this.value,
  });

  final IconData icon;
  final String label;
  final String value;

  @override
  Widget build(BuildContext context) {
    final textTheme = Theme.of(context).textTheme;
    final palette = context.ferri;
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

class _HistoryRow extends StatelessWidget {
  const _HistoryRow({required this.entry});

  final frb.FileHistoryEntry entry;

  @override
  Widget build(BuildContext context) {
    final palette = context.ferri;
    final textTheme = Theme.of(context).textTheme;
    final action = entry.action.toLowerCase();
    final (icon, color) = switch (action) {
      _ when action.contains('conflict') =>
        (Icons.warning_amber_rounded, palette.danger),
      _ when action.contains('pull') => (Icons.arrow_downward, palette.syncing),
      _ when action.contains('push') => (Icons.arrow_upward, palette.primary),
      _ when action.contains('delete') => (Icons.delete_outline, palette.muted),
      _ => (Icons.drive_file_move_outline, palette.muted),
    };
    return ListTile(
      dense: true,
      leading: Icon(icon, size: 18, color: color),
      title: Text(
        entry.path.split(RegExp(r'[/\\]')).where((s) => s.isNotEmpty).toList().lastOrNull ??
            entry.path,
        style: textTheme.bodyMedium,
        overflow: TextOverflow.ellipsis,
      ),
      subtitle: Text(
        '${entry.action} · ${relativeTime(entry.recordedAt)}',
        style: textTheme.bodySmall!.copyWith(color: palette.muted),
        overflow: TextOverflow.ellipsis,
      ),
      trailing: entry.size != null
          ? Text(
              formatBytes(entry.size!),
              style: textTheme.bodySmall!.copyWith(color: palette.muted),
            )
          : null,
    );
  }
}

/// Every device this folder syncs with, each with its mode + remote path, plus
/// an "Add device" action to grow the relationship and a per-peer remove.
class _SharedWithCard extends ConsumerWidget {
  const _SharedWithCard({
    required this.folderId,
    required this.localPath,
    required this.peers,
    required this.deviceById,
    required this.myDeviceId,
  });

  final int folderId;
  final String localPath;
  final List<FolderPeer> peers;
  final Map<String, Device> deviceById;
  final String myDeviceId;

  String _peerName(String id) =>
      id == myDeviceId ? 'this device' : deviceById[id]?.name ?? id;

  String _modeLabel(String mode) => switch (mode) {
        'push' || 'send_only' => 'Send only',
        'pull' || 'receive_only' => 'Receive only',
        _ => 'Two-way',
      };

  Future<void> _addDevice(BuildContext context, WidgetRef ref) async {
    final service = ref.read(syncServiceProvider);
    final live =
        ref.read(foldersProvider).where((f) => f.id == folderId).firstOrNull;
    await runAddFolderDeviceFlow(
      context,
      service,
      folderId: folderId,
      localPath: localPath,
      existingPeers: live?.peers ?? peers,
    );
  }

  Future<void> _removeDevice(
    BuildContext context,
    WidgetRef ref,
    FolderPeer peer,
  ) async {
    final service = ref.read(syncServiceProvider);
    final name = _peerName(peer.deviceId);
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: const Text('Stop sharing with device?'),
        content: Text(
          'Stop syncing this folder with $name? Files on both devices are kept.',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(ctx, false),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(ctx, true),
            child: const Text('Remove device'),
          ),
        ],
      ),
    );
    if (confirmed != true || !context.mounted) return;
    await service.removeDeviceFromFolder(folderId, peer.deviceId);
  }

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final textTheme = Theme.of(context).textTheme;
    final palette = context.ferri;
    // Watch the live folder so peer add/remove reflects without a manual refresh.
    final live = ref
        .watch(foldersProvider)
        .where((f) => f.id == folderId)
        .firstOrNull;
    final shownPeers = live?.peers ?? peers;
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(FerriTokens.spaceL),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Expanded(
                  child: Text(
                    'SHARED WITH',
                    style: textTheme.labelSmall!.copyWith(
                      color: palette.muted,
                      letterSpacing: 1.1,
                    ),
                  ),
                ),
                TextButton.icon(
                  key: const ValueKey('detail_add_device'),
                  onPressed: () => _addDevice(context, ref),
                  icon: const Icon(Icons.add, size: 16),
                  label: const Text('Add device'),
                ),
              ],
            ),
            const SizedBox(height: FerriTokens.spaceS),
            if (shownPeers.isEmpty)
              Text(
                'Not syncing with any device yet.',
                style: textTheme.bodySmall!.copyWith(color: palette.muted),
              )
            else
              for (final peer in shownPeers)
                ListTile(
                  dense: true,
                  contentPadding: EdgeInsets.zero,
                  leading: const Icon(Icons.devices_other, size: 20),
                  title: Text(
                    _peerName(peer.deviceId),
                    style: textTheme.bodyMedium,
                  ),
                  subtitle: Text(
                    peer.remotePath == null || peer.remotePath!.isEmpty
                        ? _modeLabel(peer.mode)
                        : '${_modeLabel(peer.mode)} → ${peer.remotePath}',
                    style: textTheme.bodySmall!.copyWith(color: palette.muted),
                    overflow: TextOverflow.ellipsis,
                  ),
                  trailing: IconButton(
                    tooltip: 'Stop sharing with this device',
                    icon: const Icon(Icons.close, size: 18),
                    color: palette.muted,
                    onPressed: () => _removeDevice(context, ref, peer),
                  ),
                ),
          ],
        ),
      ),
    );
  }
}