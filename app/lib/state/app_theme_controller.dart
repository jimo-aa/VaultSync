import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'app_prefs.dart';

/// 主题模式（P4-6：设置页切换，经 SharedPreferences 持久化）。
class ThemeController extends Notifier<ThemeMode> {
  static const _key = 'app.theme.mode';

  @override
  ThemeMode build() {
    switch (ref.watch(sharedPreferencesProvider).getString(_key)) {
      case 'light':
        return ThemeMode.light;
      case 'system':
        return ThemeMode.system;
      default:
        // 默认暗色，与原型一致。
        return ThemeMode.dark;
    }
  }

  Future<void> set(ThemeMode mode) async {
    state = mode;
    final name = switch (mode) {
      ThemeMode.light => 'light',
      ThemeMode.dark => 'dark',
      ThemeMode.system => 'system',
    };
    await ref.read(sharedPreferencesProvider).setString(_key, name);
  }
}

final themeControllerProvider =
    NotifierProvider<ThemeController, ThemeMode>(ThemeController.new);