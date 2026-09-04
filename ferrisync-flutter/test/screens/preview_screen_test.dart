import 'package:ferrisync/models/sync_models.dart';
import 'package:ferrisync/providers/sync_provider.dart';
import 'package:ferrisync/screens/preview_screen.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

class MockPreviewService extends SyncService {
  MockPreviewService({this.preview, this.syncResult = 'Sync complete with peer'});

  final SyncPreview? preview;
  final String syncResult;
  int syncCalls = 0;

  @override
  Future<SyncPreview?> previewSyncFolder(SyncFolder folder, String deviceId) async =>
      preview;

  @override
  Future<String> syncFolderNow(SyncFolder folder) async {
    syncCalls++;
    return syncResult;
  }
}

SyncFolder _folder({FolderHealth health = FolderHealth.waiting}) => SyncFolder(
      id: 1,
      localPath: '/storage/Documents',
      deviceId: 'dev-1',
      direction: 'bidirectional',
      lastSyncAt: 0,
      health: health,
      deviceName: 'Pixel 8',
    );

Widget _app(SyncService service) => ProviderScope(
      overrides: [syncServiceProvider.overrideWith((ref) => service)],
      child: MaterialApp(
        home: PreviewScreen(
          folder: _folder(),
          deviceId: 'dev-1',
          peerName: 'Pixel 8',
        ),
      ),
    );

void main() {
  testWidgets('shows up-to-date when preview is empty', (tester) async {
    await tester.pumpWidget(_app(MockPreviewService(
      preview: const SyncPreview(
          wouldPush: 0, wouldPull: 0, wouldConflict: 0, pushBytes: 0, pullBytes: 0),
    )));
    await tester.pumpAndSettle();
    expect(find.text('Up to date'), findsOneWidget);
    expect(find.text('Sync now'), findsNothing);
  });

  testWidgets('shows pending counts and bytes then syncs on tap',
      (tester) async {
    final service = MockPreviewService(
      preview: const SyncPreview(
          wouldPush: 3, wouldPull: 2, wouldConflict: 0, pushBytes: 1000, pullBytes: 500),
    );
    await tester.pumpWidget(_app(service));
    await tester.pumpAndSettle();

    expect(find.text('3 files'), findsOneWidget);
    expect(find.text('2 files'), findsOneWidget);
    expect(find.textContaining('1.5 KB'), findsWidgets);

    await tester.tap(find.byKey(const ValueKey('preview_sync_now')));
    await tester.pumpAndSettle();
    expect(service.syncCalls, 1);
  });

  testWidgets('surfaces conflicts row when present', (tester) async {
    final service = MockPreviewService(
      preview: const SyncPreview(
          wouldPush: 2, wouldPull: 0, wouldConflict: 1, pushBytes: 100, pullBytes: 0),
    );
    await tester.pumpWidget(_app(service));
    await tester.pumpAndSettle();
    expect(find.text('Conflicts'), findsOneWidget);
    expect(find.text('1 file'), findsWidgets);
  });
}
