import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import '../gen/api.dart' as frb;
import '../models/sync_models.dart';
import '../providers/sync_provider.dart';
import '../theme/ferri_theme.dart';
import '../utils/add_folder_flow.dart';
import '../utils/storage_permission.dart';
import '../widgets/empty_state.dart';
import 'folder_detail_screen.dart';

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
              title: 'SHARED FOLDERS (${folders.length})',
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
                  onPressed: () => runAddFolderFlow(context, service),
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
                      onPair: () => _pairWithDevice(context, service, f),
                      onRemove: () => _confirmRemove(context, service, f),
                    ),
                    const SizedBox(height: FerriTokens.spaceS),
                  ],
                ],
              ),
            const SizedBox(height: FerriTokens.spaceXL),
            _PublishedSharesSection(service: service),
            const SizedBox(height: FerriTokens.spaceXL),
            _FolderPairingApprovals(service: service),
          ],
        ),
      ),
      floatingActionButton: FloatingActionButton(
        key: const ValueKey('add_folder_fab'),
        onPressed: () => runAddFolderFlow(context, service),
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
    _snack(context,
        'Syncing ${f.localPath.split('/').where((s) => s.isNotEmpty).last}...');
    final message = await service.syncFolderNow(f);
    if (context.mounted) _snack(context, message);
  }

  Future<void> _pairWithDevice(
      BuildContext context, SyncService service, SyncFolder f) async {
    if (!await ensureStorageAccess(context)) return;
    if (!context.mounted) return;
    await pairExistingFolder(context, service, f.localPath);
  }

  Future<void> _confirmRemove(
      BuildContext context, SyncService service, SyncFolder f) async {
    final label = f.localPath.split('/').where((s) => s.isNotEmpty).last;
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
    await service.removeFolder(f.id);
    if (!context.mounted) return;
    ScaffoldMessenger.of(context)
      ..hideCurrentSnackBar()
      ..showSnackBar(
        SnackBar(
          content: Text('Removed "$label" from sync'),
          action: SnackBarAction(
            label: 'Undo',
            onPressed: () async {
              try {
                await service.addSyncFolder(f.localPath, f.deviceId);
                if (context.mounted) _snack(context, 'Folder restored');
              } catch (_) {
                if (context.mounted) {
                  _snack(context, "Couldn't restore the folder");
                }
              }
            },
          ),
        ),
      );
  }
}

class _FolderCard extends ConsumerWidget {
  const _FolderCard({
    required this.folder,
    required this.device,
    required this.syncing,
    required this.onSync,
    required this.onPair,
    required this.onRemove,
  });

  final SyncFolder folder;
  final Device? device;
  final bool syncing;
  final VoidCallback onSync;
  final VoidCallback onPair;
  final VoidCallback onRemove;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final palette = context.ferri;
    final theme = Theme.of(context);
    final label = folder.localPath
        .split(RegExp(r'[/\\]'))
        .where((s) => s.isNotEmpty)
        .last;
    final size = ref.watch(folderSizeProvider(folder.id)).valueOrNull;
    final myName = ref.watch(deviceNameProvider);

    const staleAfter = Duration(days: 7);
    final nowMs = DateTime.now().millisecondsSinceEpoch;
    final stale = folder.lastSyncAt > 0 &&
        nowMs - folder.lastSyncAt > staleAfter.inMilliseconds;
    final online = device?.isOnline ?? false;
    final waiting = device != null && !online;

    return Card(
      key: ValueKey('folder_tile_${folder.id}'),
      child: InkWell(
        borderRadius: BorderRadius.circular(FerriTokens.radiusL),
        onTap: () {
          Navigator.of(context).push(
            MaterialPageRoute<void>(
              builder: (_) => FolderDetailScreen(folder: folder),
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
                    child: Icon(Icons.folder_outlined, color: palette.primary),
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
                      if (action == 'pair') onPair();
                      if (action == 'remove') onRemove();
                    },
                    itemBuilder: (_) => [
                      const PopupMenuItem(
                          value: 'sync', child: Text('Sync now')),
                      const PopupMenuItem(
                          value: 'pair',
                          child: Text('Sync with a device')),
                      const PopupMenuItem(
                          value: 'remove', child: Text('Remove')),
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
                    icon: Icons.swap_horiz,
                    label: myName.isEmpty
                        ? device?.name ?? folder.deviceId
                        : '$myName ↔ ${device?.name ?? folder.deviceId}',
                    dot: online,
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
                  else if (waiting)
                    _Chip(
                      icon: Icons.hourglass_top,
                      label: 'Waiting',
                      highlight: palette.muted,
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

/// Folders this device publishes for trusted peers to request. Each shows a
/// discoverable toggle and an unshare action.
class _PublishedSharesSection extends ConsumerWidget {
  const _PublishedSharesSection({required this.service});

  final SyncService service;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final shares = service.mySharedFolders;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        SectionHeader(
          title: 'PUBLISHED SHARES (${shares.length})',
          actionLabel: 'Share a folder',
          onAction: () => _showShareHelp(context, service),
        ),
        const SizedBox(height: FerriTokens.spaceS),
        if (shares.isEmpty)
          Text(
            'Publish a folder so trusted devices can discover and request '
            'to pair to it.',
            style: TextStyle(color: context.ferri.muted),
          )
        else
          for (final share in shares) ...[
            _ShareCard(share: share, service: service),
            const SizedBox(height: FerriTokens.spaceS),
          ],
      ],
    );
  }

  void _showShareHelp(BuildContext context, SyncService service) {
    final folders = service.folders;
    if (folders.isEmpty) {
      ScaffoldMessenger.of(context)
        ..hideCurrentSnackBar()
        ..showSnackBar(const SnackBar(
            content: Text('Add a sync folder first, then share it.')));
      return;
    }
    showModalBottomSheet<_SharePickerResult>(
      context: context,
      builder: (ctx) => _SharePickerSheet(folders: folders),
    ).then((choice) async {
      if (choice == null || !context.mounted) return;
      final ownerName = service.deviceName;
      final message = await service.shareFolder(choice.folderId, ownerName);
      if (context.mounted) {
        ScaffoldMessenger.of(context)
          ..hideCurrentSnackBar()
          ..showSnackBar(SnackBar(content: Text(message)));
      }
    });
  }
}

/// Result of choosing a folder to share.
class _SharePickerResult {
  const _SharePickerResult(this.folderId, this.label);
  final int folderId;
  final String label;
}

/// Bottom sheet listing existing sync folders to publish as shares.
class _SharePickerSheet extends StatelessWidget {
  const _SharePickerSheet({required this.folders});

  final List<SyncFolder> folders;

  @override
  Widget build(BuildContext context) {
    return SafeArea(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          const Padding(
            padding: EdgeInsets.all(FerriTokens.spaceL),
            child: Text('Share a folder',
                style: TextStyle(fontWeight: FontWeight.bold)),
          ),
          Flexible(
            child: ListView(
              shrinkWrap: true,
              children: [
                for (final f in folders)
                  ListTile(
                    key: ValueKey('share_pick_${f.id}'),
                    leading: const Icon(Icons.folder_outlined),
                    title: Text(f.name),
                    subtitle: Text(f.localPath),
                    onTap: () => Navigator.pop(
                        context, _SharePickerResult(f.id, f.name)),
                  ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

/// A single published share with discoverable toggle + unshare.
class _ShareCard extends ConsumerWidget {
  const _ShareCard({required this.share, required this.service});

  final frb.SharedFolder share;
  final SyncService service;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    return Card(
      key: ValueKey('share_${share.id}'),
      child: ListTile(
        leading: const Icon(Icons.folder_shared_outlined),
        title: Text(share.name),
        subtitle: Text(
          '${share.localPath}\n${share.permissions}'
          '${share.discoverable ? ' · discoverable' : ' · hidden'}',
          maxLines: 2,
          overflow: TextOverflow.ellipsis,
        ),
        trailing: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Tooltip(
              message: share.discoverable
                  ? 'Visible to trusted devices'
                  : 'Hidden from discovery',
              child: GestureDetector(
                onTap: () async {
                  final message = await service.setSharedDiscoverable(
                      share.id.toInt(), !share.discoverable);
                  if (context.mounted) {
                    ScaffoldMessenger.of(context)
                      ..hideCurrentSnackBar()
                      ..showSnackBar(SnackBar(content: Text(message)));
                  }
                },
                child: Padding(
                  padding: const EdgeInsets.all(4),
                  child: Icon(
                    share.discoverable
                        ? Icons.visibility
                        : Icons.visibility_off,
                    color: share.discoverable
                        ? Theme.of(context).colorScheme.primary
                        : null,
                  ),
                ),
              ),
            ),
            IconButton(
              tooltip: 'Stop sharing',
              icon: const Icon(Icons.link_off),
              onPressed: () async {
                final message = await service.unshareFolder(share.id.toInt());
                if (context.mounted) {
                  ScaffoldMessenger.of(context)
                    ..hideCurrentSnackBar()
                    ..showSnackBar(SnackBar(content: Text(message)));
                }
              },
            ),
          ],
        ),
      ),
    );
  }
}

/// Peers waiting for their folder-pairing request to be approved.
class _FolderPairingApprovals extends ConsumerWidget {
  const _FolderPairingApprovals({required this.service});

  final SyncService service;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final pending = service.pendingFolderPairings;
    if (pending.isEmpty) return const SizedBox.shrink();
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        SectionHeader(title: 'FOLDER PAIRING REQUESTS (${pending.length})'),
        const SizedBox(height: FerriTokens.spaceS),
        for (final p in pending) ...[
          Card(
            key: ValueKey('folder_pair_${p.deviceId}_${p.folderGuid}'),
            child: ListTile(
              leading: const Icon(Icons.link),
              title: Text('${p.deviceName} wants "${p.folderName}"'),
              subtitle: Text(p.deviceId),
              trailing: Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  TextButton(
                    onPressed: () async {
                      final message = await service.denyFolderPairing(
                          p.deviceId, p.folderGuid);
                      if (context.mounted) {
                        ScaffoldMessenger.of(context)
                          ..hideCurrentSnackBar()
                          ..showSnackBar(SnackBar(content: Text(message)));
                      }
                    },
                    child: const Text('Deny'),
                  ),
                  FilledButton(
                    onPressed: () => _approve(context, service, p),
                    child: const Text('Allow'),
                  ),
                ],
              ),
            ),
          ),
          const SizedBox(height: FerriTokens.spaceS),
        ],
      ],
    );
  }

  void _approve(
      BuildContext context, SyncService service, frb.PendingFolderPairing p) {
    // The owner's copy of the shared folder is where it shares from; use its
    // published local_path so the replica pair wires to the real folder.
    final share = service.mySharedFolders
        .where((s) => s.folderGuid == p.folderGuid)
        .firstOrNull;
    final localPath = share?.localPath ?? '';
    service
        .approveFolderPairing(
      deviceId: p.deviceId,
      folderGuid: p.folderGuid,
      folderName: p.folderName,
      localPath: localPath,
    )
        .then((message) {
      if (context.mounted) {
        ScaffoldMessenger.of(context)
          ..hideCurrentSnackBar()
          ..showSnackBar(SnackBar(content: Text(message)));
      }
    });
  }
}
