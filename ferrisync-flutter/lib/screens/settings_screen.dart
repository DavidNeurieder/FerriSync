import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import '../providers/sync_provider.dart';

class SettingsScreen extends ConsumerWidget {
  const SettingsScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final service = ref.watch(syncServiceProvider);

    return ListView(
      padding: const EdgeInsets.all(16),
      children: [
        Card(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              const Padding(
                padding: EdgeInsets.fromLTRB(16, 12, 16, 4),
                child: Text('Device', style: TextStyle(fontWeight: FontWeight.bold)),
              ),
              ListTile(
                title: const Text('Display Name'),
                subtitle: Text(service.deviceName),
                trailing: const Icon(Icons.edit),
                onTap: () {
                  // TODO: edit device name
                },
              ),
              ListTile(
                title: const Text('Device ID'),
                subtitle: Text(service.deviceId, style: const TextStyle(fontFamily: 'monospace')),
              ),
            ],
          ),
        ),
        const SizedBox(height: 16),
        Card(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              const Padding(
                padding: EdgeInsets.fromLTRB(16, 12, 16, 4),
                child: Text('Sync', style: TextStyle(fontWeight: FontWeight.bold)),
              ),
              SwitchListTile(
                title: const Text('Notifications'),
                subtitle: const Text('Show sync notifications'),
                value: true,
                onChanged: (_) {},
              ),
            ],
          ),
        ),
        const SizedBox(height: 16),
        Card(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              const Padding(
                padding: EdgeInsets.fromLTRB(16, 12, 16, 4),
                child: Text('About', style: TextStyle(fontWeight: FontWeight.bold)),
              ),
              ListTile(
                title: const Text('Version'),
                subtitle: const Text('0.1.0'),
              ),
              ListTile(
                title: const Text('License'),
                subtitle: const Text('AGPL-3.0'),
              ),
            ],
          ),
        ),
      ],
    );
  }
}
