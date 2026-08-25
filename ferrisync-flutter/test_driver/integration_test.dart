import 'package:integration_test/integration_test_driver.dart';

/// Host-side driver for `flutter drive` runs of the integration_test/ suites.
/// Unlike `flutter test`, drive mode boots the real app activity, so
/// MethodChannels registered in MainActivity are live.
Future<void> main() => integrationDriver();
