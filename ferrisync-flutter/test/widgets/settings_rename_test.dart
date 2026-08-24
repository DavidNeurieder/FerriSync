import 'package:ferrisync/providers/sync_provider.dart';
import 'package:ferrisync/screens/settings_screen.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

class _FakeSyncService extends SyncService {
  bool renameCalled = false;
  String? renamedTo;
  bool failRename = false;

  @override
  String get deviceName => 'Test Phone';

  @override
  Future<void> setDeviceName(String name) async {
    if (failRename) throw Exception('engine exploded');
    renameCalled = true;
    renamedTo = name;
  }
}

Widget _wrap(_FakeSyncService service) => ProviderScope(
      overrides: [
        syncServiceProvider.overrideWith((ref) => service),
      ],
      child: const MaterialApp(
        home: Scaffold(body: SettingsScreen()),
      ),
    );

void main() {
  testWidgets('rename dialog prefills, saves trimmed name', (tester) async {
    final service = _FakeSyncService();
    await tester.pumpWidget(_wrap(service));

    await tester.tap(find.text('Display Name'));
    await tester.pumpAndSettle();

    expect(find.text('Device Name'), findsOneWidget);
    expect(find.byType(TextField), findsOneWidget);
    expect(
      tester.widget<TextField>(find.byType(TextField)).controller!.text,
      'Test Phone',
    );

    await tester.enterText(find.byType(TextField), '  Pocket Ferri  ');
    await tester.tap(find.text('Save'));
    await tester.pump();

    expect(service.renameCalled, isTrue);
    expect(service.renamedTo, 'Pocket Ferri');
    // Success snackbar confirms.
    await tester.pump();
    expect(find.text('Renamed to "Pocket Ferri"'), findsOneWidget);
  });

  testWidgets('empty name is not submitted', (tester) async {
    final service = _FakeSyncService();
    await tester.pumpWidget(_wrap(service));

    await tester.tap(find.text('Display Name'));
    await tester.pumpAndSettle();

    await tester.enterText(find.byType(TextField), '   ');
    await tester.tap(find.text('Save'));
    await tester.pump();

    expect(service.renameCalled, isFalse);
    expect(find.byType(AlertDialog), findsOneWidget);
  });

  testWidgets('failed rename shows error snackbar', (tester) async {
    final service = _FakeSyncService()..failRename = true;
    await tester.pumpWidget(_wrap(service));

    await tester.tap(find.text('Display Name'));
    await tester.pumpAndSettle();

    await tester.enterText(find.byType(TextField), 'New Name');
    await tester.tap(find.text('Save'));
    await tester.pump();

    expect(find.textContaining('Rename failed'), findsOneWidget);
  });
}
