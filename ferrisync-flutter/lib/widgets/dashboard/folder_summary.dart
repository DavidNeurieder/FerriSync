import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import '../../models/sync_models.dart';
import '../../providers/sync_provider.dart';
import '../../screens/folder_detail_screen.dart';
import '../../theme/ferri_theme.dart';
import '../../utils/relative_time.dart';
import '../empty_state.dart';

/// Dashboard's FOLDERS summary: one row per shared folder with its shared
/// health state as a chip, tapping through to the folder detail screen.
class FolderSummarySection extends ConsumerWidget {
  const FolderSummarySection({super.key, required this.folders});

  final List<SyncFolder> folders;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final devices = ref.watch(devicesProvider);
    final myName = ref.watch(deviceNameProvider);
    final deviceById = {for (final d in devices) d.id: d};

    if (folders.isEmpty) {
      return const EmptyState(
        icon: Icons.folder_shared_outlined,
        title: 'No folders shared yet',
        subtitle: 'Share a folder to keep it in sync across devices.',
      );
    }

    return Column(
      children: [
        for (final f in folders) ...[
          _FolderSummaryRow(
            folder: f,
            relationshipLabel:
                '$myName ↔ ${deviceById[f.deviceId]?.name ?? f.deviceName ?? f.deviceId}',
          ),
          const Divider(height: 1, indent: 16, endIndent: 16),
        ],
      ],
    );
  }
}

class _FolderSummaryRow extends ConsumerWidget {
  const _FolderSummaryRow({
    required this.folder,
    required this.relationshipLabel,
  });

  final SyncFolder folder;
  final String relationshipLabel;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final palette = context.ferri;
    final textTheme = Theme.of(context).textTheme;
    final label = folder.localPath
        .split(RegExp(r'[/\\]'))
        .where((s) => s.isNotEmpty)
        .last;

    final (icon, color, chipLabel) = _healthLook(folder.health, palette);

    return ListTile(
      onTap: () {
        Navigator.of(context).push(
          MaterialPageRoute<void>(
            builder: (_) => FolderDetailScreen(folder: folder),
          ),
        );
      },
      leading: Container(
        width: 36,
        height: 36,
        decoration: BoxDecoration(
          color: color.withValues(alpha: 0.12),
          borderRadius: BorderRadius.circular(FerriTokens.radiusS),
        ),
        child: Icon(icon, size: 18, color: color),
      ),
      title: Text(label, style: textTheme.bodyLarge),
      subtitle: Text(
        relationshipLabel,
        style: textTheme.bodySmall!.copyWith(color: palette.muted),
        overflow: TextOverflow.ellipsis,
      ),
      trailing: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          if (folder.lastSyncAt > 0)
            Padding(
              padding: const EdgeInsets.only(right: FerriTokens.spaceS),
              child: Text(
                relativeTime(folder.lastSyncAt),
                style: textTheme.bodySmall!.copyWith(color: palette.muted),
              ),
            ),
          _HealthChip(label: chipLabel, color: color),
        ],
      ),
    );
  }

  (IconData, Color, String) _healthLook(
          FolderHealth h, FerriPalette palette) =>
      switch (h) {
        FolderHealth.healthy => (
            Icons.check_circle,
            palette.success,
            'Synced'
          ),
        FolderHealth.syncing => (Icons.sync, palette.syncing, 'Syncing'),
        FolderHealth.waiting => (
            Icons.hourglass_top,
            palette.warning,
            'Waiting'
          ),
        FolderHealth.offline => (Icons.cloud_off, palette.muted, 'Offline'),
        FolderHealth.error => (Icons.error, palette.danger, 'Error'),
        FolderHealth.conflict => (
            Icons.warning_amber_rounded,
            palette.danger,
            'Conflict'
          ),
        FolderHealth.notConfigured => (
            Icons.folder_shared_outlined,
            palette.muted,
            'Unconfigured'
          ),
      };
}

class _HealthChip extends StatelessWidget {
  const _HealthChip({required this.label, required this.color});

  final String label;
  final Color color;

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.symmetric(
        horizontal: FerriTokens.spaceS,
        vertical: 4,
      ),
      decoration: BoxDecoration(
        color: color.withValues(alpha: 0.12),
        borderRadius: BorderRadius.circular(999),
        border: Border.all(color: color.withValues(alpha: 0.35)),
      ),
      child: Text(
        label,
        style: Theme.of(context).textTheme.bodySmall!.copyWith(
              color: color,
              fontWeight: FontWeight.w600,
            ),
      ),
    );
  }
}