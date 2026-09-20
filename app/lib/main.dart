import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'package:window_manager/window_manager.dart';

import 'core/l10n/vs_l10n.dart';
import 'core/theme/app_theme.dart';
import 'l10n/app_localizations.dart';
import 'presentation/router/app_router.dart';
import 'state/app_prefs.dart';
import 'state/app_theme_controller.dart';
import 'state/locale_controller.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  final prefs = await SharedPreferences.getInstance();
  await windowManager.ensureInitialized();

  const windowOptions = WindowOptions(
    title: 'VaultSync',
    minimumSize: Size(1024, 680),
    size: Size(1280, 800),
    center: true,
    // 隐藏原生标题栏：拖动 / 最小化 / 最大化 / 关闭由壳层自绘
    // （见 presentation/shell/window_chrome.dart），原生按钮一并隐藏。
    titleBarStyle: TitleBarStyle.hidden,
    windowButtonVisibility: false,
  );
  windowManager.waitUntilReadyToShow(windowOptions, () async {
    await windowManager.show();
    await windowManager.focus();
  });

  runApp(ProviderScope(
    overrides: [sharedPreferencesProvider.overrideWithValue(prefs)],
    child: const VaultSyncApp(),
  ));
}

class VaultSyncApp extends ConsumerWidget {
  const VaultSyncApp({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    // P4-6：主题切换状态接入（SharedPreferences 持久化），默认暗色与原型一致。
    final themeMode = ref.watch(themeControllerProvider);
    // P4-7：界面语言；null = 跟随系统（在 supportedLocales 内解析，兜底 zh）。
    final locale = ref.watch(localeControllerProvider);
    return MaterialApp.router(
      // 用字面量：此 context 位于 MaterialApp 之上，尚无 Localizations 祖先。
      title: 'VaultSync',
      // 自绘标题栏占用了右上角（窗口控制按钮），调试彩带会盖住关闭键。
      debugShowCheckedModeBanner: false,
      theme: AppTheme.light(),
      darkTheme: AppTheme.dark(),
      themeMode: themeMode,
      routerConfig: ref.watch(appRouterProvider),
      localizationsDelegates: const [
        AppLocalizations.delegate,
        GlobalMaterialLocalizations.delegate,
        GlobalWidgetsLocalizations.delegate,
        GlobalCupertinoLocalizations.delegate,
      ],
      supportedLocales: AppLocalizations.supportedLocales,
      locale: locale,
      // 状态层 / 服务层的文案经此拿到当前语言（它们不在组件树内）。
      builder: (context, child) {
        VsL10n.attach(AppLocalizations.of(context));
        return child ?? const SizedBox.shrink();
      },
      localeResolutionCallback: (device, supported) {
        // 跟随系统时：设备语言受支持则用它，否则回落模板语言（zh）。
        if (device != null) {
          for (final s in supported) {
            if (s.languageCode == device.languageCode) return s;
          }
        }
        return supported.isEmpty ? const Locale('zh') : supported.first;
      },
    );
  }
}
