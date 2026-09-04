import 'package:ferrisync/models/sync_models.dart';
import 'package:ferrisync/providers/sync_provider.dart';
import 'package:ferrisync/screens/devices_screen.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

/// `SyncService` that mimics the real removeDevice → refresh() → notifyListeners
/// chain, but with in-memory state so the widget reactivity is observable
/// without an engine.
class RemovingSyncService extends SyncService {
  RemovingSyncService(List<Device> initial) : _devices = initial;

  List<Device> _devices;

  @override
  List<Device> get devices => List.unmodifiable(_devices);

  @override
  Future<String> removeDevice(String deviceId) async {
    _devices = _devices.where((d) => d.id != deviceId).toList();
    await refresh();
    return 'Device removed';
  }

  @override
  Future<void> refresh() async {
    notifyListeners();
  }
}

Widget app(SyncService service) {
  return ProviderScope(
    overrides: [
      syncServiceProvider.overrideWith((ref) => service),
    ],
    child: const MaterialApp(home: DevicesScreen()),
  );
}

void main() {
  testWidgets('removing a paired device updates the list immediately',
      (tester) async {
    final service = RemovingSyncService([
      Device(id: '1', name: 'Pixel 8', lastSeen: 100),
      Device(id: '2', name: 'Laptop', lastSeen: 200),
    ]);
    await tester.pumpWidget(app(service));
    expect(find.text('Pixel 8'), findsOneWidget);

    // Open the per-device menu and confirm removal.
    await tester.tap(find.byIcon(Icons.more_vert).first);
    await tester.pumpAndSettle();
    await tester.tap(find.text('Remove'));
    await tester.pumpAndSettle();
    await tester.tap(find.widgetWithText(FilledButton, 'Remove'));
    await tester.pumpAndSettle();

    // The removed device must be gone right away, before any navigation.
    expect(find.text('Pixel 8'), findsNothing,
        reason: 'the removed device should disappear immediately');
    expect(find.text('Laptop'), findsOneWidget,
        reason: 'the other device must remain visible');
  });
}