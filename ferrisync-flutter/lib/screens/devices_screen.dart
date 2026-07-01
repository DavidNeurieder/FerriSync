import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:mobile_scanner/mobile_scanner.dart';
import '../models/sync_models.dart';
import '../providers/sync_provider.dart';

class DevicesScreen extends ConsumerWidget {
  const DevicesScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final service = ref.watch(syncServiceProvider);
    final devices = ref.watch(devicesProvider);

    return Scaffold(
      body: devices.isEmpty
          ? const Center(child: Text('No devices paired'))
          : ListView.builder(
              itemCount: devices.length,
              itemBuilder: (_, i) => _deviceTile(devices[i]),
            ),
      floatingActionButton: FloatingActionButton(
        onPressed: () => _showPairDialog(context, service),
        child: const Icon(Icons.add),
      ),
    );
  }

  Widget _deviceTile(Device d) {
    return ListTile(
      leading: Icon(Icons.devices, color: d.isOnline ? Colors.green : Colors.grey),
      title: Text(d.name),
      subtitle: Text('ID: ${d.id}\nLast seen: ${d.lastSeenFormatted}'),
      trailing: IconButton(
        icon: const Icon(Icons.delete_outline),
        onPressed: () {
          // TODO: forget device via FRB
        },
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
              await service.pairWithDevice(ip, port);
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
            onDetect: (capture) {
              final barcode = capture.barcodes.firstOrNull;
              if (barcode?.rawValue case final value?) {
                Navigator.pop(context);
                final parts = value.split(':');
                final ip = parts.isNotEmpty ? parts[0] : '';
                final port = parts.length > 1 ? int.tryParse(parts[1]) ?? 9847 : 9847;
                service.pairWithDevice(ip, port);
                service.refresh();
              }
            },
          ),
        ),
      ),
    );
  }
}
