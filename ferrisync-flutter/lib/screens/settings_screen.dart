import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:permission_handler/permission_handler.dart';
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
                onTap: () => _editDeviceName(context, service),
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
                value: service.notificationsEnabled,
                onChanged: (value) =>
                    _toggleNotifications(context, ref, service, value),
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

  Future<void> _toggleNotifications(
    BuildContext context,
    WidgetRef ref,
    SyncService service,
    bool enabled,
  ) async {
    final messenger = ScaffoldMessenger.of(context);
    final nowEnabled = await service.setNotificationsEnabled(enabled);
    if (nowEnabled) return;

    if (enabled) {
      // The user tried to turn notifications ON but Android denied the
      // runtime permission; point them at the system settings page.
      messenger.showSnackBar(
        SnackBar(
          content: const Text(
            'Notification permission was denied. Enable it in system settings.',
          ),
          action: SnackBarAction(
            label: 'Open settings',
            onPressed: () => openAppSettings(),
          ),
        ),
      );
    } else {
      messenger.showSnackBar(
        const SnackBar(content: Text('Sync notifications off')),
      );
    }
  }

  Future<void> _editDeviceName(BuildContext context, SyncService service) async {
    final controller = TextEditingController(text: service.deviceName);
    final newName = await showDialog<String>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        title: const Text('Device Name'),
        content: TextField(
          controller: controller,
          autofocus: true,
          maxLength: 64,
          decoration: const InputDecoration(
            labelText: 'Display Name',
            hintText: 'How other devices see this device',
          ),
          onSubmitted: (value) =>
              Navigator.of(dialogContext).pop(value.trim()),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(dialogContext).pop(),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () =>
                Navigator.of(dialogContext).pop(controller.text.trim()),
            child: const Text('Save'),
          ),
        ],
      ),
    );
    if (newName == null || newName.isEmpty || newName == service.deviceName) {
      return;
    }

    final messenger = ScaffoldMessenger.of(context);
    try {
      await service.setDeviceName(newName);
      messenger.showSnackBar(
        SnackBar(content: Text('Renamed to "$newName"')),
      );
    } catch (e) {
      messenger.showSnackBar(
        SnackBar(content: Text('Rename failed: $e')),
      );
    }
  }
}
