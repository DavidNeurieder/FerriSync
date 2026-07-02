import 'package:ferrisync/models/sync_models.dart';
import 'package:ferrisync/widgets/device_tile.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('DeviceTile', () {
    final onlineDevice = Device(id: '1', name: 'Pixel 8', lastSeen: 100, isOnline: true);
    final offlineDevice = Device(id: '2', name: 'Old Phone', lastSeen: 172800);

    testWidgets('renders device name', (WidgetTester tester) async {
      await tester.pumpWidget(MaterialApp(
        home: Scaffold(body: DeviceTile(device: onlineDevice)),
      ));

      expect(find.text('Pixel 8'), findsOneWidget);
    });

    testWidgets('renders lastSeen text', (WidgetTester tester) async {
      await tester.pumpWidget(MaterialApp(
        home: Scaffold(body: DeviceTile(device: offlineDevice)),
      ));

      expect(find.textContaining('Last seen:'), findsOneWidget);
    });

    testWidgets('shows green icon for online device', (WidgetTester tester) async {
      await tester.pumpWidget(MaterialApp(
        home: Scaffold(body: DeviceTile(device: onlineDevice)),
      ));

      final icon = tester.widget<Icon>(find.byIcon(Icons.devices));
      expect(icon.color, Colors.green);
    });

    testWidgets('shows grey icon for offline device', (WidgetTester tester) async {
      await tester.pumpWidget(MaterialApp(
        home: Scaffold(body: DeviceTile(device: offlineDevice)),
      ));

      final icon = tester.widget<Icon>(find.byIcon(Icons.devices));
      expect(icon.color, Colors.grey);
    });

    testWidgets('calls onTap when tapped', (WidgetTester tester) async {
      bool tapped = false;
      await tester.pumpWidget(MaterialApp(
        home: Scaffold(body: DeviceTile(
          device: onlineDevice,
          onTap: () => tapped = true,
        )),
      ));

      await tester.tap(find.text('Pixel 8'));
      expect(tapped, true);
    });

    testWidgets('shows delete button when onDelete is provided', (WidgetTester tester) async {
      await tester.pumpWidget(MaterialApp(
        home: Scaffold(body: DeviceTile(
          device: onlineDevice,
          onDelete: () {},
        )),
      ));

      expect(find.byIcon(Icons.delete_outline), findsOneWidget);
    });

    testWidgets('calls onDelete when delete button tapped', (WidgetTester tester) async {
      bool deleted = false;
      await tester.pumpWidget(MaterialApp(
        home: Scaffold(body: DeviceTile(
          device: onlineDevice,
          onDelete: () => deleted = true,
        )),
      ));

      await tester.tap(find.byIcon(Icons.delete_outline));
      expect(deleted, true);
    });

    testWidgets('hides delete button when onDelete is null', (WidgetTester tester) async {
      await tester.pumpWidget(MaterialApp(
        home: Scaffold(body: DeviceTile(device: onlineDevice)),
      ));

      expect(find.byIcon(Icons.delete_outline), findsNothing);
    });
  });
}
