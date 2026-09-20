import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../core/theme/app_theme.dart';
import '../../core/theme/design_tokens.dart';
import '../../l10n/app_localizations.dart';
import '../../state/notification_controller.dart';
import '../../state/session_controller.dart';
import '../../state/sync_controller.dart';
import '../../state/vault_controller.dart';
import '../widgets/vs_ui.dart';
import 'window_chrome.dart';

/// 全局搜索词：顶栏搜索框写入，保险箱页消费（Ctrl K）。
class GlobalSearch extends Notifier<String> {
  @override
  String build() => '';
  void set(String q) => state = q;
}

final globalSearchProvider = NotifierProvider<GlobalSearch, String>(
  GlobalSearch.new,
);

// 保险箱面包屑已并入 vaultUiProvider（state/vault_controller.dart），
// 顶栏读取 crumbs 渲染，跳转经 controller 统一刷新。

/// 各页顶栏标题与副标题（对应原型 tb-title / tb-crumbs）。
/// gen-l10n 无动态 key 查找，故按路由映射到类型安全 getter。
(String, String) _pageMeta(AppLocalizations l, String location) =>
    switch (location) {
      '/vault' => (l.shellTitleVault, ''),
      '/devices' => (l.shellTitleDevices, l.shellSubDevices),
      '/sync' => (l.shellTitleSync, l.shellSubSync),
      '/security' => (l.shellTitleSecurity, l.shellSubSecurity),
      '/stego' => (l.shellTitleStego, l.shellSubStego),
      '/settings' => (l.shellTitleSettings, l.shellSubSettings),
      _ => (l.shellTitleVault, ''),
    };

/// 应用主壳：1:1 还原原型侧边栏 + 顶栏 + 内容区。
class HomeShell extends ConsumerStatefulWidget {
  const HomeShell({required this.child, super.key});

  final Widget child;

  @override
  ConsumerState<HomeShell> createState() => _HomeShellState();
}

class _HomeShellState extends ConsumerState<HomeShell> {
  final _searchFocus = FocusNode();
  final _searchCtl = TextEditingController();
  Timer? _searchDebounce;
  bool _notifOpen = false;

  /// gen-l10n 无动态 key 查找，故按 localeKey 映射到类型安全 getter。
  static String _navLabel(AppLocalizations l, String key) => switch (key) {
        'navVault' => l.navVault,
        'navDevices' => l.navDevices,
        'navSync' => l.navSync,
        'navSecurity' => l.navSecurity,
        'navStego' => l.navStego,
        'navSettings' => l.navSettings,
        _ => key,
      };

  static const _spaceItems = [
    (route: '/vault', icon: 'vault', localeKey: 'navVault'),
    (route: '/devices', icon: 'devices', localeKey: 'navDevices'),
    (route: '/sync', icon: 'sync', localeKey: 'navSync'),
  ];
  static const _controlItems = [
    (route: '/security', icon: 'shield', localeKey: 'navSecurity'),
    (route: '/stego', icon: 'stego', localeKey: 'navStego'),
    (route: '/settings', icon: 'gear', localeKey: 'navSettings'),
  ];

  @override
  void initState() {
    super.initState();
    // Ctrl K 聚焦全局搜索；Esc 收起下拉。
    HardwareKeyboard.instance.addHandler(_onKey);
  }

  @override
  void dispose() {
    HardwareKeyboard.instance.removeHandler(_onKey);
    _searchDebounce?.cancel();
    _searchFocus.dispose();
    _searchCtl.dispose();
    super.dispose();
  }

  bool _onKey(KeyEvent e) {
    if (e is KeyDownEvent &&
        e.logicalKey == LogicalKeyboardKey.keyK &&
        VsShortcut.ctrlPressed &&
        mounted) {
      _searchFocus.requestFocus();
      return true;
    }
    return false;
  }

  void _onSearchChanged(String q) {
    _searchDebounce?.cancel();
    _searchDebounce = Timer(const Duration(milliseconds: 220), () {
      ref.read(globalSearchProvider.notifier).set(q.trim());
    });
  }

  @override
  Widget build(BuildContext context) {
    final v = context.vs;
    final location = GoRouterState.of(context).uri.path;
    final l = AppLocalizations.of(context);
    final narrow = MediaQuery.widthOf(context) < 1100;
    final meta = _pageMeta(l, location);
    final syncing = ref.watch(syncProvider).running;

    return Scaffold(
      body: Listener(
        // 空闲自动落锁（P4-1/P4-5）：任一指针/滚轮活动都重置倒计时。
        // Listener 不参与手势竞技场，不会抢子级的手势。
        behavior: HitTestBehavior.translucent,
        onPointerDown: (_) => _activity(),
        onPointerMove: (_) => _activity(),
        onPointerSignal: (_) => _activity(),
        child: Stack(
          children: [
            Row(
              children: [
                if (!narrow) _Sidebar(location: location, l: l),
                VerticalDivider(width: 1, color: v.borderSoft),
                Expanded(
                  child: Column(
                    children: [
                      _Topbar(
                        location: location,
                        title: meta.$1,
                        sub: meta.$2,
                        searchFocus: _searchFocus,
                        searchCtl: _searchCtl,
                        onSearchChanged: _onSearchChanged,
                        onNotifToggle: () =>
                            setState(() => _notifOpen = !_notifOpen),
                        notifOpen: _notifOpen,
                        onNotifClose: () => setState(() => _notifOpen = false),
                      ),
                      Divider(height: 1, color: v.borderSoft),
                      Expanded(
                        child: SingleChildScrollView(
                          padding: const EdgeInsets.all(DesignTokens.sp5),
                          child: Center(
                            child: ConstrainedBox(
                              constraints: const BoxConstraints(
                                maxWidth: DesignTokens.contentMaxW,
                              ),
                              child: widget.child,
                            ),
                          ),
                        ),
                      ),
                    ],
                  ),
                ),
              ],
            ),
            // Toast 层（右下角，toastin/toastout 动画）
            const Positioned(right: 20, bottom: 20, child: _ToastHost()),
            // 通知下拉面板（点击面板外关闭）
            if (_notifOpen) ...[
              Positioned.fill(
                child: GestureDetector(
                  behavior: HitTestBehavior.translucent,
                  onTap: () => setState(() => _notifOpen = false),
                ),
              ),
              Positioned(
                top: DesignTokens.topbarH + 8,
                right: 20,
                child: _NotifDropdown(),
              ),
            ],
            if (syncing)
              Positioned(
                left: 0,
                right: 0,
                top: 0,
                child: LinearProgressIndicator(
                  minHeight: 2,
                  color: v.gold,
                  backgroundColor: Colors.transparent,
                ),
              ),
          ],
        ),
      ),
    );
  }

  void _activity() => ref.read(sessionProvider.notifier).notifyActivity();
}

// ============================================================
// 侧边栏
// ============================================================

class _Sidebar extends ConsumerWidget {
  const _Sidebar({required this.location, required this.l});
  final String location;
  final AppLocalizations l;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final v = context.vs;
    // 冲突数优先取保险箱内的真实扫描结果（同步页周期刷新），
    // 尚无扫描时退回最近一次同步摘要。
    final sync = ref.watch(syncProvider);
    final conflicts = sync.conflicts.isNotEmpty
        ? sync.conflicts.length
        : (sync.lastSummary?.conflicts.length ?? 0);

    return Container(
      width: DesignTokens.sidebarW,
      color: v.surface,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          // logo 区（同时作为窗口拖拽区，对应原生标题栏左侧）
          WindowDragArea(
            child: Padding(
              padding: const EdgeInsets.fromLTRB(18, 18, 18, 14),
              child: Row(
                children: [
                  Container(
                    width: 30,
                    height: 30,
                    decoration: BoxDecoration(
                      borderRadius: BorderRadius.circular(9),
                      gradient: LinearGradient(
                        begin: Alignment.topLeft,
                        end: Alignment.bottomRight,
                        colors: [v.gold, v.goldGradEnd],
                      ),
                      boxShadow: [
                        BoxShadow(
                          color: v.gold.withValues(alpha: .25),
                          offset: const Offset(0, 2),
                          blurRadius: 8,
                        ),
                      ],
                    ),
                    child: const Center(
                      child:
                          AppIcon('logo', size: 17, color: Color(0xFF17130A)),
                    ),
                  ),
                  const SizedBox(width: 10),
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          'VaultSync',
                          style: TextStyle(
                            fontSize: 14.5,
                            fontWeight: FontWeight.w700,
                            letterSpacing: 0.02,
                            color: v.text,
                          ),
                        ),
                        Text(
                          l.shellVersionTagline,
                          style: TextStyle(fontSize: 12, color: v.text3),
                          overflow: TextOverflow.ellipsis,
                        ),
                      ],
                    ),
                  ),
                ],
              ),
            ),
          ),
          // 导航
          Expanded(
            child: SingleChildScrollView(
              padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 6),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  _navSec(v, l.shellNavSectionSpace),
                  for (final it in _HomeShellSpaceItems.items)
                    _NavItem(
                      it: it,
                      label: _navLabelFor(it.localeKey, l),
                      active: location.startsWith(it.route),
                      badge: it.route == '/sync' && conflicts > 0
                          ? '$conflicts'
                          : null,
                      badgeGold: it.route == '/sync',
                    ),
                  _navSec(v, l.shellNavSectionControl),
                  for (final it in _HomeShellControlItems.items)
                    _NavItem(
                      it: it,
                      label: _navLabelFor(it.localeKey, l),
                      active: location.startsWith(it.route),
                    ),
                ],
              ),
            ),
          ),
          // 底部：立即锁定
          Container(
            padding: const EdgeInsets.fromLTRB(14, 12, 14, 16),
            decoration: BoxDecoration(
              border: Border(top: BorderSide(color: v.borderSoft)),
            ),
            child: VsButton(
              label: l.shellLockNow,
              icon: 'lock',
              tone: VsBtnTone.ghost,
              block: true,
              onPressed: () => ref.read(sessionProvider.notifier).lock(),
            ),
          ),
        ],
      ),
    );
  }

  static String _navLabelFor(String key, AppLocalizations l) =>
      _HomeShellState._navLabel(l, key);

  Widget _navSec(VsScheme v, String title) => Padding(
        padding: const EdgeInsets.fromLTRB(10, 14, 10, 6),
        child: Text(
          title,
          style: TextStyle(
            fontSize: 11,
            fontWeight: FontWeight.w600,
            letterSpacing: 0.08,
            color: v.text3,
          ),
        ),
      );
}

// 常量拆分以便 _Sidebar 内 for-in（Dart 不能在 const 集合上直接展开类字段）。
class _HomeShellSpaceItems {
  static const items = _HomeShellState._spaceItems;
}

class _HomeShellControlItems {
  static const items = _HomeShellState._controlItems;
}

class _NavItem extends StatefulWidget {
  const _NavItem({
    required this.it,
    required this.label,
    required this.active,
    this.badge,
    this.badgeGold = false,
  });
  final ({String route, String icon, String localeKey}) it;
  final String label;
  final bool active;
  final String? badge;
  final bool badgeGold;

  @override
  State<_NavItem> createState() => _NavItemState();
}

class _NavItemState extends State<_NavItem> {
  bool _hover = false;
  bool _focused = false;

  void _activate() => context.go(widget.it.route);

  /// 键盘可达（P4-7 无障碍）：Tab 聚焦、Enter / Space 激活，聚焦态与 hover 同款高亮。
  KeyEventResult _onKey(FocusNode node, KeyEvent event) {
    if (event is! KeyDownEvent) return KeyEventResult.ignored;
    final k = event.logicalKey;
    if (k == LogicalKeyboardKey.enter ||
        k == LogicalKeyboardKey.space ||
        k == LogicalKeyboardKey.numpadEnter) {
      _activate();
      return KeyEventResult.handled;
    }
    return KeyEventResult.ignored;
  }

  @override
  Widget build(BuildContext context) {
    final v = context.vs;
    final active = widget.active;
    final hot = _hover || _focused;
    return Padding(
      padding: const EdgeInsets.only(bottom: 2),
      child: Focus(
        canRequestFocus: true,
        onKeyEvent: _onKey,
        onFocusChange: (f) => setState(() => _focused = f),
        child: Semantics(
          button: true,
          selected: active,
          label: widget.label,
          child: GestureDetector(
            onTap: _activate,
            child: MouseRegion(
              cursor: SystemMouseCursors.click,
              onEnter: (_) => setState(() => _hover = true),
              onExit: (_) => setState(() => _hover = false),
              child: AnimatedContainer(
                duration: const Duration(milliseconds: 130),
                padding:
                    const EdgeInsets.symmetric(horizontal: 10, vertical: 8),
                decoration: BoxDecoration(
                  color: active
                      ? v.goldSoft
                      : hot
                          ? v.surfaceAlt
                          : Colors.transparent,
                  borderRadius: BorderRadius.circular(DesignTokens.rSm),
                ),
                child: Row(
                  children: [
                    AppIcon(
                      widget.it.icon,
                      size: 18,
                      color: active ? v.gold : v.text2,
                    ),
                    const SizedBox(width: 10),
                    Expanded(
                      child: Text(
                        widget.label,
                        style: TextStyle(
                          fontSize: 13.5,
                          fontWeight:
                              active ? FontWeight.w600 : FontWeight.w400,
                          color: active
                              ? v.gold
                              : hot
                                  ? v.text
                                  : v.text2,
                        ),
                      ),
                    ),
                    if (widget.badge != null)
                      Container(
                        padding: const EdgeInsets.symmetric(horizontal: 6),
                        decoration: BoxDecoration(
                          color: widget.badgeGold ? v.gold : v.danger,
                          borderRadius: BorderRadius.circular(10),
                        ),
                        constraints: const BoxConstraints(minWidth: 17),
                        child: Text(
                          widget.badge!,
                          textAlign: TextAlign.center,
                          style: TextStyle(
                            fontSize: 11,
                            height: 17 / 11,
                            color: widget.badgeGold
                                ? const Color(0xFF17130A)
                                : Colors.white,
                          ),
                        ),
                      ),
                  ],
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}

// ============================================================
// 顶栏
// ============================================================

class _Topbar extends ConsumerWidget {
  const _Topbar({
    required this.location,
    required this.title,
    required this.sub,
    required this.searchFocus,
    required this.searchCtl,
    required this.onSearchChanged,
    required this.notifOpen,
    required this.onNotifToggle,
    required this.onNotifClose,
  });

  final String location;
  final String title;
  final String sub;
  final FocusNode searchFocus;
  final TextEditingController searchCtl;
  final ValueChanged<String> onSearchChanged;
  final bool notifOpen;
  final VoidCallback onNotifToggle;
  final VoidCallback onNotifClose;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final v = context.vs;
    final l = AppLocalizations.of(context);
    final unread = ref.watch(notificationProvider).unread;

    // 整条顶栏即窗口标题栏（原生标题栏已隐藏）：空白处可拖动窗口 / 双击最大化。
    return WindowDragArea(
      child: Container(
        height: DesignTokens.topbarH,
        padding: const EdgeInsets.only(left: DesignTokens.sp5),
        color: v.bg,
        child: Row(
          children: [
            Text(
              title,
              style: TextStyle(
                fontSize: 16,
                fontWeight: FontWeight.w600,
                color: v.text,
              ),
            ),
            if (location.startsWith('/vault')) ...[
              const SizedBox(width: 8),
              Expanded(child: _VaultCrumbsBar(current: location)),
              const SizedBox(width: 8),
            ] else if (sub.isNotEmpty) ...[
              const SizedBox(width: 12),
              Text(sub, style: TextStyle(fontSize: 12.5, color: v.text3)),
            ],
            const Spacer(),
            SizedBox(
              width: 300,
              child: Stack(
                alignment: Alignment.centerLeft,
                children: [
                  TextField(
                    controller: searchCtl,
                    focusNode: searchFocus,
                    onChanged: onSearchChanged,
                    style: TextStyle(fontSize: 13, color: v.text),
                    decoration: InputDecoration(
                      hintText: l.shellSearchHint,
                      filled: true,
                      fillColor: v.surface,
                      prefixIcon: Padding(
                        padding: const EdgeInsets.only(left: 11, right: 8),
                        child: AppIcon('search', size: 15, color: v.text3),
                      ),
                      prefixIconConstraints: const BoxConstraints(
                        minWidth: 0,
                        minHeight: 0,
                      ),
                      suffixIcon: Padding(
                        padding: const EdgeInsets.only(right: 9),
                        child: Container(
                          padding: const EdgeInsets.symmetric(
                            horizontal: 5,
                            vertical: 1,
                          ),
                          decoration: BoxDecoration(
                            border: Border.all(color: v.border),
                            borderRadius: BorderRadius.circular(5),
                            color: v.surfaceAlt,
                          ),
                          child: Text(
                            'Ctrl K',
                            style: TextStyle(
                              fontSize: 10.5,
                              fontFamily: DesignTokens.monoFamily,
                              color: v.text3,
                            ),
                          ),
                        ),
                      ),
                      suffixIconConstraints: const BoxConstraints(
                        minWidth: 0,
                        minHeight: 0,
                      ),
                    ),
                  ),
                ],
              ),
            ),
            const SizedBox(width: 8),
            Stack(
              clipBehavior: Clip.none,
              children: [
                VsIconButton(
                  icon: 'bell',
                  tooltip: l.shellNotifications,
                  onPressed: onNotifToggle,
                ),
                if (unread > 0)
                  Positioned(
                    top: 5,
                    right: 5,
                    child: Container(
                      width: 7,
                      height: 7,
                      decoration: BoxDecoration(
                        color: v.danger,
                        shape: BoxShape.circle,
                      ),
                    ),
                  ),
              ],
            ),
            const SizedBox(width: 6),
            // 原生标题栏按钮位（用户指定：通知铃铛右侧）
            const WindowCaptionButtons(),
          ],
        ),
      ),
    );
  }
}

// ============================================================
// 顶栏面包屑（.tb-crumbs）
// ============================================================

void _clearVaultSearch(WidgetRef ref) =>
    ref.read(globalSearchProvider.notifier).set('');

class _VaultCrumbsBar extends ConsumerWidget {
  const _VaultCrumbsBar({required this.current});
  final String current;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final v = context.vs;
    final l = AppLocalizations.of(context);
    final crumbs = ref.watch(vaultUiProvider).crumbs;
    return SingleChildScrollView(
      scrollDirection: Axis.horizontal,
      child: Row(
        children: [
          for (var i = 0; i < crumbs.length; i++) ...[
            if (i > 0) AppIcon('chev-r', size: 12, color: v.text3),
            MouseRegion(
              cursor: i == crumbs.length - 1
                  ? MouseCursor.defer
                  : SystemMouseCursors.click,
              child: GestureDetector(
                onTap: i == crumbs.length - 1
                    ? null
                    : () {
                        _clearVaultSearch(ref);
                        ref.read(vaultUiProvider.notifier).goToCrumb(i);
                      },
                child: Container(
                  padding: const EdgeInsets.symmetric(
                    horizontal: 5,
                    vertical: 2,
                  ),
                  decoration: BoxDecoration(
                    color: i == crumbs.length - 1
                        ? Colors.transparent
                        : Colors.transparent,
                    borderRadius: BorderRadius.circular(5),
                  ),
                  child: Text(
                    (crumbs[i].$2 == null || crumbs[i].$2!.isEmpty)
                        ? l.shellAllFiles
                        : crumbs[i].$2!,
                    style: TextStyle(
                      fontSize: 12.5,
                      color: i == crumbs.length - 1 ? v.text2 : v.text3,
                    ),
                  ),
                ),
              ),
            ),
          ],
        ],
      ),
    );
  }
}

// ============================================================
// 通知下拉面板
// ============================================================

class _NotifDropdown extends ConsumerWidget {
  const _NotifDropdown();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final v = context.vs;
    final l = AppLocalizations.of(context);
    final state = ref.watch(notificationProvider);
    return Container(
      width: 340,
      decoration: BoxDecoration(
        color: v.surface,
        borderRadius: BorderRadius.circular(DesignTokens.rMd),
        border: Border.all(color: v.border),
        boxShadow: v.shadow3,
      ),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 15, vertical: 12),
            child: Row(
              children: [
                Text(
                  l.shellNotifications,
                  style: TextStyle(
                    fontSize: 13,
                    fontWeight: FontWeight.w600,
                    color: v.text,
                  ),
                ),
                const Spacer(),
                GestureDetector(
                  onTap: () =>
                      ref.read(notificationProvider.notifier).markAllRead(),
                  child: MouseRegion(
                    cursor: SystemMouseCursors.click,
                    child: Text(
                      l.shellMarkAllRead,
                      style: TextStyle(fontSize: 12, color: v.text2),
                    ),
                  ),
                ),
              ],
            ),
          ),
          Divider(height: 1, color: v.borderSoft),
          ConstrainedBox(
            constraints: const BoxConstraints(maxHeight: 380),
            child: SingleChildScrollView(
              child: state.items.isEmpty
                  ? Padding(
                      padding: const EdgeInsets.all(22),
                      child: Text(
                        l.shellNoNotifications,
                        style: TextStyle(fontSize: 12.5, color: v.text3),
                      ),
                    )
                  : Column(
                      children: [for (final n in state.items) _NotifItem(n: n)],
                    ),
            ),
          ),
        ],
      ),
    );
  }
}

class _NotifItem extends StatelessWidget {
  const _NotifItem({required this.n});
  final AppNotification n;

  @override
  Widget build(BuildContext context) {
    final v = context.vs;
    final (icon, tone) = switch (n.grade) {
      NotifGrade.ok => ('check', VsTone.ok),
      NotifGrade.warning => ('warn', VsTone.warn),
      NotifGrade.danger => ('oct', VsTone.danger),
      NotifGrade.info => ('info', VsTone.info),
    };
    final time =
        '${n.ts.hour.toString().padLeft(2, '0')}:${n.ts.minute.toString().padLeft(2, '0')}';
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 15, vertical: 11),
      decoration: BoxDecoration(
        border: Border(bottom: BorderSide(color: v.borderSoft)),
      ),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Container(
            width: 30,
            height: 30,
            decoration: BoxDecoration(
              color: vsToneSoft(v, tone),
              borderRadius: BorderRadius.circular(9),
            ),
            child: Center(
              child: AppIcon(icon, size: 14, color: vsToneColor(v, tone)),
            ),
          ),
          const SizedBox(width: 11),
          Expanded(
            child: Text(
              n.message,
              style: TextStyle(fontSize: 12.5, color: v.text, height: 1.5),
            ),
          ),
          const SizedBox(width: 8),
          Text(time, style: TextStyle(fontSize: 11, color: v.text3)),
        ],
      ),
    );
  }
}

// ============================================================
// Toast 宿主：监听通知流，右下角滑入 / 3.4s 后滑出。
// ============================================================

class _ToastHost extends ConsumerStatefulWidget {
  const _ToastHost();

  @override
  ConsumerState<_ToastHost> createState() => _ToastHostState();
}

class _ToastHostData {
  _ToastHostData(this.grade, this.message);
  final NotifGrade grade;
  final String message;
}

class _ToastHostState extends ConsumerState<_ToastHost> {
  final List<({int id, _ToastHostData data, bool out})> _toasts = [];
  int _seq = 0;
  StreamSubscription<AppNotification>? _sub;

  @override
  void initState() {
    super.initState();
    _sub = ref.read(notificationProvider.notifier).stream.listen((n) {
      if (!mounted) return;
      final id = _seq++;
      setState(
        () => _toasts.add((
          id: id,
          data: _ToastHostData(n.grade, n.message),
          out: false,
        )),
      );
      Timer(const Duration(milliseconds: 3400), () {
        if (!mounted) return;
        setState(() {
          final i = _toasts.indexWhere((t) => t.id == id);
          if (i >= 0) _toasts[i] = (id: id, data: _toasts[i].data, out: true);
        });
        Timer(const Duration(milliseconds: 260), () {
          if (mounted) setState(() => _toasts.removeWhere((t) => t.id == id));
        });
      });
      // 最多同时 4 条
      while (_toasts.length > 4) {
        _toasts.removeAt(0);
      }
    });
  }

  @override
  void dispose() {
    _sub?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.end,
      children: [
        for (final t in _toasts)
          Padding(
            padding: const EdgeInsets.only(top: 8),
            child: _ToastAnim(
              key: ValueKey(t.id),
              out: t.out,
              child: SizedBox(
                width: 330,
                child: VsToastCard(
                  grade: t.data.grade,
                  message: t.data.message,
                ),
              ),
            ),
          ),
      ],
    );
  }
}

class _ToastAnim extends StatefulWidget {
  const _ToastAnim({required this.child, required this.out, super.key});
  final Widget child;
  final bool out;

  @override
  State<_ToastAnim> createState() => _ToastAnimState();
}

class _ToastAnimState extends State<_ToastAnim>
    with SingleTickerProviderStateMixin {
  late final AnimationController _c = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 260),
  );
  late final Animation<double> _t = CurvedAnimation(
    parent: _c,
    curve: const Cubic(.22, 1, .3, 1),
  );

  @override
  void initState() {
    super.initState();
    _c.forward();
  }

  @override
  void didUpdateWidget(_ToastAnim old) {
    super.didUpdateWidget(old);
    if (widget.out && !old.out) _c.reverse();
  }

  @override
  void dispose() {
    _c.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return FadeTransition(
      opacity: _t,
      child: SlideTransition(
        position: Tween(
          begin: const Offset(30 / 330, 0),
          end: Offset.zero,
        ).animate(_t),
        child: widget.child,
      ),
    );
  }
}

/// 各页面骨架阶段的统一占位页（安全中心 / 隐写术，P5 接入真实引擎）。
class PlaceholderPage extends StatelessWidget {
  const PlaceholderPage({required this.title, required this.icon, super.key});

  final String title;
  final String icon;

  @override
  Widget build(BuildContext context) {
    final l = AppLocalizations.of(context);
    return SizedBox(
      height: MediaQuery.heightOf(context) - DesignTokens.topbarH - 48,
      child: VsEmpty(
        icon: icon,
        title: title,
        sub: l.shellPlaceholderSub,
      ),
    );
  }
}
