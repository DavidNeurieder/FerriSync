import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import '../providers/sync_provider.dart';
import '../theme/ferri_theme.dart';

/// First-launch experience: a short pitch and one obvious path forward.
/// Shown instead of the dashboard until the user has passed it (a marker
/// file keeps it a one-time thing, even across app restarts).
class WelcomeScreen extends ConsumerWidget {
  const WelcomeScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final palette = context.ferri;
    final service = ref.read(syncServiceProvider);
    final textTheme = Theme.of(context).textTheme;

    return Scaffold(
      body: SafeArea(
        child: Padding(
          padding: const EdgeInsets.all(FerriTokens.spaceL),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              const Spacer(flex: 2),
              Container(
                width: 72,
                height: 72,
                decoration: BoxDecoration(
                  color: palette.primary.withValues(alpha: 0.14),
                  borderRadius: BorderRadius.circular(FerriTokens.radiusL),
                ),
                child: Icon(Icons.share_outlined, color: palette.primary, size: 36),
              ),
              const SizedBox(height: FerriTokens.spaceL),
              Text(
                'Welcome to FerriSync',
                style: textTheme.headlineSmall!.copyWith(fontWeight: FontWeight.w700),
              ),
              const SizedBox(height: FerriTokens.spaceM),
              Text(
                'Sync files directly between your devices.',
                style: textTheme.titleMedium!.copyWith(color: palette.muted),
              ),
              const SizedBox(height: FerriTokens.spaceS),
              Text(
                'No cloud. No account.',
                style: textTheme.bodyMedium!.copyWith(color: palette.muted),
              ),
              const Spacer(flex: 3),
              SizedBox(
                width: double.infinity,
                child: FilledButton.icon(
                  key: const ValueKey('welcome_get_started'),
                  onPressed: () async {
                    await service.completeOnboarding();
                    if (context.mounted) context.go('/devices');
                  },
                  icon: const Icon(Icons.arrow_forward),
                  label: const Text('Get started'),
                ),
              ),
              const SizedBox(height: FerriTokens.spaceS),
              Text(
                'You\'ll pair a device over your local network — '
                'the files never leave it.',
                style: textTheme.bodySmall!.copyWith(color: palette.muted),
                textAlign: TextAlign.center,
              ),
            ],
          ),
        ),
      ),
    );
  }
}