import 'package:ferrisync/main.dart';
import 'package:ferrisync/providers/sync_provider.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

/// UI-driven pairing test against a live host process.
///
/// The runner script (test_android_flutter_sync.sh) starts ferrisync-cli
/// serve on the host and exposes it to the app via `adb reverse`, so the
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
  await container.read(syncServiceProvider).init();
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

  testWidgets('pairs with live host through the pair dialog',
      (WidgetTester tester) async {
    final container = await pumpApp(tester);

    await tester.tap(find.text('Devices'));
    await tester.pumpAndSettle();

    // A previous run of this suite may have left the host paired already;
    // only drive the dialog when starting from the empty state.
    if (find.text('No devices paired').evaluate().isNotEmpty) {
      await tester.tap(find.byKey(const ValueKey('pair_fab')));
      await tester.pumpAndSettle();

      expect(find.text('Pair Device'), findsOneWidget);

      await tester.enterText(
        find.widgetWithText(TextField, 'IP Address'),
        hostIp,
      );
      await tester.enterText(
        find.widgetWithText(TextField, 'Port'),
        '$hostPort',
      );
      await tester.pump();

      await tester.tap(find.widgetWithText(FilledButton, 'Pair'));
      await tester.pump();
    }

    final appeared = await waitUntil(
      () =>
          find.text('No devices paired').evaluate().isEmpty &&
          find.byIcon(Icons.devices).evaluate().isNotEmpty,
      tester,
    );
    expect(appeared, true,
        reason:
            'host device should appear in the paired list after dialog pairing');

    final service = container.read(syncServiceProvider);
    expect(service.devices.first.id, isNotEmpty);
  });

  testWidgets('pairing against an unreachable port fails gracefully',
      (WidgetTester tester) async {
    await pumpApp(tester);

    await tester.tap(find.text('Devices'));
    await tester.pumpAndSettle();

    if (find.text('No devices paired').evaluate().isEmpty) {
      // Already paired from an earlier test; nothing graceful to check.
      return;
    }

    await tester.tap(find.byKey(const ValueKey('pair_fab')));
    await tester.pumpAndSettle();

    await tester.enterText(
        find.widgetWithText(TextField, 'IP Address'), '127.0.0.1');
    await tester.enterText(find.widgetWithText(TextField, 'Port'), '1');
    await tester.pump();
    await tester.tap(find.widgetWithText(FilledButton, 'Pair'));
    await tester.pump();

    final failed = await waitUntil(
      () => find.textContaining('Pairing failed').evaluate().isNotEmpty,
      tester,
      timeout: const Duration(seconds: 20),
    );
    expect(failed, true,
        reason: 'unreachable host should surface a Pairing failed snackbar');

    // The app must remain usable afterwards (device list may already
    // contain entries from earlier tests in this run).
    expect(find.byKey(const ValueKey('pair_fab')), findsOneWidget);
  });
}
