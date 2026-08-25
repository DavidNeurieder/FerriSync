import 'package:ferrisync/providers/sync_provider.dart';
import 'package:ferrisync/screens/settings_screen.dart';
import 'package:ferrisync/services/notification_service.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

/// Simulates the Linux (or any non-Android) environment: no native channel
/// is available, so requestPermission cannot grant; but nothing should crash.
class _NoNativeNotifications implements NotificationsApi {
  bool prefSaved = false;

  @override
  Future<bool> areEnabled() async => false;

  @override
  Future<bool> requestPermission() async => false;

  @override
  Future<void> show({required String title, required String body}) async {}

  @override
  Future<bool> getPref() async => false;

  @override
  Future<void> setPref(bool enabled) async => prefSaved = enabled;

  @override
  Future<bool> nativeHandlerPresent() async => false;
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
  testWidgets('notification toggle works without native handler',
      (tester) async {
    final notifications = _NoNativeNotifications();
    final service = SyncService(notifications: notifications);
    await tester.pumpWidget(_wrap(service));

    // Switch starts off.
    expect(
      tester.widget<SwitchListTile>(
        find.widgetWithText(SwitchListTile, 'Notifications'),
      ).value,
      isFalse,
    );

    // Tapping to enable: permission is denied (no native handler on Linux),
    // so the switch stays off and the denial snackbar appears.
    await tester.tap(find.text('Notifications'));
    await tester.pumpAndSettle();

    expect(service.notificationsEnabled, isFalse);
    expect(notifications.prefSaved, isFalse);
    expect(find.text('Open settings'), findsOneWidget);
  });

  testWidgets('show() is a no-op and does not throw', (tester) async {
    final notifications = _NoNativeNotifications();
    final service = SyncService(notifications: notifications);
    await tester.pumpWidget(_wrap(service));

    // Calling show should not throw on a platform without native support.
    await notifications.show(title: 'Test', body: 'Body');
    // No exception means success.
  });

  testWidgets('nativeHandlerPresent returns false', (tester) async {
    final notifications = _NoNativeNotifications();
    expect(await notifications.nativeHandlerPresent(), isFalse);
  });
}
