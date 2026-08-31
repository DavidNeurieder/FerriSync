import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import '../gen/api.dart' as frb;
import '../models/sync_models.dart';
import '../providers/sync_provider.dart';
import '../theme/ferri_theme.dart';
import '../utils/format_bytes.dart';
import '../utils/relative_time.dart';
import '../widgets/dashboard/attention_panel.dart';
import '../widgets/dashboard/folder_summary.dart';
import '../widgets/dashboard/status_hero.dart';
import '../widgets/empty_state.dart';
import '../widgets/presence_dot.dart';

/// The command center: "is everything OK?" answer, quick stats, attention
/// items, this device, the folder health summary, paired devices and a
/// live-ish view of recent activity.
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

    final connectedDevices = service.connectedDevices;
    final offlineDevices =
        devices.where((d) => !d.isOnline && d.lastSeen > 0).toList();
    final conflictCount = service.conflictCount;
    final lastSync = feed.firstOrNull?.tsSec ??
        folders.fold<int>(
          0,
          (max, f) => f.lastSyncAt > max ? f.lastSyncAt : max,
        );

    final hero = buildHeroView(
      context,
      status: status,
      service: service,
      conflictCount: conflictCount,
      offlineDevices: offlineDevices,
      lastSync: lastSync,
      hasFolders: folders.isNotEmpty,
    );
    final attention = service.attentionItems;
    final syncedFolders = folders.where((f) => f.health == FolderHealth.healthy).length;

    return RefreshIndicator(
      onRefresh: service.refresh,
      child: ListView(
        physics: const AlwaysScrollableScrollPhysics(),
        padding: const EdgeInsets.all(FerriTokens.spaceL),
        children: [
          StatusHero(data: hero, mode: status),
          if (attention.isNotEmpty) ...[
            const SizedBox(height: FerriTokens.spaceM),
            AttentionPanel(items: attention),
          ],
          const SizedBox(height: FerriTokens.spaceL),
          Row(
            children: [
              Expanded(
                child: _Stat(
                  label: 'Devices connected',
                  value: '$connectedDevices',
                ),
              ),
              const SizedBox(width: FerriTokens.spaceM),
              Expanded(
                child: _Stat(
                  label: 'Folders synced',
                  value: devices.isEmpty ? '${folders.length}' : '$syncedFolders',
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
          if (folders.isNotEmpty) ...[
            const SizedBox(height: FerriTokens.spaceXL),
            SectionHeader(title: 'YOUR FOLDERS (${folders.length})'),
            const SizedBox(height: FerriTokens.spaceS),
            Card(child: FolderSummarySection(folders: folders)),
          ],
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

/// This device: name plus a nudge to manage identity in Settings. The raw
/// device ID now lives under Settings → Advanced (Device identity).
class _ThisDeviceCard extends StatelessWidget {
  const _ThisDeviceCard({required this.service});

  final SyncService service;

  @override
  Widget build(BuildContext context) {
    final palette = context.ferri;
    final textTheme = Theme.of(context).textTheme;
    return Card(
      child: ListTile(
        leading: Icon(Icons.smartphone, color: palette.primary, size: 26),
        title: Text(service.deviceName, style: textTheme.titleMedium),
        subtitle: Text(
          'This device · manage in Settings',
          style: textTheme.bodySmall!.copyWith(color: palette.muted),
        ),
        trailing: const Icon(Icons.chevron_right, size: 18, color: Colors.grey),
        onTap: () => context.go('/settings'),
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
      leading: PresenceDot(presence: device.presence),
      title: Text(device.name),
      subtitle: Text(
        device.isOnline ? 'Online' : 'Last seen ${device.lastSeenFormatted}',
        style: textTheme.bodySmall!.copyWith(color: palette.muted),
      ),
      trailing: const Icon(Icons.chevron_right, size: 18, color: Colors.grey),
      onTap: () => context.go('/devices'),
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
  final sent = s.pushedBytes.toInt();
  final received = s.pulledBytes.toInt();
  final hasBytes = sent + received > 0;
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
              baseDir.isEmpty ? 'Sync' : '$baseDir sync',
              style: Theme.of(ctx).textTheme.titleLarge,
            ),
            const SizedBox(height: 4),
            _DetailRow(
              label: 'Direction',
              value: isPush
                  ? 'Sent to ${s.peerDevice}'
                  : 'Received from ${s.peerDevice}',
            ),
            _DetailRow(label: 'Device', value: s.peerDevice),
            _DetailRow(label: 'Started', value: verboseTime(s.ts)),
            _DetailRow(label: 'Files pushed', value: '${s.pushedCount}'),
            _DetailRow(label: 'Files pulled', value: '${s.pulledCount}'),
            if (hasBytes) ...[
              _DetailRow(
                label: 'Data sent',
                value: sent > 0 ? formatBytes(sent) : '—',
              ),
              _DetailRow(
                label: 'Data received',
                value: received > 0 ? formatBytes(received) : '—',
              ),
            ],
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
                  context.go('/conflicts');
                },
                icon: const Icon(Icons.warning_amber_rounded, size: 16),
                label: const Text('Resolve conflicts'),
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