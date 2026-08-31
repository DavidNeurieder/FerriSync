import 'package:flutter/material.dart';
import 'package:go_router/go_router.dart';
import '../../models/sync_models.dart';
import '../../providers/sync_provider.dart';
import '../../theme/ferri_theme.dart';
import '../../utils/format_bytes.dart';
import '../../utils/humanize_error.dart';
import '../../utils/relative_time.dart';
import '../sync_status_chip.dart';

/// Status hero: colored band, headline, subcopy, live progress while syncing,
/// one obvious primary action, and a status chip.
class StatusHero extends StatelessWidget {
  const StatusHero({super.key, required this.data, required this.mode});

  final HeroViewData data;
  final SyncStatus mode;

  @override
  Widget build(BuildContext context) {
    final textTheme = Theme.of(context).textTheme;
    final color = data.color;

    return AnimatedContainer(
      duration: const Duration(milliseconds: 300),
      curve: Curves.easeOut,
      padding: const EdgeInsets.all(FerriTokens.spaceL),
      decoration: BoxDecoration(
        color: color.withValues(alpha: 0.10),
        borderRadius: BorderRadius.circular(FerriTokens.radiusL),
        border: Border.all(color: color.withValues(alpha: 0.30)),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              AnimatedContainer(
                duration: const Duration(milliseconds: 300),
                padding: const EdgeInsets.all(FerriTokens.spaceM),
                decoration: BoxDecoration(
                  color: color.withValues(alpha: 0.18),
                  shape: BoxShape.circle,
                ),
                child: AnimatedSwitcher(
                  duration: const Duration(milliseconds: 300),
                  child: Icon(data.icon,
                      key: ValueKey(data.headline), color: color, size: 28),
                ),
              ),
              const SizedBox(width: FerriTokens.spaceL),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      data.headline,
                      style: textTheme.titleLarge!.copyWith(
                        fontWeight: FontWeight.w700,
                        color: color,
                      ),
                    ),
                    const SizedBox(height: FerriTokens.spaceXS),
                    Text(data.subcopy, style: textTheme.bodyMedium),
                  ],
                ),
              ),
            ],
          ),
          if (data.showProgress) ...[
            const SizedBox(height: FerriTokens.spaceL),
            LinearProgressIndicator(value: data.progressValue),
          ],
          const SizedBox(height: FerriTokens.spaceM),
          Row(
            children: [
              SyncStatusChip(status: mode, label: data.chipLabel, compact: true),
              if (data.actionLabel != null) ...[
                const SizedBox(width: FerriTokens.spaceM),
                FilledButton.icon(
                  onPressed: data.onAction,
                  icon: const Icon(Icons.arrow_forward, size: 16),
                  label: Text(data.actionLabel!),
                ),
              ],
            ],
          ),
        ],
      ),
    );
  }
}

typedef HeroViewData = ({
  IconData icon,
  Color color,
  String headline,
  String subcopy,
  String chipLabel,
  String? actionLabel,
  VoidCallback? onAction,
  bool showProgress,
  double? progressValue,
});

/// "624 / 800 files · 78% · 342 MB remaining" from the live transfer plan.
String syncProgressCopy(SyncService service) {
  final done = service.syncFilesDone;
  final total = service.syncFilesTotal;
  final pct = service.syncProgressValue == null
      ? null
      : (service.syncProgressValue! * 100).round();
  final remaining =
      (service.syncBytesTotal - service.syncBytesDone).clamp(0, service.syncBytesTotal);
  return [
    '$done / $total files',
    if (pct != null) '$pct%',
    if (service.syncBytesTotal > 0 && remaining > 0) '${formatBytes(remaining)} remaining',
  ].join(' · ');
}

/// Derive the hero's look-and-feel from the app's shared state.
HeroViewData buildHeroView(
  BuildContext context, {
  required SyncStatus status,
  required SyncService service,
  required int conflictCount,
  required List<Device> offlineDevices,
  required int lastSync,
  required bool hasFolders,
}) {
  final palette = context.ferri;
  if (status == SyncStatus.syncing) {
    final label = service.syncingFolderLabel;
    return (
      icon: Icons.sync,
      color: palette.syncing,
      headline: label == null ? 'Syncing folders…' : 'Syncing $label',
      subcopy: service.hasLiveProgress
          ? syncProgressCopy(service)
          : 'Preparing files…',
      chipLabel: 'Syncing',
      actionLabel: null,
      onAction: null,
      showProgress: true,
      progressValue: service.syncProgressValue,
    );
  }
  if (status == SyncStatus.error) {
    final why = humanizeError(
      service.lastErrorMessage,
      fallback: 'The last sync stopped before it finished.',
    );
    return (
      icon: Icons.error,
      color: palette.danger,
      headline: 'Needs attention',
      subcopy: why,
      chipLabel: 'Needs attention',
      actionLabel: 'Try again',
      onAction: service.refresh,
      showProgress: false,
      progressValue: null,
    );
  }
  if (conflictCount > 0) {
    return (
      icon: Icons.warning_amber_rounded,
      color: palette.danger,
      headline: 'Needs your attention',
      subcopy: conflictCount == 1
          ? '1 file has a conflict.'
          : '$conflictCount files have conflicts.',
      chipLabel: 'Attention',
      actionLabel: 'Review conflicts',
      onAction: () => context.go('/conflicts'),
      showProgress: false,
      progressValue: null,
    );
  }
  if (offlineDevices.isNotEmpty) {
    final d = offlineDevices.first;
    return (
      icon: Icons.cloud_off,
      color: palette.muted,
      headline: '${d.name} is offline',
      subcopy:
          'Your files are safe. Sync resumes automatically when it reconnects.',
      chipLabel: 'Offline',
      actionLabel: null,
      onAction: null,
      showProgress: false,
      progressValue: null,
    );
  }
  return (
    icon: Icons.check_circle,
    color: palette.success,
    headline: 'Everything is in sync',
    subcopy: hasFolders
        ? lastSync > 0
            ? 'Last sync ${relativeTime(lastSync)}'
            : 'Up to date'
        : 'Sync a folder to keep your files in lockstep',
    chipLabel: 'In sync',
    actionLabel: hasFolders ? null : 'Add a folder',
    onAction: hasFolders ? null : () => context.go('/folders'),
    showProgress: false,
    progressValue: null,
  );
}