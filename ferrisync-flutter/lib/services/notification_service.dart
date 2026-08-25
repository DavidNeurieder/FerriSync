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
}

class NotificationsService implements NotificationsApi {
  static const _channel =
      MethodChannel('com.example.ferrisync/notifications');

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
      return await _channel.invokeMethod<bool>('requestPermission') ?? false;
    } on PlatformException {
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
    }
  }
}
