import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import '../gen/sync_engine/conflicts.dart' as frb_conflicts;
import '../providers/sync_provider.dart';
import '../theme/ferri_theme.dart';
import '../utils/format_bytes.dart';
import '../utils/relative_time.dart';
import '../widgets/conflicts/conflict_compare_view.dart';
import '../widgets/empty_state.dart';

/// Conflict resolution: discoverable file-by-file, without exposing the
/// engine's backup naming. One clear "which version do you want?" per file.
class ConflictsScreen extends ConsumerWidget {
  const ConflictsScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final service = ref.watch(syncServiceProvider);
    final conflicts = service.conflicts;
    final folders = service.folders;
    final devices = service.devices;
    final thisDeviceName = service.deviceName;

    String? folderName(int id) {
      for (final f in folders) {
        if (f.id == id) {
          return f.localPath
              .split(RegExp(r'[/\\]'))
              .where((s) => s.isNotEmpty)
              .last;
        }
      }
      return null;
    }

    String? peerName(int folderId) {
      for (final f in folders) {
        if (f.id == folderId) {
          for (final d in devices) {
            if (d.id == f.deviceId) return d.name;
          }
        }
      }
      return null;
    }

    return Scaffold(
      body: RefreshIndicator(
        onRefresh: service.refresh,
        child: ListView(
          physics: const AlwaysScrollableScrollPhysics(),
          padding: const EdgeInsets.all(FerriTokens.spaceL),
          children: [
            SectionHeader(title: 'FILE CONFLICTS (${conflicts.length})'),
            const SizedBox(height: FerriTokens.spaceS),
            if (conflicts.isEmpty)
              const EmptyState(
                icon: Icons.check_circle_outline,
                title: 'No conflicts',
                subtitle: 'When two devices edit the same file, both versions '
                    'are kept until you pick one.',
              )
            else
              Card(
                child: Column(
                  children: [
                    for (final c in conflicts) ...[
                      _ConflictTile(
                        conflict: c,
                        folderLabel: folderName(c.folderId.toInt()),
                        onTap: () => _review(
                          context,
                          service,
                          c,
                          folderName: folderName(c.folderId.toInt()),
                          peerName: peerName(c.folderId.toInt()),
                          thisDeviceName: thisDeviceName,
                        ),
                      ),
                      if (c != conflicts.last)
                        const Divider(height: 1, indent: 16, endIndent: 16),
                    ],
                  ],
                ),
              ),
            const SizedBox(height: FerriTokens.spaceL),
            Text(
              'Every conflict keeps both versions on disk — nothing is '
              'deleted until you choose.',
              style: Theme.of(context)
                  .textTheme
                  .bodySmall!
                  .copyWith(color: context.ferri.muted),
            ),
          ],
        ),
      ),
    );
  }

  void _review(
    BuildContext context,
    SyncService service,
    frb_conflicts.ConflictEntry conflict, {
    required String? folderName,
    required String? peerName,
    required String thisDeviceName,
  }) {
    showModalBottomSheet<void>(
      context: context,
      showDragHandle: true,
      isScrollControlled: true,
      builder: (ctx) => _CompareVariantSheet(
        thisDeviceName: thisDeviceName,
        peerName: peerName,
        folderName: folderName,
        conflict: conflict,
        onCompare: () => _compare(
          ctx,
          service,
          folderId: conflict.folderId.toInt(),
          winnerPath: conflict.path,
          loserPath: conflict.backupPath,
          fileBase: conflict.path
                  .split('/')
                  .where((s) => s.isNotEmpty)
                  .lastOrNull ??
              conflict.path,
          winnerName: peerName ?? 'the other device',
          loserName: thisDeviceName.isEmpty ? 'This device' : thisDeviceName,
        ),
        onResolve: (action) =>
            service.resolveConflict(conflict.folderId.toInt(), conflict.backupPath, action),
      ),
    );
  }

  /// Load both conflict versions and present the text diff in a full-screen
  /// modal. Falls back to the metadata-only sheet when the engine is not
  /// ready or the file is not textual (binary / non-UTF-8).
  Future<void> _compare(
    BuildContext context,
    SyncService service, {
    required int folderId,
    required String winnerPath,
    required String loserPath,
    required String fileBase,
    required String winnerName,
    required String loserName,
  }) async {
    final contents = await service.readConflictContents(
      folderId,
      winnerPath,
      loserPath,
    );
    if (!context.mounted) return;
    if (contents == null || !contents.textual) {
      _showNotTextual(context, fileBase);
      return;
    }
    await showModalBottomSheet<void>(
      context: context,
      showDragHandle: true,
      isScrollControlled: true,
      builder: (ctx) => DraggableScrollableSheet(
        expand: false,
        initialChildSize: 0.9,
        maxChildSize: 0.95,
        builder: (ctx, scrollController) => SingleChildScrollView(
          controller: scrollController,
          padding: const EdgeInsets.fromLTRB(24, 4, 24, 24),
          child: ConflictCompareView(
            contents: contents,
            fileName: fileBase,
            winnerLabel: winnerName,
            loserLabel: loserName,
          ),
        ),
      ),
    );
  }

  void _showNotTextual(BuildContext context, String fileBase) {
    final palette = context.ferri;
    final textTheme = Theme.of(context).textTheme;
    showModalBottomSheet<void>(
      context: context,
      showDragHandle: true,
      builder: (ctx) => SafeArea(
        child: Padding(
          padding: const EdgeInsets.all(FerriTokens.spaceL),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Icon(Icons.description_outlined,
                  size: 40, color: palette.muted),
              const SizedBox(height: FerriTokens.spaceM),
              Text('$fileBase isn\'t plain text',
                  style: textTheme.titleMedium),
              const SizedBox(height: FerriTokens.spaceS),
              Text(
                'This file can\'t be previewed. Compare the sizes and '
                'modification times, then choose a version.',
                textAlign: TextAlign.center,
                style: textTheme.bodyMedium!.copyWith(color: palette.muted),
              ),
              const SizedBox(height: FerriTokens.spaceL),
              FilledButton(
                onPressed: () => Navigator.of(ctx).pop(),
                child: const Text('Got it'),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class _ConflictTile extends StatelessWidget {
  const _ConflictTile({
    required this.conflict,
    required this.folderLabel,
    required this.onTap,
  });

  final frb_conflicts.ConflictEntry conflict;
  final String? folderLabel;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final palette = context.ferri;
    final textTheme = Theme.of(context).textTheme;
    final name =
        conflict.path.split('/').where((s) => s.isNotEmpty).lastOrNull ?? conflict.path;
    final label = folderLabel == null
        ? 'in a sync folder'
        : 'in $folderLabel';
    return ListTile(
      dense: true,
      onTap: onTap,
      leading: Icon(Icons.warning_amber_rounded, size: 22, color: palette.danger),
      title: Text(name, style: textTheme.bodyLarge, overflow: TextOverflow.ellipsis),
      subtitle: Text(
        label,
        style: textTheme.bodySmall!.copyWith(color: palette.muted),
        overflow: TextOverflow.ellipsis,
      ),
      trailing: Text(
        formatBytes(conflict.winnerSize.toInt()),
        style: textTheme.bodySmall!.copyWith(
          color: palette.muted,
          fontFeatures: const [FontFeature.tabularFigures()],
        ),
      ),
    );
  }
}

class _CompareVariantSheet extends ConsumerWidget {
  const _CompareVariantSheet({
    required this.thisDeviceName,
    required this.peerName,
    required this.folderName,
    required this.conflict,
    required this.onCompare,
    required this.onResolve,
  });

  final String thisDeviceName;
  final String? peerName;
  final String? folderName;
  final frb_conflicts.ConflictEntry conflict;
  final VoidCallback onCompare;
  final Future<String> Function(String action) onResolve;

  String get _fileBase =>
      conflict.path.split('/').where((s) => s.isNotEmpty).lastOrNull ?? conflict.path;

  /// The real file holds the winner version; in practice that is the peer's
  /// (its push overwrote our older copy, which lives in the backup).
  String get _winnerDevice => peerName ?? 'the other device';

  String get _loserDevice => thisDeviceName.isEmpty ? 'This device' : thisDeviceName;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final palette = context.ferri;
    final textTheme = Theme.of(context).textTheme;
    final width = MediaQuery.of(context).size.width;

    return SafeArea(
      child: SingleChildScrollView(
        padding: const EdgeInsets.fromLTRB(24, 4, 24, 24),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(_fileBase, style: textTheme.titleLarge),
            const SizedBox(height: 2),
            Text(
              'Two versions exist. Pick which one to keep.',
              style: textTheme.bodyMedium!.copyWith(color: palette.muted),
            ),
            if (folderName != null) ...[
              const SizedBox(height: 2),
              Text(
                'in $folderName',
                style: textTheme.bodySmall!.copyWith(color: palette.muted),
              ),
            ],
            const SizedBox(height: FerriTokens.spaceL),
            _VariantCard(
              icon: Icons.dns_outlined,
              color: palette.primary,
              deviceLabel: _winnerDevice,
              owner: 'On $_winnerDevice',
              mtime: conflict.winnerMtimeSecs.toInt(),
              size: formatBytes(conflict.winnerSize.toInt()),
              width: width,
            ),
            const SizedBox(height: FerriTokens.spaceS),
            _VariantCard(
              icon: Icons.smartphone_outlined,
              color: palette.syncing,
              deviceLabel: _loserDevice,
              owner: 'On $_loserDevice',
              mtime: conflict.loserMtimeSecs.toInt(),
              size: formatBytes(conflict.loserSize.toInt()),
              width: width,
            ),
            const SizedBox(height: FerriTokens.spaceL),
            OutlinedButton.icon(
              key: const ValueKey('compare_versions'),
              onPressed: onCompare,
              style: OutlinedButton.styleFrom(minimumSize: Size(width - 48, 48)),
              icon: const Icon(Icons.difference_outlined, size: 18),
              label: const Text('Compare versions'),
            ),
            const SizedBox(height: FerriTokens.spaceL),
            FilledButton.icon(
              key: const ValueKey('keep_winner'),
              onPressed: () => _run(context, 'keep_original', confirmation: 'Kept $_winnerDevice'),
              style: FilledButton.styleFrom(
                minimumSize: Size(width - 48, 48),
                backgroundColor: palette.primary,
              ),
              icon: const Icon(Icons.check_circle_outline, size: 18),
              label: Text('Keep $_winnerDevice'),
            ),
            const SizedBox(height: FerriTokens.spaceS),
            OutlinedButton.icon(
              key: const ValueKey('keep_loser'),
              onPressed: () => _run(context, 'keep_backup', confirmation: 'Kept $_loserDevice'),
              style: OutlinedButton.styleFrom(minimumSize: Size(width - 48, 48)),
              icon: const Icon(Icons.smartphone, size: 18),
              label: Text('Keep $_loserDevice'),
            ),
            const SizedBox(height: FerriTokens.spaceS),
            OutlinedButton.icon(
              key: const ValueKey('keep_both'),
              onPressed: () =>
                  _run(context, 'keep_both', confirmation: 'Kept both versions on this device'),
              style: OutlinedButton.styleFrom(minimumSize: Size(width - 48, 48)),
              icon: const Icon(Icons.layers_outlined, size: 18),
              label: const Text('Keep both'),
            ),
          ],
        ),
      ),
    );
  }

  Future<void> _run(
      BuildContext context, String action, {required String confirmation}) async {
    final messenger = ScaffoldMessenger.of(context);
    // Overwriting the current file is destructive to that version, so ask
    // once before proceeding. "Keep both" never removes anything.
    if (action != 'keep_both') {
      final version = action == 'keep_backup' ? _loserDevice : _winnerDevice;
      final other = action == 'keep_backup' ? _winnerDevice : _loserDevice;
      final ok = await showDialog<bool>(
        context: context,
        builder: (ctx) => AlertDialog(
          title: Text('Keep $version?'),
          content: Text(
            '$other\'s version will be removed.\n\n'
            'This can\'t be undone.',
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(ctx, false),
              child: const Text('Cancel'),
            ),
            FilledButton(
              onPressed: () => Navigator.pop(ctx, true),
              child: Text('Keep $version'),
            ),
          ],
        ),
      );
      if (ok != true) return;
    }
    await onResolve(action);
    if (context.mounted) Navigator.of(context).pop();
    messenger.showSnackBar(SnackBar(content: Text(confirmation)));
  }
}

class _VariantCard extends StatelessWidget {
  const _VariantCard({
    required this.icon,
    required this.color,
    required this.deviceLabel,
    required this.owner,
    required this.mtime,
    required this.size,
    required this.width,
  });

  final IconData icon;
  final Color color;
  final String deviceLabel;
  final String owner;
  final int mtime;
  final String size;
  final double width;

  @override
  Widget build(BuildContext context) {
    final textTheme = Theme.of(context).textTheme;
    final palette = context.ferri;
    return Container(
      width: width - 48,
      padding: const EdgeInsets.all(FerriTokens.spaceM),
      decoration: BoxDecoration(
        color: palette.surfaceHigh,
        borderRadius: BorderRadius.circular(FerriTokens.radiusM),
      ),
      child: Row(
        children: [
          Container(
            width: 40,
            height: 40,
            decoration: BoxDecoration(
              color: color.withValues(alpha: 0.14),
              borderRadius: BorderRadius.circular(FerriTokens.radiusS),
            ),
            child: Icon(icon, color: color, size: 20),
          ),
          const SizedBox(width: FerriTokens.spaceM),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  'Modified ${verboseTime(mtime)} · $size',
                  style: textTheme.bodyMedium!.copyWith(color: palette.muted),
                ),
                const SizedBox(height: 2),
                Text(
                  deviceLabel,
                  style: textTheme.titleSmall!.copyWith(fontWeight: FontWeight.w600),
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}