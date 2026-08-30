import 'package:ferrisync/gen/sync_engine/conflicts.dart' as frb;
import 'package:ferrisync/models/sync_models.dart';
import 'package:ferrisync/providers/sync_provider.dart';
import 'package:ferrisync/screens/conflicts_screen.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

class ConflictsMockService extends SyncService {
  ConflictsMockService({
    this.testConflicts = const [],
    this.testFolders = const [],
    this.testDevices = const [],
    this.testDeviceName = '',
  });

  final List<frb.ConflictEntry> testConflicts;
  final List<SyncFolder> testFolders;
  final List<Device> testDevices;
  final String testDeviceName;
  final List<(int, String, String)> resolveCalls = [];

  @override
  List<frb.ConflictEntry> get conflicts => testConflicts;

  @override
  List<SyncFolder> get folders => testFolders;

  @override
  List<Device> get devices => testDevices;

  @override
  String get deviceName => testDeviceName;

  @override
  Future<void> refresh() async {}

  @override
  Future<String> resolveConflict(
      int folderId, String backupPath, String action) async {
    resolveCalls.add((folderId, backupPath, action));
    return 'Conflict resolved — version kept on this device';
  }
}

frb.ConflictEntry conflictEntry({
  int folderId = 1,
  String path = 'notes.txt',
  String backupPath = 'notes.txt.clash',
  String loserLabel = 'local',
  int winnerMtimeSecs = 200,
  int winnerSize = 4096,
  int loserMtimeSecs = 100,
  int loserSize = 2048,
}) {
  return frb.ConflictEntry(
    folderId: folderId,
    path: path,
    backupPath: backupPath,
    loserLabel: loserLabel,
    winnerMtimeSecs: winnerMtimeSecs,
    winnerSize: BigInt.from(winnerSize),
    loserMtimeSecs: loserMtimeSecs,
    loserSize: BigInt.from(loserSize),
  );
}

Widget createTestApp(SyncService service) {
  return ProviderScope(
    overrides: [
      syncServiceProvider.overrideWith((ref) => service),
    ],
    child: const MaterialApp(home: ConflictsScreen()),
  );
}

void main() {
  group('ConflictsScreen', () {
    testWidgets('shows the empty state when there are no conflicts',
        (WidgetTester tester) async {
      await tester.pumpWidget(createTestApp(ConflictsMockService()));

      expect(find.text('No conflicts'), findsOneWidget);
      expect(find.text('FILE CONFLICTS (0)'), findsOneWidget);
    });

    testWidgets('lists discovered conflicts with folder label',
        (WidgetTester tester) async {
      await tester.pumpWidget(createTestApp(ConflictsMockService(
        testConflicts: [
          conflictEntry(path: 'reports/q3.md', backupPath: 'reports/q3.md.clash'),
        ],
        testFolders: [
          SyncFolder(
            id: 1,
            localPath: '/storage/docs',
            deviceId: 'dev-1',
            direction: 'bidirectional',
            lastSyncAt: 100,
          ),
        ],
      )));

      expect(find.text('q3.md'), findsOneWidget);
      expect(find.text('in docs'), findsOneWidget);
      expect(find.text('4.0 KB'), findsOneWidget);
    });

    testWidgets('keep this device asks, resolves backup to keep_both',
        (WidgetTester tester) async {
      final service = ConflictsMockService(
        testDeviceName: 'My Phone',
        testConflicts: [
          conflictEntry(),
        ],
        testFolders: [
          SyncFolder(
            id: 1,
            localPath: '/storage/docs',
            deviceId: 'dev-1',
            direction: 'bidirectional',
            lastSyncAt: 100,
          ),
        ],
        testDevices: [
          Device(id: 'dev-1', name: 'Pixel 8', lastSeen: 100),
        ],
      );
      await tester.pumpWidget(createTestApp(service));

      await tester.tap(find.text('notes.txt'));
      await tester.pumpAndSettle();

      expect(find.text('Two versions exist. Pick which one to keep.'),
          findsOneWidget);
      expect(find.text('Pixel 8'), findsOneWidget);
      expect(find.text('My Phone'), findsOneWidget);

      await tester.tap(find.byKey(const ValueKey('keep_winner')));
      await tester.pumpAndSettle();

      // Destructive to the loser version, so a confirmation is required.
      expect(find.text('Keep Pixel 8?'), findsOneWidget);
      await tester.tap(find.descendant(
        of: find.byType(AlertDialog),
        matching: find.text('Keep Pixel 8'),
      ));
      await tester.pumpAndSettle();

      expect(service.resolveCalls, hasLength(1));
      expect(service.resolveCalls.first.$3, 'keep_original');
      expect(find.text('Kept Pixel 8'), findsOneWidget);
    });

    testWidgets('keep both never asks for confirmation',
        (WidgetTester tester) async {
      final service = ConflictsMockService(testConflicts: [conflictEntry()]);
      await tester.pumpWidget(createTestApp(service));

      await tester.tap(find.text('notes.txt'));
      await tester.pumpAndSettle();

      await tester.tap(find.byKey(const ValueKey('keep_both')));
      await tester.pumpAndSettle();

      expect(find.text('Keep Pixel 8?'), findsNothing);
      expect(service.resolveCalls.first.$3, 'keep_both');
    });
  });
}