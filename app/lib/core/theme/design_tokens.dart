import 'package:flutter/material.dart';

/// 设计 Token，取自 docs/03-前端设计.md §5（亮/暗双主题）。
abstract final class DesignTokens {
  // 色彩
  static const bgLight = Color(0xFFF7F8FA);
  static const bgDark = Color(0xFF10131A);
  static const surfaceLight = Color(0xFFFFFFFF);
  static const surfaceDark = Color(0xFF1A1F2B);
  static const surfaceAltLight = Color(0xFFEEF0F4);
  static const surfaceAltDark = Color(0xFF242A38);
  static const borderLight = Color(0xFFE3E6EC);
  static const borderDark = Color(0xFF2E3544);
  static const textPrimaryLight = Color(0xFF1A2333);
  static const textPrimaryDark = Color(0xFFE8EBF2);
  static const textSecondaryLight = Color(0xFF5A6A7F);
  static const textSecondaryDark = Color(0xFF8A94A6);
  static const accentLight = Color(0xFFC7A24A); // 香槟金
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

  // 排版：Display 32/H1 24/H2 18/Body 14/Caption 12/Mono 13
  static const fontFamily = 'Segoe UI';
  static const monoFamily = 'Consolas';

  static const display = TextStyle(fontSize: 32, fontWeight: FontWeight.w600);
  static const h1 = TextStyle(fontSize: 24, fontWeight: FontWeight.w600);
  static const h2 = TextStyle(fontSize: 18, fontWeight: FontWeight.w600);
  static const body = TextStyle(fontSize: 14);
  static const caption = TextStyle(fontSize: 12);
  static const mono = TextStyle(fontSize: 13, fontFamily: monoFamily);
}
