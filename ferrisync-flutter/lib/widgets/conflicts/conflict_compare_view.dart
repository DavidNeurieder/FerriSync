import 'package:flutter/material.dart';

import '../../gen/api.dart' as frb;
import '../../theme/ferri_theme.dart';

/// Read-only text comparison of the two conflict versions.
///
/// Renders a lightweight per-line diff (added / removed / unchanged) so the
/// user can see exactly what differs between the winner and loser before
/// picking a version. Used only when `ConflictContents.textual` is true;
/// binary conflicts keep the metadata-only cards.
class ConflictCompareView extends StatelessWidget {
  const ConflictCompareView({
    super.key,
    required this.contents,
    required this.fileName,
    required this.winnerLabel,
    required this.loserLabel,
  });

  /// The two versions fetched from the engine.
  final frb.ConflictContents contents;

  /// Base file name, shown in the header.
  final String fileName;

  /// Human label of the device owning the winner (real) version.
  final String winnerLabel;

  /// Human label of the device owning the loser (backup) version.
  final String loserLabel;

  @override
  Widget build(BuildContext context) {
    final textTheme = Theme.of(context).textTheme;
    final palette = context.ferri;
    final lines = _diffLines(contents.winner, contents.loser);

    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          children: [
            Expanded(
              child: Text(
                fileName,
                style: textTheme.titleLarge,
                overflow: TextOverflow.ellipsis,
              ),
            ),
            Icon(Icons.compare_arrows_rounded, color: palette.syncing, size: 20),
          ],
        ),
        const SizedBox(height: FerriTokens.spaceS),
        Text(
          '$winnerLabel changed:',
          style: textTheme.titleSmall!.copyWith(
            color: palette.primary,
            fontWeight: FontWeight.w600,
          ),
        ),
        const SizedBox(height: FerriTokens.spaceXS),
        Text(
          '$loserLabel had:',
          style: textTheme.titleSmall!.copyWith(
            color: palette.muted,
            fontWeight: FontWeight.w600,
          ),
        ),
        const SizedBox(height: FerriTokens.spaceL),
        if (contents.winnerTruncated || contents.loserTruncated)
          Padding(
            padding: const EdgeInsets.only(bottom: FerriTokens.spaceS),
            child: Text(
              'Showing the first 512 KB of a larger file.',
              style: textTheme.bodySmall!.copyWith(color: palette.muted),
            ),
          ),
        Flexible(
          child: Container(
            width: double.infinity,
            padding: const EdgeInsets.all(FerriTokens.spaceM),
            decoration: BoxDecoration(
              color: palette.surfaceHigh,
              borderRadius: BorderRadius.circular(FerriTokens.radiusM),
            ),
            child: ListView.builder(
              shrinkWrap: true,
              itemCount: lines.length,
              itemBuilder: (context, index) {
                final line = lines[index];
                return _LineRow(line: line);
              },
            ),
          ),
        ),
      ],
    );
  }
}

/// A single rendered diff line with its side marker and emphasis color.
class _LineRow extends StatelessWidget {
  const _LineRow({required this.line});

  final _Line line;

  @override
  Widget build(BuildContext context) {
    final palette = context.ferri;
    final textTheme = Theme.of(context).textTheme;
    final (marker, color) = switch (line.kind) {
      _LineKind.unchanged => ('  ', Colors.transparent),
      _LineKind.removed => ('-', palette.danger),
      _LineKind.added => ('+', Colors.green.withValues(alpha: 0.85)),
    };
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 1),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          SizedBox(
            width: 20,
            child: Text(
              marker,
              style: textTheme.bodySmall!.copyWith(
                color: color == Colors.transparent ? palette.muted : color,
                fontFamily: 'monospace',
                fontWeight: FontWeight.w700,
              ),
            ),
          ),
          Expanded(
            child: Text(
              line.text,
              style: textTheme.bodySmall!.copyWith(
                color: color == Colors.transparent ? null : color,
                fontFamily: 'monospace',
              ),
            ),
          ),
        ],
      ),
    );
  }
}

enum _LineKind { unchanged, added, removed }

class _Line {
  const _Line(this.kind, this.text);
  final _LineKind kind;
  final String text;
}

/// Compute a minimal diff of two texts, line by line. A conservative,
/// dependency-free approach: mark lines that only appear in one side using a
/// per-line count, and align shared runs. This keeps the output readable for
/// typical edited text files without pulling in a diff library.
List<_Line> _diffLines(String winner, String loser) {
  final loserLines = loser.split('\n');
  final winnerLines = winner.split('\n');

  final loserCount = <String, int>{};
  for (final l in loserLines) {
    loserCount[l] = (loserCount[l] ?? 0) + 1;
  }

  final kept = <_Line>[];
  final loserRemaining = <String, int>{...loserCount};

  for (final w in winnerLines) {
    final available = loserRemaining[w] ?? 0;
    if (available > 0) {
      kept.add(_Line(_LineKind.unchanged, w));
      loserRemaining[w] = available - 1;
    } else {
      kept.add(_Line(_LineKind.added, w));
    }
  }

  final used = <String, int>{};
  final result = <_Line>[];
  for (var i = 0, j = 0; i < kept.length || j < loserLines.length;) {
    if (i < kept.length && kept[i].kind == _LineKind.unchanged) {
      result.add(kept[i]);
      final text = kept[i].text;
      used[text] = (used[text] ?? 0) + 1;
      i++;
      j++;
      continue;
    }
    if (i < kept.length && kept[i].kind == _LineKind.added) {
      // No matching loser line remaining — emit it as added.
      result.add(kept[i]);
      i++;
      continue;
    }
    if (j < loserLines.length) {
      final l = loserLines[j];
      final usedCount = used[l] ?? 0;
      final totalInWinner =
          kept.where((k) => k.kind == _LineKind.unchanged && k.text == l).length;
      if (usedCount < totalInWinner) {
        // This loser line was already matched to an unchanged winner line
        // emitted earlier; nothing left to emit here for it.
        used[l] = usedCount + 1;
        j++;
        continue;
      }
      result.add(_Line(_LineKind.removed, l));
      j++;
      continue;
    }
    if (i < kept.length) {
      result.add(kept[i]);
      i++;
      continue;
    }
    if (j < loserLines.length) {
      result.add(_Line(_LineKind.removed, loserLines[j]));
      j++;
      continue;
    }
    break;
  }

  return result;
}
