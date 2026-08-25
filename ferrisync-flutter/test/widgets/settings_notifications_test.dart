import 'package:ferrisync/providers/sync_provider.dart';
import 'package:ferrisync/screens/settings_screen.dart';
import 'package:ferrisync/services/notification_service.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

class _FakeNotifications implements NotificationsApi {
  bool permissionGranted;
  bool prefSaved = false;
  final List<({String title, String body})> shown = [];

  _FakeNotifications({this.permissionGranted = true});

  @override
  Future<bool> areEnabled() async => true;

  @override
  Future<bool> requestPermission() async => permissionGranted;

  @override
  Future<void> show({required String title, required String body}) async =>
      shown.add((title: title, body: body));

  @override
  Future<bool> getPref() async => false;

  @override
  Future<void> setPref(bool enabled) async => prefSaved = enabled;

  @override
  Future<bool> nativeHandlerPresent() async => true;
}

Widget _wrap(SyncService service) => ProviderScope(
      overrides: [
        syncServiceProvider.overrideWith((ref) => service),
      ],
      child: const MaterialApp(
        home: Scaffold(body: SettingsScreen()),
      ),
    );

void main() {
  testWidgets('toggle reflects service state and can be switched on',
      (tester) async {
    final notifications = _FakeNotifications();
    final service = SyncService(notifications: notifications);
    await tester.pumpWidget(_wrap(service));

    expect(
      tester.widget<SwitchListTile>(
        find.widgetWithText(SwitchListTile, 'Notifications'),
      ).value,
      isFalse,
    );

    await tester.tap(find.text('Notifications'));
    await tester.pump();

    expect(service.notificationsEnabled, isTrue);
    expect(notifications.prefSaved, isTrue);
    expect(find.text('Open settings'), findsNothing);
  });

  testWidgets('denied permission reverts the switch and offers settings',
      (tester) async {
    final notifications =
        _FakeNotifications(permissionGranted: false);
    final service = SyncService(notifications: notifications);
    await tester.pumpWidget(_wrap(service));

    await tester.tap(find.text('Notifications'));
    await tester.pump();

    expect(service.notificationsEnabled, isFalse);
    expect(notifications.prefSaved, isFalse);
    expect(find.text('Open settings'), findsOneWidget);
  });

  testWidgets('turning off persists immediately', (tester) async {
    final notifications = _FakeNotifications();
    final service = SyncService(notifications: notifications);
    await tester.pumpWidget(_wrap(service));

    // Start from an enabled state.
    await service.setNotificationsEnabled(true);
    await tester.pumpAndSettle();
    expect(
      tester.widget<SwitchListTile>(
        find.widgetWithText(SwitchListTile, 'Notifications'),
      ).value,
      isTrue,
    );

    await tester.tap(find.text('Notifications'));
    await tester.pump();

    expect(service.notificationsEnabled, isFalse);
    expect(notifications.prefSaved, isFalse);
    expect(find.text('Sync notifications off'), findsOneWidget);
  });
}
