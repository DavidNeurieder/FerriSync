import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import '../models/sync_models.dart';
import '../providers/sync_provider.dart';
import '../theme/ferri_theme.dart';
import 'storage_permission.dart';

/// Runs the full "add a folder" journey and reports whether a folder was
/// actually configured. Shared between the Folders screen and the onboarding
/// wizard so both paths behave identically:
///
///   storage permission → pick a directory → choose a peer → review → add.
///
/// Returns true when a folder was successfully added, false when the user
/// cancelled, lacked a paired device, or the setup failed.
Future<bool> runAddFolderFlow(BuildContext context, SyncService service) async {
  if (!await ensureStorageAccess(context)) return false;

  if (context.mounted) {
    final result = await FilePicker.platform.getDirectoryPath();
    if (result == null) return false;

    if (!context.mounted) return false;
    await service.refresh();
    if (!context.mounted) return false;

    final devices = service.devices;
    if (devices.isEmpty) {
      _snack(context, 'Pair a device first');
      return false;
    }

    if (!context.mounted) return false;
    final device = await _pickDevice(context, service, devices);
    if (device == null) return false;
    if (!context.mounted) return false;

    final proceed = await _reviewSetup(context, service, result, device);
    if (!proceed || !context.mounted) return false;

    try {
      await service.addSyncFolder(result, device.id);
      if (context.mounted) {
        _snack(context, 'Syncing — ${service.deviceName} ↔ ${device.name}. '
            'Enable this folder on ${device.name} to start.');
      }
      return true;
    } catch (e) {
      if (context.mounted) _snack(context, 'Failed: $e');
      return false;
    }
  }
  return false;
}

void _snack(BuildContext context, String message) {
  ScaffoldMessenger.of(context)
    ..hideCurrentSnackBar()
    ..showSnackBar(SnackBar(content: Text(message)));
}

Future<Device?> _pickDevice(
    BuildContext context, SyncService service, List<Device> devices) {
  return showDialog<Device>(
    context: context,
    builder: (ctx) => AlertDialog(
      title: const Text('Select Device'),
      content: SizedBox(
        width: double.maxFinite,
        child: ListView.builder(
          shrinkWrap: true,
          itemCount: devices.length,
          itemBuilder: (_, i) => ListTile(
            leading: const Icon(Icons.devices),
            title: Text(devices[i].name),
            subtitle: Text(devices[i].id),
            onTap: () => Navigator.pop(ctx, devices[i]),
          ),
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(ctx),
          child: const Text('Cancel'),
        ),
      ],
    ),
  );
}

/// Lightweight review step shown before anything is configured: the last
/// place the user sees what will happen.
Future<bool> _reviewSetup(BuildContext context, SyncService service,
    String localPath, Device device) async {
  final label = localPath
      .split(RegExp(r'[/\\]'))
      .where((s) => s.isNotEmpty)
      .last;
  return await showModalBottomSheet<bool>(
        context: context,
        showDragHandle: true,
        isScrollControlled: true,
        builder: (ctx) => SafeArea(
          child: SingleChildScrollView(
            padding: const EdgeInsets.fromLTRB(24, 4, 24, 24),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  'Ready to sync',
                  style: Theme.of(ctx).textTheme.headlineSmall!
                      .copyWith(fontWeight: FontWeight.w700),
                ),
                const SizedBox(height: 4),
                Text(label, style: Theme.of(ctx).textTheme.titleMedium),
                const SizedBox(height: FerriTokens.spaceL),
                _ReviewRow(label: 'This device', value: service.deviceName),
                _ReviewRow(label: 'Remote device', value: device.name),
                _ReviewRow(label: 'Local folder', value: localPath),
                const _ReviewRow(label: 'Sync mode', value: 'Automatic'),
                const SizedBox(height: FerriTokens.spaceL),
                Text(
                  'Enable the same folder on ${device.name} to start syncing.',
                  style: Theme.of(ctx).textTheme.bodySmall!
                      .copyWith(color: context.ferri.muted),
                ),
                const SizedBox(height: FerriTokens.spaceL),
                SizedBox(
                  width: double.infinity,
                  child: FilledButton.icon(
                    key: const ValueKey('review_start_syncing'),
                    onPressed: () => Navigator.pop(ctx, true),
                    icon: const Icon(Icons.play_arrow, size: 18),
                    label: const Text('Start syncing'),
                  ),
                ),
              ],
            ),
          ),
        ),
      ) ??
      false;
}

class _ReviewRow extends StatelessWidget {
  const _ReviewRow({required this.label, required this.value});

  final String label;
  final String value;

  @override
  Widget build(BuildContext context) {
    final textTheme = Theme.of(context).textTheme;
    final palette = context.ferri;
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 4),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          SizedBox(
            width: 130,
            child: Text(
              label,
              style: textTheme.bodySmall!.copyWith(color: palette.muted),
            ),
          ),
          Expanded(
            child: Text(value, style: textTheme.bodySmall),
          ),
        ],
      ),
    );
  }
}
