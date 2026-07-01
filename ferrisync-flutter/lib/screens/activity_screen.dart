import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import '../providers/sync_provider.dart';

class ActivityScreen extends ConsumerWidget {
  const ActivityScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    // TODO: wire up to SyncEvent stream from FRB
    final events = <String>[
      'Device paired: Pixel 8 (a1b2c3d4) — 2 min ago',
      'Synced: photos/IMG_001.jpg → Pixel 8 — 5 min ago',
      'Synced: docs/report.pdf ← Laptop — 12 min ago',
      'Conflict resolved: notes.txt — 1h ago',
    ];

    return ListView.builder(
      itemCount: events.length,
      itemBuilder: (_, i) => ListTile(
        leading: const Icon(Icons.info_outline),
        title: Text(events[i]),
        dense: true,
      ),
    );
  }
}
