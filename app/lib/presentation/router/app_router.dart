import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../state/session_controller.dart';
import '../pages/lock_screen.dart';
import '../pages/vault_page.dart';
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
      GoRoute(path: '/lock', builder: (context, state) => const LockScreen()),
      ShellRoute(
        builder: (context, state, child) => HomeShell(child: child),
        routes: [
          GoRoute(
            path: '/vault',
            builder: (context, state) => const VaultPage(),
          ),
          _placeholder('/devices', '设备', 'devices'),
          _placeholder('/sync', '同步', 'sync'),
          _placeholder('/security', '安全中心', 'shield'),
          _placeholder('/stego', '隐写术', 'stego'),
          _placeholder('/settings', '设置', 'gear'),
        ],
      ),
    ],
  );
});

GoRoute _placeholder(String path, String title, String icon) => GoRoute(
  path: path,
  builder: (context, state) => PlaceholderPage(title: title, icon: icon),
);
