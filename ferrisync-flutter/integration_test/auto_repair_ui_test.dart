import 'package:ferrisync/main.dart';
import 'package:ferrisync/providers/sync_provider.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

/// End-to-end auto-repair test against a live host process.
///
/// The runner script (test_linux_flutter_auto_repair.sh) starts `ferrisync
/// serve` on a test port with auto-accept (stdin is not a TTY). This suite:
///
///   1. boots the real engine and pairs with the host  -> host is a *known* device
///   2. turns on auto-repair (persisted marker in the data dir)
///   3. re-initializes the engine, simulating an app restart
///   4. asserts the startup re-pair pass reports a successful re-pair of the
///      previously-known device at its last-known address
///
/// This reproduces the "flutter linux does not pair again with a running
/// repl cli" scenario and guards the whole path (marker -> engine init ->
/// pair handshake -> last_seen refresh), not just a unit.
const hostIp = String.fromEnvironment('FERRISYNC_HOST',
    defaultValue: '127.0.0.1');
const hostPort =
    int.fromEnvironment('FERRISYNC_PORT', defaultValue: 9847);

Future<bool> waitUntil(bool Function() condition, WidgetTester tester,
    {Duration timeout = const Duration(seconds: 60)}) async {
  final deadline = DateTime.now().add(timeout);
  while (DateTime.now().isBefore(deadline)) {
    await tester.pump(const Duration(milliseconds: 250));
    if (condition()) return true;
  }
  return false;
}

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('re-pairs a known device on startup', (WidgetTester tester) async {
    final container = ProviderContainer();
    final service = container.read(syncServiceProvider);

    // First boot: fresh engine, then get past onboarding.
    await service.init();
    await service.completeOnboarding();

    // Establish the relationship: the host is now a known/trusted device with
    // a persisted cert + last-known address.
    await service.pairWithDevice(hostIp, hostPort);
    await service.refresh();

    final paired = await waitUntil(
      () => service.devices.isNotEmpty,
      tester,
      timeout: const Duration(seconds: 30),
    );
    expect(paired, true,
        reason: 'host should appear as a known device before the restart');

    // Turn auto-repair on (persists the data-dir marker read on next boot).
    await service.setAutoRepairEnabled(true);
    expect(service.autoRepairEnabled, isTrue);

    // Simulate an app restart while the host is still serving. A fresh launch
    // must repopulate the persisted device list on its own (no manual refresh).
    await service.init();

    expect(service.devices.isNotEmpty, isTrue,
        reason: 'restart should reload the known device from the store without '
            'a user refresh');

    final repared = await waitUntil(
      () => (service.autoRepairResult ?? '').contains('Re-paired'),
      tester,
    );
    expect(repared, true,
        reason: 'startup auto-repair should re-pair the known host; '
            'got: ${service.autoRepairResult}');
    // The host is always among the trusted devices (a fresh store re-pairs
    // exactly this one); prior test runs may have left more, so assert >= 1.
    expect(service.autoRepairResult, contains('Re-paired'));
    expect(service.autoRepairResult, isNot(contains('0 device')));

    // The engine shell still renders after the re-pair.
    await tester.pumpWidget(
      UncontrolledProviderScope(
        container: container,
        child: const FerriSyncApp(),
      ),
    );
    await tester.pumpAndSettle();
    expect(find.byType(NavigationBar), findsWidgets);

    // Restore default (off) so later runs start clean.
    await service.setAutoRepairEnabled(false);

    container.dispose();
  });
}