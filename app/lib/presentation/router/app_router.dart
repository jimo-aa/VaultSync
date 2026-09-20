import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../state/session_controller.dart';
import '../pages/devices_page.dart';
import '../pages/lock_screen.dart';
import '../pages/settings_page.dart';
import '../pages/sync_page.dart';
import '../pages/vault_page.dart';
import '../../l10n/app_localizations.dart';
import '../shell/home_shell.dart';

/// 路由：锁屏守卫 —— 会话未解锁时强制跳转 /lock。
final appRouterProvider = Provider<GoRouter>((ref) {
  final notifier = ValueNotifier(0);
  ref.listen(sessionProvider, (_, _) => notifier.value++);
  ref.onDispose(notifier.dispose);

  return GoRouter(
    initialLocation: '/vault',
    refreshListenable: notifier,
    redirect: (context, state) {
      final locked = ref.read(sessionProvider) is SessionLocked;
      final onLock = state.uri.path == '/lock';
      if (locked && !onLock) return '/lock';
      if (!locked && onLock) return '/vault';
      return null;
    },
    routes: [
      GoRoute(
        path: '/lock',
        pageBuilder: (context, state) =>
            const NoTransitionPage(child: LockScreen()),
      ),
      ShellRoute(
        builder: (context, state, child) => HomeShell(child: child),
        routes: [
          GoRoute(
            path: '/vault',
            pageBuilder: (context, state) =>
                const NoTransitionPage(child: VaultPage()),
          ),
          GoRoute(
            path: '/devices',
            pageBuilder: (context, state) =>
                const NoTransitionPage(child: DevicesPage()),
          ),
          GoRoute(
            path: '/sync',
            pageBuilder: (context, state) =>
                const NoTransitionPage(child: SyncPage()),
          ),
          // 安全中心与隐写术属 P5（无 FFI 导出），当前维持占位页。
          _placeholder('/security', (l) => l.shellTitleSecurity, 'shield'),
          _placeholder('/stego', (l) => l.shellTitleStego, 'stego'),
          GoRoute(
            path: '/settings',
            pageBuilder: (context, state) =>
                const NoTransitionPage(child: SettingsPage()),
          ),
        ],
      ),
    ],
  );
});

GoRoute _placeholder(
  String path,
  String Function(AppLocalizations l) title,
  String icon,
) =>
    GoRoute(
      path: path,
      pageBuilder: (context, state) => NoTransitionPage(
        child: PlaceholderPage(
          title: title(AppLocalizations.of(context)),
          icon: icon,
        ),
      ),
    );
