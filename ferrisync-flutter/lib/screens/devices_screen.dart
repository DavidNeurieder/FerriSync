import 'dart:async';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:mobile_scanner/mobile_scanner.dart';
import '../gen/api.dart' as frb;
import '../models/sync_models.dart';
import '../providers/sync_provider.dart';
import '../theme/ferri_theme.dart';
import '../widgets/empty_state.dart';

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
                  onPressed: () => _showPairDialog(context, service),
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
      floatingActionButton: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          FloatingActionButton.small(
            key: const ValueKey('scan_fab'),
            heroTag: 'scan',
            onPressed: () => _showScanDialog(context, service),
            child: const Icon(Icons.search),
          ),
          const SizedBox(height: 8),
          FloatingActionButton(
            key: const ValueKey('pair_fab'),
            heroTag: 'pair',
            onPressed: () => _showPairDialog(context, service),
            child: const Icon(Icons.add),
          ),
        ],
      ),
    );
  }

  void _showSnack(BuildContext context, String message) {
    ScaffoldMessenger.of(context)
      ..hideCurrentSnackBar()
      ..showSnackBar(SnackBar(content: Text(message)));
  }

  void _showScanDialog(BuildContext context, SyncService service) {
    Navigator.push(
      context,
      MaterialPageRoute(builder: (_) => _ScanPage(service: service)),
    );
  }

  void _showPairDialog(BuildContext context, SyncService service) {
    final ipCtrl = TextEditingController();
    final portCtrl = TextEditingController(text: '9847');

    showDialog(
      context: context,
      builder: (ctx) => AlertDialog(
        title: const Text('Pair Device'),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            TextField(
              controller: ipCtrl,
              decoration: const InputDecoration(
                labelText: 'IP Address',
                hintText: '192.168.1.x',
              ),
            ),
            TextField(
              controller: portCtrl,
              decoration: const InputDecoration(labelText: 'Port'),
              keyboardType: TextInputType.number,
            ),
            const SizedBox(height: FerriTokens.spaceM),
            OutlinedButton.icon(
              onPressed: () {
                Navigator.pop(ctx);
                _showScanner(context, service);
              },
              icon: const Icon(Icons.qr_code_scanner),
              label: const Text('Scan QR Code'),
            ),
          ],
        ),
        actions: [
          TextButton(onPressed: () => Navigator.pop(ctx), child: const Text('Cancel')),
          FilledButton(
            onPressed: () async {
              Navigator.pop(ctx);
              final ip = ipCtrl.text.trim();
              final port = int.tryParse(portCtrl.text.trim()) ?? 9847;
              if (ip.isEmpty) return;
              try {
                final result = await service.pairWithDevice(ip, port);
                if (context.mounted) _showSnack(context, result);
              } catch (e) {
                if (context.mounted) _showSnack(context, 'Pairing failed: $e');
              }
              service.refresh();
            },
            child: const Text('Pair'),
          ),
        ],
      ),
    );
  }

  void _showScanner(BuildContext context, SyncService service) {
    Navigator.push(
      context,
      MaterialPageRoute(
        builder: (_) => Scaffold(
          appBar: AppBar(title: const Text('Scan QR Code')),
          body: MobileScanner(
            onDetect: (capture) async {
              final barcode = capture.barcodes.firstOrNull;
              if (barcode?.rawValue case final value?) {
                Navigator.pop(context);
                final parts = value.split(':');
                final ip = parts.isNotEmpty ? parts[0] : '';
                final port =
                    parts.length > 1 ? int.tryParse(parts[1]) ?? 9847 : 9847;
                if (ip.isEmpty) return;
                try {
                  final result = await service.pairWithDevice(ip, port);
                  if (context.mounted) _showSnack(context, result);
                } catch (e) {
                  if (context.mounted) _showSnack(context, 'Pairing failed: $e');
                }
                service.refresh();
              }
            },
          ),
        ),
      ),
    );
  }

  Future<void> _confirmRemove(
      BuildContext context, SyncService service, Device d) async {
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: const Text('Remove device'),
        content: Text(
          'Remove ${d.name} (${d.id})? '
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
                    'wants to connect · $id',
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
    required this.onRemove,
    required this.onRename,
  });

  final Device device;
  final int folderCount;
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
    return Padding(
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
                  color: device.isOnline ? palette.success : palette.muted,
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
                  '${device.isOnline ? 'Online' : 'Last seen ${device.lastSeenFormatted}'}',
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
    );
  }
}

class _ScanPage extends StatefulWidget {
  final SyncService service;
  const _ScanPage({required this.service});

  @override
  State<_ScanPage> createState() => _ScanPageState();
}

class _ScanPageState extends State<_ScanPage> {
  List<frb.DiscoveredDevice> _devices = [];
  bool _scanning = false;
  int? _pairingIndex;

  @override
  void initState() {
    super.initState();
    _startScan();
  }

  Future<void> _startScan() async {
    setState(() => _scanning = true);
    try {
      final devices = await widget.service.discoverDevices(timeoutSecs: 4);
      if (mounted) {
        setState(() {
          _devices = devices;
          _scanning = false;
        });
      }
    } catch (_) {
      if (mounted) setState(() => _scanning = false);
    }
  }

  Future<void> _pair(int index) async {
    if (_pairingIndex != null) return;
    final d = _devices[index];
    setState(() => _pairingIndex = index);
    try {
      final result = await widget.service.pairWithDevice(d.ip, d.port);
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text(result)),
      );
      widget.service.refresh();
      Navigator.pop(context);
    } catch (e) {
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text('Pairing failed: $e')),
      );
    } finally {
      if (mounted) setState(() => _pairingIndex = null);
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('Scan Network')),
      body: _scanning
          ? const Center(
              child: Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  CircularProgressIndicator(),
                  SizedBox(height: 16),
                  Text('Scanning for FerriSync servers...'),
                ],
              ),
            )
          : _devices.isEmpty
              ? EmptyState(
                  icon: Icons.wifi_find,
                  title: 'No servers found',
                  subtitle:
                      'Make sure the other device is on the same network '
                      'and has a folder listening.',
                  action: FilledButton.icon(
                    onPressed: _startScan,
                    icon: const Icon(Icons.refresh),
                    label: const Text('Scan again'),
                  ),
                )
              : ListView.builder(
                  itemCount: _devices.length,
                  itemBuilder: (_, i) {
                    final d = _devices[i];
                    return _Reveal(
                      delay: Duration(milliseconds: i * 60),
                      child: ListTile(
                        leading: const Icon(Icons.dns),
                        title: Text(d.name),
                        subtitle: Text('${d.ip}:${d.port}'),
                        trailing: _pairingIndex == i
                            ? const SizedBox(
                                width: 18,
                                height: 18,
                                child:
                                    CircularProgressIndicator(strokeWidth: 2),
                              )
                            : FilledButton(
                                onPressed: () => _pair(i),
                                child: const Text('Pair'),
                              ),
                      ),
                    );
                  },
                ),
    );
  }
}

/// Subtle fade+slide entrance; used to animate discovered devices appearing.
class _Reveal extends StatefulWidget {
  const _Reveal({required this.child, this.delay = Duration.zero});

  final Widget child;
  final Duration delay;

  @override
  State<_Reveal> createState() => _RevealState();
}

class _RevealState extends State<_Reveal>
    with SingleTickerProviderStateMixin {
  late final AnimationController _controller;

  @override
  void initState() {
    super.initState();
    _controller = AnimationController(
      vsync: this,
      duration: const Duration(milliseconds: 260),
    )..forward();
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final offset = CurvedAnimation(
      parent: _controller,
      curve: Curves.easeOutCubic,
    );
    return FadeTransition(
      opacity: offset,
      child: SlideTransition(
        position: Tween<Offset>(
          begin: const Offset(0, 0.08),
          end: Offset.zero,
        ).animate(offset),
        child: widget.child,
      ),
    );
  }
}