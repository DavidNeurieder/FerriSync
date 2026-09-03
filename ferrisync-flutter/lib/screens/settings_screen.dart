import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:permission_handler/permission_handler.dart';
import '../providers/sync_provider.dart';
import 'diagnostics_screen.dart';

class SettingsScreen extends ConsumerWidget {
  const SettingsScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final service = ref.watch(syncServiceProvider);
    final themeMode = ref.watch(themeModeProvider);
    final devices = ref.watch(devicesProvider);

    return ListView(
      padding: const EdgeInsets.all(16),
      children: [
        _Section(label: 'General', children: [
          ListTile(
            title: const Text('Display Name'),
            subtitle: Text(service.deviceName),
            trailing: const Icon(Icons.edit),
            onTap: () => _editDeviceName(context, service),
          ),
          ListTile(
            title: const Text('Theme'),
            subtitle: Text(switch (themeMode) {
              ThemeMode.dark => 'Dark',
              ThemeMode.light => 'Light',
              ThemeMode.system => 'System',
            }),
            trailing: const Icon(Icons.brightness_6),
            onTap: () => _pickTheme(context, ref, themeMode),
          ),
        ]),
        _Section(label: 'Sync', children: [
          SwitchListTile(
            title: const Text('Notifications'),
            subtitle: const Text('Show sync notifications'),
            value: service.notificationsEnabled,
            onChanged: (value) =>
                _toggleNotifications(context, ref, service, value),
          ),
          SwitchListTile(
            key: const ValueKey('auto_repair_toggle'),
            title: const Text('Auto re-pair known devices'),
            subtitle: const Text(
                'On startup, discover trusted devices and re-pair them'),
            value: service.autoRepairEnabled,
            onChanged: (value) => _toggleAutoRepair(context, service, value),
          ),
        ]),
        _Section(label: 'Security', children: [
          ListTile(
            title: const Text('Trusted devices'),
            subtitle: Text(
              devices.isEmpty
                  ? 'No devices paired yet'
                  : '${devices.length} device${devices.length == 1 ? '' : 's'}',
            ),
          ),
        ]),
        _Section(
          label: 'Danger zone',
          children: [
            ListTile(
              key: const ValueKey('remove_all_devices'),
              title: const Text('Remove all trusted devices',
                  style: TextStyle(color: Colors.redAccent)),
              subtitle: const Text('Unpairs every device and deletes '
                  'associated folders, metadata and history'),
              onTap: () => _confirmRemoveAll(context, service),
            ),
            ListTile(
              key: const ValueKey('factory_reset'),
              title: const Text('Factory reset',
                  style: TextStyle(color: Colors.redAccent)),
              subtitle: const Text('Erase the device identity and all data, '
                  'restoring a fresh-install state'),
              onTap: () => _confirmFactoryReset(context, service),
            ),
          ],
        ),
        _Section(label: 'Advanced', children: [
          ListTile(
            key: const ValueKey('diagnostics'),
            title: const Text('Diagnostics'),
            subtitle: const Text('Run the same health checks as '
                '`ferrisync doctor`'),
            trailing: const Icon(Icons.chevron_right),
            onTap: () => Navigator.of(context).push(
              MaterialPageRoute<void>(
                builder: (_) => const DiagnosticsScreen(),
              ),
            ),
          ),
          ListTile(
            key: const ValueKey('device_identity'),
            title: const Text('Device identity'),
            subtitle: Text(
              service.deviceId,
              style: const TextStyle(fontFamily: 'monospace', fontSize: 12),
              maxLines: 2,
              overflow: TextOverflow.ellipsis,
            ),
            trailing: IconButton(
              tooltip: 'Copy device ID',
              icon: const Icon(Icons.copy, size: 18),
              onPressed: () => _copyDeviceId(context, service),
            ),
          ),
        ]),
        const _Section(label: 'About', children: [
          ListTile(
            title: Text('Version'),
            subtitle: Text('0.1.0'),
          ),
          ListTile(
            title: Text('License'),
            subtitle: Text('AGPL-3.0'),
          ),
        ]),
      ],
    );
  }

  Future<void> _pickTheme(
    BuildContext context,
    WidgetRef ref,
    ThemeMode current,
  ) async {
    final choice = await showDialog<ThemeMode>(
      context: context,
      builder: (ctx) => SimpleDialog(
        title: const Text('Theme'),
        children: [
          SimpleDialogOption(
            onPressed: () => Navigator.pop(ctx, ThemeMode.dark),
            child: Row(children: [
              if (current == ThemeMode.dark) ...[
                const SizedBox(width: 4),
                const Icon(Icons.check, size: 18),
                const SizedBox(width: 12),
              ],
              const Text('Dark'),
            ]),
          ),
          SimpleDialogOption(
            onPressed: () => Navigator.pop(ctx, ThemeMode.light),
            child: Row(children: [
              if (current == ThemeMode.light) ...[
                const SizedBox(width: 4),
                const Icon(Icons.check, size: 18),
                const SizedBox(width: 12),
              ],
              const Text('Light'),
            ]),
          ),
          SimpleDialogOption(
            onPressed: () => Navigator.pop(ctx, ThemeMode.system),
            child: Row(children: [
              if (current == ThemeMode.system) ...[
                const SizedBox(width: 4),
                const Icon(Icons.check, size: 18),
                const SizedBox(width: 12),
              ],
              const Text('System'),
            ]),
          ),
        ],
      ),
    );
    if (choice != null && choice != current) {
      ref.read(themeModeProvider.notifier).state = choice;
    }
  }

  Future<void> _confirmRemoveAll(
    BuildContext context,
    SyncService service,
  ) async {
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: const Text('Remove all devices?'),
        content: const Text('This unpairs every trusted device and deletes '
            'their folders, sync metadata and history. Local files are kept.'),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(ctx, false),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(ctx, true),
            child: const Text('Remove all'),
          ),
        ],
      ),
    );
    if (confirmed != true || !context.mounted) return;
    final messenger = ScaffoldMessenger.of(context);
    final message = await service.removeAllDevices();
    messenger
      ..hideCurrentSnackBar()
      ..showSnackBar(SnackBar(content: Text(message)));
  }

  Future<void> _confirmFactoryReset(
    BuildContext context,
    SyncService service,
  ) async {
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: const Text('Factory reset?'),
        content: const Text(
            'This erases the device identity and all paired devices, '
            'folders, history and metadata, restoring a fresh-install state. '
            'A new device id will be generated. Local files are kept. '
            'This cannot be undone.'),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(ctx, false),
            child: const Text('Cancel'),
          ),
          FilledButton(
            style: FilledButton.styleFrom(
              backgroundColor: Theme.of(context).colorScheme.error,
            ),
            onPressed: () => Navigator.pop(ctx, true),
            child: const Text('Factory reset'),
          ),
        ],
      ),
    );
    if (confirmed != true || !context.mounted) return;
    final messenger = ScaffoldMessenger.of(context);
    final message = await service.factoryReset();
    messenger
      ..hideCurrentSnackBar()
      ..showSnackBar(SnackBar(content: Text(message)));
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

  Future<void> _toggleAutoRepair(
      BuildContext context, SyncService service, bool enabled) async {
    final messenger = ScaffoldMessenger.of(context);
    await service.setAutoRepairEnabled(enabled);
    messenger
      ..hideCurrentSnackBar()
      ..showSnackBar(SnackBar(
        content: Text(enabled
            ? 'Auto re-pair will run on the next startup'
            : 'Auto re-pair turned off'),
      ));
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
    if (!context.mounted) return;

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
Future<void> _copyDeviceId(
      BuildContext context, SyncService service) async {
    await Clipboard.setData(ClipboardData(text: service.deviceId));
    if (!context.mounted) return;
    ScaffoldMessenger.of(context)
      ..hideCurrentSnackBar()
      ..showSnackBar(const SnackBar(content: Text('Device ID copied')));
  }
}

/// Grouped card used for each Settings section.
class _Section extends StatelessWidget {
  const _Section({required this.label, required this.children});

  final String label;
  final List<Widget> children;

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        const SizedBox(height: 12),
        Padding(
          padding: const EdgeInsets.fromLTRB(4, 8, 4, 6),
          child: Text(
            label.toUpperCase(),
            style: Theme.of(context).textTheme.labelSmall!.copyWith(
                  letterSpacing: 1.1,
                  fontWeight: FontWeight.w700,
                  color: Theme.of(context).colorScheme.outline,
                ),
          ),
        ),
        Card(child: Column(children: children)),
      ],
    );
  }
}
