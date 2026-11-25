import 'dart:io';

import 'package:ferrisync/main.dart';
import 'package:ferrisync/providers/sync_provider.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

/// The host runner publishes a share named `auto-share` before starting its
/// server. Pairing with it must wire that share into the app's Folders list
/// without a separate browse → request step.
const hostIp = String.fromEnvironment('FERRISYNC_HOST',
    defaultValue: '127.0.0.1');
const hostPort =
    int.fromEnvironment('FERRISYNC_PORT', defaultValue: 9847);
const autoShareName = 'auto-share';

Future<ProviderContainer> pumpApp(WidgetTester tester) async {
  final container = ProviderContainer();
  final service = container.read(syncServiceProvider);
  await service.init();
  await service.completeOnboarding();
  await tester.pumpWidget(
    UncontrolledProviderScope(
      container: container,
      child: const FerriSyncApp(),
    ),
  );
  await tester.pumpAndSettle();
  return container;
}

Future<bool> waitUntil(bool Function() condition, WidgetTester tester,
    {Duration timeout = const Duration(seconds: 30)}) async {
  final deadline = DateTime.now().add(timeout);
  while (DateTime.now().isBefore(deadline)) {
    await tester.pump(const Duration(milliseconds: 250));
    if (condition()) return true;
  }
  return false;
}

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('pairing adopts the host published share',
      (WidgetTester tester) async {
    final container = await pumpApp(tester);
    final service = container.read(syncServiceProvider);
    await service.refresh();

    if (service.devices.isEmpty) {
      final message = await service.pairWithDevice(hostIp, hostPort);
      expect(message, contains('auto-paired 1 shared folder(s)'),
          reason: 'pair result should report the adopted share: $message');
      await service.refresh();
    }
    expect(service.devices, isNotEmpty);

    final appeared = await waitUntil(
      () => service.folders.any((f) =>
          f.name == autoShareName && f.peers.isNotEmpty && f.deviceId.isNotEmpty),
      tester,
    );
    expect(appeared, true,
        reason:
            'host share should appear in the app Folders list after pairing; '
            'folders=${service.folders.map((f) => f.name).toList()}');

    final adopted = service.folders.firstWhere(
        (f) => f.name == autoShareName && f.peers.isNotEmpty);
    expect(Directory(adopted.localPath).existsSync(), isTrue,
        reason: 'adopted replica directory should be created: ${adopted.localPath}');

    addTearDown(() async {
      await service.removeFolder(adopted.id.toInt());
      final dir = Directory(adopted.localPath);
      if (dir.existsSync()) dir.deleteSync(recursive: true);
    });
  });
}
