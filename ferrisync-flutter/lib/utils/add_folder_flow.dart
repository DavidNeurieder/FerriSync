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

String _modeLabel(String mode) => switch (mode) {
      'push' || 'send_only' => 'Send only',
      'pull' || 'receive_only' => 'Receive only',
      _ => 'Two-way',
    };

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
/// (using the chosen [localPath] as this device's copy) and closes the flow.
Future<bool> _pickRemoteFolder(BuildContext context, SyncService service,
    List<Device> devices, String localPath) async {
  final added = await Navigator.of(context).push<bool>(
    MaterialPageRoute(
      builder: (_) => _ChooseRemoteFolderPage(
        service: service,
        devices: devices,
        localPath: localPath,
      ),
    ),
  );
  return added ?? false;
}

class _ChooseRemoteFolderPage extends StatefulWidget {
  const _ChooseRemoteFolderPage({
    required this.service,
    required this.devices,
    required this.localPath,
  });
  final SyncService service;
  final List<Device> devices;
  final String localPath;

  @override
  State<_ChooseRemoteFolderPage> createState() =>
      _ChooseRemoteFolderPageState();
}

class _ChooseRemoteFolderPageState extends State<_ChooseRemoteFolderPage> {
  Device? _selected;

  void _toggle(Device d) {
    setState(() {
      _selected = _selected?.id == d.id ? null : d;
    });
  }

  @override
  Widget build(BuildContext context) {
    final textTheme = Theme.of(context).textTheme;
    final palette = context.ferri;
    return Scaffold(
      appBar: AppBar(
        title: const Text('Sync with another device'),
      ),
      body: SafeArea(
        child: ListView(
          padding: const EdgeInsets.fromLTRB(16, 8, 16, 24),
          children: [
            Text(
              'Step 2 of 2 · Choose a device',
              style: textTheme.labelSmall!.copyWith(
                letterSpacing: 1.1,
                fontWeight: FontWeight.w700,
                color: palette.primary,
              ),
            ),
            const SizedBox(height: 4),
            Text(
              'Your local folder:\n${widget.localPath}',
              style: textTheme.bodySmall!.copyWith(color: palette.muted),
            ),
            const SizedBox(height: FerriTokens.spaceL),
            for (final d in widget.devices) ...[
              Card(
                color: _selected?.id == d.id
                    ? palette.primary.withValues(alpha: 0.08)
                    : palette.surfaceHigh,
                child: InkWell(
                  key: ValueKey('choose_device_${d.id}'),
                  onTap: () => _toggle(d),
                  borderRadius: BorderRadius.circular(12),
                  child: Padding(
                    padding: const EdgeInsets.symmetric(
                        horizontal: 16, vertical: 12),
                    child: Row(
                      children: [
                        Icon(Icons.devices, color: palette.primary),
                        const SizedBox(width: FerriTokens.spaceM),
                        Expanded(
                          child: Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              Text(
                                d.name,
                                style: textTheme.titleMedium,
                              ),
                              const SizedBox(height: 2),
                              Text(
                                d.lastSeen == 0
                                    ? 'Never seen online'
                                    : 'Available',
                                style: textTheme.bodySmall!
                                    .copyWith(color: palette.muted),
                              ),
                            ],
                          ),
                        ),
                        Icon(
                          _selected?.id == d.id
                              ? Icons.expand_more
                              : Icons.chevron_right,
                          color: palette.muted,
                        ),
                      ],
                    ),
                  ),
                ),
              ),
              const SizedBox(height: FerriTokens.spaceS),
              if (_selected?.id == d.id)
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
                _modeLabel(f.mode),
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