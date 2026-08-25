import 'package:ferrisync/models/sync_models.dart';
import 'package:ferrisync/providers/sync_provider.dart';
import 'package:ferrisync/screens/devices_screen.dart';
import 'package:ferrisync/services/notification_service.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

class _FakeNotifications implements NotificationsApi {
  @override
  Future<bool> areEnabled() async => false;
  @override
  Future<bool> requestPermission() async => false;
  @override
  Future<void> show({required String title, required String body}) async {}
  @override
  Future<bool> getPref() async => false;
  @override
  Future<void> setPref(bool enabled) async {}
  @override
  Future<bool> nativeHandlerPresent() async => true;
}

/// Minimal SyncService subclass that overrides the devices getter
/// (so we can inject test data without calling init) and removeDevice
/// (to track calls without touching the engine).
class _FakeService extends SyncService {
  List<Device> _testDevices = [];
  String? lastRemovedDeviceId;

  _FakeService() : super(notifications: _FakeNotifications());

  @override
  List<Device> get devices => _testDevices;

  void setDevices(List<Device> d) {
    _testDevices = d;
    notifyListeners();
  }

  @override
  Future<String> removeDevice(String deviceId) async {
    lastRemovedDeviceId = deviceId;
    return 'Device removed — 2 folder(s), 5 session(s) deleted';
  }

  @override
  Future<void> refresh() async {}
}

Widget _wrap(_FakeService service) => ProviderScope(
      overrides: [
        syncServiceProvider.overrideWith((ref) => service),
      ],
      child: const MaterialApp(
        home: Scaffold(body: DevicesScreen()),
      ),
    );

void main() {
  testWidgets('delete icon shows confirmation dialog', (tester) async {
    final service = _FakeService();
    service.setDevices([
      Device(
        id: 'peer-1',
        name: 'Desktop',
        lastSeen: 1700000000,
      ),
    ]);
    await tester.pumpWidget(_wrap(service));

    expect(find.text('Desktop'), findsOneWidget);
    expect(find.byIcon(Icons.delete_outline), findsOneWidget);

    await tester.tap(find.byIcon(Icons.delete_outline));
    await tester.pumpAndSettle();

    expect(find.text('Remove device'), findsOneWidget);
    expect(
      find.textContaining('Remove Desktop'),
      findsOneWidget,
    );
  });

  testWidgets('confirming removal calls removeDevice and shows snackbar',
      (tester) async {
    final service = _FakeService();
    service.setDevices([
      Device(
        id: 'peer-1',
        name: 'Desktop',
        lastSeen: 1700000000,
      ),
    ]);
    await tester.pumpWidget(_wrap(service));

    await tester.tap(find.byIcon(Icons.delete_outline));
    await tester.pumpAndSettle();

    await tester.tap(find.text('Remove'));
    await tester.pumpAndSettle();

    expect(service.lastRemovedDeviceId, 'peer-1');
    expect(
      find.textContaining('Device removed — 2 folder(s)'),
      findsOneWidget,
    );
    // Dialog should be dismissed.
    expect(find.text('Remove device'), findsNothing);
  });

  testWidgets('canceling dismissal does not call removeDevice', (tester) async {
    final service = _FakeService();
    service.setDevices([
      Device(
        id: 'peer-1',
        name: 'Desktop',
        lastSeen: 1700000000,
      ),
    ]);
    await tester.pumpWidget(_wrap(service));

    await tester.tap(find.byIcon(Icons.delete_outline));
    await tester.pumpAndSettle();

    await tester.tap(find.text('Cancel'));
    await tester.pumpAndSettle();

    expect(service.lastRemovedDeviceId, isNull);
    expect(find.text('Remove device'), findsNothing);
  });

  testWidgets('empty state shows no-paired message', (tester) async {
    final service = _FakeService();
    await tester.pumpWidget(_wrap(service));

    expect(find.text('No devices paired'), findsOneWidget);
    expect(find.byIcon(Icons.delete_outline), findsNothing);
  });
}
