import 'dart:async';

import 'package:flutter/services.dart';

/// Platform contract for sync notifications. Abstracted so tests (and the
/// provider) can run without a real Android side, mirroring the injectable
/// loader pattern used by FolderContentScreen.
abstract class NotificationsApi {
  /// Whether Android will currently display our notifications at all.
  Future<bool> areEnabled();

  /// Runs the POST_NOTIFICATIONS runtime request; resolves with the outcome.
  Future<bool> requestPermission();

  /// Posts a sync-completion notification.
  Future<void> show({required String title, required String body});

  /// Loads the persisted "notify on sync completion" preference.
  Future<bool> getPref();

  /// Persists the "notify on sync completion" preference.
  Future<void> setPref(bool enabled);

  /// True when the Android side has a live channel handler. The flutter
  /// test/drive harnesses boot Dart without MainActivity, so all methods
  /// above degrade gracefully there; suites use this probe to adapt.
  Future<bool> nativeHandlerPresent();
}

class NotificationsService implements NotificationsApi {
  // Must match the channel registered in android/.../MainActivity.kt
  // (notificationChannelName = "ferrisync/notifications"). A mismatch leaves
  // no native handler, so MethodChannel invocations never resolve.
  static const _channel = MethodChannel('ferrisync/notifications');

  /// Upper bound on waiting for the native permission flow. If the platform
  /// callback is ever lost (OEM quirks, process state), we degrade to
  /// "denied" so the toggle can never hang silently.
  static const _permissionTimeout = Duration(seconds: 8);

  @override
  Future<bool> areEnabled() async {
    try {
      return await _channel.invokeMethod<bool>('areNotificationsEnabled') ??
          false;
    } on PlatformException {
      return false;
    } on MissingPluginException {
      // Tests / unsupported platforms: treat as enabled so logic still runs.
      return true;
    }
  }

  @override
  Future<bool> requestPermission() async {
    try {
      return await _channel
              .invokeMethod<bool>('requestPermission')
              .timeout(_permissionTimeout) ??
          false;
    } on PlatformException {
      return false;
    } on MissingPluginException {
      return false;
    } on TimeoutException {
      return false;
    }
  }

  @override
  Future<void> show({required String title, required String body}) async {
    try {
      await _channel
          .invokeMethod<void>('show', {'title': title, 'body': body});
    } on PlatformException {
      // Notification delivery is best-effort; never fail a sync over it.
    } on MissingPluginException {
      // No native handler (e.g. headless test harness): drop silently.
    }
  }

  @override
  Future<bool> getPref() async {
    try {
      return await _channel.invokeMethod<bool>('getPref') ?? false;
    } on PlatformException {
      return false;
    } on MissingPluginException {
      return false;
    }
  }

  @override
  Future<void> setPref(bool enabled) async {
    try {
      await _channel.invokeMethod<void>('setPref', enabled);
    } on PlatformException {
      // Ignore; the in-memory toggle still reflects user intent.
    } on MissingPluginException {
      // Ignore; nothing to persist against.
    }
  }

  @override
  Future<bool> nativeHandlerPresent() async {
    try {
      return await _channel.invokeMethod<bool>('areNotificationsEnabled') !=
          null;
    } on MissingPluginException {
      return false;
    } on PlatformException {
      // A handler exists and answered with an error: platform side is live.
      return true;
    }
  }
}
