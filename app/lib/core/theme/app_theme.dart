import 'package:flutter/material.dart';

import 'design_tokens.dart';

/// 亮/暗双主题，由 DesignTokens 构造（docs/03 §5）。
class AppTheme {
  AppTheme._();

  static ThemeData light() => _build(
    brightness: Brightness.light,
    background: DesignTokens.bgLight,
    surface: DesignTokens.surfaceLight,
    surfaceAlt: DesignTokens.surfaceAltLight,
    border: DesignTokens.borderLight,
    textPrimary: DesignTokens.textPrimaryLight,
    textSecondary: DesignTokens.textSecondaryLight,
    accent: DesignTokens.accentLight,
    accentAlt: DesignTokens.accentAltLight,
    danger: DesignTokens.dangerLight,
  );

  static ThemeData dark() => _build(
    brightness: Brightness.dark,
    background: DesignTokens.bgDark,
    surface: DesignTokens.surfaceDark,
    surfaceAlt: DesignTokens.surfaceAltDark,
    border: DesignTokens.borderDark,
    textPrimary: DesignTokens.textPrimaryDark,
    textSecondary: DesignTokens.textSecondaryDark,
    accent: DesignTokens.accentDark,
    accentAlt: DesignTokens.accentAltDark,
    danger: DesignTokens.dangerDark,
  );

  static ThemeData _build({
    required Brightness brightness,
    required Color background,
    required Color surface,
    required Color surfaceAlt,
    required Color border,
    required Color textPrimary,
    required Color textSecondary,
    required Color accent,
    required Color accentAlt,
    required Color danger,
  }) {
    final scheme = ColorScheme(
      brightness: brightness,
      primary: accent,
      onPrimary: brightness == Brightness.dark ? DesignTokens.bgDark : Colors.white,
      secondary: accentAlt,
      onSecondary: Colors.white,
      error: danger,
      onError: Colors.white,
      surface: surface,
      onSurface: textPrimary,
      surfaceContainerHighest: surfaceAlt,
      outline: border,
      outlineVariant: border,
    );

    return ThemeData(
      useMaterial3: true,
      brightness: brightness,
      colorScheme: scheme,
      scaffoldBackgroundColor: background,
      fontFamily: DesignTokens.fontFamily,
      dividerColor: border,
      textTheme: Typography.material2021(platform: TargetPlatform.windows)
          .englishLike
          .copyWith(
            displayLarge: DesignTokens.display.copyWith(color: textPrimary),
            headlineMedium: DesignTokens.h1.copyWith(color: textPrimary),
            titleMedium: DesignTokens.h2.copyWith(color: textPrimary),
            bodyMedium: DesignTokens.body.copyWith(color: textPrimary),
            bodySmall: DesignTokens.caption.copyWith(color: textSecondary),
          ),
      navigationRailTheme: NavigationRailThemeData(
        backgroundColor: surface,
        indicatorColor: surfaceAlt,
        selectedIconTheme: IconThemeData(color: accent),
        selectedLabelTextStyle: TextStyle(color: accent, fontWeight: FontWeight.w600),
        unselectedIconTheme: IconThemeData(color: textSecondary),
        unselectedLabelTextStyle: TextStyle(color: textSecondary),
      ),
      cardTheme: CardThemeData(
        color: surface,
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(10),
          side: BorderSide(color: border),
        ),
      ),
      inputDecorationTheme: InputDecorationTheme(
        filled: true,
        fillColor: surfaceAlt,
        border: OutlineInputBorder(
          borderRadius: BorderRadius.circular(8),
          borderSide: BorderSide(color: border),
        ),
        enabledBorder: OutlineInputBorder(
          borderRadius: BorderRadius.circular(8),
          borderSide: BorderSide(color: border),
        ),
      ),
      dialogTheme: DialogThemeData(
        backgroundColor: surface,
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(12)),
      ),
      snackBarTheme: SnackBarThemeData(
        backgroundColor: surfaceAlt,
        contentTextStyle: DesignTokens.body.copyWith(color: textPrimary),
        behavior: SnackBarBehavior.floating,
      ),
    );
  }
}
