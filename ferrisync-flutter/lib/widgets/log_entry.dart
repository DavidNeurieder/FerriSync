import 'package:flutter/material.dart';

enum LogLevel { info, success, warning, error }

class LogEntryWidget extends StatelessWidget {
  final String message;
  final LogLevel level;
  final DateTime timestamp;

  const LogEntryWidget({
    super.key,
    required this.message,
    this.level = LogLevel.info,
    DateTime? timestamp,
  }) : timestamp = timestamp ?? DateTime.now();

  IconData get _icon => switch (level) {
        LogLevel.info => Icons.info_outline,
        LogLevel.success => Icons.check_circle_outline,
        LogLevel.warning => Icons.warning_amber,
        LogLevel.error => Icons.error_outline,
      };

  Color _color(BuildContext context) => switch (level) {
        LogLevel.info => Theme.of(context).colorScheme.onSurfaceVariant,
        LogLevel.success => Colors.green,
        LogLevel.warning => Colors.orange,
        LogLevel.error => Colors.red,
      };

  @override
  Widget build(BuildContext context) {
    final time =
        '${timestamp.hour}:${timestamp.minute.toString().padLeft(2, '0')}:${timestamp.second.toString().padLeft(2, '0')}';

    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 2),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Icon(_icon, size: 16, color: _color(context)),
          const SizedBox(width: 8),
          Text(time, style: const TextStyle(fontFamily: 'monospace', fontSize: 12)),
          const SizedBox(width: 8),
          Expanded(child: Text(message, style: const TextStyle(fontSize: 13))),
        ],
      ),
    );
  }
}
