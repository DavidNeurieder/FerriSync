import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:file_picker/file_picker.dart';
import '../models/sync_models.dart';
import '../providers/sync_provider.dart';

class FoldersScreen extends ConsumerWidget {
  const FoldersScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final service = ref.watch(syncServiceProvider);
    final folders = ref.watch(foldersProvider);

    return Scaffold(
      body: folders.isEmpty
          ? const Center(child: Text('No sync folders configured'))
          : ListView.builder(
              itemCount: folders.length,
              itemBuilder: (_, i) => _folderTile(folders[i]),
            ),
      floatingActionButton: FloatingActionButton(
        onPressed: () => _addFolder(context, service),
        child: const Icon(Icons.add),
      ),
    );
  }

  Widget _folderTile(SyncFolder f) {
    return ListTile(
      leading: const Icon(Icons.folder),
      title: Text(f.localPath.split('/').last),
      subtitle: Text('Device: ${f.deviceId}\nDirection: ${f.direction}\nLast sync: ${f.lastSyncFormatted}'),
      trailing: Switch(
        value: true,
        onChanged: (_) {
          // TODO: toggle folder active
        },
      ),
    );
  }

  void _addFolder(BuildContext context, SyncService service) async {
    final result = await FilePicker.platform.getDirectoryPath();
    if (result == null) return;

    // Show device picker
    if (context.mounted) {
      showDialog(
        context: context,
        builder: (ctx) => AlertDialog(
          title: const Text('Select Device'),
          content: const Text('Device selection — TODO: populate from paired devices'),
          actions: [
            TextButton(onPressed: () => Navigator.pop(ctx), child: const Text('Cancel')),
            FilledButton(
              onPressed: () {
                Navigator.pop(ctx);
                service.syncFolder(result, 'device-id');
              },
              child: const Text('Add'),
            ),
          ],
        ),
      );
    }
  }
}
