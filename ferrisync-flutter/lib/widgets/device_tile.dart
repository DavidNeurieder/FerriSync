import 'package:flutter/material.dart';
import '../models/sync_models.dart';

class DeviceTile extends StatelessWidget {
  final Device device;
  final VoidCallback? onTap;
  final VoidCallback? onDelete;

  const DeviceTile({
    super.key,
    required this.device,
    this.onTap,
    this.onDelete,
  });

  @override
  Widget build(BuildContext context) {
    return ListTile(
      onTap: onTap,
      leading: Icon(
        Icons.devices,
        color: device.isOnline ? Colors.green : Colors.grey,
        size: 32,
      ),
      title: Text(device.name),
      subtitle: Text('Last seen: ${device.lastSeenFormatted}'),
      trailing: onDelete != null
          ? IconButton(
              icon: const Icon(Icons.delete_outline),
              onPressed: onDelete,
            )
          : null,
    );
  }
}
