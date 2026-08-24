import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:file_picker/file_picker.dart';
import '../models/sync_models.dart';
import '../providers/sync_provider.dart';
import '../utils/storage_permission.dart';

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
              itemBuilder: (ctx, i) => _folderTile(ctx, service, folders[i]),
            ),
      floatingActionButton: FloatingActionButton(
        onPressed: () => _addFolder(context, service),
        child: const Icon(Icons.add),
      ),
    );
  }

  Widget _folderTile(BuildContext context, SyncService service, SyncFolder f) {
    return ListTile(
      leading: const Icon(Icons.folder),
      title: Text(f.localPath.split('/').last),
      subtitle: Text('${f.localPath}\nDevice: ${f.deviceId} · Last sync: ${f.lastSyncFormatted}'),
      trailing: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          IconButton(
            key: ValueKey('sync_now_${f.id}'),
            icon: const Icon(Icons.sync),
            tooltip: 'Sync now',
            onPressed: () => _syncNow(context, service, f),
          ),
          Switch(
            value: true,
            onChanged: (_) {
              // TODO: toggle folder active
            },
          ),
        ],
      ),
    );
  }

  void _syncNow(BuildContext context, SyncService service, SyncFolder f) async {
    if (!await ensureStorageAccess(context)) return;

    ScaffoldMessenger.of(context)
      ..hideCurrentSnackBar()
      ..showSnackBar(
        SnackBar(content: Text('Syncing ${f.localPath.split('/').last}...')),
      );
    final message = await service.syncFolderNow(f);
    if (context.mounted) {
      ScaffoldMessenger.of(context)
        ..hideCurrentSnackBar()
        ..showSnackBar(
          SnackBar(content: Text(message)),
        );
    }
  }

  void _addFolder(BuildContext context, SyncService service) async {
    if (!await ensureStorageAccess(context)) return;

    final result = await FilePicker.platform.getDirectoryPath();
    if (result == null) return;

    if (!context.mounted) return;
    await service.refresh();
    if (!context.mounted) return;

    final devices = service.devices;
    if (devices.isEmpty) {
      if (context.mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          const SnackBar(content: Text('Pair a device first')),
        );
      }
      return;
    }

    if (!context.mounted) return;
    final device = await showDialog<Device>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: const Text('Select Device'),
        content: SizedBox(
          width: double.maxFinite,
          child: ListView.builder(
            shrinkWrap: true,
            itemCount: devices.length,
            itemBuilder: (_, i) => ListTile(
              leading: const Icon(Icons.devices),
              title: Text(devices[i].name),
              subtitle: Text(devices[i].id),
              onTap: () => Navigator.pop(ctx, devices[i]),
            ),
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(ctx),
            child: const Text('Cancel'),
          ),
        ],
      ),
    );
    if (device == null) return;

    try {
      await service.addSyncFolder(result, device.id);
      if (context.mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('Folder added for ${device.name}')),
        );
      }
    } catch (e) {
      if (context.mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('Failed: $e')),
        );
      }
    }
  }
}
