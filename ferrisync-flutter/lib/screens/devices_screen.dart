import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import '../models/sync_models.dart';
import '../providers/sync_provider.dart';
import '../theme/ferri_theme.dart';
import '../widgets/empty_state.dart';
import 'add_device_screen.dart';
import 'device_detail_screen.dart';

class DevicesScreen extends ConsumerWidget {
  const DevicesScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final service = ref.watch(syncServiceProvider);
    final devices = ref.watch(devicesProvider);
    final folders = ref.watch(foldersProvider);
    final pending = service.pendingPairings;

    final folderCountByDevice = <String, int>{};
    for (final f in folders) {
      folderCountByDevice[f.deviceId] =
          (folderCountByDevice[f.deviceId] ?? 0) + 1;
    }

    return Scaffold(
      body: RefreshIndicator(
        onRefresh: service.refresh,
        child: ListView(
          physics: const AlwaysScrollableScrollPhysics(),
          padding: const EdgeInsets.all(FerriTokens.spaceL),
          children: [
            if (pending.isNotEmpty) ...[
              SectionHeader(title: 'PAIRING REQUESTS (${pending.length})'),
              const SizedBox(height: FerriTokens.spaceS),
              for (final (name, id) in pending) ...[
                _PairingRequestCard(
                  name: name,
                  id: id,
                  onDeny: () async {
                    final msg = await service.denyPairing(id);
                    if (context.mounted) _showSnack(context, msg);
                  },
                  onAllow: () async {
                    final msg = await service.approvePairing(id, name);
                    if (context.mounted) _showSnack(context, msg);
                  },
                ),
                const SizedBox(height: FerriTokens.spaceS),
              ],
              const SizedBox(height: FerriTokens.spaceL),
            ],
            SectionHeader(title: 'PAIRED DEVICES (${devices.length})'),
            const SizedBox(height: FerriTokens.spaceS),
            if (devices.isEmpty)
              EmptyState(
                icon: Icons.hub_outlined,
                title: 'No devices paired',
                subtitle:
                    'Pair a device on your local network to share folders.',
                action: FilledButton.icon(
                  key: const ValueKey('pair_a_device'),
                  onPressed: () => openAddDevice(context, ref),
                  icon: const Icon(Icons.add_link),
                  label: const Text('Pair a device'),
                ),
              )
            else
              Card(
                child: Column(
                  children: [
                    for (final d in devices) ...[
                      _DeviceCard(
                        device: d,
                        folderCount: folderCountByDevice[d.id] ?? 0,
                        onTap: () => Navigator.of(context).push(
                          MaterialPageRoute<void>(
                            builder: (_) => DeviceDetailScreen(device: d),
                          ),
                        ),
                        onRemove: () =>
                            _confirmRemove(context, service, d),
                        onRename: () => _promptRename(context, service, d),
                      ),
                      if (d != devices.last)
                        const Divider(height: 1, indent: 16, endIndent: 16),
                    ],
                  ],
                ),
              ),
          ],
        ),
      ),
      floatingActionButton: FloatingActionButton.extended(
        key: const ValueKey('add_device_fab'),
        onPressed: () => openAddDevice(context, ref),
        icon: const Icon(Icons.add),
        label: const Text('Add device'),
      ),
    );
  }

  void _showSnack(BuildContext context, String message) {
    ScaffoldMessenger.of(context)
      ..hideCurrentSnackBar()
      ..showSnackBar(SnackBar(content: Text(message)));
  }

  Future<void> _confirmRemove(
      BuildContext context, SyncService service, Device d) async {
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: const Text('Remove device'),
        content: Text(
          'Remove ${d.name}? '
          'This deletes all associated folders and history.',
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
    final msg = await service.removeDevice(d.id);
    if (context.mounted) _showSnack(context, msg);
  }

  Future<void> _promptRename(
      BuildContext context, SyncService service, Device d) async {
    final ctrl = TextEditingController(text: d.name);
    final name = await showDialog<String>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: Text('Rename ${d.name}'),
        content: TextField(
          controller: ctrl,
          autofocus: true,
          decoration: const InputDecoration(labelText: 'Device name'),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(ctx),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(ctx, ctrl.text.trim()),
            child: const Text('Rename'),
          ),
        ],
      ),
    );
    if (name == null || name.isEmpty || !context.mounted) return;
    final msg = await service.renameRemoteDevice(d.id, name);
    if (context.mounted) _showSnack(context, msg);
  }
}

class _PairingRequestCard extends StatelessWidget {
  const _PairingRequestCard({
    required this.name,
    required this.id,
    required this.onDeny,
    required this.onAllow,
  });

  final String name;
  final String id;
  final VoidCallback onDeny;
  final VoidCallback onAllow;

  @override
  Widget build(BuildContext context) {
    final palette = context.ferri;
    return Card(
      color: palette.surfaceHigh,
      child: Padding(
        padding: const EdgeInsets.all(FerriTokens.spaceL),
        child: Row(
          children: [
            Icon(Icons.person_add_alt_1, size: 28, color: palette.warning),
            const SizedBox(width: FerriTokens.spaceM),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    name,
                    style: Theme.of(context).textTheme.titleSmall,
                  ),
                  const SizedBox(height: 2),
                  Text(
                    'wants to pair with this device',
                    style: Theme.of(context)
                        .textTheme
                        .bodySmall!
                        .copyWith(color: palette.muted),
                  ),
                ],
              ),
            ),
            TextButton(
              onPressed: onDeny,
              style: TextButton.styleFrom(foregroundColor: palette.danger),
              child: const Text('Deny'),
            ),
            FilledButton(
              onPressed: onAllow,
              style: FilledButton.styleFrom(
                backgroundColor: palette.success,
                foregroundColor: Colors.black87,
              ),
              child: const Text('Allow'),
            ),
          ],
        ),
      ),
    );
  }
}

class _DeviceCard extends StatelessWidget {
  const _DeviceCard({
    required this.device,
    required this.folderCount,
    required this.onTap,
    required this.onRemove,
    required this.onRename,
  });

  final Device device;
  final int folderCount;
  final VoidCallback onTap;
  final VoidCallback onRemove;
  final VoidCallback onRename;

  IconData _iconFor(String name) {
    final n = name.toLowerCase();
    if (n.contains('laptop') || n.contains('desktop') || n.contains('pc')) {
      return Icons.laptop_mac;
    }
    if (n.contains('phone') ||
        n.contains('pixel') ||
        n.contains('sm-g') ||
        n.contains('motorola')) {
      return Icons.smartphone;
    }
    if (n.contains('server') || n.contains('nas') || n.contains('cloud')) {
      return Icons.dns;
    }
    return Icons.devices_other;
  }

  @override
  Widget build(BuildContext context) {
    final palette = context.ferri;
    final textTheme = Theme.of(context).textTheme;
    return InkWell(
      onTap: onTap,
      borderRadius: BorderRadius.circular(FerriTokens.radiusM),
      child: Padding(
        padding: const EdgeInsets.symmetric(
          horizontal: FerriTokens.spaceM,
          vertical: FerriTokens.spaceM,
        ),
        child: Row(
          children: [
          Stack(
            alignment: Alignment.bottomRight,
            children: [
              Container(
                width: 40,
                height: 40,
                decoration: BoxDecoration(
                  color: palette.surfaceHigh,
                  borderRadius: BorderRadius.circular(FerriTokens.radiusS),
                ),
                child: Icon(_iconFor(device.name),
                    color: palette.primary, size: 22),
              ),
              Container(
                width: 12,
                height: 12,
                decoration: BoxDecoration(
                  color: _presenceColor(device.presence, palette),
                  shape: BoxShape.circle,
                  border: Border.all(color: palette.surface, width: 2),
                ),
              ),
            ],
          ),
          const SizedBox(width: FerriTokens.spaceM),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  device.name,
                  style: textTheme.titleMedium
                      ?.copyWith(fontWeight: FontWeight.w600),
                  overflow: TextOverflow.ellipsis,
                ),
                const SizedBox(height: 2),
                Text(
                  '$folderCount folder${folderCount == 1 ? '' : 's'} · '
                  '${_presenceText(device)}',
                  style: textTheme.bodySmall!.copyWith(color: palette.muted),
                ),
              ],
            ),
          ),
          PopupMenuButton<String>(
            tooltip: 'Device actions',
            onSelected: (action) {
              if (action == 'rename') onRename();
              if (action == 'remove') onRemove();
            },
            itemBuilder: (_) => [
              const PopupMenuItem(value: 'rename', child: Text('Rename')),
              const PopupMenuItem(value: 'remove', child: Text('Remove')),
            ],
            icon: Icon(Icons.more_vert, color: palette.muted),
          ),
        ],
        ),
      ),
    );
  }
}

/// Presence vocabulary used across device rows and the detail sheet.
String _presenceText(Device d) => switch (d.presence) {
      Presence.connected => 'Connected',
      Presence.recentlySeen => 'Recently seen',
      Presence.offline =>
        d.lastSeen > 0 ? 'Offline · last seen ${d.lastSeenFormatted}' : 'Offline',
    };

Color _presenceColor(Presence p, FerriPalette palette) => switch (p) {
      Presence.connected => palette.success,
      Presence.recentlySeen => palette.warning,
      Presence.offline => palette.muted,
    };

