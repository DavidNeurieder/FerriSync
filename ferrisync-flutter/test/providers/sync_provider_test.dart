import 'package:ferrisync/models/sync_models.dart';
import 'package:ferrisync/providers/sync_provider.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  group('SyncService', () {
    late SyncService service;

    setUp(() {
      service = SyncService();
    });

    test('initial state has empty deviceId and deviceName', () {
      expect(service.deviceId, '');
      expect(service.deviceName, '');
      expect(service.status, SyncStatus.idle);
      expect(service.devices, isEmpty);
      expect(service.folders, isEmpty);
    });

    test('init sets deviceId and deviceName', () async {
      const channel = MethodChannel('plugins.flutter.io/path_provider');
      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
          .setMockMethodCallHandler(channel, (MethodCall methodCall) async {
        if (methodCall.method == 'getApplicationSupportDirectory') {
          return '/tmp/ferrisync_test';
        }
        return null;
      });

      await service.init();

      expect(service.deviceId, isNotEmpty);
      expect(service.deviceName, isNotEmpty);
      expect(service.deviceId, '00000000-0000-0000-0000-000000000000');
      expect(service.deviceName, 'Flutter Device');

      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
          .setMockMethodCallHandler(channel, null);
    });

    test('pairWithDevice returns confirmation message', () async {
      final result = await service.pairWithDevice('192.168.1.100', 9847);
      expect(result, 'Paired with device at 192.168.1.100:9847');
    });

    test('pairWithDevice handles different IP and port', () async {
      final result = await service.pairWithDevice('10.0.0.5', 9000);
      expect(result, 'Paired with device at 10.0.0.5:9000');
    });

    test('syncFolder sets status to syncing immediately', () async {
      final future = service.syncFolder('/path', 'device-id');
      expect(service.status, SyncStatus.syncing);
      await future;
    });

    test('syncFolder resets status to idle after completion', () async {
      await service.syncFolder('/path', 'device-id');
      expect(service.status, SyncStatus.idle);
    });

    test('refresh completes without error', () async {
      await service.refresh();
      expect(service.devices, isEmpty);
      expect(service.folders, isEmpty);
    });
  });

  group('Providers', () {
    test('syncServiceProvider creates SyncService', () {
      final container = ProviderContainer();
      addTearDown(container.dispose);

      final service = container.read(syncServiceProvider);
      expect(service, isA<SyncService>());
    });

    test('deviceIdProvider reads from SyncService', () {
      final container = ProviderContainer();
      addTearDown(container.dispose);

      expect(container.read(deviceIdProvider), '');
    });

    test('deviceNameProvider reads from SyncService', () {
      final container = ProviderContainer();
      addTearDown(container.dispose);

      expect(container.read(deviceNameProvider), '');
    });

    test('devicesProvider reads from SyncService', () {
      final container = ProviderContainer();
      addTearDown(container.dispose);

      expect(container.read(devicesProvider), isEmpty);
    });

    test('foldersProvider reads from SyncService', () {
      final container = ProviderContainer();
      addTearDown(container.dispose);

      expect(container.read(foldersProvider), isEmpty);
    });

    test('syncStatusProvider reads from SyncService', () {
      final container = ProviderContainer();
      addTearDown(container.dispose);

      expect(container.read(syncStatusProvider), SyncStatus.idle);
    });
  });
}
