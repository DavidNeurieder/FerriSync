import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import '../providers/sync_provider.dart';
import '../theme/ferri_theme.dart';
import '../utils/add_folder_flow.dart';
import '../widgets/add_device_flow.dart';

/// First-launch wizard, capped at four steps: Welcome → Name → Find devices →
/// Choose what to sync. Unlike the old single-pitch welcome screen this walks
/// a new user through the whole first-run in one place.
class OnboardingScreen extends ConsumerStatefulWidget {
  const OnboardingScreen({super.key});

  @override
  ConsumerState<OnboardingScreen> createState() => _OnboardingScreenState();
}

class _OnboardingScreenState extends ConsumerState<OnboardingScreen> {
  int _step = 0; // 0..3

  static const _totalSteps = 4;

  void _goTo(int step) => setState(() => _step = step);

  Future<void> _finish(String target) async {
    final service = ref.read(syncServiceProvider);
    await service.completeOnboarding();
    if (mounted) context.go(target);
  }

  void _skipAll() => _finish('/');

  Widget _stepView() {
    switch (_step) {
      case 0:
        return _WelcomeStep(onContinue: () => _goTo(1));
      case 1:
        return _NameStep(
          onContinue: (name) async {
            final service = ref.read(syncServiceProvider);
            try {
              await service.setDeviceName(name);
            } catch (_) {}
            _goTo(2);
          },
        );
      case 2:
        return _FindStep(
          onPaired: () => _goTo(3),
          onSkip: () => _goTo(3),
          service: ref.read(syncServiceProvider),
        );
      case 3:
        return _ChooseFoldersStep(
          onDone: () => _finish('/'),
          service: ref.read(syncServiceProvider),
        );
      default:
        return _WelcomeStep(onContinue: () => _goTo(1));
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      body: SafeArea(
        child: Column(
          children: [
            if (_step > 0)
              Align(
                alignment: Alignment.centerLeft,
                child: Padding(
                  padding: const EdgeInsets.symmetric(
                    horizontal: FerriTokens.spaceS,
                    vertical: 4,
                  ),
                  child: IconButton(
                    tooltip: 'Back',
                    onPressed: () => _goTo(_step - 1),
                    icon: const Icon(Icons.arrow_back),
                  ),
                ),
              ),
            Expanded(
              child: AnimatedSwitcher(
                duration: const Duration(milliseconds: 200),
                child: KeyedSubtree(
                  key: ValueKey(_step),
                  child: _stepView(),
                ),
              ),
            ),
            _ProgressDots(current: _step, total: _totalSteps),
            const SizedBox(height: FerriTokens.spaceL),
            if (_step == 0)
              Padding(
                padding: const EdgeInsets.symmetric(
                  horizontal: FerriTokens.spaceL,
                ),
                child: TextButton(
                  onPressed: _skipAll,
                  child: const Text('Skip for now'),
                ),
              ),
          ],
        ),
      ),
    );
  }
}

class _ProgressDots extends StatelessWidget {
  const _ProgressDots({required this.current, required this.total});

  final int current;
  final int total;

  @override
  Widget build(BuildContext context) {
    final palette = context.ferri;
    return Row(
      mainAxisAlignment: MainAxisAlignment.center,
      children: [
        for (var i = 0; i < total; i++)
          AnimatedContainer(
            duration: const Duration(milliseconds: 200),
            margin: const EdgeInsets.symmetric(horizontal: 4),
            width: i == current ? 20 : 8,
            height: 8,
            decoration: BoxDecoration(
              color: i <= current ? palette.primary : palette.surfaceHigh,
              borderRadius: BorderRadius.circular(4),
            ),
          ),
      ],
    );
  }
}

class _WelcomeStep extends StatelessWidget {
  const _WelcomeStep({required this.onContinue});

  final VoidCallback onContinue;

  @override
  Widget build(BuildContext context) {
    final palette = context.ferri;
    final textTheme = Theme.of(context).textTheme;
    return Padding(
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
          const _PitchRow(icon: Icons.wifi_rounded, text: 'Local network — direct transfers'),
          const _PitchRow(icon: Icons.cloud_off, text: 'No cloud'),
          const _PitchRow(icon: Icons.person_off, text: 'No account'),
          const _PitchRow(icon: Icons.lock_outline, text: 'Encrypted connections'),
          const Spacer(flex: 3),
          SizedBox(
            width: double.infinity,
            child: FilledButton.icon(
              key: const ValueKey('onboarding_get_started'),
              onPressed: onContinue,
              icon: const Icon(Icons.arrow_forward),
              label: const Text('Get started'),
            ),
          ),
        ],
      ),
    );
  }
}

class _PitchRow extends StatelessWidget {
  const _PitchRow({required this.icon, required this.text});

  final IconData icon;
  final String text;

  @override
  Widget build(BuildContext context) {
    final palette = context.ferri;
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 6),
      child: Row(
        children: [
          Icon(icon, size: 18, color: palette.primary),
          const SizedBox(width: FerriTokens.spaceM),
          Text(text, style: Theme.of(context).textTheme.bodyLarge),
        ],
      ),
    );
  }
}

class _NameStep extends StatefulWidget {
  const _NameStep({required this.onContinue});

  final ValueChanged<String> onContinue;

  @override
  State<_NameStep> createState() => _NameStepState();
}

class _NameStepState extends State<_NameStep> {
  late final TextEditingController _controller;
  bool _busy = false;

  @override
  void initState() {
    super.initState();
    _controller = TextEditingController();
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final palette = context.ferri;
    final textTheme = Theme.of(context).textTheme;
    return Padding(
      padding: const EdgeInsets.all(FerriTokens.spaceL),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const Spacer(flex: 1),
          Text(
            'Name this device',
            style: textTheme.headlineSmall!.copyWith(fontWeight: FontWeight.w700),
          ),
          const SizedBox(height: FerriTokens.spaceS),
          Text(
            'This is how other devices will see it.',
            style: textTheme.bodyMedium!.copyWith(color: palette.muted),
          ),
          const SizedBox(height: FerriTokens.spaceL),
          TextField(
            key: const ValueKey('onboarding_name_field'),
            controller: _controller,
            autofocus: true,
            decoration: const InputDecoration(
              labelText: 'Device name',
              hintText: 'e.g. My Laptop',
            ),
          ),
          const Spacer(flex: 2),
          SizedBox(
            width: double.infinity,
            child: FilledButton.icon(
              key: const ValueKey('onboarding_name_continue'),
              onPressed: _busy
                  ? null
                  : () {
                      setState(() => _busy = true);
                      final name = _controller.text.trim();
                      widget.onContinue(
                          name.isEmpty ? 'FerriSync device' : name);
                    },
              icon: const Icon(Icons.arrow_forward),
              label: const Text('Continue'),
            ),
          ),
        ],
      ),
    );
  }
}

class _FindStep extends StatelessWidget {
  const _FindStep({
    required this.service,
    required this.onPaired,
    required this.onSkip,
  });

  final SyncService service;
  final VoidCallback onPaired;
  final VoidCallback onSkip;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.all(FerriTokens.spaceL),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Expanded(
            child: SingleChildScrollView(
              child: AddDeviceFlow(
                service: service,
                compact: true,
                onPaired: (_) => onPaired(),
                onCancelled: onSkip,
              ),
            ),
          ),
          TextButton(
            onPressed: onSkip,
            child: const Text('Skip this step'),
          ),
        ],
      ),
    );
  }
}

class _ChooseFoldersStep extends StatelessWidget {
  const _ChooseFoldersStep({required this.service, required this.onDone});

  final SyncService service;
  final VoidCallback onDone;

  @override
  Widget build(BuildContext context) {
    final textTheme = Theme.of(context).textTheme;
    final palette = context.ferri;
    return Padding(
      padding: const EdgeInsets.all(FerriTokens.spaceL),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const Spacer(flex: 1),
          Text(
            'Choose what to sync',
            style: textTheme.headlineSmall!.copyWith(fontWeight: FontWeight.w700),
          ),
          const SizedBox(height: FerriTokens.spaceS),
          Text(
            "Pick a folder on this device and a peer to keep it in sync with. "
            "You can add more later.",
            style: textTheme.bodyMedium!.copyWith(color: palette.muted),
          ),
          const Spacer(flex: 2),
          SizedBox(
            width: double.infinity,
            child: FilledButton.icon(
              key: const ValueKey('onboarding_add_folder'),
              onPressed: () async {
                await runAddFolderFlow(context, service);
              },
              icon: const Icon(Icons.add),
              label: const Text('Add a folder'),
            ),
          ),
          const SizedBox(height: FerriTokens.spaceS),
          SizedBox(
            width: double.infinity,
            child: OutlinedButton(
              key: const ValueKey('onboarding_done'),
              onPressed: onDone,
              child: const Text('Done'),
            ),
          ),
        ],
      ),
    );
  }
}
