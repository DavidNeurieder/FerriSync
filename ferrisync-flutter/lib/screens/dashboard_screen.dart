import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import '../gen/api.dart' as frb;
import '../models/sync_models.dart';
import '../providers/sync_provider.dart';
import '../theme/ferri_theme.dart';
import '../utils/format_bytes.dart';
import '../utils/humanize_error.dart';
import '../utils/relative_time.dart';
import '../widgets/empty_state.dart';
import '../widgets/sync_status_chip.dart';

/// The command center: "is everything OK?" answer, quick stats, attention
/// items, this device, paired devices and a live-ish view of recent activity.
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
    final offlineDevices =
        devices.where((d) => !d.isOnline && d.lastSeen > 0).toList();
    final conflictCount = service.recentConflicts;
    final lastSync = feed.firstOrNull?.tsSec ??
        folders.fold<int>(
          0,
          (max, f) => f.lastSyncAt > max ? f.lastSyncAt : max,
        );

    final hero = _heroView(
      context,
      status: status,
      service: service,
      conflictCount: conflictCount,
      offlineDevices: offlineDevices,
      lastSync: lastSync,
      hasFolders: folders.isNotEmpty,
    );
    final attention = service.attentionItems;

    return RefreshIndicator(
      onRefresh: service.refresh,
      child: ListView(
        physics: const AlwaysScrollableScrollPhysics(),
        padding: const EdgeInsets.all(FerriTokens.spaceL),
        children: [
          _HeroCard(status: status, data: hero),
          if (attention.isNotEmpty) ...[
            const SizedBox(height: FerriTokens.spaceM),
            _AttentionPanel(items: attention),
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
                      _SessionRow(session: s, onTap: () => _showSessionDetail(context, s))
                    else if (e.entry case final h?)
                      _HistoryRow(entry: h, onTap: () => _showHistoryDetail(context, h)),
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

/// Unified "what needs me?" panel. Only shown when something genuinely needs
/// attention, so a healthy app never interrupts.
class _AttentionPanel extends StatelessWidget {
  const _AttentionPanel({required this.items});

  final List<AttentionItem> items;

  @override
  Widget build(BuildContext context) {
    final palette = context.ferri;
    final textTheme = Theme.of(context).textTheme;
    return Card(
      margin: EdgeInsets.zero,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(
              FerriTokens.spaceL,
              FerriTokens.spaceM,
              FerriTokens.spaceL,
              FerriTokens.spaceS,
            ),
            child: Text(
              'NEEDS ATTENTION',
              style: textTheme.labelSmall!.copyWith(
                letterSpacing: 1.1,
                fontWeight: FontWeight.w700,
                color: palette.danger,
              ),
            ),
          ),
          for (final item in items)
            ListTile(
              dense: true,
              leading: Icon(
                item.kind == AttentionKind.conflictFiles
                    ? Icons.warning_amber_rounded
                    : Icons.cloud_off,
                size: 20,
                color: item.kind == AttentionKind.conflictFiles
                    ? palette.danger
                    : palette.muted,
              ),
              title: Text(item.label, style: textTheme.bodyMedium),
              trailing: const Icon(Icons.chevron_right,
                  size: 18, color: Colors.grey),
              onTap: () => switch (item.kind) {
                AttentionKind.conflictFiles => context.go('/folders'),
                AttentionKind.offlineDevice => context.go('/devices'),
              },
            ),
        ],
      ),
    );
  }
}

/// Status hero: colored band, headline, subcopy, live progress while syncing,
/// one obvious primary action, and a status chip.
class _HeroCard extends StatelessWidget {
  const _HeroCard({required this.status, required this.data});

  final SyncStatus status;
  final _HeroView data;

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
                  child: Icon(data.icon, key: ValueKey(data.headline), color: color, size: 28),
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
            const LinearProgressIndicator(),
          ],
          const SizedBox(height: FerriTokens.spaceM),
          Row(
            children: [
              SyncStatusChip(status: status, label: data.chipLabel, compact: true),
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

typedef _HeroView = ({
  IconData icon,
  Color color,
  String headline,
  String subcopy,
  String chipLabel,
  String? actionLabel,
  VoidCallback? onAction,
  bool showProgress,
});

_HeroView _heroView(
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
    final done = service.syncedFilesNow;
    return (
      icon: Icons.sync,
      color: palette.syncing,
      headline: label == null ? 'Syncing folders…' : 'Syncing $label',
      subcopy: done == 0
          ? 'Preparing files…'
          : '$done file${done == 1 ? '' : 's'} synced',
      chipLabel: 'Syncing',
      actionLabel: null,
      onAction: null,
      showProgress: true,
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
      onAction: () => context.go('/folders'),
      showProgress: false,
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
  );
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

// ── Activity detail ──

class _DetailRow extends StatelessWidget {
  const _DetailRow({required this.label, required this.value, this.highlight});

  final String label;
  final String value;
  final bool? highlight;

  @override
  Widget build(BuildContext context) {
    final textTheme = Theme.of(context).textTheme;
    final palette = context.ferri;
    final emphasized = highlight ?? false;
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
            child: Text(
              value,
              style: emphasized
                  ? textTheme.bodySmall!.copyWith(
                      color: palette.danger, fontWeight: FontWeight.w600)
                  : textTheme.bodySmall,
            ),
          ),
        ],
      ),
    );
  }
}

void _showSessionDetail(BuildContext context, frb.SessionEntry s) {
  final baseDir = s.folderPath.split('/').where((e) => e.isNotEmpty).last;
  final isPush = s.direction != 'pull';
  showModalBottomSheet<void>(
    context: context,
    showDragHandle: true,
    builder: (ctx) => SafeArea(
      child: Padding(
        padding: const EdgeInsets.fromLTRB(24, 8, 24, 24),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              baseDir.isEmpty ? 'Sync session' : '$baseDir sync',
              style: Theme.of(ctx).textTheme.titleLarge,
            ),
            const SizedBox(height: 4),
            _DetailRow(
              label: 'Direction',
              value: isPush
                  ? 'Sent to ${s.peerDevice}'
                  : 'Received from ${s.peerDevice}',
            ),
            _DetailRow(label: 'Peer', value: s.peerDevice),
            _DetailRow(label: 'Started', value: verboseTime(s.ts)),
            _DetailRow(label: 'Files pushed', value: '${s.pushedCount}'),
            _DetailRow(label: 'Files pulled', value: '${s.pulledCount}'),
            _DetailRow(
              label: 'Conflicts',
              value: '${s.conflictsCount}',
              highlight: s.conflictsCount > BigInt.zero,
            ),
          ],
        ),
      ),
    ),
  );
}

void _showHistoryDetail(BuildContext context, frb.FileHistoryEntry h) {
  final action = h.action.toLowerCase();
  final isConflict = action.contains('conflict');
  showModalBottomSheet<void>(
    context: context,
    showDragHandle: true,
    builder: (ctx) => SafeArea(
      child: Padding(
        padding: const EdgeInsets.fromLTRB(24, 8, 24, 24),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              h.path.split('/').where((e) => e.isNotEmpty).last,
              style: Theme.of(ctx).textTheme.titleLarge,
            ),
            const SizedBox(height: 4),
            _DetailRow(
              label: 'Action',
              value: h.action,
              highlight: isConflict,
            ),
            _DetailRow(label: 'Device', value: h.deviceId ?? 'unknown'),
            if (h.size != null)
              _DetailRow(label: 'Size', value: formatBytes(h.size!)),
            _DetailRow(label: 'Time', value: verboseTime(h.recordedAt)),
            if (isConflict) ...[
              const SizedBox(height: FerriTokens.spaceL),
              FilledButton.icon(
                onPressed: () {
                  Navigator.of(ctx).pop();
                  context.go('/folders');
                },
                icon: const Icon(Icons.folder_open, size: 16),
                label: const Text('Review in Folders'),
              ),
            ],
          ],
        ),
      ),
    ),
  );
}

class _SessionRow extends StatelessWidget {
  const _SessionRow({required this.session, this.onTap});

  final frb.SessionEntry session;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final palette = context.ferri;
    final textTheme = Theme.of(context).textTheme;
    final isPush = session.direction != 'pull';
    final folderName = session.folderPath.split('/').where((s) => s.isNotEmpty).lastOrNull ?? '';
    final hadConflicts = session.conflictsCount > BigInt.zero;

    return ListTile(
      dense: true,
      onTap: onTap,
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
  const _HistoryRow({required this.entry, this.onTap});

  final frb.FileHistoryEntry entry;
  final VoidCallback? onTap;

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
      onTap: onTap,
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
              formatBytes(entry.size!),
              style: textTheme.bodySmall!.copyWith(color: palette.muted),
            )
          : null,
    );
  }
}