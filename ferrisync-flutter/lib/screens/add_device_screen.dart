import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import '../gen/api.dart' as frb;
import '../providers/sync_provider.dart';
import '../theme/ferri_theme.dart';
import '../widgets/add_device_flow.dart';

/// Full-screen guided "Add a device" journey: search → connect → wait for
/// approval → paired. Replaces the previous scan page + pair-by-address +
/// QR split into one cohesive flow.
class AddDeviceScreen extends ConsumerStatefulWidget {
  const AddDeviceScreen({super.key});

  @override
  ConsumerState<AddDeviceScreen> createState() => _AddDeviceScreenState();
}

class _AddDeviceScreenState extends ConsumerState<AddDeviceScreen> {
  @override
  Widget build(BuildContext context) {
    final service = ref.read(syncServiceProvider);
    return Scaffold(
      appBar: AppBar(title: const Text('Add device')),
      body: SafeArea(
        child: SingleChildScrollView(
          padding: const EdgeInsets.all(FerriTokens.spaceL),
          child: AddDeviceFlow(
            service: service,
            onPaired: (d) => _onPaired(context, d),
            onCancelled: () => Navigator.of(context).maybePop(),
          ),
        ),
      ),
    );
  }

  void _onPaired(BuildContext context, frb.DiscoveredDevice? d) {
    if (!mounted) return;
    final name = d?.name ?? 'device';
    ScaffoldMessenger.of(context)
      ..hideCurrentSnackBar()
      ..showSnackBar(SnackBar(content: Text('Paired with $name')));
    Navigator.of(context).pop();
  }
}

/// Small helper for inline add-device sections that want a tappable entry
/// into [AddDeviceScreen].
void openAddDevice(BuildContext context, WidgetRef ref) {
  Navigator.of(context).push(
    MaterialPageRoute<void>(builder: (_) => const AddDeviceScreen()),
  );
}
