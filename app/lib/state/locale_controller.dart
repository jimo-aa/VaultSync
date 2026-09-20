import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'app_prefs.dart';

/// 界面语言（P4-7）。null = 跟随系统；目前支持 zh / en（见 l10n.yaml 与 ARB）。
class LocaleController extends Notifier<Locale?> {
  static const _key = 'app.locale';

  @override
  Locale? build() {
    final v = ref.watch(sharedPreferencesProvider).getString(_key);
    if (v == null || v.isEmpty || v == 'system') return null;
    return Locale(v);
  }

  Future<void> set(Locale? locale) async {
    state = locale;
    await ref
        .read(sharedPreferencesProvider)
        .setString(_key, locale?.languageCode ?? 'system');
  }
}

final localeControllerProvider =
    NotifierProvider<LocaleController, Locale?>(LocaleController.new);
