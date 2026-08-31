import 'package:flutter/material.dart';
import '../models/sync_models.dart';
import '../theme/ferri_theme.dart';

/// Small colored dot reflecting a device's shared presence state
/// (Connected / Recently seen / Offline). Used by device rows on the
/// dashboard and the devices screen.
class PresenceDot extends StatelessWidget {
  const PresenceDot({super.key, required this.presence, this.size = 10});

  final Presence presence;
  final double size;

  @override
  Widget build(BuildContext context) {
    final palette = context.ferri;
    final color = switch (presence) {
      Presence.connected => palette.success,
      Presence.recentlySeen => palette.warning,
      Presence.offline => palette.muted,
    };
    final active = presence != Presence.offline;
    return SizedBox(
      width: size + 10,
      height: size + 10,
      child: Center(
        child: Container(
          width: size,
          height: size,
          decoration: BoxDecoration(
            color: color,
            shape: BoxShape.circle,
            boxShadow: active
                ? [
                    BoxShadow(
                      color: color.withValues(alpha: 0.5),
                      blurRadius: 6,
                    ),
                  ]
                : null,
          ),
        ),
      ),
    );
  }
}