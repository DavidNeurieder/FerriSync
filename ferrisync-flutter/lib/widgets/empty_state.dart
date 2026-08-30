import 'package:flutter/material.dart';
import '../theme/ferri_theme.dart';

/// Friendly empty/error state used across screens: icon, short title, an
/// optional explanation and an optional call-to-action.
class EmptyState extends StatelessWidget {
  const EmptyState({
    super.key,
    required this.icon,
    required this.title,
    this.subtitle,
    this.action,
  });

  final IconData icon;
  final String title;
  final String? subtitle;
  final Widget? action;

  @override
  Widget build(BuildContext context) {
    final palette = context.ferri;
    final textTheme = Theme.of(context).textTheme;
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(FerriTokens.spaceXL),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Container(
              padding: const EdgeInsets.all(FerriTokens.spaceL),
              decoration: BoxDecoration(
                color: palette.surfaceHigh,
                shape: BoxShape.circle,
              ),
              child: Icon(icon, size: 36, color: palette.muted),
            ),
            const SizedBox(height: FerriTokens.spaceL),
            Text(title, style: textTheme.titleMedium, textAlign: TextAlign.center),
            if (subtitle != null) ...[
              const SizedBox(height: FerriTokens.spaceS),
              Text(
                subtitle!,
                style: textTheme.bodyMedium!.copyWith(color: palette.muted),
                textAlign: TextAlign.center,
              ),
            ],
            if (action != null) ...[
              const SizedBox(height: FerriTokens.spaceL),
              action!,
            ],
          ],
        ),
      ),
    );
  }
}

/// Section heading used by the dashboard ("YOUR DEVICES", "SYNC ACTIVITY").
class SectionHeader extends StatelessWidget {
  const SectionHeader({
    super.key,
    required this.title,
    this.actionLabel,
    this.onAction,
  });

  final String title;
  final String? actionLabel;
  final VoidCallback? onAction;

  @override
  Widget build(BuildContext context) {
    final palette = context.ferri;
    return Row(
      children: [
        Expanded(
          child: Text(
            title,
            style: Theme.of(context).textTheme.titleSmall!.copyWith(
                  letterSpacing: 1.1,
                  fontWeight: FontWeight.w700,
                  color: palette.muted,
                ),
          ),
        ),
        if (actionLabel != null)
          TextButton(
            onPressed: onAction,
            style: TextButton.styleFrom(foregroundColor: palette.primary),
            child: Text(actionLabel!),
          ),
      ],
    );
  }
}