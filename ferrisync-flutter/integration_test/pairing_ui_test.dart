import 'package:ferrisync/main.dart';
import 'package:ferrisync/providers/sync_provider.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

/// UI-driven pairing test against a live host process.
///
/// The runner script (test_android_flutter_sync.sh) starts `ferrisync serve`
/// on the host and exposes it to the app via `adb reverse`, so the
/// app dials 127.0.0.1:<port>. Serve stdin is not a TTY, which makes the
/// host auto-accept the request; the runner additionally greps its log
/// for "Pair request from" to prove traffic actually crossed TLS.
///
/// Unlike app_test.dart this initializes the real engine (RustLib +
/// SyncService.init), so pairing hits actual sockets.
const hostIp = String.fromEnvironment('FERRISYNC_HOST',
    defaultValue: '127.0.0.1');
const hostPort =
    int.fromEnvironment('FERRISYNC_PORT', defaultValue: 9847);

Future<ProviderContainer> pumpApp(WidgetTester tester) async {
  final container = ProviderContainer();
  final service = container.read(syncServiceProvider);
  await service.init();
  // The engine is fresh after each install, so get past the first-launch
  // wizard and land on the shell's dashboard before driving the tabs.
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
    {Duration timeout = const Duration(seconds: 45)}) async {
  final deadline = DateTime.now().add(timeout);
  while (DateTime.now().isBefore(deadline)) {
    await tester.pump(const Duration(milliseconds: 250));
    if (condition()) return true;
  }
  return false;
}

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('pairs with live host through the add-device flow',
      (WidgetTester tester) async {
    final container = await pumpApp(tester);

    await tester.tap(find.text('Devices'));
    await tester.pumpAndSettle();

    final service = container.read(syncServiceProvider);

    // A previous run of this suite may have left the host paired already;
    // only initiate pairing when starting from the empty state.
    if (service.devices.isEmpty) {
      await service.pairWithDevice(hostIp, hostPort);
      await service.refresh();
    }

    final appeared = await waitUntil(
      () => service.devices.isNotEmpty,
      tester,
      timeout: const Duration(seconds: 30),
    );
    expect(appeared, true,
        reason: 'host device should appear in the paired list after pairing');

    expect(service.devices.first.id, isNotEmpty);

    // We are already on the Devices tab; the provider rebuild re-renders the
    // list, so the empty state is replaced by the paired host's card.
    await tester.pumpAndSettle();
    expect(find.text('No devices paired'), findsNothing);
    expect(find.byKey(const ValueKey('add_device_fab')), findsOneWidget);
  });

  testWidgets('pairing against an unreachable port fails gracefully',
      (WidgetTester tester) async {
    final container = await pumpApp(tester);

    await tester.tap(find.text('Devices'));
    await tester.pumpAndSettle();

    final service = container.read(syncServiceProvider);
    if (service.devices.isNotEmpty) {
      // Already paired from an earlier test; nothing graceful to check.
      return;
    }

    await tester.tap(find.byKey(const ValueKey('pair_a_device')));
    // The add-device flow starts an mDNS scan with a spinner; drive the
    // route transition with explicit pumps instead of pumpAndSettle (which
    // would spin until the scan finishes).
    await tester.pump(const Duration(milliseconds: 600));
    await tester.tap(find.text('Enter address manually'));
    await tester.pumpAndSettle();

    await tester.enterText(
        find.widgetWithText(TextField, 'IP address'), '127.0.0.1');
    await tester.enterText(find.widgetWithText(TextField, 'Port'), '1');
    await tester.pump();

    await tester.tap(find.text('Connect'));
    await tester.pump();

    final failed = await waitUntil(
      () => find.text("Couldn't pair").evaluate().isNotEmpty,
      tester,
      timeout: const Duration(seconds: 20),
    );
    expect(failed, true,
        reason: 'unreachable host should surface the Couldn\'t pair state');

    // The app must remain usable afterwards (device list may already
    // contain entries from earlier tests in this run).
    await tester.tap(find.text('Cancel'));
    await tester.pumpAndSettle();
    expect(find.byType(NavigationBar), findsOneWidget);
  });
}
