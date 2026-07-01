import 'package:flutter/material.dart';

class SyncProgressBar extends StatelessWidget {
  final double progress;
  final String label;
  final int filesSynced;
  final int totalFiles;

  const SyncProgressBar({
    super.key,
    this.progress = 0.0,
    this.label = 'Syncing...',
    this.filesSynced = 0,
    this.totalFiles = 0,
  });

  @override
  Widget build(BuildContext context) {
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(label, style: Theme.of(context).textTheme.titleSmall),
            const SizedBox(height: 8),
            ClipRRect(
              borderRadius: BorderRadius.circular(4),
              child: LinearProgressIndicator(
                value: totalFiles > 0 ? filesSynced / totalFiles : null,
                minHeight: 8,
              ),
            ),
            if (totalFiles > 0) ...[
              const SizedBox(height: 4),
              Text(
                '$filesSynced / $totalFiles files',
                style: Theme.of(context).textTheme.bodySmall,
              ),
            ],
          ],
        ),
      ),
    );
  }
}
