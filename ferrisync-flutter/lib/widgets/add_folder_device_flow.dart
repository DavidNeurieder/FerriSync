import 'package:flutter/material.dart';
import '../models/sync_models.dart';
import '../providers/sync_provider.dart';

/// Add one more device to an existing folder: pick a paired device, choose its
/// sync mode, and set where the folder's copy lives on that device. Runs the
/// whole "add device to this folder" journey and reports success.
Future<bool> runAddFolderDeviceFlow(
  BuildContext context,
  SyncService service, {
  required int folderId,
  required String localPath,
  required List<FolderPeer> existingPeers,
}) async {
  await service.refresh();
  if (!context.mounted) return false;

  final devices = service.devices
      .where((d) => d.id != service.deviceId)
      .where((d) => !existingPeers.any((p) => p.deviceId == d.id))
      .toList();
  if (devices.isEmpty) {
    ScaffoldMessenger.of(context)
      ..hideCurrentSnackBar()
      ..showSnackBar(const SnackBar(content: Text('No other paired devices')));
    return false;
  }

  final result = await showDialog<({String deviceId, String mode, String? remotePath})>(
    context: context,
    builder: (ctx) => _AddDeviceDialog(
      devices: devices,
      localPath: localPath,
    ),
  );
  if (result == null || !context.mounted) return false;

  try {
    await service.addDeviceToFolder(
      folderId,
      result.deviceId,
      localPath,
      mode: result.mode,
      remotePath: result.remotePath,
    );
    if (context.mounted) {
      ScaffoldMessenger.of(context)
        ..hideCurrentSnackBar()
        ..showSnackBar(const SnackBar(content: Text('Now syncing with the added device')));
    }
    return true;
  } catch (e) {
    if (context.mounted) {
      ScaffoldMessenger.of(context)
        ..hideCurrentSnackBar()
        ..showSnackBar(SnackBar(content: Text('Failed: $e')));
    }
    return false;
  }
}

class _AddDeviceDialog extends StatefulWidget {
  const _AddDeviceDialog({required this.devices, required this.localPath});

  final List<Device> devices;
  final String localPath;

  @override
  State<_AddDeviceDialog> createState() => _AddDeviceDialogState();
}

class _AddDeviceDialogState extends State<_AddDeviceDialog> {
  String? _selected;
  String _mode = 'bidirectional';
  final _remoteController = TextEditingController();

  @override
  void dispose() {
    _remoteController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final textTheme = Theme.of(context).textTheme;
    final palette = Theme.of(context).colorScheme;
    return AlertDialog(
      title: const Text('Add device'),
      content: SizedBox(
        width: double.maxFinite,
        child: SingleChildScrollView(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                'Where should this folder live on the chosen device?',
                style: textTheme.bodySmall,
              ),
              const SizedBox(height: 8),
              DropdownButtonFormField<String>(
                key: ValueKey(_selected),
                initialValue: _selected,
                isExpanded: true,
                decoration: const InputDecoration(
                  labelText: 'Device',
                  border: OutlineInputBorder(),
                ),
                items: [
                  for (final d in widget.devices)
                    DropdownMenuItem(value: d.id, child: Text(d.name)),
                ],
                onChanged: (v) => setState(() => _selected = v),
              ),
              const SizedBox(height: 12),
              DropdownButtonFormField<String>(
                key: ValueKey(_mode),
                initialValue: _mode,
                isExpanded: true,
                decoration: const InputDecoration(
                  labelText: 'Sync mode',
                  border: OutlineInputBorder(),
                ),
                items: const [
                  DropdownMenuItem(value: 'bidirectional', child: Text('Two-way')),
                  DropdownMenuItem(
                      value: 'send_only', child: Text('Send only')),
                  DropdownMenuItem(
                      value: 'receive_only', child: Text('Receive only')),
                ],
                onChanged: (v) {
                  if (v != null) setState(() => _mode = v);
                },
              ),
              const SizedBox(height: 12),
              TextField(
                controller: _remoteController,
                decoration: const InputDecoration(
                  labelText: 'Destination path on device',
                  hintText: 'Defaults to the local path',
                  border: OutlineInputBorder(),
                ),
              ),
              Text(
                'The device will store this folder there. Leave empty to use the same path as this machine.',
                style: textTheme.bodySmall!.copyWith(color: palette.outline),
              ),
            ],
          ),
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: const Text('Cancel'),
        ),
        FilledButton(
          onPressed: _selected == null
              ? null
              : () {
                  final remote = _remoteController.text.trim();
                  Navigator.pop(
                    context,
                    (
                      deviceId: _selected!,
                      mode: _mode,
                      remotePath: remote.isEmpty ? null : remote,
                    ),
                  );
                },
          child: const Text('Add device'),
        ),
      ],
    );
  }
}