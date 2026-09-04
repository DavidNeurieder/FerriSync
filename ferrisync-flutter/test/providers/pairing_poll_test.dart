import 'package:ferrisync/providers/sync_provider.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

/// Counts polling calls so a test can observe the live pairing poll without
/// any engine running.
class PollingSyncService extends SyncService {
  int devicePolls = 0;
  int folderPolls = 0;

  @override
  Future<void> pollPendingPairings() async {
    devicePolls++;
  }

  @override
  Future<void> pollPendingFolderPairings() async {
    folderPolls++;
  }
}

void main() {
  testWidgets('arriving pairing requests are polled once the poll is running',
      (tester) async {
    final service = PollingSyncService();
    await tester.pumpWidget(
      ProviderScope(
        overrides: [syncServiceProvider.overrideWith((ref) => service)],
        child: const MaterialApp(home: Scaffold(body: SizedBox())),
      ),
    );

    // No polling before the engine is ready.
    await tester.pump(const Duration(seconds: 10));
    expect(service.devicePolls, 0);

    // Once started, the poll ticks every 3s and covers both device and folder
    // pairing requests — a desktop app therefore surfaces an incoming request
    // without the user needing to navigate or pull-to-refresh.
    service.startPairingPoll();
    await tester.pump(const Duration(milliseconds: 3100));
    expect(service.devicePolls, 1);
    expect(service.folderPolls, 1);

    await tester.pump(const Duration(seconds: 3));
    expect(service.devicePolls, 2);
    expect(service.folderPolls, 2);

    // Riverpod won't dispose an override instance before the framework's
    // pending-timer invariant check, so cancel the poll timer explicitly.
    service.dispose();
  });
}