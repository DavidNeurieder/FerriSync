import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import '../models/sync_models.dart';
import '../providers/sync_provider.dart';

class DashboardScreen extends ConsumerWidget {
  const DashboardScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final service = ref.watch(syncServiceProvider);
    final devices = ref.watch(devicesProvider);
    final status = ref.watch(syncStatusProvider);

    final theme = Theme.of(context);

    return RefreshIndicator(
      onRefresh: () => service.refresh(),
      child: ListView(
        padding: const EdgeInsets.all(16),
        children: [
          _StatusCard(status: status),
          const SizedBox(height: 16),
          _InfoCard(
            title: 'Device',
            children: [
              _infoRow('ID', service.deviceId),
              _infoRow('Name', service.deviceName),
            ],
          ),
          const SizedBox(height: 16),
          _InfoCard(
            title: 'Paired Devices (${devices.length})',
            children: devices.isEmpty
                ? [const Padding(padding: EdgeInsets.all(8), child: Text('No devices paired.'))]
                : devices.map((d) => _deviceTile(d)).toList(),
          ),
        ],
      ),
    );
  }

  Widget _StatusCard({required SyncStatus status}) {
    final (icon, color, text) = switch (status) {
      SyncStatus.idle => (Icons.check_circle, Colors.green, 'Idle'),
      SyncStatus.syncing => (Icons.sync, Colors.blue, 'Syncing...'),
      SyncStatus.error => (Icons.error, Colors.red, 'Error'),
    };
    return Card(
      child: ListTile(
        leading: Icon(icon, color: color, size: 32),
        title: Text(text),
        subtitle: const Text('Sync status'),
      ),
    );
  }

  Widget _InfoCard({required String title, required List<Widget> children}) {
    return Card(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(16, 12, 16, 4),
            child: Text(title, style: TextStyle(fontWeight: FontWeight.bold)),
          ),
          ...children,
          const SizedBox(height: 8),
        ],
      ),
    );
  }

  Widget _infoRow(String label, String value) {
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 4),
      child: Row(
        children: [
          SizedBox(width: 80, child: Text(label, style: TextStyle(color: Colors.grey))),
          Expanded(child: Text(value, style: const TextStyle(fontFamily: 'monospace'))),
        ],
      ),
    );
  }

  Widget _deviceTile(Device d) {
    return ListTile(
      dense: true,
      leading: Icon(Icons.devices, color: d.isOnline ? Colors.green : Colors.grey),
      title: Text(d.name),
      subtitle: Text('Last seen: ${d.lastSeenFormatted}'),
    );
  }
}
