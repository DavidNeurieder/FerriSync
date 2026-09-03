import 'package:flutter/material.dart';
import '../gen/api.dart' as frb;
import '../models/sync_models.dart';
import '../providers/sync_provider.dart';
import '../theme/ferri_theme.dart';

/// Bottom sheet that browses a paired device's discoverable shared folders
/// (over TLS, after trust) and lets the user request pairing to one. The peer
/// is reached automatically via its last-known address — no IP/port to enter.
class BrowseSharedFoldersSheet extends StatefulWidget {
  const BrowseSharedFoldersSheet({
    super.key,
    required this.device,
    required this.service,
  });

  final Device device;
  final SyncService service;

  @override
  State<BrowseSharedFoldersSheet> createState() => _BrowseSheetState();
}

class _BrowseSheetState extends State<BrowseSharedFoldersSheet> {
  List<frb.RemoteSharedFolder> _folders = const [];
  bool _busy = false;
  String? _error;

  @override
  void initState() {
    super.initState();
    _autoBrowse();
  }

  Future<void> _autoBrowse() async {
    setState(() {
      _busy = true;
      _error = null;
    });
    final folders = await widget.service.remoteFoldersFor(widget.device);
    if (!mounted) return;
    setState(() {
      _folders = folders;
      _busy = false;
      if (folders.isEmpty) {
        _error = 'No discoverable shared folders on this device.';
      }
    });
  }

  String _modeLabel(String mode) => switch (mode) {
        'push' || 'send_only' => 'Send only',
        'pull' || 'receive_only' => 'Receive only',
        _ => 'Two-way',
      };

  Future<void> _request(BuildContext context, frb.RemoteSharedFolder f) async {
    final result = await widget.service.syncRemoteFolder(
      device: widget.device,
      folder: f,
    );
    if (!context.mounted) return;
    if (result.folderGuid == null) {
      ScaffoldMessenger.of(context)
        ..hideCurrentSnackBar()
        ..showSnackBar(SnackBar(content: Text(result.message)));
      return;
    }
    // Pair approved: close the sheet, then reflect the newly-paired folder.
    await widget.service.refresh();
    if (!context.mounted) return;
    Navigator.of(context).pop();
    ScaffoldMessenger.of(context)
      ..hideCurrentSnackBar()
      ..showSnackBar(SnackBar(content: Text('Paired to "${f.name}".')));
  }

  @override
  Widget build(BuildContext context) {
    final palette = context.ferri;
    final textTheme = Theme.of(context).textTheme;
    return SafeArea(
      child: Padding(
        padding: const EdgeInsets.fromLTRB(24, 4, 24, 16),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Row(
              children: [
                Icon(Icons.folder_shared_outlined, color: palette.primary),
                const SizedBox(width: FerriTokens.spaceM),
                Expanded(
                  child: Text(
                    'Browse shared folders · ${widget.device.name}',
                    style: textTheme.titleLarge!
                        .copyWith(fontWeight: FontWeight.w700),
                    overflow: TextOverflow.ellipsis,
                  ),
                ),
              ],
            ),
            const SizedBox(height: FerriTokens.spaceL),
            if (_busy)
              const Padding(
                padding: EdgeInsets.symmetric(vertical: FerriTokens.spaceL),
                child: Center(child: CircularProgressIndicator()),
              )
            else if (_error != null) ...[
              Text(
                _error!,
                style: textTheme.bodySmall!.copyWith(color: palette.muted),
              ),
              const SizedBox(height: FerriTokens.spaceM),
              OutlinedButton.icon(
                onPressed: _autoBrowse,
                icon: const Icon(Icons.refresh, size: 18),
                label: const Text('Try again'),
              ),
            ] else if (_folders.isEmpty)
              Padding(
                padding:
                    const EdgeInsets.symmetric(vertical: FerriTokens.spaceL),
                child: Column(
                  children: [
                    Icon(Icons.folder_open, color: palette.muted, size: 32),
                    const SizedBox(height: FerriTokens.spaceM),
                    const Text('Nothing discoverable from that device.'),
                  ],
                ),
              )
            else ...[
              Text(
                'FOLDERS SHARED',
                style: textTheme.labelSmall!.copyWith(
                  letterSpacing: 1.1,
                  fontWeight: FontWeight.w700,
                  color: palette.muted,
                ),
              ),
              const SizedBox(height: FerriTokens.spaceS),
              Flexible(
                child: ListView.builder(
                  shrinkWrap: true,
                  itemCount: _folders.length,
                  itemBuilder: (ctx, i) {
                    final f = _folders[i];
                    return ListTile(
                      key: ValueKey('browse_${f.folderGuid}'),
                      leading: const Icon(Icons.folder_outlined),
                      title: Text(f.name),
                      subtitle: Text(
                        _modeLabel(f.mode),
                        style: textTheme.bodySmall!
                            .copyWith(color: palette.muted),
                      ),
                      trailing: const Icon(Icons.add_link),
                      onTap: () => _request(ctx, f),
                    );
                  },
                ),
              ),
            ],
          ],
        ),
      ),
    );
  }
}