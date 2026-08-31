import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import '../gen/diagnostics.dart' as frb_diag;
import '../providers/sync_provider.dart';
import '../theme/ferri_theme.dart';
import '../widgets/empty_state.dart';

/// Diagnostic checks exposed as `ferrisync doctor` on the CLI, surfaced in the
/// app as a self-service health check. Each probe reuses the exact same Rust
/// model — there is no second, Dart-side implementation to drift.
class DiagnosticsScreen extends ConsumerStatefulWidget {
  const DiagnosticsScreen({super.key});

  @override
  ConsumerState<DiagnosticsScreen> createState() => _DiagnosticsScreenState();
}

class _DiagnosticsScreenState extends ConsumerState<DiagnosticsScreen> {
  Future<List<frb_diag.DiagnosticCheck>>? _checks;

  @override
  void initState() {
    super.initState();
    _checks = _load();
  }

  Future<List<frb_diag.DiagnosticCheck>> _load() async {
    return ref.read(syncServiceProvider).runDiagnostics();
  }

  void _rerun() {
    setState(() {
      _checks = _load();
    });
  }

  @override
  Widget build(BuildContext context) {
    final palette = context.ferri;
    final textTheme = Theme.of(context).textTheme;
    return Scaffold(
      appBar: AppBar(
        title: const Text('Diagnostics'),
        actions: [
          IconButton(
            tooltip: 'Run checks',
            onPressed: _rerun,
            icon: const Icon(Icons.refresh),
          ),
        ],
      ),
      body: FutureBuilder<List<frb_diag.DiagnosticCheck>>(
        future: _checks,
        builder: (context, snapshot) {
          final checks = snapshot.data;
          if (snapshot.connectionState != ConnectionState.done) {
            return const Center(child: CircularProgressIndicator());
          }
          if (checks == null || checks.isEmpty) {
            return const EmptyState(
              icon: Icons.medical_information_outlined,
              title: 'No checks ran',
              subtitle: 'This device has not finished its diagnostic scan yet. '
                  'Pull to refresh or tap the refresh button.',
            );
          }

          final sorted = [...checks];
          sorted.sort((a, b) => _rank(a.status).compareTo(_rank(b.status)));

          final problemCount = checks
              .where((c) =>
                  c.status == frb_diag.CheckStatus.fail ||
                  c.status == frb_diag.CheckStatus.warn)
              .length;

          return RefreshIndicator(
            onRefresh: () async => _rerun(),
            child: ListView(
              physics: const AlwaysScrollableScrollPhysics(),
              padding: const EdgeInsets.all(FerriTokens.spaceL),
              children: [
                SectionHeader(title: 'NODE HEALTH (${checks.length})'),
                const SizedBox(height: FerriTokens.spaceS),
                Card(
                  child: Padding(
                    padding: const EdgeInsets.all(FerriTokens.spaceL),
                    child: Row(
                      children: [
                        Icon(
                          problemCount == 0
                              ? Icons.check_circle_outline
                              : Icons.warning_amber_outlined,
                          color: problemCount == 0
                              ? palette.success
                              : palette.warning,
                        ),
                        const SizedBox(width: FerriTokens.spaceM),
                        Expanded(
                          child: Text(
                            problemCount == 0
                                ? 'Everything looks healthy.'
                                : '$problemCount check${problemCount == 1 ? '' : 's'} '
                                    'need${problemCount == 1 ? 's' : ''} attention.',
                            style: textTheme.bodyMedium,
                          ),
                        ),
                      ],
                    ),
                  ),
                ),
                const SizedBox(height: FerriTokens.spaceS),
                Card(
                  child: Column(
                    children: [
                      for (final c in sorted) ...[
                        _CheckTile(check: c),
                        if (c != sorted.last)
                          const Divider(
                            height: 1,
                            indent: 16,
                            endIndent: 16,
                          ),
                      ],
                    ],
                  ),
                ),
                const SizedBox(height: FerriTokens.spaceL),
                Text(
                  'The same checks power `ferrisync doctor` on desktop.',
                  style: textTheme.bodySmall!.copyWith(color: palette.muted),
                ),
              ],
            ),
          );
        },
      ),
    );
  }

  static int _rank(frb_diag.CheckStatus s) => switch (s) {
        frb_diag.CheckStatus.fail => 0,
        frb_diag.CheckStatus.warn => 1,
        frb_diag.CheckStatus.info => 2,
        frb_diag.CheckStatus.pass => 3,
      };
}

class _CheckTile extends StatelessWidget {
  const _CheckTile({required this.check});

  final frb_diag.DiagnosticCheck check;

  @override
  Widget build(BuildContext context) {
    final palette = context.ferri;
    final textTheme = Theme.of(context).textTheme;
    final (icon, color, label) = _style(check.status, palette);
    return Padding(
      padding: const EdgeInsets.all(FerriTokens.spaceM),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Icon(icon, size: 20, color: color),
          const SizedBox(width: FerriTokens.spaceM),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(
                  children: [
                    Expanded(
                      child: Text(
                        _title(check.name),
                        style: textTheme.bodyLarge!.copyWith(
                          fontWeight: FontWeight.w600,
                        ),
                      ),
                    ),
                    const SizedBox(width: FerriTokens.spaceS),
                    Text(
                      label,
                      style: textTheme.labelSmall!.copyWith(
                        color: color,
                        fontWeight: FontWeight.w700,
                        letterSpacing: 0.6,
                      ),
                    ),
                  ],
                ),
                const SizedBox(height: 2),
                Text(
                  check.message,
                  style: textTheme.bodySmall!.copyWith(color: palette.muted),
                ),
                if (check.hints.isNotEmpty) ...[
                  const SizedBox(height: 4),
                  for (final hint in check.hints)
                    Padding(
                      padding: const EdgeInsets.only(top: 2),
                      child: Row(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text('• ',
                              style: textTheme.bodySmall
                                  ?.copyWith(color: palette.muted)),
                          Expanded(
                            child: Text(
                              hint,
                              style: textTheme.bodySmall
                                  ?.copyWith(color: palette.muted),
                            ),
                          ),
                        ],
                      ),
                    ),
                ],
              ],
            ),
          ),
        ],
      ),
    );
  }

  static String _title(String name) => switch (name) {
        'data_dir' => 'Storage directory',
        'storage' => 'Metadata database',
        'identity' => 'Device identity',
        'pairings' => 'Trusted devices',
        'folders' => 'Sync folders',
        'network_interface' => 'Network interface',
        'port_bind' => 'Sync port',
        'firewall' => 'Firewall',
        'mdns' => 'Local discovery (mDNS)',
        _ => name,
      };

  static (IconData, Color, String) _style(
    frb_diag.CheckStatus status,
    FerriPalette palette,
  ) =>
      switch (status) {
        frb_diag.CheckStatus.pass => (
            Icons.check_circle,
            palette.success,
            'PASS',
          ),
        frb_diag.CheckStatus.warn => (
            Icons.warning_amber,
            palette.warning,
            'WARN',
          ),
        frb_diag.CheckStatus.fail => (
            Icons.error,
            palette.danger,
            'FAIL',
          ),
        frb_diag.CheckStatus.info => (
            Icons.info_outline,
            palette.primary,
            'INFO',
          ),
      };
}
