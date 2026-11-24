import 'package:ferrisync/gen/api.dart' as frb;
import 'package:ferrisync/models/sync_models.dart';
import 'package:ferrisync/providers/sync_provider.dart';
import 'package:ferrisync/screens/add_device_screen.dart';
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

class _FakeService extends SyncService {
  List<frb.DiscoveredDevice> discovered = [];
  List<String> localAddresses = [];
  int pairCalls = 0;
  String? lastPairedIp;
  int? lastPairedPort;
  bool _paired = false;

  _FakeService() : super(notifications: _FakeNotifications());

  @override
  List<Device> get devices => _paired
      ? [
          Device(
              id: 'peer-1',
              name: 'Pixel 9',
              lastSeen: 1,
              presence: Presence.connected),
        ]
      : [];

  @override
  Future<List<frb.DiscoveredDevice>> discoverDevices(
      {int timeoutSecs = 3}) async {
    return discovered;
  }

  @override
  Future<String> pairWithDevice(String ip, int port) async {
    pairCalls++;
    lastPairedIp = ip;
    lastPairedPort = port;
    return 'Paired with device at $ip:$port';
  }

  @override
  Future<void> refresh() async {}

  @override
  Future<void> pollPendingPairings() async {}

  @override
  Future<List<String>> localPairingAddresses() async => localAddresses;

  void markPaired() => _paired = true;
}

Widget wrap(SyncService service) {
  return ProviderScope(
    overrides: [syncServiceProvider.overrideWith((ref) => service)],
    child: const MaterialApp(home: AddDeviceScreen()),
  );
}

void main() {
  testWidgets('shows a no-devices state when discovery finds nothing',
      (WidgetTester tester) async {
    final service = _FakeService()..discovered = [];
    await tester.pumpWidget(wrap(service));
    await tester.pumpAndSettle();

    expect(find.text('Add device'), findsOneWidget);
    expect(find.text('No devices found'), findsOneWidget);
  });

  testWidgets('lists a discovered device and pairs on request',
      (WidgetTester tester) async {
    final service = _FakeService()
      ..discovered = [
        const frb.DiscoveredDevice(
            id: 'peer-1',
            name: 'Pixel 9',
            ip: '192.168.1.5',
            port: 9847,
            controlPort: 9846),
      ];
    await tester.pumpWidget(wrap(service));
    await tester.pumpAndSettle();

    expect(find.text('Pixel 9'), findsOneWidget);
    // Technical details (IP/port) are hidden from the device list.
    expect(find.text('192.168.1.5:9847'), findsNothing);

    // Tapping the card asks for confirmation before pairing.
    await tester.tap(find.text('Pixel 9'));
    await tester.pumpAndSettle();
    expect(find.text('Pair with Pixel 9?'), findsOneWidget);

    // Register the device as paired *before* confirming so the handshake's
    // first poll succeeds.
    service.markPaired();
    await tester.tap(find.text('Pair'));
    await tester.pumpAndSettle();

    expect(service.pairCalls, 1);
    expect(service.lastPairedIp, '192.168.1.5');
    // Discovery advertises a control port: pairing must use it even when a
    // folder-server port is present, so folder-less peers remain reachable.
    expect(service.lastPairedPort, 9846);
    // Once paired, the flow reports Paired and pops back.
    expect(find.byType(AddDeviceScreen), findsNothing);
  });

  testWidgets('falls back to the folder port for legacy discoveries',
      (WidgetTester tester) async {
    final service = _FakeService()
      ..discovered = [
        const frb.DiscoveredDevice(
            id: 'peer-1', name: 'Older Desktop', ip: '192.168.1.6', port: 9847),
      ];
    await tester.pumpWidget(wrap(service));
    await tester.pumpAndSettle();

    await tester.tap(find.text('Older Desktop'));
    await tester.pumpAndSettle();
    service.markPaired();
    await tester.tap(find.text('Pair'));
    await tester.pumpAndSettle();

    expect(service.lastPairedPort, 9847);
  });

  testWidgets('offers both QR scan and QR display paths',
      (WidgetTester tester) async {
    final service = _FakeService()..discovered = [];
    await tester.pumpWidget(wrap(service));
    await tester.pumpAndSettle();

    // The manual IP/port path is no longer exposed.
    expect(find.text('Enter address manually'), findsNothing);
    expect(find.text('IP address'), findsNothing);
    expect(find.text('Port'), findsNothing);

    // Discovery + both QR actions remain.
    expect(find.text('Pair with a QR code'), findsOneWidget);
    expect(find.text('Show QR code'), findsOneWidget);
  });

  testWidgets('show-QR explains that a folder must be shared first',
      (WidgetTester tester) async {
    final service = _FakeService()..discovered = [];
    await tester.pumpWidget(wrap(service));
    await tester.pumpAndSettle();

    await tester.tap(find.text('Show QR code'));
    await tester.pumpAndSettle();

    expect(
      find.text('Share a folder first — there is no address to show yet.'),
      findsOneWidget,
    );
    expect(find.text('My pairing QR'), findsNothing);
  });

  testWidgets('show-QR displays this device\'s pairing address',
      (WidgetTester tester) async {
    final service = _FakeService()
      ..discovered = []
      ..localAddresses = ['192.168.1.42:9847'];
    await tester.pumpWidget(wrap(service));
    await tester.pumpAndSettle();

    await tester.tap(find.text('Show QR code'));
    await tester.pumpAndSettle();

    expect(find.text('My pairing QR'), findsOneWidget);
    expect(find.text('192.168.1.42:9847'), findsOneWidget);
    expect(find.text('Copy address'), findsOneWidget);
  });
}
