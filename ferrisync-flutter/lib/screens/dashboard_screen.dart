import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import '../gen/api.dart' as frb;
import '../models/sync_models.dart';
import '../providers/sync_provider.dart';
import '../theme/ferri_theme.dart';
import '../utils/relative_time.dart';
import '../widgets/empty_state.dart';
import '../widgets/sync_status_chip.dart';

/// The command center: overall sync state, quick stats, this device, paired
/// devices and a live-ish view of recent sync activity.
class DashboardScreen extends ConsumerWidget {
  const DashboardScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final service = ref.watch(syncServiceProvider);
    final devices = ref.watch(devicesProvider);
    final folders = ref.watch(foldersProvider);
    final status = ref.watch(syncStatusProvider);
    final sessions = service.sessions;
    final history = service.history;

    // One merged, newest-first timeline of sessions and per-file events.
    // Timestamps are unix seconds in both sources.
    final feed =
        <({frb.SessionEntry? session, frb.FileHistoryEntry? entry, int tsSec})>[
      for (final s in sessions) (session: s, entry: null, tsSec: s.ts),
      for (final h in history) (session: null, entry: h, tsSec: h.recordedAt),
    ]..sort((a, b) => b.tsSec.compareTo(a.tsSec));

    final onlineDevices = devices.where((d) => d.isOnline).length;
    final conflictCount = feed
        .where((e) => (e.session?.conflictsCount ?? BigInt.zero) > BigInt.zero)
        .length;
    final lastSync = feed.firstOrNull?.tsSec ??
        folders.fold<int>(
          0,
          (max, f) => f.lastSyncAt > max ? f.lastSyncAt : max,
        );

    return RefreshIndicator(
      onRefresh: service.refresh,
      child: ListView(
        physics: const AlwaysScrollableScrollPhysics(),
        padding: const EdgeInsets.all(FerriTokens.spaceL),
        children: [
          _HeroCard(status: status),
          if (conflictCount > 0) ...[
            const SizedBox(height: FerriTokens.spaceM),
            _ConflictBanner(count: conflictCount),
          ],
          const SizedBox(height: FerriTokens.spaceL),
          Row(
            children: [
              Expanded(
                child: _Stat(
                  label: 'Devices connected',
                  value: '$onlineDevices',
                ),
              ),
              const SizedBox(width: FerriTokens.spaceM),
              Expanded(
                child: _Stat(
                  label: 'Folders synced',
                  value: '${folders.length}',
                ),
              ),
              const SizedBox(width: FerriTokens.spaceM),
              Expanded(
                child: _Stat(
                  label: 'Last sync',
                  value: relativeTime(lastSync == 0 ? null : lastSync),
                ),
              ),
            ],
          ),
          const SizedBox(height: FerriTokens.spaceL),
          _ThisDeviceCard(service: service),
          const SizedBox(height: FerriTokens.spaceXL),
          SectionHeader(title: 'YOUR DEVICES (${devices.length})'),
          const SizedBox(height: FerriTokens.spaceS),
          if (devices.isEmpty)
            EmptyState(
              icon: Icons.hub_outlined,
              title: 'No devices paired',
              subtitle: 'Pair a device on your local network to start syncing folders.',
              action: FilledButton.icon(
                onPressed: () => context.go('/devices'),
                icon: const Icon(Icons.add_link),
                label: const Text('Pair a device'),
              ),
            )
          else
            Card(
              child: Column(
                children: [
                  for (final d in devices) _DeviceRow(device: d),
                ],
              ),
            ),
          const SizedBox(height: FerriTokens.spaceXL),
          const SectionHeader(title: 'SYNC ACTIVITY'),
          const SizedBox(height: FerriTokens.spaceS),
          if (feed.isEmpty)
            const EmptyState(
              icon: Icons.history,
              title: 'No syncs yet',
              subtitle: 'When a folder syncs, it shows up here.',
            )
          else
            Card(
              child: Column(
                children: [
                  for (final e in feed) ...[
                    if (e.session case final s?)
                      _SessionRow(session: s)
                    else if (e.entry case final h?)
                      _HistoryRow(entry: h),
                    if (e != feed.last) const Divider(height: 1, indent: 16, endIndent: 16),
                  ],
                ],
              ),
            ),
        ],
      ),
    );
  }
}

/// Attention strip for syncs that recorded conflicts.
class _ConflictBanner extends StatelessWidget {
  const _ConflictBanner({required this.count});

  final int count;

  @override
  Widget build(BuildContext context) {
    final palette = context.ferri;
    return Container(
      padding: const EdgeInsets.symmetric(
        horizontal: FerriTokens.spaceM,
        vertical: FerriTokens.spaceS,
      ),
      decoration: BoxDecoration(
        color: palette.danger.withValues(alpha: 0.10),
        borderRadius: BorderRadius.circular(FerriTokens.radiusM),
        border: Border.all(color: palette.danger.withValues(alpha: 0.35)),
      ),
      child: Row(
        children: [
          Icon(Icons.warning_amber_rounded, size: 20, color: palette.danger),
          const SizedBox(width: FerriTokens.spaceS),
          Expanded(
            child: Text(
              '$count sync session${count == 1 ? '' : 's'} had '
              'conflicts — review them in the activity feed',
              style: Theme.of(context)
                  .textTheme
                  .bodySmall!
                  .copyWith(color: palette.danger, fontWeight: FontWeight.w500),
            ),
          ),
        ],
      ),
    );
  }
}

/// Status hero: colored band, headline, subcopy and a status chip.
class _HeroCard extends StatelessWidget {
  const _HeroCard({required this.status});

  final SyncStatus status;

  @override
  Widget build(BuildContext context) {
    final palette = context.ferri;
    final textTheme = Theme.of(context).textTheme;
    final (icon, color, headline, subcopy, chipLabel) = switch (status) {
      SyncStatus.idle => (
          Icons.check_circle,
          palette.success,
          'Everything is in sync',
          'All folders are up to date',
          'In sync',
        ),
      SyncStatus.syncing => (
          Icons.sync,
          palette.syncing,
          'Syncing folders…',
          'Changes are flowing between your devices',
          'Syncing',
        ),
      SyncStatus.error => (
          Icons.error,
          palette.danger,
          'Needs attention',
          'A sync ran into a problem — pull to retry',
          'Needs attention',
        ),
    };

    return AnimatedContainer(
      duration: const Duration(milliseconds: 300),
      curve: Curves.easeOut,
      padding: const EdgeInsets.all(FerriTokens.spaceL),
      decoration: BoxDecoration(
        color: color.withValues(alpha: 0.10),
        borderRadius: BorderRadius.circular(FerriTokens.radiusL),
        border: Border.all(color: color.withValues(alpha: 0.30)),
      ),
      child: Row(
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
              child: Icon(icon, key: ValueKey(status), color: color, size: 28),
            ),
          ),
          const SizedBox(width: FerriTokens.spaceL),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  headline,
                  style: textTheme.titleLarge!.copyWith(
                    fontWeight: FontWeight.w700,
                    color: color,
                  ),
                ),
                const SizedBox(height: FerriTokens.spaceXS),
                Text(subcopy, style: textTheme.bodyMedium),
                const SizedBox(height: FerriTokens.spaceM),
                SyncStatusChip(status: status, label: chipLabel, compact: true),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

class _Stat extends StatelessWidget {
  const _Stat({required this.label, required this.value});

  final String label;
  final String value;

  @override
  Widget build(BuildContext context) {
    final textTheme = Theme.of(context).textTheme;
    return Card(
      child: Padding(
        padding: const EdgeInsets.symmetric(
          vertical: FerriTokens.spaceM,
          horizontal: FerriTokens.spaceS,
        ),
        child: Column(
          children: [
            Text(
              value,
              style: textTheme.titleMedium!.copyWith(fontWeight: FontWeight.w700),
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
            ),
            const SizedBox(height: FerriTokens.spaceXS),
            Text(
              label,
              style: textTheme.bodySmall!.copyWith(color: context.ferri.muted),
              textAlign: TextAlign.center,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
            ),
          ],
        ),
      ),
    );
  }
}

class _ThisDeviceCard extends StatelessWidget {
  const _ThisDeviceCard({required this.service});

  final SyncService service;

  @override
  Widget build(BuildContext context) {
    final palette = context.ferri;
    final textTheme = Theme.of(context).textTheme;
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(FerriTokens.spaceL),
        child: Row(
          children: [
            Icon(Icons.smartphone, color: palette.primary, size: 28),
            const SizedBox(width: FerriTokens.spaceM),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    'THIS DEVICE',
                    style: textTheme.labelSmall!.copyWith(
                      letterSpacing: 1.1,
                      fontWeight: FontWeight.w700,
                      color: palette.muted,
                    ),
                  ),
                  const SizedBox(height: FerriTokens.spaceXS),
                  Text(service.deviceName, style: textTheme.titleMedium),
                  const SizedBox(height: 2),
                  Text(
                    service.deviceId,
                    style: textTheme.bodySmall!.copyWith(
                      color: palette.muted,
                      fontFeatures: const [FontFeature.tabularFigures()],
                    ),
                  ),
                ],
              ),
            ),
            IconButton(
              tooltip: 'Copy device ID',
              icon: Icon(Icons.copy, size: 18, color: palette.muted),
              onPressed: () async {
                await Clipboard.setData(ClipboardData(text: service.deviceId));
                if (context.mounted) {
                  ScaffoldMessenger.of(context)
                    ..hideCurrentSnackBar()
                    ..showSnackBar(
                      const SnackBar(content: Text('Device ID copied')),
                    );
                }
              },
            ),
          ],
        ),
      ),
    );
  }
}

class _DeviceRow extends StatelessWidget {
  const _DeviceRow({required this.device});

  final Device device;

  @override
  Widget build(BuildContext context) {
    final palette = context.ferri;
    final textTheme = Theme.of(context).textTheme;
    return ListTile(
      dense: true,
      leading: _OnlineDot(online: device.isOnline),
      title: Text(device.name),
      subtitle: Text(
        device.isOnline
            ? 'Online'
            : 'Last seen ${device.lastSeenFormatted}',
        style: textTheme.bodySmall!.copyWith(color: palette.muted),
      ),
      trailing: const Icon(Icons.chevron_right, size: 18, color: Colors.grey),
      onTap: () => context.go('/devices'),
    );
  }
}

class _OnlineDot extends StatelessWidget {
  const _OnlineDot({required this.online});

  final bool online;

  @override
  Widget build(BuildContext context) {
    final color = online ? context.ferri.success : context.ferri.muted;
    return SizedBox(
      width: 20,
      height: 20,
      child: Center(
        child: Container(
          width: 10,
          height: 10,
          decoration: BoxDecoration(
            color: color,
            shape: BoxShape.circle,
            boxShadow: online
                ? [
                    BoxShadow(
                      color: color.withValues(alpha: 0.5),
                      blurRadius: 6,
                    ),
                  ]
                : null,
          ),
        ),
      ),
    );
  }
}

class _SessionRow extends StatelessWidget {
  const _SessionRow({required this.session});

  final frb.SessionEntry session;

  @override
  Widget build(BuildContext context) {
    final palette = context.ferri;
    final textTheme = Theme.of(context).textTheme;
    final isPush = session.direction != 'pull';
    final folderName = session.folderPath.split('/').where((s) => s.isNotEmpty).lastOrNull ?? '';
    final hadConflicts = session.conflictsCount > BigInt.zero;

    return ListTile(
      dense: true,
      leading: Icon(
        isPush ? Icons.arrow_upward : Icons.arrow_downward,
        size: 18,
        color: isPush ? palette.primary : palette.syncing,
      ),
      title: Text(folderName, style: textTheme.bodyLarge),
      subtitle: Text(
        '${session.peerDevice} · ${relativeTime(session.ts)}'
        '${hadConflicts ? ' · conflict!' : ''}',
        style: textTheme.bodySmall!.copyWith(color: palette.muted),
      ),
      trailing: Text(
        '↑ ${session.pushedCount} ↓ ${session.pulledCount}',
        style: textTheme.bodySmall!.copyWith(
          color: hadConflicts ? palette.danger : palette.muted,
          fontFeatures: const [FontFeature.tabularFigures()],
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

    final (icon, color) = switch ((entry.deviceId)) {
      _ when action.contains('conflict') =>
        (Icons.warning_amber_rounded, palette.danger),
      _ when action.contains('pull') =>
        (Icons.arrow_downward, palette.syncing),
      _ when action.contains('push') => (Icons.arrow_upward, palette.primary),
      _ when action.contains('delete') => (Icons.delete_outline, palette.muted),
      _ => (Icons.drive_file_move_outline, palette.muted),
    };

    return ListTile(
      dense: true,
      leading: Icon(icon, size: 18, color: color),
      title: Text(
        entry.path.split('/').where((s) => s.isNotEmpty).lastOrNull ?? entry.path,
        style: textTheme.bodyLarge,
        overflow: TextOverflow.ellipsis,
      ),
      subtitle: Text(
        '${entry.action} · ${relativeTime(entry.recordedAt)}',
        style: textTheme.bodySmall!.copyWith(color: palette.muted),
        overflow: TextOverflow.ellipsis,
      ),
      trailing: entry.size != null
          ? Text(
              formatSize(entry.size!),
              style: textTheme.bodySmall!.copyWith(color: palette.muted),
            )
          : null,
    );
  }

  static String formatSize(int bytes) {
    if (bytes < 1024) return '$bytes B';
    const units = ['KB', 'MB', 'GB', 'TB'];
    var value = bytes.toDouble();
    var unit = -1;
    while (value >= 1024 && unit < units.length - 1) {
      value /= 1024;
      unit++;
    }
    return '${value.toStringAsFixed(value >= 100 ? 0 : 1)} ${units[unit]}';
  }
}