import 'package:ferrisync/main.dart';
import 'package:ferrisync/providers/sync_provider.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

class WelcomeMockService extends SyncService {
  WelcomeMockService({
    bool ready = true,
    bool onboardingDone = true,
  })  : _ready = ready,
        _onboardingDone = onboardingDone;

  final bool _ready;
  bool _onboardingDone;
  int completeOnboardingCalls = 0;

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
  group('WelcomeScreen', () {
    testWidgets('first launch redirects every other route to welcome',
        (WidgetTester tester) async {
      final service = WelcomeMockService(ready: true, onboardingDone: false);
      await tester.pumpWidget(createApp(service));
      await tester.pumpAndSettle();

      expect(find.text('Welcome to FerriSync'), findsOneWidget);
      expect(find.text('Get started'), findsOneWidget);
    });

    testWidgets('get started completes onboarding and lands on devices',
        (WidgetTester tester) async {
      final service = WelcomeMockService(ready: true, onboardingDone: false);
      await tester.pumpWidget(createApp(service));
      await tester.pumpAndSettle();

      await tester.tap(find.text('Get started'));
      await tester.pumpAndSettle();

      expect(service.completeOnboardingCalls, 1);
      expect(find.text('Welcome to FerriSync'), findsNothing);
      expect(find.byType(NavigationBar), findsOneWidget);
      expect(find.text('Pair a device'), findsOneWidget);
    });

    testWidgets('returning user lands straight on the dashboard',
        (WidgetTester tester) async {
      final service = WelcomeMockService(ready: true, onboardingDone: true);
      await tester.pumpWidget(createApp(service));
      await tester.pumpAndSettle();

      expect(find.text('Welcome to FerriSync'), findsNothing);
      expect(find.text('Everything is in sync'), findsOneWidget);
    });
  });
}