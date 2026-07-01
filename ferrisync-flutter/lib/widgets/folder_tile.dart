import 'package:flutter/material.dart';
import '../models/sync_models.dart';

class FolderTile extends StatelessWidget {
  final SyncFolder folder;
  final VoidCallback? onToggle;
  final bool enabled;

  const FolderTile({
    super.key,
    required this.folder,
    this.onToggle,
    this.enabled = true,
  });

  @override
  Widget build(BuildContext context) {
    return Card(
      margin: const EdgeInsets.symmetric(horizontal: 12, vertical: 4),
      child: ListTile(
        leading: const Icon(Icons.folder, size: 32),
        title: Text(
          folder.localPath.split('/').last,
          style: const TextStyle(fontWeight: FontWeight.w600),
        ),
        subtitle: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(folder.localPath),
            Text('${folder.direction} — last sync: ${folder.lastSyncFormatted}'),
          ],
        ),
        trailing: Switch(value: enabled, onChanged: (_) => onToggle?.call()),
      ),
    );
  }
}
