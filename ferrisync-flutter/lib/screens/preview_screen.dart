import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import '../models/sync_models.dart';
import '../providers/sync_provider.dart';
import '../theme/ferri_theme.dart';
import '../utils/format_bytes.dart';

/// "Ready to synchronize" review screen. Shows what a sync against one peer
/// would transfer (counts + bytes), then either runs it or (right now) leaves
/// the heavy lifting to the existing sync path. A dry-run backs every figure,
/// so nothing moves until the user taps "Sync now".
class PreviewScreen extends ConsumerStatefulWidget {
  const PreviewScreen({
    super.key,
    required this.folder,
    required this.deviceId,
    required this.peerName,
  });

  final SyncFolder folder;
  final String deviceId;
  final String peerName;

  @override
  ConsumerState<PreviewScreen> createState() => _PreviewScreenState();
}

class _PreviewScreenState extends ConsumerState<PreviewScreen> {
  SyncPreview? _preview;
  bool _loading = true;
  bool _syncing = false;
  String? _error;
  String _label = '';

  @override
  void initState() {
    super.initState();
    _label = widget.folder.name.isEmpty
        ? widget.folder.localPath
            .split(RegExp(r'[/\\]'))
            .where((s) => s.isNotEmpty)
            .last
        : widget.folder.name;
    _loadPreview();
  }

  Future<void> _loadPreview() async {
    final service = ref.read(syncServiceProvider);
    if (!mounted) return;
    setState(() {
      _loading = true;
      _error = null;
    });
    try {
      final preview = await service.previewSyncFolder(widget.folder, widget.deviceId);
      if (!mounted) return;
      setState(() {
        _preview = preview;
        _loading = false;
      });
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _error = e.toString();
        _loading = false;
      });
    }
  }

  Future<void> _syncNow() async {
    if (_syncing) return;
    setState(() => _syncing = true);
    final service = ref.read(syncServiceProvider);
    final message = await service.syncFolderNow(widget.folder);
    if (!mounted) return;
    setState(() => _syncing = false);
    ScaffoldMessenger.of(context)
      ..hideCurrentSnackBar()
      ..showSnackBar(SnackBar(content: Text(message)));
    Navigator.of(context).pop();
  }

  @override
  Widget build(BuildContext context) {
    final palette = context.ferri;
    final textTheme = Theme.of(context).textTheme;
    return Scaffold(
      appBar: AppBar(title: const Text('Review changes')),
      body: RefreshIndicator(
        onRefresh: _loadPreview,
        child: ListView(
          padding: const EdgeInsets.all(FerriTokens.spaceL),
          children: [
            Card(
              child: Padding(
                padding: const EdgeInsets.all(FerriTokens.spaceL),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Row(
                      children: [
                        Icon(Icons.folder_outlined, color: palette.primary, size: 22),
                        const SizedBox(width: FerriTokens.spaceM),
                        Expanded(
                          child: Text(
                            _label,
                            style: textTheme.titleMedium!
                                .copyWith(fontWeight: FontWeight.w700),
                            overflow: TextOverflow.ellipsis,
                          ),
                        ),
                      ],
                    ),
                    const SizedBox(height: 4),
                    Text(
                      '$_label ↔ ${widget.peerName}',
                      style: textTheme.bodySmall!.copyWith(color: palette.muted),
                    ),
                  ],
                ),
              ),
            ),
            const SizedBox(height: FerriTokens.spaceM),
            if (_loading)
              const Padding(
                padding: EdgeInsets.symmetric(vertical: FerriTokens.spaceXL),
                child: Center(child: CircularProgressIndicator()),
              )
            else if (_error != null)
              _MessageCard(
                icon: Icons.error_outline,
                color: palette.danger,
                message: 'Could not preview changes.',
                detail: '$_error',
              )
            else if (_preview == null)
              _MessageCard(
                icon: Icons.help_outline,
                color: palette.muted,
                message: 'No preview available.',
                detail: 'The device may be offline right now.',
              )
            else if (_preview!.isEmpty)
              Card(
                child: Padding(
                  padding: const EdgeInsets.all(FerriTokens.spaceL),
                  child: Row(
                    children: [
                      Icon(Icons.check_circle, color: palette.success, size: 28),
                      const SizedBox(width: FerriTokens.spaceM),
                      Expanded(
                        child: Text(
                          'Up to date',
                          style: textTheme.titleMedium!
                              .copyWith(fontWeight: FontWeight.w700),
                        ),
                      ),
                    ],
                  ),
                ),
              )
            else
              _buildPreview(palette, textTheme),
            const SizedBox(height: FerriTokens.spaceL),
            if (_preview != null && !_preview!.isEmpty)
              FilledButton.icon(
                key: const ValueKey('preview_sync_now'),
                onPressed: _syncing ? null : _syncNow,
                icon: _syncing
                    ? const SizedBox(
                        width: 16,
                        height: 16,
                        child: CircularProgressIndicator(strokeWidth: 2),
                      )
                    : const Icon(Icons.sync, size: 18),
                label: Text(_syncing ? 'Syncing…' : 'Sync now'),
              ),
          ],
        ),
      ),
    );
  }

  Widget _buildPreview(FerriPalette palette, TextTheme textTheme) {
    final p = _preview!;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Card(
          child: Padding(
            padding: const EdgeInsets.all(FerriTokens.spaceL),
            child: Column(
              children: [
                _PreviewRow(
                  icon: Icons.arrow_upward,
                  color: palette.primary,
                  label: 'To ${widget.peerName}',
                  value: '${p.wouldPush}',
                  bytes: p.pushBytes,
                ),
                const Divider(height: 20),
                _PreviewRow(
                  icon: Icons.arrow_downward,
                  color: palette.syncing,
                  label: 'From ${widget.peerName}',
                  value: '${p.wouldPull}',
                  bytes: p.pullBytes,
                ),
                if (p.wouldConflict > 0) ...[
                  const Divider(height: 20),
                  _PreviewRow(
                    icon: Icons.warning_amber_rounded,
                    color: palette.danger,
                    label: 'Conflicts',
                    value: '${p.wouldConflict}',
                    bytes: 0,
                  ),
                ],
                const Divider(height: 20),
                Row(
                  mainAxisAlignment: MainAxisAlignment.spaceBetween,
                  children: [
                    Text(
                      'Total',
                      style: textTheme.bodyMedium!
                          .copyWith(color: palette.muted),
                    ),
                    Text(
                      formatBytes(p.pushBytes + p.pullBytes),
                      style: textTheme.titleMedium!
                          .copyWith(fontWeight: FontWeight.w700),
                    ),
                  ],
                ),
              ],
            ),
          ),
        ),
      ],
    );
  }
}

class _PreviewRow extends StatelessWidget {
  const _PreviewRow({
    required this.icon,
    required this.color,
    required this.label,
    required this.value,
    required this.bytes,
  });

  final IconData icon;
  final Color color;
  final String label;
  final String value;
  final int bytes;

  @override
  Widget build(BuildContext context) {
    final textTheme = Theme.of(context).textTheme;
    final palette = context.ferri;
    final count = int.tryParse(value) ?? 0;
    return Row(
      children: [
        Icon(icon, color: color, size: 20),
        const SizedBox(width: FerriTokens.spaceM),
        Expanded(
          child: Text(label, style: textTheme.bodyMedium),
        ),
        Text(
          '$value file${count == 1 ? '' : 's'}',
          style: textTheme.bodyMedium!.copyWith(fontWeight: FontWeight.w600),
        ),
        if (bytes > 0) ...[
          const SizedBox(width: FerriTokens.spaceS),
          Text(
            '· ${formatBytes(bytes)}',
            style: textTheme.bodySmall!.copyWith(color: palette.muted),
          ),
        ],
      ],
    );
  }
}

class _MessageCard extends StatelessWidget {
  const _MessageCard({
    required this.icon,
    required this.color,
    required this.message,
    this.detail,
  });

  final IconData icon;
  final Color color;
  final String message;
  final String? detail;

  @override
  Widget build(BuildContext context) {
    final textTheme = Theme.of(context).textTheme;
    final palette = context.ferri;
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(FerriTokens.spaceL),
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Icon(icon, color: color, size: 28),
            const SizedBox(width: FerriTokens.spaceM),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(message, style: textTheme.titleMedium),
                  if (detail != null) ...[
                    const SizedBox(height: 4),
                    Text(
                      detail!,
                      style:
                          textTheme.bodySmall!.copyWith(color: palette.muted),
                    ),
                  ],
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}
