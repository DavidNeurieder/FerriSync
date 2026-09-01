import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import '../models/sync_models.dart';
import '../providers/sync_provider.dart';
import '../theme/ferri_theme.dart';
import 'storage_permission.dart';

/// One folder ↔ device selection with its sync mode, gathered by the wizard.
class _PeerSelection {
  _PeerSelection(this.device, this.mode);
  final Device device;
  String mode; // bidirectional | send_only | receive_only
}

/// Runs the full "add a folder" journey and reports whether a folder was
/// actually configured. Shared between the Folders screen and the onboarding
/// wizard so both paths behave identically:
///
///   storage permission → pick a directory → choose one or more peers
///   (each with a sync mode) → review → add.
///
/// Multi-device: one local folder can sync with several peers at once, each
/// with its own mode. Returns true when a folder was successfully added.
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
    final selections = await _pickDevices(context, service, devices);
    if (selections.isEmpty) return false;
    if (!context.mounted) return false;

    final label = result
        .split(RegExp(r'[/\\]'))
        .where((s) => s.isNotEmpty)
        .last;
    final name = await _askFolderName(context, label);
    if (name == null) return false;
    if (!context.mounted) return false;

    final proceed = await _reviewSetup(context, service, result, selections);
    if (!proceed || !context.mounted) return false;

    try {
      await service.addSyncFolderWithPeers(
        result,
        name,
        [
          for (final s in selections)
            (deviceId: s.device.id, mode: s.mode, remotePath: null),
        ],
      );
      if (context.mounted) {
        final names = selections.map((s) => s.device.name).join(', ');
        _snack(context, 'Syncing $label with $names. '
            'Enable the same folder on each device to start.');
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

/// Multi-device picker with a per-device sync mode. Returns the chosen
/// (device, mode) pairs, or an empty list when cancelled.
Future<List<_PeerSelection>> _pickDevices(BuildContext context,
    SyncService service, List<Device> devices) async {
  final state = _PickerState(devices);
  final confirmed = await showDialog<bool>(
    context: context,
    builder: (ctx) => StatefulBuilder(
      builder: (ctx, setState) {
        return AlertDialog(
          title: const Text('Choose Devices'),
          content: SizedBox(
            width: double.maxFinite,
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                for (final d in devices)
                  CheckboxListTile(
                    value: state.isSelected(d.id),
                    onChanged: (v) => setState(() => state.toggle(d.id, v ?? false)),
                    title: Text(d.name),
                    subtitle: Text(d.id),
                    secondary: state.isSelected(d.id)
                        ? _ModeDropdown(
                            mode: state.modeOf(d.id),
                            onChanged: (m) =>
                                setState(() => state.setMode(d.id, m)),
                          )
                        : const Icon(Icons.devices),
                  ),
              ],
            ),
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(ctx, false),
              child: const Text('Cancel'),
            ),
            FilledButton(
              onPressed: () => Navigator.pop(ctx, true),
              child: const Text('Continue'),
            ),
          ],
        );
      },
    ),
  );

  if (!(confirmed ?? false)) return [];
  return state.selectedEntries(devices);
}

class _PickerState {
  _PickerState(this.all);
  final List<Device> all;
  final Map<String, String> _modes = {};

  bool isSelected(String id) => _modes.containsKey(id);
  String modeOf(String id) => _modes[id] ?? 'bidirectional';
  void toggle(String id, bool selected) {
    if (selected) {
      _modes.putIfAbsent(id, () => 'bidirectional');
    } else {
      _modes.remove(id);
    }
  }

  void setMode(String id, String mode) => _modes[id] = mode;
  List<_PeerSelection> selectedEntries(List<Device> devices) =>
      [for (final d in devices) if (isSelected(d.id)) _PeerSelection(d, modeOf(d.id))];
}

class _ModeDropdown extends StatelessWidget {
  const _ModeDropdown({required this.mode, required this.onChanged});
  final String mode;
  final ValueChanged<String> onChanged;

  @override
  Widget build(BuildContext context) {
    return DropdownButton<String>(
      value: mode,
      isDense: true,
      items: const [
        DropdownMenuItem(value: 'bidirectional', child: Text('Two-way')),
        DropdownMenuItem(value: 'send_only', child: Text('Send only')),
        DropdownMenuItem(value: 'receive_only', child: Text('Receive only')),
      ],
      onChanged: (m) {
        if (m != null) onChanged(m);
      },
    );
  }
}

String _modeLabel(String mode) => switch (mode) {
      'send_only' => 'Send only',
      'receive_only' => 'Receive only',
      _ => 'Two-way',
    };

Future<String?> _askFolderName(BuildContext context, String fallback) {
  final controller = TextEditingController(text: fallback);
  return showDialog<String>(
    context: context,
    builder: (ctx) => AlertDialog(
      title: const Text('Folder name'),
      content: TextField(
        controller: controller,
        autofocus: true,
        decoration: const InputDecoration(labelText: 'Name'),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(ctx),
          child: const Text('Cancel'),
        ),
        FilledButton(
          onPressed: () => Navigator.pop(ctx, controller.text.trim()),
          child: const Text('Save'),
        ),
      ],
    ),
  );
}

/// Review step shown before anything is configured: the last place the user
/// sees what will happen, including every peer and its mode.
Future<bool> _reviewSetup(BuildContext context, SyncService service,
    String localPath, List<_PeerSelection> selections) async {
  final label = localPath
      .split(RegExp(r'[/\\]'))
      .where((s) => s.isNotEmpty)
      .last;
  final names = selections.map((s) => s.device.name).join(', ');
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
                _ReviewRow(label: 'Local folder', value: localPath),
                const SizedBox(height: 8),
                Text('Remote devices',
                    style: Theme.of(ctx).textTheme.labelLarge),
                const SizedBox(height: 4),
                for (final s in selections)
                  _ReviewRow(
                    label: s.device.name,
                    value: _modeLabel(s.mode),
                  ),
                const SizedBox(height: FerriTokens.spaceL),
                Text(
                  'Enable the same folder on $names to start syncing.',
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
