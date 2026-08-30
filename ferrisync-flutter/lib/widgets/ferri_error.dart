import 'package:flutter/material.dart';
import '../theme/ferri_theme.dart';

/// Standard human-readable error component: What happened, why, what's next,
/// and one obvious recovery action.
class FerriError extends StatelessWidget {
  const FerriError({
    super.key,
    required this.title,
    this.why,
    this.next,
    this.actionLabel = 'Try again',
    this.onAction,
    this.compact = false,
  });

  final String title;
  final String? why;
  final String? next;
  final String actionLabel;
  final VoidCallback? onAction;
  final bool compact;

  @override
  Widget build(BuildContext context) {
    final palette = context.ferri;
    final textTheme = Theme.of(context).textTheme;

    final content = Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Icon(Icons.error_outline, color: palette.danger, size: compact ? 20 : 24),
            const SizedBox(width: FerriTokens.spaceS),
            Expanded(
              child: Text(
                title,
                style: textTheme.bodyMedium!.copyWith(
                  color: palette.danger,
                  fontWeight: FontWeight.w600,
                ),
              ),
            ),
          ],
        ),
        if ((why ?? '').isNotEmpty) ...[
          const SizedBox(height: FerriTokens.spaceXS),
          Text(
            why!,
            style: textTheme.bodySmall!.copyWith(
                color: Theme.of(context).colorScheme.onSurfaceVariant),
          ),
        ],
        if ((next ?? '').isNotEmpty) ...[
          const SizedBox(height: FerriTokens.spaceXS),
          Text(
            next!,
            style: textTheme.bodySmall!.copyWith(color: palette.muted),
          ),
        ],
        if (onAction != null) ...[
          const SizedBox(height: FerriTokens.spaceM),
          Align(
            alignment: Alignment.centerLeft,
            child: FilledButton.tonalIcon(
              onPressed: onAction,
              icon: const Icon(Icons.refresh, size: 16),
              label: Text(actionLabel),
            ),
          ),
        ],
      ],
    );

    if (compact) return content;

    return Container(
      padding: const EdgeInsets.all(FerriTokens.spaceL),
      decoration: BoxDecoration(
        color: palette.danger.withValues(alpha: 0.08),
        borderRadius: BorderRadius.circular(FerriTokens.radiusM),
        border: Border.all(color: palette.danger.withValues(alpha: 0.30)),
      ),
      child: content,
    );
  }
}