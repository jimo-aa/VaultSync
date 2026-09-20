import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:shared_preferences/shared_preferences.dart';

/// 全局持久化存储（桌面用 SharedPreferences）。
/// 由 `main()` 在 runApp 前 `SharedPreferences.getInstance()` 后注入（overrideWithValue）。
final sharedPreferencesProvider = Provider<SharedPreferences>(
  (ref) => throw StateError('SharedPreferences 未注入：main 中 override'),
);

Future<String?> readPref(WidgetRef ref, String key) async =>
    ref.read(sharedPreferencesProvider).getString(key);

Future<void> writePref(WidgetRef ref, String key, String value) async {
  await ref.read(sharedPreferencesProvider).setString(key, value);
}
