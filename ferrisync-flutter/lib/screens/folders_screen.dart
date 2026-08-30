import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:file_picker/file_picker.dart';
import '../models/sync_models.dart';
import '../providers/sync_provider.dart';
import '../theme/ferri_theme.dart';
import '../utils/storage_permission.dart';
import '../widgets/empty_state.dart';
import 'folder_content_screen.dart';

class FoldersScreen extends ConsumerWidget {
  const FoldersScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final service = ref.watch(syncServiceProvider);
    final folders = ref.watch(foldersProvider);
    final devices = ref.watch(devicesProvider);

    final deviceById = {for (final d in devices) d.id: d};

    return Scaffold(
      body: RefreshIndicator(
        onRefresh: service.refresh,
        child: ListView(
          physics: const AlwaysScrollableScrollPhysics(),
          padding: const EdgeInsets.all(FerriTokens.spaceL),
          children: [
            SectionHeader(
              title:
                  'SHARED FOLDERS (${folders.length})',
            ),
            const SizedBox(height: FerriTokens.spaceS),
            if (folders.isEmpty)
              EmptyState(
                icon: Icons.folder_shared_outlined,
                title: 'No folders shared yet',
                subtitle: 'Pick a folder on this device and choose a peer '
                    'to keep them in sync.',
                action: FilledButton.icon(
                  key: const ValueKey('add_folder_empty'),
                  onPressed: () => _addFolder(context, service),
                  icon: const Icon(Icons.add),
                  label: const Text('Add a folder'),
                ),
              )
            else
              Column(
                children: [
                  for (final f in folders) ...[
                    _FolderCard(
                      folder: f,
                      device: deviceById[f.deviceId],
                      syncing: service.status == SyncStatus.syncing,
                      onSync: () => _syncNow(context, service, f),
                      onRemove: () => _confirmRemove(context, service, f),
                    ),
                    const SizedBox(height: FerriTokens.spaceS),
                  ],
                ],
              ),
          ],
        ),
      ),
      floatingActionButton: FloatingActionButton(
        key: const ValueKey('add_folder_fab'),
        onPressed: () => _addFolder(context, service),
        child: const Icon(Icons.add),
      ),
    );
  }

  void _snack(BuildContext context, String message) {
    ScaffoldMessenger.of(context)
      ..hideCurrentSnackBar()
      ..showSnackBar(SnackBar(content: Text(message)));
  }

  void _syncNow(BuildContext context, SyncService service, SyncFolder f) async {
    if (!await ensureStorageAccess(context)) return;
    if (!context.mounted) return;
    _snack(context, 'Syncing ${f.localPath.split('/').where((s) => s.isNotEmpty).last}...');
    final message = await service.syncFolderNow(f);
    if (context.mounted) _snack(context, message);
  }

  Future<void> _confirmRemove(
      BuildContext context, SyncService service, SyncFolder f) async {
    final label =
        f.localPath.split('/').where((s) => s.isNotEmpty).last;
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: Text('Remove "$label"?'),
        content: Text(
          'This stops syncing ${f.localPath} and deletes its local '
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
    if (confirmed != true || !context.mounted) return;
    final message = await service.removeFolder(f.id);
    if (context.mounted) _snack(context, message);
  }

  void _addFolder(BuildContext context, SyncService service) async {
    if (!await ensureStorageAccess(context)) return;

    final result = await FilePicker.platform.getDirectoryPath();
    if (result == null) return;

    if (!context.mounted) return;
    await service.refresh();
    if (!context.mounted) return;

    final devices = service.devices;
    if (devices.isEmpty) {
      if (context.mounted) _snack(context, 'Pair a device first');
      return;
    }

    if (!context.mounted) return;
    final device = await showDialog<Device>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: const Text('Select Device'),
        content: SizedBox(
          width: double.maxFinite,
          child: ListView.builder(
            shrinkWrap: true,
            itemCount: devices.length,
            itemBuilder: (_, i) => ListTile(
              leading: const Icon(Icons.devices),
              title: Text(devices[i].name),
              subtitle: Text(devices[i].id),
              onTap: () => Navigator.pop(ctx, devices[i]),
            ),
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(ctx),
            child: const Text('Cancel'),
          ),
        ],
      ),
    );
    if (device == null) return;

    try {
      await service.addSyncFolder(result, device.id);
      if (context.mounted) {
        _snack(context, 'Folder added for ${device.name}');
      }
    } catch (e) {
      if (context.mounted) _snack(context, 'Failed: $e');
    }
  }
}

class _FolderCard extends ConsumerWidget {
  const _FolderCard({
    required this.folder,
    required this.device,
    required this.syncing,
    required this.onSync,
    required this.onRemove,
  });

  final SyncFolder folder;
  final Device? device;
  final bool syncing;
  final VoidCallback onSync;
  final VoidCallback onRemove;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final palette = context.ferri;
    final theme = Theme.of(context);
    final label = folder.localPath
        .split(RegExp(r'[/\\]'))
        .where((s) => s.isNotEmpty)
        .last;
    final size = ref
        .watch(folderSizeProvider(folder.id))
        .valueOrNull;
    final states = ref.watch(folderFileStatesProvider(folder)).value;

    const staleAfter = Duration(days: 7);
    final nowMs = DateTime.now().millisecondsSinceEpoch;
    final stale = folder.lastSyncAt > 0 &&
        nowMs - folder.lastSyncAt > staleAfter.inMilliseconds;

    return Card(
      key: ValueKey('folder_tile_${folder.id}'),
      child: InkWell(
        borderRadius: BorderRadius.circular(FerriTokens.radiusL),
        onTap: () {
          Navigator.of(context).push(
            MaterialPageRoute<void>(
              builder: (_) => FolderContentScreen(
                folder: folder,
                states: states ?? const {},
              ),
            ),
          );
        },
        child: Padding(
          padding: const EdgeInsets.all(FerriTokens.spaceL),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Row(
                children: [
                  Container(
                    width: 40,
                    height: 40,
                    decoration: BoxDecoration(
                      color: palette.surfaceHigh,
                      borderRadius: BorderRadius.circular(FerriTokens.radiusS),
                    ),
                    child:
                        Icon(Icons.folder_outlined, color: palette.primary),
                  ),
                  const SizedBox(width: FerriTokens.spaceM),
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          label,
                          style: theme.textTheme.titleMedium
                              ?.copyWith(fontWeight: FontWeight.w600),
                          overflow: TextOverflow.ellipsis,
                        ),
                        Text(
                          folder.localPath,
                          style: theme.textTheme.bodySmall!
                              .copyWith(color: palette.muted),
                          overflow: TextOverflow.ellipsis,
                        ),
                      ],
                    ),
                  ),
                  PopupMenuButton<String>(
                    tooltip: 'Folder actions',
                    onSelected: (action) {
                      if (action == 'sync') onSync();
                      if (action == 'remove') onRemove();
                    },
                    itemBuilder: (_) => [
                      const PopupMenuItem(value: 'sync', child: Text('Sync now')),
                      const PopupMenuItem(value: 'remove', child: Text('Remove')),
                    ],
                    icon: Icon(Icons.more_vert, color: palette.muted),
                  ),
                ],
              ),
              const SizedBox(height: FerriTokens.spaceM),
              Wrap(
                spacing: FerriTokens.spaceS,
                runSpacing: FerriTokens.spaceS,
                crossAxisAlignment: WrapCrossAlignment.center,
                children: [
                  _Chip(
                    icon: Icons.device_hub_outlined,
                    label: device?.name ?? folder.deviceId,
                    dot: device?.isOnline ?? false,
                    dotColor: palette.success,
                  ),
                  _Chip(
                    icon: _directionIcon(folder.direction),
                    label: _directionLabel(folder.direction),
                  ),
                  _Chip(
                    icon: Icons.schedule,
                    label: folder.lastSyncFormatted,
                  ),
                  if (size != null && size != '—')
                    _Chip(icon: Icons.storage_outlined, label: size),
                  if (syncing)
                    _Chip(
                      icon: Icons.sync,
                      label: 'Syncing…',
                      highlight: palette.primary,
                    )
                  else if (folder.lastSyncAt == 0)
                    _Chip(
                      icon: Icons.warning_amber_rounded,
                      label: 'Never synced',
                      highlight: palette.warning,
                    )
                  else if (stale)
                    _Chip(
                      icon: Icons.hourglass_bottom,
                      label: 'Stale',
                      highlight: palette.muted,
                    ),
                ],
              ),
            ],
          ),
        ),
      ),
    );
  }

  IconData _directionIcon(String direction) => switch (direction) {
        'push' => Icons.arrow_forward,
        'pull' => Icons.arrow_back,
        _ => Icons.swap_horiz,
      };

  String _directionLabel(String direction) => switch (direction) {
        'push' => 'Push only',
        'pull' => 'Pull only',
        _ => 'Two-way',
      };
}

class _Chip extends StatelessWidget {
  const _Chip({
    required this.icon,
    required this.label,
    this.dot = false,
    this.dotColor,
    this.highlight,
  });

  final IconData icon;
  final String label;
  final bool dot;
  final Color? dotColor;
  final Color? highlight;

  @override
  Widget build(BuildContext context) {
    final palette = context.ferri;
    final accent = highlight ?? palette.muted;
    return Container(
      padding: const EdgeInsets.symmetric(
        horizontal: FerriTokens.spaceS,
        vertical: 4,
      ),
      decoration: BoxDecoration(
        color: palette.surfaceHigh,
        borderRadius: BorderRadius.circular(FerriTokens.radiusS),
        border: highlight == null
            ? null
            : Border.all(color: accent.withValues(alpha: 0.35)),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          if (dot) ...[
            Container(
              width: 8,
              height: 8,
              decoration: BoxDecoration(
                color: dotColor ?? palette.muted,
                shape: BoxShape.circle,
              ),
            ),
            const SizedBox(width: 4),
          ],
          Icon(icon, size: 14, color: accent),
          const SizedBox(width: 4),
          Text(
            label,
            style: Theme.of(context)
                .textTheme
                .bodySmall!
                .copyWith(color: accent, fontWeight: FontWeight.w500),
          ),
        ],
      ),
    );
  }
}