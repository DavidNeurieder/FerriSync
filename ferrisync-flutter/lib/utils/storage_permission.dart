import 'package:flutter/material.dart';
import 'package:permission_handler/permission_handler.dart';

/// Ensures the app may read/write user-chosen folders. On Android 11+
/// this means "All files access" (MANAGE_EXTERNAL_STORAGE), granted via
/// the system settings page — SAF picks do not help native file I/O.
///
/// Returns true when access is available.
Future<bool> ensureStorageAccess(BuildContext context) async {
  if (await Permission.manageExternalStorage.isGranted) {
    return true;
  }

  if (!context.mounted) return false;
  final proceed = await showDialog<bool>(
    context: context,
    builder: (ctx) => AlertDialog(
      title: const Text('Storage access needed'),
      content: const Text(
        'FerriSync reads and writes the folders you choose to sync. '
        'Android requires "All files access" for this. You will be '
        'taken to the system settings to allow it.',
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(ctx, false),
          child: const Text('Cancel'),
        ),
        FilledButton(
          onPressed: () => Navigator.pop(ctx, true),
          child: const Text('Open settings'),
        ),
      ],
    ),
  );

  if (proceed != true) return false;

  await Permission.manageExternalStorage.request();
  if (!context.mounted) return false;

  final granted = await Permission.manageExternalStorage.isGranted;
  if (!granted) {
    ScaffoldMessenger.of(context).showSnackBar(
      const SnackBar(
        content:
            Text('Sync needs "All files access" enabled in system settings'),
      ),
    );
  }
  return granted;
}
