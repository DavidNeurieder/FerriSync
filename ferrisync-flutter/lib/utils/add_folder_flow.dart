import 'dart:io';

import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import '../gen/api.dart' as frb;
import '../models/sync_models.dart';
import '../providers/sync_provider.dart';
import '../theme/ferri_theme.dart';
import 'storage_permission.dart';

/// Add a folder by first choosing the local folder on this device, then
/// choosing which of a paired peer's shared folders to sync it to. Selecting a
/// peer's discoverable shared folder pairs to it using the chosen local folder
/// as this device's copy.
///
///   storage permission → pick the local folder → choose a peer's shared
///   folder → pair (tap the shared folder = done).
///
/// Returns true when a folder was successfully paired. Shared between the
/// Folders screen and the onboarding wizard so both behave identically.
Future<bool> runAddFolderFlow(BuildContext context, SyncService service) async {
  if (!await ensureStorageAccess(context)) return false;

  if (!context.mounted) return false;
  final localPath = await _pickDirectoryPath(context);
  if (localPath == null) return false;

  if (!context.mounted) return false;
  await service.refresh();
  if (!context.mounted) return false;

  final devices = service.devices;
  if (devices.isEmpty) {
    _snack(context, 'Pair a device first');
    return false;
  }

  return _pickRemoteFolder(context, service, devices, localPath);
}

void _snack(BuildContext context, String message) {
  ScaffoldMessenger.of(context)
    ..hideCurrentSnackBar()
    ..showSnackBar(SnackBar(content: Text(message)));
}

/// Picks the local folder to keep in sync. Prefers the native directory
/// dialog; on desktop (Linux/Windows/macOS) where `file_picker` shells out to
/// an external helper (`kdialog`/`zenity`/`qarma`) that may not be installed or
/// run headless, a cancelled (or failed) native picker falls back to a manual
/// path entry so the flow always does something instead of silently returning.
Future<String?> _pickDirectoryPath(BuildContext context) async {
  var result = await FilePicker.platform.getDirectoryPath();
  if (result != null) return result;

  final isDesktop =
      Platform.isLinux || Platform.isWindows || Platform.isMacOS;
  if (!isDesktop || !context.mounted) return null;

  final controller = TextEditingController();
  final entered = await showDialog<String>(
    context: context,
    builder: (ctx) => AlertDialog(
      title: const Text('Enter folder path'),
      content: TextField(
        controller: controller,
        autofocus: true,
        keyboardType: TextInputType.text,
        decoration: const InputDecoration(
          labelText: 'Absolute path to the folder to sync',
          hintText: '/home/you/Documents',
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(ctx),
          child: const Text('Cancel'),
        ),
        FilledButton(
          key: const ValueKey('manual_folder_confirm'),
          onPressed: () => Navigator.pop(ctx, controller.text.trim()),
          child: const Text('Use this folder'),
        ),
      ],
    ),
  );
  return (entered == null || entered.isEmpty) ? null : entered;
}

/// Choose a peer's shared folder to sync the picked local folder with.
/// Selecting a device loads its discoverable shared folders; tapping one pairs
/// (using the chosen [localPath] as this device's copy) and closes the dialog.
Future<bool> _pickRemoteFolder(BuildContext context, SyncService service,
    List<Device> devices, String localPath) async {
  final added = await showDialog<bool>(
    context: context,
    builder: (ctx) => _ChooseRemoteFolderDialog(
      service: service,
      devices: devices,
      localPath: localPath,
    ),
  );
  return added ?? false;
}

class _ChooseRemoteFolderDialog extends StatefulWidget {
  const _ChooseRemoteFolderDialog({
    required this.service,
    required this.devices,
    required this.localPath,
  });
  final SyncService service;
  final List<Device> devices;
  final String localPath;

  @override
  State<_ChooseRemoteFolderDialog> createState() =>
      _ChooseRemoteFolderDialogState();
}

class _ChooseRemoteFolderDialogState extends State<_ChooseRemoteFolderDialog> {
  final Set<String> _expanded = {};

  void _toggle(String id) {
    setState(() {
      if (!_expanded.add(id)) _expanded.remove(id);
    });
  }

  @override
  Widget build(BuildContext context) {
    final textTheme = Theme.of(context).textTheme;
    final palette = context.ferri;
    return AlertDialog(
      title: const Text('Choose Remote Folder'),
      content: SizedBox(
        width: double.maxFinite,
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxHeight: 480),
          child: ListView(
            shrinkWrap: true,
            children: [
              Padding(
                padding: const EdgeInsets.only(left: 16, bottom: 8),
                child: Text(
                  'Local folder: ${widget.localPath}',
                  maxLines: 2,
                  overflow: TextOverflow.ellipsis,
                  style: textTheme.bodySmall!.copyWith(color: palette.muted),
                ),
              ),
              for (final d in widget.devices) ...[
                CheckboxListTile(
                  value: _expanded.contains(d.id),
                  onChanged: (_) => _toggle(d.id),
                  title: Text(d.name),
                  subtitle: Text(d.id),
                  secondary:
                      _expanded.contains(d.id)
                          ? const Icon(Icons.expand_more)
                          : const Icon(Icons.devices),
                ),
                if (_expanded.contains(d.id))
                  _RemoteSharesList(
                    service: widget.service,
                    device: d,
                    localPath: widget.localPath,
                    onAdded: () => Navigator.of(context).pop(true),
                  ),
              ],
            ],
          ),
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context, false),
          child: const Text('Cancel'),
        ),
      ],
    );
  }
}

/// Browsed shared folders for one selected device. Tapping a folder pairs to it
/// (placing this device's copy at [localPath]) and reports success via
/// [onAdded].
class _RemoteSharesList extends StatefulWidget {
  const _RemoteSharesList({
    required this.service,
    required this.device,
    required this.localPath,
    required this.onAdded,
  });
  final SyncService service;
  final Device device;
  final String localPath;
  final VoidCallback onAdded;

  @override
  State<_RemoteSharesList> createState() => _RemoteSharesListState();
}

class _RemoteSharesListState extends State<_RemoteSharesList> {
  List<frb.RemoteSharedFolder> _folders = const [];
  bool _loading = true;
  bool _pairing = false;
  /// Set when the peer was unreachable or didn't answer the browse request.
  String? _reachError;
  /// True when the peer answered but had nothing published — distinguishes
  /// "not reachable" from "hasn't published any shared folders."
  bool _reachedNoShares = false;

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    setState(() {
      _loading = true;
      _reachError = null;
      _reachedNoShares = false;
    });
    List<frb.RemoteSharedFolder> folders;
    try {
      folders = await widget.service.remoteFoldersFor(widget.device);
    } catch (e) {
      folders = const [];
      if (mounted) {
        setState(() => _reachError = '$e');
      }
    }
    if (!mounted) return;
    setState(() {
      _folders = folders;
      _loading = false;
      _reachedNoShares = _reachError == null && folders.isEmpty;
    });
  }

  Future<void> _pair(frb.RemoteSharedFolder folder) async {
    if (_pairing) return;
    setState(() => _pairing = true);
    final result = await widget.service.syncRemoteFolder(
      device: widget.device,
      folder: folder,
      localPath: widget.localPath,
    );
    if (!mounted) return;
    setState(() => _pairing = false);
    _snack(context, result.message);
    if (result.folderGuid != null) {
      await widget.service.refresh();
      widget.onAdded();
    }
  }

  @override
  Widget build(BuildContext context) {
    final textTheme = Theme.of(context).textTheme;
    final palette = context.ferri;
    if (_loading) {
      return const Padding(
        padding: EdgeInsets.symmetric(vertical: 16),
        child: Center(child: CircularProgressIndicator()),
      );
    }
    if (_folders.isEmpty) {
      final message = _reachedNoShares
          ? "${widget.device.name} hasn't published any shared folders.\n"
              "Already-paired folders aren't listed here — a peer must publish "
              '(share) a folder for it to be discoverable and choosable.'
          : "Couldn't reach ${widget.device.name}: $_reachError\n"
              "Make sure it's running and on this network, then try again.";
      return Padding(
        padding: const EdgeInsets.fromLTRB(16, 0, 16, 12),
        child: Text(
          message,
          style: textTheme.bodySmall!.copyWith(color: palette.muted),
        ),
      );
    }
    return Padding(
      padding: const EdgeInsets.only(left: 16, right: 4),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Padding(
            padding: const EdgeInsets.only(top: 4, bottom: 4),
            child: Text(
              'SHARED BY ${widget.device.name.toUpperCase()}',
              style: textTheme.labelSmall!.copyWith(
                letterSpacing: 1.1,
                fontWeight: FontWeight.w700,
                color: palette.muted,
              ),
            ),
          ),
          for (final f in _folders)
            ListTile(
              key: ValueKey('device_${widget.device.id}_share_${f.folderGuid}'),
              dense: true,
              leading: Icon(f.mode == 'receive_only'
                  ? Icons.download_for_offline_outlined
                  : Icons.folder_outlined),
              title: Text(f.name),
              subtitle: Text(
                f.localPath.isEmpty ? f.mode : '${f.localPath} · ${f.mode}',
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: textTheme.bodySmall!.copyWith(color: palette.muted),
              ),
              trailing: _pairing
                  ? const SizedBox(
                      width: 16,
                      height: 16,
                      child: CircularProgressIndicator(strokeWidth: 2),
                    )
                  : const Icon(Icons.add_link),
              onTap: _pairing ? null : () => _pair(f),
            ),
        ],
      ),
    );
  }
}