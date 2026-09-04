import 'package:flutter/material.dart';
import 'package:go_router/go_router.dart';
import '../../models/sync_models.dart';
import '../../theme/ferri_theme.dart';

/// The plan's "click the status line → health overview": a lightweight summary
/// of the whole device ecosystem. This is KDE-Connect-style: the app tells you
/// how your devices + folders are doing without digging into screens. Each
/// row deep-links to the relevant place.
void showHealthOverview(
  BuildContext context, {
  required List<Device> devices,
  required List<SyncFolder> folders,
  required int conflictCount,
}) {
  final connected = devices.where((d) => d.isOnline).length;
  final problems = folders.where((f) => f.health.needsAttention).length;
  showModalBottomSheet<void>(
    context: context,
    showDragHandle: true,
    builder: (ctx) {
      final palette = context.ferri;
      final textTheme = Theme.of(ctx).textTheme;
      final healthy = folders.isNotEmpty &&
          conflictCount == 0 &&
          problems == 0 &&
          folders.every((f) => f.health == FolderHealth.healthy);
      final headline = conflictCount > 0
          ? '$conflictCount conflict'
              '${conflictCount == 1 ? '' : 's'} need attention'
          : problems > 0
              ? 'Some folders need attention'
              : connected == 0
                  ? 'No devices connected'
                  : 'Everything is up to date';

      return SafeArea(
        child: Padding(
          padding: const EdgeInsets.fromLTRB(24, 4, 24, 24),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Row(
                children: [
                  Icon(
                    healthy ? Icons.check_circle : Icons.info_outline,
                    color: healthy ? palette.success : palette.warning,
                    size: 22,
                  ),
                  const SizedBox(width: FerriTokens.spaceM),
                  Expanded(
                    child: Text(
                      headline,
                      style: textTheme.titleLarge!
                          .copyWith(fontWeight: FontWeight.w700),
                    ),
                  ),
                ],
              ),
              const SizedBox(height: FerriTokens.spaceL),
              _OverviewRow(
                label: 'Devices connected',
                value: '$connected / ${devices.length}',
                icon: Icons.devices,
                onTap: () {
                  Navigator.of(ctx).pop();
                  context.go('/devices');
                },
              ),
              _OverviewRow(
                label: 'Folders with issues',
                value: '$problems / ${folders.length}',
                icon: Icons.folder_outlined,
                onTap: () {
                  Navigator.of(ctx).pop();
                  context.go('/folders');
                },
              ),
              if (conflictCount > 0)
                _OverviewRow(
                  label: 'Conflicts',
                  value: '$conflictCount',
                  icon: Icons.warning_amber_rounded,
                  iconColor: palette.danger,
                  onTap: () {
                    Navigator.of(ctx).pop();
                    context.go('/conflicts');
                  },
                ),
            ],
          ),
        ),
      );
    },
  );
}

class _OverviewRow extends StatelessWidget {
  const _OverviewRow({
    required this.label,
    required this.value,
    required this.icon,
    this.iconColor,
    this.onTap,
  });

  final String label;
  final String value;
  final IconData icon;
  final Color? iconColor;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final palette = context.ferri;
    final textTheme = Theme.of(context).textTheme;
    return ListTile(
      contentPadding: EdgeInsets.zero,
      leading: Icon(icon, size: 20, color: iconColor ?? palette.muted),
      title: Text(label, style: textTheme.bodyMedium),
      trailing: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Text(
            value,
            style: textTheme.bodyMedium!.copyWith(fontWeight: FontWeight.w600),
          ),
          const SizedBox(width: FerriTokens.spaceS),
          const Icon(Icons.chevron_right, size: 18, color: Colors.grey),
        ],
      ),
      onTap: onTap,
    );
  }
}
