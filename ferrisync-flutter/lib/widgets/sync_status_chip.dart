import 'package:flutter/material.dart';
import '../models/sync_models.dart';
import '../theme/ferri_theme.dart';

/// Small pill that renders a sync status with its semantic color.
/// Pulses gently while syncing so a running transfer stays visible at a glance.
class SyncStatusChip extends StatefulWidget {
  const SyncStatusChip({
    super.key,
    required this.status,
    this.label,
    this.compact = false,
  });

  final SyncStatus status;
  final String? label;
  final bool compact;

  @override
  State<SyncStatusChip> createState() => _SyncStatusChipState();
}

class _SyncStatusChipState extends State<SyncStatusChip>
    with SingleTickerProviderStateMixin {
  late final AnimationController _controller;
  Animation<double>? _pulse;

  bool get _isSyncing => widget.status == SyncStatus.syncing;

  @override
  void initState() {
    super.initState();
    _controller = AnimationController(
      vsync: this,
      duration: const Duration(milliseconds: 1200),
    );
  }

  @override
  void didUpdateWidget(covariant SyncStatusChip oldWidget) {
    super.didUpdateWidget(oldWidget);
    _syncPulse();
  }

  void _syncPulse() {
    _controller.stop();
    if (_isSyncing) {
      _pulse = TweenSequence<double>([
        TweenSequenceItem(tween: Tween(begin: 1.0, end: 1.08), weight: 1),
        TweenSequenceItem(tween: Tween(begin: 1.08, end: 1.0), weight: 1),
      ]).animate(CurvedAnimation(parent: _controller, curve: Curves.easeInOut));
      _controller.repeat();
    }
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final palette = context.ferri;
    final (icon, color, label) = switch (widget.status) {
      SyncStatus.idle => (Icons.check_circle,
          palette.success, widget.label ?? 'In sync'),
      SyncStatus.syncing => (
          Icons.sync,
          palette.syncing,
          widget.label ?? 'Syncing'
        ),
      SyncStatus.error => (
          Icons.error,
          palette.danger,
          widget.label ?? 'Needs attention'
        ),
    };

    Widget chip = Container(
      padding: EdgeInsets.symmetric(
        horizontal: widget.compact ? FerriTokens.spaceS : FerriTokens.spaceM,
        vertical: FerriTokens.spaceXS,
      ),
      decoration: BoxDecoration(
        color: color.withValues(alpha: 0.14),
        borderRadius: BorderRadius.circular(999),
        border: Border.all(color: color.withValues(alpha: 0.35)),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          AnimatedRotation(
            turns: widget.status == SyncStatus.syncing ? 1 : 0,
            duration: const Duration(milliseconds: 1200),
            curve: Curves.linear,
            child: Icon(icon, size: 14, color: color),
          ),
          const SizedBox(width: 6),
          Text(
            label,
            style: TextStyle(
              color: color,
              fontSize: widget.compact ? 11 : 12,
              fontWeight: FontWeight.w600,
            ),
          ),
        ],
      ),
    );

    final pulse = _pulse;
    if (pulse != null && _isSyncing) {
      chip = ScaleTransition(scale: pulse, child: chip);
    }
    return chip;
  }
}