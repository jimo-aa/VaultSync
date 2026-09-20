import 'package:flutter/material.dart';

/// 设计 Token，1:1 对应 prototype/styles.css（docs/03-前端设计.md §5）。
abstract final class DesignTokens {
  // ---------- 基础色彩 ----------
  static const bgLight = Color(0xFFF7F8FA);
  static const bgDark = Color(0xFF10131A);
  static const surfaceLight = Color(0xFFFFFFFF);
  static const surfaceDark = Color(0xFF1A1F2B);
  static const surfaceAltLight = Color(0xFFEEF0F4);
  static const surfaceAltDark = Color(0xFF242A38);
  static const surfaceHoverLight = Color(0xFFE6E9F0);
  static const surfaceHoverDark = Color(0xFF2A3140);
  static const borderLight = Color(0xFFE3E6EC);
  static const borderDark = Color(0xFF2E3544);
  static const borderSoftLight = Color(0xFFEBEEF3);
  static const borderSoftDark = Color(0xFF262D3B);
  static const textPrimaryLight = Color(0xFF1A2333);
  static const textPrimaryDark = Color(0xFFE8EBF2);
  static const textSecondaryLight = Color(0xFF5A6A7F);
  static const textSecondaryDark = Color(0xFF8A94A6);
  static const textTertiaryLight = Color(0xFF93A0B2);
  static const textTertiaryDark = Color(0xFF5E6980);

  // ---------- 语义色 ----------
  static const accentLight = Color(0xFFB98F2E); // 香槟金
  static const accentDark = Color(0xFFD9B25F);
  static const accentAltLight = Color(0xFF2050E0);
  static const accentAltDark = Color(0xFF4C7BFF);
  static const successLight = Color(0xFF1E9B5A);
  static const successDark = Color(0xFF34C77B);
  static const warningLight = Color(0xFFD9820B);
  static const warningDark = Color(0xFFF0A63C);
  static const dangerLight = Color(0xFFC53A3A);
  static const dangerDark = Color(0xFFE55252);
  static const encryptLight = Color(0xFF7A5CC0);
  static const encryptDark = Color(0xFF9B7FE0);

  // ---------- soft 底色（rgba 原值换算） ----------
  static const goldSoftDark = Color(0x24D9B25F); // rgba(217,178,95,.14)
  static const goldSoftLight = Color(0x29C7A24A); // rgba(199,162,74,.16)
  static const blueSoftDark = Color(0x244C7BFF);
  static const blueSoftLight = Color(0x1A2050E0);
  static const okSoftDark = Color(0x2134C77B);
  static const okSoftLight = Color(0x1C1E9B5A);
  static const warnSoftDark = Color(0x21F0A63C);
  static const warnSoftLight = Color(0x1FD9820B);
  static const dangerSoftDark = Color(0x21E55252);
  static const dangerSoftLight = Color(0x1AC53A3A);
  static const encSoftDark = Color(0x269B7FE0);
  static const encSoftLight = Color(0x1F7A5CC0);
  static const backdropDark = Color(0x9E06080C); // rgba(6,8,12,.62)
  static const backdropLight = Color(0x611A2333); // rgba(26,35,51,.38)

  // ---------- 间距 / 圆角 ----------
  static const sp1 = 4.0;
  static const sp2 = 8.0;
  static const sp3 = 12.0;
  static const sp4 = 16.0;
  static const sp5 = 24.0;
  static const sp6 = 32.0;
  static const rSm = 8.0;
  static const rMd = 12.0;
  static const rLg = 16.0;

  // ---------- 布局 ----------
  static const sidebarW = 232.0;
  static const topbarH = 56.0;
  static const contentMaxW = 1280.0;

  // ---------- 排版 ----------
  static const fontFamily = 'Segoe UI';
  static const monoFamily = 'Consolas';

  static const display = TextStyle(fontSize: 32, fontWeight: FontWeight.w600);
  static const h1 =
      TextStyle(fontSize: 21, fontWeight: FontWeight.w600, letterSpacing: 0.01);
  static const h2 = TextStyle(fontSize: 18, fontWeight: FontWeight.w600);
  static const body = TextStyle(fontSize: 13.5);
  static const caption = TextStyle(fontSize: 12);
  static const mono =
      TextStyle(fontSize: 12, fontFamily: monoFamily, letterSpacing: 0.01);
}

/// 语义色系扩展：组件经 `Theme.of(context).vs` 读取，亮暗主题各一份。
class VsScheme extends ThemeExtension<VsScheme> {
  const VsScheme({
    required this.bg,
    required this.surface,
    required this.surfaceAlt,
    required this.surfaceHover,
    required this.border,
    required this.borderSoft,
    required this.text,
    required this.text2,
    required this.text3,
    required this.gold,
    required this.goldSoft,
    required this.blue,
    required this.blueSoft,
    required this.ok,
    required this.okSoft,
    required this.warn,
    required this.warnSoft,
    required this.danger,
    required this.dangerSoft,
    required this.enc,
    required this.encSoft,
    required this.backdrop,
    required this.shadow1,
    required this.shadow2,
    required this.shadow3,
    required this.goldGradEnd,
  });

  final Color bg;
  final Color surface;
  final Color surfaceAlt;
  final Color surfaceHover;
  final Color border;
  final Color borderSoft;
  final Color text;
  final Color text2;
  final Color text3;
  final Color gold;
  final Color goldSoft;
  final Color blue;
  final Color blueSoft;
  final Color ok;
  final Color okSoft;
  final Color warn;
  final Color warnSoft;
  final Color danger;
  final Color dangerSoft;
  final Color enc;
  final Color encSoft;
  final Color backdrop;

  /// 侧边 logo 渐变终点（dark: #A8823A / light: #8F6D24）。
  final Color goldGradEnd;

  final List<BoxShadow> shadow1;
  final List<BoxShadow> shadow2;
  final List<BoxShadow> shadow3;

  static const dark = VsScheme(
    bg: DesignTokens.bgDark,
    surface: DesignTokens.surfaceDark,
    surfaceAlt: DesignTokens.surfaceAltDark,
    surfaceHover: DesignTokens.surfaceHoverDark,
    border: DesignTokens.borderDark,
    borderSoft: DesignTokens.borderSoftDark,
    text: DesignTokens.textPrimaryDark,
    text2: DesignTokens.textSecondaryDark,
    text3: DesignTokens.textTertiaryDark,
    gold: DesignTokens.accentDark,
    goldSoft: DesignTokens.goldSoftDark,
    blue: DesignTokens.accentAltDark,
    blueSoft: DesignTokens.blueSoftDark,
    ok: DesignTokens.successDark,
    okSoft: DesignTokens.okSoftDark,
    warn: DesignTokens.warningDark,
    warnSoft: DesignTokens.warnSoftDark,
    danger: DesignTokens.dangerDark,
    dangerSoft: DesignTokens.dangerSoftDark,
    enc: DesignTokens.encryptDark,
    encSoft: DesignTokens.encSoftDark,
    backdrop: DesignTokens.backdropDark,
    goldGradEnd: Color(0xFFA8823A),
    shadow1: [
      BoxShadow(offset: Offset(0, 2), blurRadius: 8, color: Color(0x47000000)),
    ],
    shadow2: [
      BoxShadow(offset: Offset(0, 8), blurRadius: 28, color: Color(0x6B000000)),
    ],
    shadow3: [
      BoxShadow(
          offset: Offset(0, 18), blurRadius: 56, color: Color(0x8C000000)),
    ],
  );

  static const light = VsScheme(
    bg: DesignTokens.bgLight,
    surface: DesignTokens.surfaceLight,
    surfaceAlt: DesignTokens.surfaceAltLight,
    surfaceHover: DesignTokens.surfaceHoverLight,
    border: DesignTokens.borderLight,
    borderSoft: DesignTokens.borderSoftLight,
    text: DesignTokens.textPrimaryLight,
    text2: DesignTokens.textSecondaryLight,
    text3: DesignTokens.textTertiaryLight,
    gold: DesignTokens.accentLight,
    goldSoft: DesignTokens.goldSoftLight,
    blue: DesignTokens.accentAltLight,
    blueSoft: DesignTokens.blueSoftLight,
    ok: DesignTokens.successLight,
    okSoft: DesignTokens.okSoftLight,
    warn: DesignTokens.warningLight,
    warnSoft: DesignTokens.warnSoftLight,
    danger: DesignTokens.dangerLight,
    dangerSoft: DesignTokens.dangerSoftLight,
    enc: DesignTokens.encryptLight,
    encSoft: DesignTokens.encSoftLight,
    backdrop: DesignTokens.backdropLight,
    goldGradEnd: Color(0xFF8F6D24),
    shadow1: [
      BoxShadow(offset: Offset(0, 2), blurRadius: 8, color: Color(0x141A2333)),
    ],
    shadow2: [
      BoxShadow(offset: Offset(0, 8), blurRadius: 28, color: Color(0x211A2333)),
    ],
    shadow3: [
      BoxShadow(
          offset: Offset(0, 18), blurRadius: 56, color: Color(0x331A2333)),
    ],
  );

  @override
  VsScheme copyWith({Brightness? brightness}) => this;

  @override
  VsScheme lerp(VsScheme? other, double t) => other == null || t == 0
      ? this
      : (t == 1 ? other : _lerped(this, other, t));

  static VsScheme _lerped(VsScheme a, VsScheme b, double t) {
    Color c(Color x, Color y) => Color.lerp(x, y, t)!;
    List<BoxShadow> s(List<BoxShadow> x, List<BoxShadow> y) =>
        BoxShadow.lerpList(x, y, t)!;
    return VsScheme(
      bg: c(a.bg, b.bg),
      surface: c(a.surface, b.surface),
      surfaceAlt: c(a.surfaceAlt, b.surfaceAlt),
      surfaceHover: c(a.surfaceHover, b.surfaceHover),
      border: c(a.border, b.border),
      borderSoft: c(a.borderSoft, b.borderSoft),
      text: c(a.text, b.text),
      text2: c(a.text2, b.text2),
      text3: c(a.text3, b.text3),
      gold: c(a.gold, b.gold),
      goldSoft: c(a.goldSoft, b.goldSoft),
      blue: c(a.blue, b.blue),
      blueSoft: c(a.blueSoft, b.blueSoft),
      ok: c(a.ok, b.ok),
      okSoft: c(a.okSoft, b.okSoft),
      warn: c(a.warn, b.warn),
      warnSoft: c(a.warnSoft, b.warnSoft),
      danger: c(a.danger, b.danger),
      dangerSoft: c(a.dangerSoft, b.dangerSoft),
      enc: c(a.enc, b.enc),
      encSoft: c(a.encSoft, b.encSoft),
      backdrop: c(a.backdrop, b.backdrop),
      goldGradEnd: c(a.goldGradEnd, b.goldGradEnd),
      shadow1: s(a.shadow1, b.shadow1),
      shadow2: s(a.shadow2, b.shadow2),
      shadow3: s(a.shadow3, b.shadow3),
    );
  }
}

extension VsSchemeX on ThemeData {
  /// `Theme.of(context).vs` —— 原型语义色快捷入口。
  VsScheme get vs => extension<VsScheme>() ?? VsScheme.dark;
}
