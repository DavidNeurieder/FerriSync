import 'package:ferrisync/widgets/startup_banner.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

Widget _wrap(Widget child) => MaterialApp(home: Scaffold(body: child));

void main() {
  testWidgets('shows progress while the engine is starting',
      (tester) async {
    await tester.pumpWidget(
      _wrap(const StartupBanner(initializing: true, error: null)),
    );

    expect(find.byType(LinearProgressIndicator), findsOneWidget);
  });

  testWidgets('renders nothing when healthy', (tester) async {
    await tester.pumpWidget(
      _wrap(const StartupBanner(initializing: false, error: null)),
    );

    expect(find.byType(LinearProgressIndicator), findsNothing);
    expect(find.textContaining('engine'), findsNothing);
  });

  testWidgets('surfaces the init error in-app instead of hanging',
      (tester) async {
    await tester.pumpWidget(
      _wrap(const StartupBanner(
        initializing: false,
        error: 'engine did not start within 20s',
      )),
    );

    expect(find.textContaining('Sync engine problem'), findsOneWidget);
    expect(find.textContaining('did not start within 20s'), findsOneWidget);
    expect(find.byIcon(Icons.warning_amber_rounded), findsOneWidget);
  });
}
