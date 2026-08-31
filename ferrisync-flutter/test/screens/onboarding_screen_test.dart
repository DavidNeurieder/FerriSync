import 'package:ferrisync/main.dart';
import 'package:ferrisync/providers/sync_provider.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

class OnboardingMockService extends SyncService {
  OnboardingMockService({
    bool ready = true,
    bool onboardingDone = false,
  })  : _ready = ready,
        _onboardingDone = onboardingDone;

  final bool _ready;
  bool _onboardingDone;
  int completeOnboardingCalls = 0;
  String? lastSetName;

  @override
  bool get isReady => _ready;

  @override
  bool get hasCompletedOnboarding => _onboardingDone;

  @override
  Future<void> completeOnboarding() async {
    completeOnboardingCalls++;
    _onboardingDone = true;
  }

  @override
  Future<void> refresh() async {}

  @override
  Future<void> setDeviceName(String name) async {
    lastSetName = name;
  }
}

Widget createApp(SyncService service) {
  return ProviderScope(
    overrides: [
      syncServiceProvider.overrideWith((ref) => service),
    ],
    child: const FerriSyncApp(),
  );
}

void main() {
  group('OnboardingScreen', () {
    testWidgets('first launch opens the welcome step', (WidgetTester tester) async {
      final service = OnboardingMockService(onboardingDone: false);
      await tester.pumpWidget(createApp(service));
      await tester.pumpAndSettle();

      expect(find.text('Welcome to FerriSync'), findsOneWidget);
      expect(find.text('Get started'), findsOneWidget);
    });

    testWidgets('walks all four steps and completes onboarding',
        (WidgetTester tester) async {
      final service = OnboardingMockService(onboardingDone: false);
      await tester.pumpWidget(createApp(service));
      await tester.pumpAndSettle();

      // Step 1 -> Step 2 (name)
      await tester.tap(find.text('Get started'));
      await tester.pump(const Duration(milliseconds: 400));
      await tester.pumpAndSettle();
      expect(find.text('Name this device'), findsOneWidget);

      // Step 2 -> Step 3 (find devices)
      await tester.enterText(
          find.byKey(const ValueKey('onboarding_name_field')), 'My Laptop');
      await tester.tap(find.byKey(const ValueKey('onboarding_name_continue')));
      await tester.pump(const Duration(milliseconds: 400));
      await tester.pumpAndSettle();
      expect(service.lastSetName, 'My Laptop');
      // AddDeviceFlow scans and finds nothing (no native bridge in tests).
      expect(find.text('Find your devices'), findsOneWidget);

      // Step 3 -> Step 4 (choose folders)
      await tester.tap(find.text('Skip this step'));
      await tester.pump(const Duration(milliseconds: 400));
      await tester.pumpAndSettle();
      expect(find.text('Choose what to sync'), findsOneWidget);

      // Step 4 -> done (lands on the dashboard)
      await tester.tap(find.byKey(const ValueKey('onboarding_done')));
      await tester.pumpAndSettle();

      expect(service.completeOnboardingCalls, 1);
      expect(find.byType(NavigationBar), findsOneWidget);
      expect(find.text('Everything is in sync'), findsOneWidget);
    });

    testWidgets('uses the default name when the field is left blank',
        (WidgetTester tester) async {
      final service = OnboardingMockService(onboardingDone: false);
      await tester.pumpWidget(createApp(service));
      await tester.pumpAndSettle();

      await tester.tap(find.text('Get started'));
      await tester.pump(const Duration(milliseconds: 400));
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(const ValueKey('onboarding_name_continue')));
      await tester.pump(const Duration(milliseconds: 400));
      await tester.pumpAndSettle();

      expect(service.lastSetName, 'FerriSync device');
    });

    testWidgets('returning user lands straight on the dashboard',
        (WidgetTester tester) async {
      final service = OnboardingMockService(onboardingDone: true);
      await tester.pumpWidget(createApp(service));
      await tester.pumpAndSettle();

      expect(find.text('Welcome to FerriSync'), findsNothing);
      expect(find.text('Everything is in sync'), findsOneWidget);
    });
  });
}
