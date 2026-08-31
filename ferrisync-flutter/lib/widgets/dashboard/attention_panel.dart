import 'package:flutter/material.dart';
import 'package:go_router/go_router.dart';
import '../../models/sync_models.dart';
import '../../theme/ferri_theme.dart';

/// Unified "what needs me?" panel. Only shown when something genuinely needs
/// attention, so a healthy app never interrupts.
class AttentionPanel extends StatelessWidget {
  const AttentionPanel({super.key, required this.items});

  final List<AttentionItem> items;

  @override
  Widget build(BuildContext context) {
    final palette = context.ferri;
    final textTheme = Theme.of(context).textTheme;
    return Card(
      margin: EdgeInsets.zero,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(
              FerriTokens.spaceL,
              FerriTokens.spaceM,
              FerriTokens.spaceL,
              FerriTokens.spaceS,
            ),
            child: Text(
              'NEEDS ATTENTION',
              style: textTheme.labelSmall!.copyWith(
                letterSpacing: 1.1,
                fontWeight: FontWeight.w700,
                color: palette.danger,
              ),
            ),
          ),
          for (final item in items)
            ListTile(
              dense: true,
              leading: Icon(
                _iconFor(item.kind),
                size: 20,
                color: item.kind == AttentionKind.conflictFiles
                    ? palette.danger
                    : palette.muted,
              ),
              title: Text(item.label, style: textTheme.bodyMedium),
              trailing:
                  const Icon(Icons.chevron_right, size: 18, color: Colors.grey),
              onTap: () => switch (item.kind) {
                AttentionKind.conflictFiles => context.go('/conflicts'),
                AttentionKind.offlineDevice => context.go('/devices'),
                AttentionKind.folderHealth => context.go('/folders'),
              },
            ),
        ],
      ),
    );
  }

  IconData _iconFor(AttentionKind kind) => switch (kind) {
        AttentionKind.conflictFiles => Icons.warning_amber_rounded,
        AttentionKind.offlineDevice ||
        AttentionKind.folderHealth =>
          Icons.cloud_off,
      };
}