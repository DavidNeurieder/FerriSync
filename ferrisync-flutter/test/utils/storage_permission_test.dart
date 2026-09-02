import 'package:ferrisync/utils/storage_permission.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

/// Regression test for the Linux crash:
///
///   Unhandled Exception: MissingPluginException(No implementation found for
///   method checkPermissionStatus on channel .../permissions/methods)
///
/// On desktop the `permission_handler` plugin has no native implementation, so
/// hitting `Permission.manageExternalStorage.isGranted` throws. Storage grants
/// (the Android "All files access" gate) don't apply on desktop, so
/// [ensureStorageAccess] must short-circuit before touching the plugin.
void main() {
  testWidgets(
      'on desktop, ensureStorageAccess returns true without the '
      'permission_handler plugin', (WidgetTester tester) async {
    // Intentionally do NOT register a mock handler for the permissions channel:
    // any plugin call would throw MissingPluginException, exactly like the
    // reported Linux crash.
    final BuildContext ctx = await _buildContext(tester);
    final granted = await ensureStorageAccess(ctx);
    expect(granted, isTrue,
        reason: 'desktop must grant storage access without calling the plugin');
  });
}

Future<BuildContext> _buildContext(WidgetTester tester) async {
  late BuildContext captured;
  await tester.pumpWidget(
    Builder(builder: (context) {
      captured = context;
      return const SizedBox.shrink();
    }),
  );
  return captured;
}