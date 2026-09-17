import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../core/icons/app_icon.dart';
import '../../core/theme/design_tokens.dart';
import '../../service/engine_service.dart';

/// 应用主壳：左侧导航 + 内容区（对应原型的侧边导航结构）。
class HomeShell extends ConsumerWidget {
  const HomeShell({required this.child, super.key});

  final Widget child;

  static const _items = [
    (route: '/vault', icon: 'vault', label: '保险箱'),
    (route: '/devices', icon: 'devices', label: '设备'),
    (route: '/sync', icon: 'sync', label: '同步'),
    (route: '/security', icon: 'shield', label: '安全中心'),
    (route: '/stego', icon: 'stego', label: '隐写术'),
    (route: '/settings', icon: 'gear', label: '设置'),
  ];

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final location = GoRouterState.of(context).uri.path;
    final selectedIndex = _items.indexWhere((it) => location.startsWith(it.route));

    return Scaffold(
      body: Row(
        children: [
          NavigationRail(
            extended: MediaQuery.widthOf(context) >= 1100,
            minExtendedWidth: 180,
            selectedIndex: selectedIndex < 0 ? 0 : selectedIndex,
            onDestinationSelected: (i) => context.go(_items[i].route),
            leading: Padding(
              padding: const EdgeInsets.symmetric(vertical: 16),
              child: AppIcon('logo', size: 28, color: Theme.of(context).colorScheme.primary),
            ),
            destinations: [
              for (final it in _items)
                NavigationRailDestination(
                  icon: AppIcon(it.icon),
                  selectedIcon: AppIcon(it.icon),
                  label: Text(it.label),
                ),
            ],
            trailing: Expanded(
              child: Align(
                alignment: Alignment.bottomCenter,
                child: Padding(
                  padding: const EdgeInsets.only(bottom: 12),
                  child: Text(
                    'engine ${ref.watch(vaultCoreBridgeProvider).coreVersion()}',
                    style: DesignTokens.mono.copyWith(
                      fontSize: 10,
                      color: Theme.of(context).colorScheme.outline,
                    ),
                    overflow: TextOverflow.ellipsis,
                  ),
                ),
              ),
            ),
          ),
          VerticalDivider(width: 1, color: Theme.of(context).dividerColor),
          Expanded(child: child),
        ],
      ),
    );
  }
}

/// 各页面骨架阶段的统一占位页。
class PlaceholderPage extends ConsumerWidget {
  const PlaceholderPage({required this.title, required this.icon, super.key});

  final String title;
  final String icon;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final theme = Theme.of(context);
    return Center(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          AppIcon(icon, size: 48, color: theme.colorScheme.primary),
          const SizedBox(height: 16),
          Text(title, style: theme.textTheme.titleMedium),
          const SizedBox(height: 8),
          Text(
            '骨架占位页 —— 功能开发见 docs/09-开发任务面板.md',
            style: theme.textTheme.bodySmall,
          ),
        ],
      ),
    );
  }
}
