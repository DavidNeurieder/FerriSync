import 'dart:async';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:mobile_scanner/mobile_scanner.dart';
import '../gen/api.dart' as frb;
import '../models/sync_models.dart';
import '../providers/sync_provider.dart';

class DevicesScreen extends ConsumerWidget {
  const DevicesScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final service = ref.watch(syncServiceProvider);
    final devices = ref.watch(devicesProvider);
    final pending = service.pendingPairings;

    return Scaffold(
      body: Column(
        children: [
          for (final (name, id) in pending)
            Card(
              margin: const EdgeInsets.fromLTRB(16, 16, 16, 0),
              color: Theme.of(context).colorScheme.secondaryContainer,
              child: Padding(
                padding: const EdgeInsets.all(16),
                child: Row(
                  children: [
                    const Icon(Icons.person_add, size: 32),
                    const SizedBox(width: 12),
                    Expanded(
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text(
                            'Pairing request',
                            style: Theme.of(context).textTheme.titleSmall,
                          ),
                          Text('$name wants to connect'),
                        ],
                      ),
                    ),
                    TextButton(
                      onPressed: () async {
                        final msg = await service.denyPairing(id);
                        if (context.mounted) {
                          ScaffoldMessenger.of(context)
                            ..hideCurrentSnackBar()
                            ..showSnackBar(SnackBar(content: Text(msg)));
                        }
                      },
                      child: const Text('Deny'),
                    ),
                    FilledButton(
                      onPressed: () async {
                        final msg = await service.approvePairing(id, name);
                        if (context.mounted) {
                          ScaffoldMessenger.of(context)
                            ..hideCurrentSnackBar()
                            ..showSnackBar(SnackBar(content: Text(msg)));
                        }
                      },
                      child: const Text('Allow'),
                    ),
                  ],
                ),
              ),
            ),
          Expanded(
            child: devices.isEmpty
                ? const Center(child: Text('No devices paired'))
                : ListView.builder(
                    itemCount: devices.length,
                    itemBuilder: (ctx, i) =>
                        _deviceTile(ctx, service, devices[i]),
                  ),
          ),
        ],
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

  Widget _deviceTile(BuildContext context, SyncService service, Device d) {
    return ListTile(
      leading: Icon(Icons.devices, color: d.isOnline ? Colors.green : Colors.grey),
      title: Text(d.name),
      subtitle: Text('ID: ${d.id}\nLast seen: ${d.lastSeenFormatted}'),
      trailing: IconButton(
        key: ValueKey('delete_${d.id}'),
        icon: const Icon(Icons.delete_outline),
        onPressed: () async {
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
          if (context.mounted) {
            ScaffoldMessenger.of(context)
              ..hideCurrentSnackBar()
              ..showSnackBar(SnackBar(content: Text(msg)));
          }
        },
      ),
    );
  }

  void _showScanDialog(BuildContext context, SyncService service) {
    Navigator.push(
      context,
      MaterialPageRoute(
        builder: (_) => _ScanPage(service: service),
      ),
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
              decoration: const InputDecoration(labelText: 'IP Address', hintText: '192.168.1.x'),
            ),
            TextField(
              controller: portCtrl,
              decoration: const InputDecoration(labelText: 'Port'),
              keyboardType: TextInputType.number,
            ),
            const SizedBox(height: 12),
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
                if (context.mounted) {
                  ScaffoldMessenger.of(context).showSnackBar(
                    SnackBar(content: Text(result)),
                  );
                }
              } catch (e) {
                if (context.mounted) {
                  ScaffoldMessenger.of(context).showSnackBar(
                    SnackBar(content: Text('Pairing failed: $e')),
                  );
                }
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
                  if (context.mounted) {
                    ScaffoldMessenger.of(context).showSnackBar(
                      SnackBar(content: Text(result)),
                    );
                  }
                } catch (e) {
                  if (context.mounted) {
                    ScaffoldMessenger.of(context).showSnackBar(
                      SnackBar(content: Text('Pairing failed: $e')),
                    );
                  }
                }
                service.refresh();
              }
            },
          ),
        ),
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
              ? Center(
                  child: Column(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      const Text('No servers found'),
                      const SizedBox(height: 16),
                      FilledButton.icon(
                        onPressed: _startScan,
                        icon: const Icon(Icons.refresh),
                        label: const Text('Scan again'),
                      ),
                    ],
                  ),
                )
              : ListView.builder(
                  itemCount: _devices.length,
                  itemBuilder: (_, i) {
                    final d = _devices[i];
                    return ListTile(
                      leading: const Icon(Icons.dns),
                      title: Text(d.name),
                      subtitle: Text('${d.ip}:${d.port}'),
                      trailing: _pairingIndex == i
                          ? const SizedBox(
                              width: 18,
                              height: 18,
                              child: CircularProgressIndicator(strokeWidth: 2),
                            )
                          : FilledButton(
                              onPressed: () => _pair(i),
                              child: const Text('Pair'),
                            ),
                    );
                  },
                ),
    );
  }
}
