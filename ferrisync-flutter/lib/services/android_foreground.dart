import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

/// Platform channel to the Android host activity controlling the
/// "serving folders" foreground service. Calls are safe no-ops on
/// platforms without the channel (desktop, tests).
class AndroidForeground {
  static const MethodChannel _channel = MethodChannel('ferrisync/service');

  static bool _started = false;

  /// Start (or refresh) the foreground service. Idempotent.
  static Future<void> startServing() async {
    try {
      await _channel.invokeMethod<void>('start');
      _started = true;
    } on MissingPluginException {
      // Not on Android (or engine not wired up).
    } on PlatformException catch (e) {
      debugPrint('foreground service start failed: ${e.message}');
    }
  }

  /// Stop the foreground service if we started it.
  static Future<void> stopServing() async {
    if (!_started) return;
    _started = false;
    try {
      await _channel.invokeMethod<void>('stop');
    } on MissingPluginException {
      // ignore
    } on PlatformException catch (e) {
      debugPrint('foreground service stop failed: ${e.message}');
    }
  }
}
