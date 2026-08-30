import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'providers/sync_provider.dart';
import 'screens/dashboard_screen.dart';
import 'screens/devices_screen.dart';
import 'screens/folders_screen.dart';
import 'screens/settings_screen.dart';
import 'theme/ferri_theme.dart';
import 'widgets/startup_banner.dart';

void main() {
  WidgetsFlutterBinding.ensureInitialized();
  final container = ProviderContainer();
  // Render the UI shell immediately; engine startup runs in the background.
  // SyncService notifies listeners when it finishes (or fails), and the app
  // shell surfaces the outcome via StartupBanner — a slow or broken engine
  // must never leave the user on a frozen splash screen again.
  unawaited(container.read(syncServiceProvider).init());
  runApp(
    UncontrolledProviderScope(
      container: container,
      child: const FerriSyncApp(),
    ),
  );
}

class FerriSyncApp extends ConsumerWidget {
  const FerriSyncApp({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final themeMode = ref.watch(themeModeProvider);
    final router = GoRouter(
      initialLocation: '/',
      routes: [
        ShellRoute(
          builder: (context, state, child) => AppShell(child: child),
          routes: [
            GoRoute(path: '/', builder: (_, __) => const DashboardScreen()),
            GoRoute(path: '/devices', builder: (_, __) => const DevicesScreen()),
            GoRoute(path: '/folders', builder: (_, __) => const FoldersScreen()),
            GoRoute(path: '/settings', builder: (_, __) => const SettingsScreen()),
          ],
        ),
      ],
    );

    return MaterialApp.router(
      title: 'FerriSync',
      theme: FerriTheme.light(),
      darkTheme: FerriTheme.dark(),
      // Dark-first identity; the light variant is opt-in from Settings.
      themeMode: themeMode,
      routerConfig: router,
    );
  }
}

class AppShell extends ConsumerWidget {
  final Widget child;
  const AppShell({super.key, required this.child});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final service = ref.watch(syncServiceProvider);
    return Scaffold(
      appBar: AppBar(
        title: const Text('FerriSync'),
        actions: [
          IconButton(
            icon: const Icon(Icons.settings),
            onPressed: () => context.go('/settings'),
          ),
        ],
      ),
      body: Column(
        children: [
          StartupBanner(
            initializing: service.initializing,
            error: service.initError,
          ),
          Expanded(child: child),
        ],
      ),
      bottomNavigationBar: NavigationBar(
        selectedIndex: _currentIndex(context),
        onDestinationSelected: (i) => _goTo(context, i),
        destinations: const [
          NavigationDestination(icon: Icon(Icons.dashboard), label: 'Home'),
          NavigationDestination(icon: Icon(Icons.devices), label: 'Devices'),
          NavigationDestination(icon: Icon(Icons.folder), label: 'Folders'),
        ],
      ),
    );
  }

  int _currentIndex(BuildContext context) {
    final loc = GoRouterState.of(context).uri.toString();
    if (loc == '/devices') return 1;
    if (loc == '/folders') return 2;
    return 0;
  }

  void _goTo(BuildContext context, int i) {
    switch (i) {
      case 0:
        context.go('/');
      case 1:
        context.go('/devices');
      case 2:
        context.go('/folders');
    }
  }
}
