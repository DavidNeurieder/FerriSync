import 'package:flutter/material.dart';

/// Startup status strip shown at the top of the app shell while the Rust
/// engine initializes, surfacing failures in-app instead of leaving the
/// user on a frozen splash screen.
class StartupBanner extends StatelessWidget {
  const StartupBanner({
    super.key,
    required this.initializing,
    required this.error,
  });

  final bool initializing;
  final String? error;

  @override
  Widget build(BuildContext context) {
    if (initializing) {
      return const LinearProgressIndicator(minHeight: 3);
    }
    final err = error;
    if (err == null) return const SizedBox.shrink();
    return Material(
      color: Theme.of(context).colorScheme.errorContainer,
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 10),
        child: Row(
          children: [
            Icon(
              Icons.warning_amber_rounded,
              color: Theme.of(context).colorScheme.onErrorContainer,
            ),
            const SizedBox(width: 12),
            Expanded(
              child: Text(
                'Sync engine problem: $err',
                style: TextStyle(
                  color: Theme.of(context).colorScheme.onErrorContainer,
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}
