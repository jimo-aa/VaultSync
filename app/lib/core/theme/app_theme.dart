import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'design_tokens.dart';

/// 亮/暗双主题，由 DesignTokens + VsScheme 构造（对应 prototype/styles.css）。
class AppTheme {
  AppTheme._();

  static ThemeData light() => _build(VsScheme.light, Brightness.light);
  static ThemeData dark() => _build(VsScheme.dark, Brightness.dark);

  static ThemeData _build(VsScheme v, Brightness brightness) {
    final scheme = ColorScheme(
      brightness: brightness,
      primary: v.gold,
      onPrimary: const Color(0xFF17130A),
      secondary: v.blue,
      onSecondary: Colors.white,
      error: v.danger,
      onError: Colors.white,
      surface: v.surface,
      onSurface: v.text,
      surfaceContainerHighest: v.surfaceAlt,
      outline: v.border,
      outlineVariant: v.borderSoft,
    );

    TextStyle t(double size,
            {Color? color,
            FontWeight w = FontWeight.w400,
            double? ls,
            String? family}) =>
        TextStyle(
            fontSize: size,
            color: color,
            fontWeight: w,
            letterSpacing: ls,
            fontFamily: family ?? DesignTokens.fontFamily,
            height: 1.55);

    return ThemeData(
      useMaterial3: true,
      brightness: brightness,
      colorScheme: scheme,
      extensions: [v],
      scaffoldBackgroundColor: v.bg,
      // 桌面端禁用 MaterialPage 转场动画：切换瞬时完成，
      // 避免转场帧中新旧页面叠滑导致的内容错位与溢出告警。
      pageTransitionsTheme: const PageTransitionsTheme(builders: {
        TargetPlatform.windows: _NoPageTransitionsBuilder(),
        TargetPlatform.linux: _NoPageTransitionsBuilder(),
        TargetPlatform.macOS: _NoPageTransitionsBuilder(),
        TargetPlatform.android: _NoPageTransitionsBuilder(),
        TargetPlatform.iOS: _NoPageTransitionsBuilder(),
        TargetPlatform.fuchsia: _NoPageTransitionsBuilder(),
      }),
      fontFamily: DesignTokens.fontFamily,
      dividerColor: v.borderSoft,
      splashFactory: InkSparkle.splashFactory,
      textTheme: TextTheme(
        displayLarge: DesignTokens.display.copyWith(color: v.text),
        headlineMedium: DesignTokens.h1.copyWith(color: v.text),
        headlineSmall: DesignTokens.h1.copyWith(color: v.text),
        titleLarge: t(15.5, color: v.text, w: FontWeight.w600),
        titleMedium: t(13.5, color: v.text, w: FontWeight.w600),
        titleSmall: t(13, color: v.text, w: FontWeight.w500),
        bodyLarge: t(13.5, color: v.text),
        bodyMedium: t(13.5, color: v.text),
        bodySmall: t(12, color: v.text2),
        labelLarge: t(13, color: v.text, w: FontWeight.w500),
        labelMedium: t(12, color: v.text2),
        labelSmall: t(11.5, color: v.text3),
      ),
      scrollbarTheme: ScrollbarThemeData(
        thickness: WidgetStateProperty.all(10),
        thumbColor: WidgetStateProperty.all(v.border),
        radius: const Radius.circular(6),
        mainAxisMargin: 2,
        crossAxisMargin: 1,
      ),
      menuTheme: MenuThemeData(
        style: MenuStyle(
          backgroundColor: WidgetStateProperty.all(v.surface),
          surfaceTintColor: WidgetStateProperty.all(Colors.transparent),
          elevation: WidgetStateProperty.all(0),
          side: WidgetStateProperty.all(BorderSide(color: v.border)),
          shape: WidgetStateProperty.all(RoundedRectangleBorder(
              borderRadius: BorderRadius.circular(DesignTokens.rMd))),
          padding:
              WidgetStateProperty.all(const EdgeInsets.symmetric(vertical: 5)),
        ),
      ),
      popupMenuTheme: PopupMenuThemeData(
        color: v.surface,
        surfaceTintColor: Colors.transparent,
        elevation: 0,
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(DesignTokens.rMd),
          side: BorderSide(color: v.border),
        ),
        textStyle: t(13, color: v.text),
      ),
      dropdownMenuTheme: DropdownMenuThemeData(
        textStyle: t(13, color: v.text),
      ),
      segmentedButtonTheme: SegmentedButtonThemeData(
        style: ButtonStyle(
          backgroundColor: WidgetStateProperty.resolveWith((states) =>
              states.contains(WidgetState.selected)
                  ? v.surfaceHover
                  : Colors.transparent),
          foregroundColor: WidgetStateProperty.resolveWith((states) =>
              states.contains(WidgetState.selected) ? v.text : v.text2),
          side: WidgetStateProperty.all(BorderSide(color: v.border)),
          shape: WidgetStateProperty.all(const RoundedRectangleBorder()),
          textStyle: WidgetStateProperty.all(t(12.5)),
          minimumSize: WidgetStateProperty.all(const Size(0, 30)),
          padding: WidgetStateProperty.all(
              const EdgeInsets.symmetric(horizontal: 11)),
        ),
      ),
      switchTheme: SwitchThemeData(
        thumbColor: WidgetStateProperty.all(Colors.white),
        trackColor: WidgetStateProperty.resolveWith((states) =>
            states.contains(WidgetState.selected) ? v.ok : v.border),
        trackOutlineColor: WidgetStateProperty.all(Colors.transparent),
      ),
      checkboxTheme: CheckboxThemeData(
        side: BorderSide(color: v.border, width: 1.5),
        fillColor: WidgetStateProperty.resolveWith((states) =>
            states.contains(WidgetState.selected)
                ? v.blue
                : Colors.transparent),
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(5)),
      ),
      radioTheme: RadioThemeData(
        fillColor: WidgetStateProperty.resolveWith((states) =>
            states.contains(WidgetState.selected) ? v.blue : v.border),
      ),
      dialogTheme: DialogThemeData(
        backgroundColor: v.surface,
        surfaceTintColor: Colors.transparent,
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(DesignTokens.rLg),
          side: BorderSide(color: v.border),
        ),
        titleTextStyle: t(15.5, color: v.text, w: FontWeight.w600),
        contentTextStyle: t(13.5, color: v.text),
      ),
      snackBarTheme: SnackBarThemeData(
        backgroundColor: v.surfaceAlt,
        contentTextStyle: t(13.5, color: v.text),
        behavior: SnackBarBehavior.floating,
      ),
      tooltipTheme: TooltipThemeData(
        decoration: BoxDecoration(
          color: v.surfaceAlt,
          border: Border.all(color: v.border),
          borderRadius: BorderRadius.circular(DesignTokens.rSm),
        ),
        textStyle: t(12, color: v.text),
      ),
      inputDecorationTheme: InputDecorationTheme(
        filled: true,
        fillColor: v.bg,
        isDense: true,
        contentPadding: const EdgeInsets.symmetric(horizontal: 12, vertical: 9),
        hintStyle: t(13, color: v.text3),
        labelStyle: t(12.5, color: v.text2, w: FontWeight.w500),
        border: OutlineInputBorder(
          borderRadius: BorderRadius.circular(DesignTokens.rSm),
          borderSide: BorderSide(color: v.border),
        ),
        enabledBorder: OutlineInputBorder(
          borderRadius: BorderRadius.circular(DesignTokens.rSm),
          borderSide: BorderSide(color: v.border),
        ),
        focusedBorder: OutlineInputBorder(
          borderRadius: BorderRadius.circular(DesignTokens.rSm),
          borderSide: BorderSide(color: v.blue, width: 1),
        ),
      ),
      textSelectionTheme: TextSelectionThemeData(
        cursorColor: v.blue,
        selectionColor: v.blueSoft,
        selectionHandleColor: v.blue,
      ),
    );
  }
}

/// Ctrl+K 聚焦全局搜索等桌面快捷键的便捷封装。
class VsShortcut {
  VsShortcut._();

  static bool get ctrlPressed =>
      HardwareKeyboard.instance.isControlPressed ||
      HardwareKeyboard.instance.isMetaPressed;
}

/// 无转场构建器（见 pageTransitionsTheme 注释）。
class _NoPageTransitionsBuilder extends PageTransitionsBuilder {
  const _NoPageTransitionsBuilder();

  @override
  Widget buildTransitions<T>(
    PageRoute<T> route,
    BuildContext context,
    Animation<double> animation,
    Animation<double> secondaryAnimation,
    Widget child,
  ) =>
      child;
}
