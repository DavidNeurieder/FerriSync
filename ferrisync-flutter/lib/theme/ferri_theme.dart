import 'package:flutter/material.dart';

/// FerriSync design tokens: brand chrome, semantic sync colors, spacing and
/// radii. This is the single source of truth for the UI's visual language.
abstract final class FerriTokens {
  static const Color bg = Color(0xFF0B0D10);
  static const Color surface = Color(0xFF14161B);
  static const Color surfaceHigh = Color(0xFF1A1D24);
  static const Color border = Color(0xFF23272E);
  static const Color primary = Color(0xFFF97316);
  static const Color primaryHover = Color(0xFFFB923C);
  static const Color onPrimary = Color(0xFF2B1804);

  // Light-mode chrome (a lighter sibling of the dark-first identity).
  static const Color lightBg = Color(0xFFF6F7F9);
  static const Color lightSurface = Color(0xFFFFFFFF);
  static const Color lightSurfaceHigh = Color(0xFFECEEF2);
  static const Color lightBorder = Color(0xFFD9DDE3);
  static const Color lightOnPrimary = Color(0xFF3F2206);

  // Semantic sync status colors (same in both modes).
  static const Color success = Color(0xFF22C55E);
  static const Color syncing = Color(0xFF3B82F6);
  static const Color warning = Color(0xFFF59E0B);
  static const Color danger = Color(0xFFEF4444);
  static const Color muted = Color(0xFF8A8F98);

  // Spacing scale.
  static const double spaceXS = 4;
  static const double spaceS = 8;
  static const double spaceM = 12;
  static const double spaceL = 16;
  static const double spaceXL = 24;

  // Radius scale.
  static const double radiusS = 8;
  static const double radiusM = 12;
  static const double radiusL = 16;
}

/// Per-brightness palette, reachable from any widget via
/// `Theme.of(context).extension<FerriPalette>()`.
@immutable
class FerriPalette extends ThemeExtension<FerriPalette> {
  const FerriPalette({
    required this.bg,
    required this.surface,
    required this.surfaceHigh,
    required this.border,
    required this.primary,
    required this.primaryHover,
    required this.onPrimary,
    required this.success,
    required this.syncing,
    required this.warning,
    required this.danger,
    required this.muted,
  });

  final Color bg;
  final Color surface;
  final Color surfaceHigh;
  final Color border;
  final Color primary;
  final Color primaryHover;
  final Color onPrimary;
  final Color success;
  final Color syncing;
  final Color warning;
  final Color danger;
  final Color muted;

  static const FerriPalette dark = FerriPalette(
    bg: FerriTokens.bg,
    surface: FerriTokens.surface,
    surfaceHigh: FerriTokens.surfaceHigh,
    border: FerriTokens.border,
    primary: FerriTokens.primary,
    primaryHover: FerriTokens.primaryHover,
    onPrimary: FerriTokens.onPrimary,
    success: FerriTokens.success,
    syncing: FerriTokens.syncing,
    warning: FerriTokens.warning,
    danger: FerriTokens.danger,
    muted: FerriTokens.muted,
  );

  static const FerriPalette light = FerriPalette(
    bg: FerriTokens.lightBg,
    surface: FerriTokens.lightSurface,
    surfaceHigh: FerriTokens.lightSurfaceHigh,
    border: FerriTokens.lightBorder,
    primary: FerriTokens.primary,
    primaryHover: FerriTokens.primaryHover,
    onPrimary: FerriTokens.lightOnPrimary,
    success: FerriTokens.success,
    syncing: FerriTokens.syncing,
    warning: FerriTokens.warning,
    danger: FerriTokens.danger,
    muted: Color(0xFF6B7280),
  );

  @override
  FerriPalette copyWith({
    Color? bg,
    Color? surface,
    Color? surfaceHigh,
    Color? border,
    Color? primary,
    Color? primaryHover,
    Color? onPrimary,
    Color? success,
    Color? syncing,
    Color? warning,
    Color? danger,
    Color? muted,
  }) {
    return FerriPalette(
      bg: bg ?? this.bg,
      surface: surface ?? this.surface,
      surfaceHigh: surfaceHigh ?? this.surfaceHigh,
      border: border ?? this.border,
      primary: primary ?? this.primary,
      primaryHover: primaryHover ?? this.primaryHover,
      onPrimary: onPrimary ?? this.onPrimary,
      success: success ?? this.success,
      syncing: syncing ?? this.syncing,
      warning: warning ?? this.warning,
      danger: danger ?? this.danger,
      muted: muted ?? this.muted,
    );
  }

  @override
  FerriPalette lerp(ThemeExtension<FerriPalette>? other, double t) {
    if (other is! FerriPalette) return this;
    Color c(Color a, Color b) => Color.lerp(a, b, t)!;
    return FerriPalette(
      bg: c(bg, other.bg),
      surface: c(surface, other.surface),
      surfaceHigh: c(surfaceHigh, other.surfaceHigh),
      border: c(border, other.border),
      primary: c(primary, other.primary),
      primaryHover: c(primaryHover, other.primaryHover),
      onPrimary: c(onPrimary, other.onPrimary),
      success: c(success, other.success),
      syncing: c(syncing, other.syncing),
      warning: c(warning, other.warning),
      danger: c(danger, other.danger),
      muted: c(muted, other.muted),
    );
  }
}

/// Theme builders. Dark-first identity; a light sibling keeps the app usable
/// in bright environments.
abstract final class FerriTheme {
  static ThemeData dark({bool isDark = true}) => _build(isDark: true);

  static ThemeData light() => _build(isDark: false);

  static ThemeData _build({required bool isDark}) {
    final palette = isDark ? FerriPalette.dark : FerriPalette.light;
    final scheme = ColorScheme.fromSeed(
      seedColor: FerriTokens.primary,
      brightness: isDark ? Brightness.dark : Brightness.light,
    );

    return ThemeData(
      useMaterial3: true,
      colorScheme: scheme,
      brightness: isDark ? Brightness.dark : Brightness.light,
      scaffoldBackgroundColor: palette.bg,
      cardTheme: CardThemeData(
        color: palette.surface,
        surfaceTintColor: Colors.transparent,
        elevation: 0,
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(FerriTokens.radiusM),
          side: BorderSide(color: palette.border),
        ),
      ),
      dividerTheme: DividerThemeData(color: palette.border, thickness: 1),
      appBarTheme: AppBarTheme(
        backgroundColor: palette.bg,
        elevation: 0,
        scrolledUnderElevation: 0,
        surfaceTintColor: Colors.transparent,
        titleTextStyle: TextStyle(
          color: scheme.onSurface,
          fontSize: 18,
          fontWeight: FontWeight.w600,
        ),
      ),
      navigationBarTheme: NavigationBarThemeData(
        backgroundColor: palette.surface,
        surfaceTintColor: Colors.transparent,
        indicatorColor: palette.primary.withValues(alpha: 0.18),
        height: 68,
        labelTextStyle: WidgetStatePropertyAll(
          TextStyle(
            fontSize: 12,
            fontWeight: FontWeight.w500,
            color: scheme.onSurfaceVariant,
          ),
        ),
        iconTheme: WidgetStateProperty.resolveWith(
          (states) => IconThemeData(
            color: states.contains(WidgetState.selected)
                ? palette.primary
                : scheme.onSurfaceVariant,
          ),
        ),
      ),
      filledButtonTheme: FilledButtonThemeData(
        style: FilledButton.styleFrom(
          backgroundColor: palette.primary,
          foregroundColor: palette.onPrimary,
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(FerriTokens.radiusS),
          ),
        ),
      ),
      inputDecorationTheme: InputDecorationTheme(
        filled: true,
        fillColor: palette.surfaceHigh,
        hintStyle: TextStyle(color: palette.muted),
        border: OutlineInputBorder(
          borderRadius: BorderRadius.circular(FerriTokens.radiusS),
          borderSide: BorderSide(color: palette.border),
        ),
        enabledBorder: OutlineInputBorder(
          borderRadius: BorderRadius.circular(FerriTokens.radiusS),
          borderSide: BorderSide(color: palette.border),
        ),
      ),
      snackBarTheme: SnackBarThemeData(
        backgroundColor: palette.surfaceHigh,
        contentTextStyle: const TextStyle(color: Colors.white),
        behavior: SnackBarBehavior.floating,
      ),
      extensions: [palette],
    );
  }
}

extension FerriColorsX on BuildContext {
  /// Falls back to the dark palette when the caller's [ThemeData] doesn't
  /// install the extension (plain `MaterialApp` in widget tests).
  FerriPalette get ferri =>
      Theme.of(this).extension<FerriPalette>() ?? FerriPalette.dark;
}