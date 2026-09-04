import 'package:ferrisync/gen/api.dart' as frb;
import 'package:ferrisync/providers/sync_provider.dart';
import 'package:ferrisync/widgets/pairing_request_alert.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

/// In-memory SyncService whose pending lists can be mutated to exercise the
/// global pairing-request popup without an engine.
class AlertSyncService extends SyncService {
  final List<(String, String)> _devices = [];
  final List<frb.PendingFolderPairing> _folders = [];
  final List<String> approved = [];
  final List<String> denied = [];

  void addDeviceRequest(String name, String id) {
    _devices.add((name, id));
    notifyListeners();
  }

  void addFolderRequest({
    required String deviceId,
    required String deviceName,
    required String folderGuid,
    required String folderName,
  }) {
    _folders.add(frb.PendingFolderPairing(
      deviceId: deviceId,
      deviceName: deviceName,
      folderGuid: folderGuid,
      folderName: folderName,
    ));
    notifyListeners();
  }

  @override
  List<(String, String)> get pendingPairings => List.unmodifiable(_devices);

  @override
  List<frb.PendingFolderPairing> get pendingFolderPairings =>
      List.unmodifiable(_folders);

  @override
  Future<String> approvePairing(String deviceId, String deviceName) async {
    approved.add(deviceId);
    _devices.removeWhere((e) => e.$2 == deviceId);
    notifyListeners();
    return 'Paired with $deviceName';
  }

  @override
  Future<String> denyPairing(String deviceId) async {
    denied.add(deviceId);
    _devices.removeWhere((e) => e.$2 == deviceId);
    notifyListeners();
    return 'Pairing denied';
  }

  @override
  Future<String> denyFolderPairing(String deviceId, String folderGuid) async {
    denied.add('$deviceId#$folderGuid');
    _folders.removeWhere(
        (p) => p.deviceId == deviceId && p.folderGuid == folderGuid);
    notifyListeners();
    return 'Folder pairing denied';
  }

  @override
  Future<String> approveFolderPairing({
    required String deviceId,
    required String folderGuid,
    required String folderName,
    required String localPath,
  }) async {
    approved.add('$deviceId#$folderGuid');
    _folders.removeWhere(
        (p) => p.deviceId == deviceId && p.folderGuid == folderGuid);
    notifyListeners();
    return 'Paired "$folderName"';
  }

  @override
  Future<void> refresh() async => notifyListeners();
}

Widget host(SyncService service) {
  return ProviderScope(
    overrides: [syncServiceProvider.overrideWith((ref) => service)],
    child: const MaterialApp(home: Scaffold(body: PairingRequestAlert())),
  );
}

void main() {
  testWidgets('a request present at startup is primed, not popped up',
      (tester) async {
    final service = AlertSyncService();
    service.addDeviceRequest('Old Phone', 'dev-old');
    await tester.pumpWidget(host(service));
    await tester.pumpAndSettle();

    expect(find.text('Pairing request'), findsNothing,
        reason: 'pre-existing requests must not replay on startup');
  });

  testWidgets('a newly arriving device request shows the approval popup',
      (tester) async {
    final service = AlertSyncService();
    await tester.pumpWidget(host(service));
    await tester.pumpAndSettle();

    service.addDeviceRequest('Pixel 12', 'dev-1');
    await tester.pumpAndSettle();

    expect(find.text('Pairing request'), findsOneWidget);
    expect(find.textContaining('Pixel 12'), findsOneWidget);

    await tester.tap(find.text('Allow'));
    await tester.pumpAndSettle();

    expect(service.approved, ['dev-1']);
    expect(service.denied, isEmpty);
  });

  testWidgets('<allow> is not re-announced while the request stays pending',
      (tester) async {
    final service = AlertSyncService();
    service.addDeviceRequest('Pixel 12', 'dev-1');
    await tester.pumpWidget(host(service));
    await tester.pumpAndSettle();

    // A request that reappears unchanged must not pop again.
    service.addDeviceRequest('Pixel 12', 'dev-1');
    await tester.pumpAndSettle();

    expect(find.text('Pairing request'), findsNothing,
        reason: 'an unchanged pending request must be announced only once');
  });

  testWidgets('a newly arriving folder request shows an approval popup',
      (tester) async {
    final service = AlertSyncService();
    await tester.pumpWidget(host(service));
    await tester.pumpAndSettle();

    service.addFolderRequest(
      deviceId: 'peer',
      deviceName: 'Phone',
      folderGuid: 'guid-1',
      folderName: 'Camera',
    );
    await tester.pumpAndSettle();

    expect(find.text('Folder pairing request'), findsOneWidget);
    expect(find.textContaining('"Camera"'), findsOneWidget);

    await tester.tap(find.text('Deny'));
    await tester.pumpAndSettle();

    expect(service.denied, ['peer#guid-1']);
    expect(service.approved, isEmpty);
  });
}