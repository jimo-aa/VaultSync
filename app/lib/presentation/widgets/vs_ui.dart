import 'dart:ui';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../../core/icons/app_icon.dart';
import '../../core/theme/design_tokens.dart';
import '../../core/theme/vs_motion.dart';
import '../../l10n/app_localizations.dart';
import '../../state/notification_controller.dart';

export '../../core/icons/app_icon.dart';

/// 原型组件库：1:1 对应 prototype/styles.css 的部件与动效。
/// 所有组件经 `Theme.of(context).vs` 取色，自动适配亮暗主题。

// ============================================================
// 页面进场动画（.page → @keyframes pagein）
// ============================================================

/// pagein .28s cubic-bezier(.22,1,.3,1)：淡入 + 上移 8px。
class PageIn extends StatefulWidget {
  const PageIn({required this.child, super.key, this.duration});
  final Widget child;
  final Duration? duration;

  @override
  State<PageIn> createState() => _PageInState();
}

class _PageInState extends State<PageIn> with SingleTickerProviderStateMixin {
  late final AnimationController _c = AnimationController(
    vsync: this,
    duration: widget.duration ?? const Duration(milliseconds: 280),
  );
  late final Animation<double> _fade =
      CurvedAnimation(parent: _c, curve: const Cubic(.22, 1, .3, 1));

  @override
  void initState() {
    super.initState();
    _c.forward();
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    // prefers-reduced-motion：时长随无障碍设置变化（initState 读不到 MediaQuery）。
    _c.duration = VsMotion.ms(context, widget.duration?.inMilliseconds ?? 280);
  }

  @override
  void dispose() {
    _c.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return AnimatedBuilder(
      animation: _fade,
      builder: (context, child) => Opacity(
        opacity: _fade.value,
        child: Transform.translate(
          offset: Offset(0, 8 * (1 - _fade.value)),
          child: child,
        ),
      ),
      child: widget.child,
    );
  }
}

// ============================================================
// 按钮（.btn 系列）
// ============================================================

enum VsBtnTone { plain, primary, gold, danger, ghost, dangerGhost }

/// .btn：7px 14px 内边距 / rSm / 13px / hover 变色 / active 下沉 1px。
class VsButton extends StatefulWidget {
  const VsButton({
    required this.label,
    super.key,
    this.icon,
    this.tone = VsBtnTone.plain,
    this.small = false,
    this.block = false,
    this.onPressed,
  });

  final String label;
  final String? icon;
  final VsBtnTone tone;
  final bool small;
  final bool block;
  final VoidCallback? onPressed;

  @override
  State<VsButton> createState() => _VsButtonState();
}

class _VsButtonState extends State<VsButton> {
  bool _hover = false;
  bool _down = false;
  bool _focused = false;

  void _activate() => widget.onPressed?.call();

  /// 键盘可达（P4-7 无障碍）：Tab 聚焦、Enter/Space 激活，聚焦态与 hover 同款高亮。
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
    final enabled = widget.onPressed != null;
    final hot = (_hover || _focused) && enabled;

    Color bg = v.surfaceAlt;
    Color fg = v.text;
    Color bc = v.border;
    switch (widget.tone) {
      case VsBtnTone.plain:
      case VsBtnTone.ghost:
        if (widget.tone == VsBtnTone.ghost) bg = Colors.transparent;
        if (hot) bg = v.surfaceAlt;
      case VsBtnTone.primary:
        bg = v.blue;
        bc = v.blue;
        fg = Colors.white;
        if (hot) fg = Colors.white;
      case VsBtnTone.gold:
        bg = v.gold;
        bc = v.gold;
        fg = const Color(0xFF17130A);
      case VsBtnTone.danger:
        bg = v.danger;
        bc = v.danger;
        fg = Colors.white;
      case VsBtnTone.dangerGhost:
        bg = Colors.transparent;
        bc = v.danger;
        fg = v.danger;
        if (hot) bg = v.dangerSoft;
    }

    final pad = widget.small
        ? const EdgeInsets.symmetric(horizontal: 10, vertical: 4)
        : const EdgeInsets.symmetric(horizontal: 14, vertical: 7);

    final btn = MouseRegion(
      cursor: enabled ? SystemMouseCursors.click : MouseCursor.defer,
      onEnter: (_) => setState(() => _hover = true),
      onExit: (_) => setState(() {
        _hover = false;
        _down = false;
      }),
      child: GestureDetector(
        onTapDown: (_) => setState(() => _down = true),
        onTapCancel: () => setState(() => _down = false),
        onTapUp: (_) => setState(() => _down = false),
        onTap: _activate,
        child: AnimatedContainer(
          duration: VsMotion.ms(context, 150),
          transform: Matrix4.translationValues(0, _down && enabled ? 1 : 0, 0),
          padding: pad,
          decoration: BoxDecoration(
            color: bg,
            borderRadius: BorderRadius.circular(DesignTokens.rSm),
            border: Border.all(color: bc),
          ),
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              if (widget.icon != null) ...[
                AppIcon(widget.icon!, size: widget.small ? 14 : 16, color: fg),
                const SizedBox(width: 6),
              ],
              Flexible(
                child: Text(
                  widget.label,
                  style: TextStyle(
                    fontSize: widget.small ? 12.5 : 13,
                    fontWeight: FontWeight.w500,
                    color: fg,
                  ),
                  overflow: TextOverflow.ellipsis,
                ),
              ),
            ],
          ),
        ),
      ),
    );

    final focusable = Focus(
      canRequestFocus: enabled,
      onKeyEvent: _onKey,
      onFocusChange: (f) => setState(() => _focused = f),
      child: Semantics(
        button: true,
        enabled: enabled,
        label: widget.label,
        child: btn,
      ),
    );

    return widget.block
        ? SizedBox(width: double.infinity, child: focusable)
        : focusable;
  }
}

/// .iconbtn：34×34 圆角图标按钮。
class VsIconButton extends StatefulWidget {
  const VsIconButton(
      {required this.icon,
      super.key,
      this.onPressed,
      this.tooltip,
      this.size = 18});
  final String icon;
  final VoidCallback? onPressed;
  final String? tooltip;
  final double size;

  @override
  State<VsIconButton> createState() => _VsIconButtonState();
}

class _VsIconButtonState extends State<VsIconButton> {
  bool _hover = false;
  bool _focused = false;

  /// 键盘可达（P4-7 无障碍）：Enter / Space 触发。
  KeyEventResult _onKey(FocusNode node, KeyEvent event) {
    if (event is! KeyDownEvent) return KeyEventResult.ignored;
    final k = event.logicalKey;
    if (k == LogicalKeyboardKey.enter ||
        k == LogicalKeyboardKey.space ||
        k == LogicalKeyboardKey.numpadEnter) {
      widget.onPressed?.call();
      return KeyEventResult.handled;
    }
    return KeyEventResult.ignored;
  }

  @override
  Widget build(BuildContext context) {
    final v = context.vs;
    final hot = _hover || _focused;
    final btn = MouseRegion(
      cursor: SystemMouseCursors.click,
      onEnter: (_) => setState(() => _hover = true),
      onExit: (_) => setState(() => _hover = false),
      child: AnimatedContainer(
        duration: VsMotion.ms(context, 150),
        width: 34,
        height: 34,
        decoration: BoxDecoration(
          color: hot ? v.surfaceAlt : Colors.transparent,
          borderRadius: BorderRadius.circular(DesignTokens.rSm),
        ),
        child: Center(
          child: AppIcon(widget.icon,
              size: widget.size, color: hot ? v.text : v.text2),
        ),
      ),
    );
    // 无 tooltip 时用图标名兜底，保证屏幕阅读器仍有标签。
    final label = widget.tooltip ?? widget.icon;
    final w = Focus(
      canRequestFocus: widget.onPressed != null,
      onKeyEvent: _onKey,
      onFocusChange: (f) => setState(() => _focused = f),
      child: Semantics(
        button: true,
        enabled: widget.onPressed != null,
        label: label,
        child: GestureDetector(onTap: widget.onPressed, child: btn),
      ),
    );
    return widget.tooltip == null
        ? w
        : Tooltip(message: widget.tooltip!, child: w);
  }
}

// ============================================================
// 徽章 / 标签（.badge / .tagchip / .chip）
// ============================================================

enum VsTone { ok, warn, danger, enc, info, gold, mute }

Color vsToneColor(VsScheme v, VsTone t) => switch (t) {
      VsTone.ok => v.ok,
      VsTone.warn => v.warn,
      VsTone.danger => v.danger,
      VsTone.enc => v.enc,
      VsTone.info => v.blue,
      VsTone.gold => v.gold,
      VsTone.mute => v.text3,
    };

Color vsToneSoft(VsScheme v, VsTone t) => switch (t) {
      VsTone.ok => v.okSoft,
      VsTone.warn => v.warnSoft,
      VsTone.danger => v.dangerSoft,
      VsTone.enc => v.encSoft,
      VsTone.info => v.blueSoft,
      VsTone.gold => v.goldSoft,
      VsTone.mute => v.surfaceAlt,
    };

/// .badge：圆点前缀胶囊；plain 去圆点。
class VsBadge extends StatelessWidget {
  const VsBadge(this.text,
      {super.key, this.tone = VsTone.mute, this.plain = false});
  final String text;
  final VsTone tone;
  final bool plain;

  @override
  Widget build(BuildContext context) {
    final v = context.vs;
    final c = vsToneColor(v, tone);
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 2),
      decoration: BoxDecoration(
        color: vsToneSoft(v, tone),
        borderRadius: BorderRadius.circular(20),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          if (!plain) ...[
            Container(
              width: 5,
              height: 5,
              decoration: BoxDecoration(color: c, shape: BoxShape.circle),
            ),
            const SizedBox(width: 4),
          ],
          Text(text,
              style: TextStyle(
                  fontSize: 11.5, fontWeight: FontWeight.w500, color: c)),
        ],
      ),
    );
  }
}

/// .tagchip：enc 色小胶囊标签。
class VsTagChip extends StatelessWidget {
  const VsTagChip(this.text,
      {super.key, this.onTap, this.dim = false, this.trailing});
  final String text;
  final VoidCallback? onTap;
  final bool dim;
  final String? trailing;

  @override
  Widget build(BuildContext context) {
    final v = context.vs;
    return GestureDetector(
      onTap: onTap,
      child: Opacity(
        opacity: dim ? 0.4 : 1,
        child: Container(
          padding: const EdgeInsets.symmetric(horizontal: 9, vertical: 2),
          decoration: BoxDecoration(
            color: v.encSoft,
            borderRadius: BorderRadius.circular(20),
          ),
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              Text(text,
                  style: TextStyle(fontSize: 11.5, color: v.enc, height: 1.6)),
              if (trailing != null) ...[
                const SizedBox(width: 4),
                Text(trailing!,
                    style:
                        TextStyle(fontSize: 11.5, color: v.enc, height: 1.6)),
              ],
            ],
          ),
        ),
      ),
    );
  }
}

/// .chip：审计过滤器等可选中圆角胶囊。
class VsChip extends StatelessWidget {
  const VsChip(this.text, {super.key, this.on = false, this.onTap});
  final String text;
  final bool on;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final v = context.vs;
    return GestureDetector(
      onTap: onTap,
      child: MouseRegion(
        cursor: SystemMouseCursors.click,
        child: Container(
          padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 4),
          decoration: BoxDecoration(
            color: on ? v.goldSoft : v.surface,
            borderRadius: BorderRadius.circular(18),
            border: Border.all(color: on ? Colors.transparent : v.border),
          ),
          child: Text(
            text,
            style: TextStyle(
              fontSize: 12,
              color: on ? v.gold : v.text2,
              fontWeight: on ? FontWeight.w600 : FontWeight.w400,
            ),
          ),
        ),
      ),
    );
  }
}

// ============================================================
// 卡片（.card / .card-h / .card-b）
// ============================================================

class VsCard extends StatelessWidget {
  const VsCard({
    super.key,
    this.header,
    this.headerExtra,
    this.child,
    this.padding,
    this.borderColor,
    this.gradient,
  });

  final String? header;
  final Widget? headerExtra;
  final Widget? child;
  final EdgeInsetsGeometry? padding;
  final Color? borderColor;
  final Gradient? gradient;

  @override
  Widget build(BuildContext context) {
    final v = context.vs;
    return Container(
      decoration: BoxDecoration(
        color: v.surface,
        borderRadius: BorderRadius.circular(DesignTokens.rMd),
        border: Border.all(color: borderColor ?? v.borderSoft),
        gradient: gradient,
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        mainAxisSize: MainAxisSize.min,
        children: [
          if (header != null)
            Container(
              padding: const EdgeInsets.symmetric(
                  horizontal: DesignTokens.sp4, vertical: DesignTokens.sp3),
              decoration: BoxDecoration(
                  border: Border(bottom: BorderSide(color: v.borderSoft))),
              child: Row(
                children: [
                  Text(header!,
                      style: TextStyle(
                          fontSize: 13.5,
                          fontWeight: FontWeight.w600,
                          color: v.text)),
                  const Spacer(),
                  if (headerExtra != null) headerExtra!,
                ],
              ),
            ),
          if (child != null)
            Padding(
              padding: padding ?? const EdgeInsets.all(DesignTokens.sp4),
              child: child,
            ),
        ],
      ),
    );
  }
}

// ============================================================
// 提示条（.modal-note）
// ============================================================

/// info/warn/danger/enc 四色提示条。
class VsNote extends StatelessWidget {
  const VsNote(this.text, {super.key, this.tone = VsTone.info, this.icon});
  final String text;
  final VsTone tone;
  final String? icon;

  @override
  Widget build(BuildContext context) {
    final v = context.vs;
    final c = vsToneColor(v, tone);
    return Container(
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(
        color: vsToneSoft(v, tone),
        borderRadius: BorderRadius.circular(DesignTokens.rSm),
      ),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          AppIcon(icon ?? _defaultIcon, size: 15, color: c),
          const SizedBox(width: 9),
          Expanded(
            child: Text(text,
                style: TextStyle(fontSize: 12.5, color: c, height: 1.5)),
          ),
        ],
      ),
    );
  }

  String get _defaultIcon => switch (tone) {
        VsTone.info => 'info',
        VsTone.warn => 'warn',
        VsTone.danger => 'oct',
        VsTone.enc => 'shield',
        _ => 'info',
      };
}

// ============================================================
// KV 键值对（.kv）
// ============================================================

class VsKv extends StatelessWidget {
  const VsKv(this.entries, {super.key, this.labelW = 92});
  final List<(String, Widget)> entries;
  final double labelW;

  VsKv.text(List<(String, String)> entries,
      {super.key, this.labelW = 92, bool mono = false})
      : entries = [
          for (final (k, val) in entries)
            (
              k,
              mono
                  ? Text(val,
                      style: const TextStyle(
                          fontSize: 12, fontFamily: DesignTokens.monoFamily))
                  : Text(val, style: const TextStyle(fontSize: 12.5))
            ),
        ];

  @override
  Widget build(BuildContext context) {
    final v = context.vs;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        for (final (k, val) in entries)
          Padding(
            padding: const EdgeInsets.only(bottom: 7),
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                SizedBox(
                  width: labelW,
                  child:
                      Text(k, style: TextStyle(fontSize: 12.5, color: v.text3)),
                ),
                Expanded(child: val),
              ],
            ),
          ),
      ],
    );
  }
}

// ============================================================
// 进度条（.pbar）
// ============================================================

class VsPbar extends StatelessWidget {
  const VsPbar({super.key, this.value = 0, this.tone, this.thick = false});
  final double value; // 0..1
  final VsTone? tone;
  final bool thick;

  @override
  Widget build(BuildContext context) {
    final v = context.vs;
    final color = switch (tone) {
      VsTone.gold => v.gold,
      VsTone.ok => v.ok,
      VsTone.danger => v.danger,
      VsTone.warn => v.warn,
      _ => v.blue,
    };
    return ClipRRect(
      borderRadius: BorderRadius.circular(thick ? 6 : 4),
      child: SizedBox(
        height: thick ? 9 : 5,
        child: Stack(
          children: [
            Container(color: v.surfaceAlt),
            AnimatedFractionallySizedBox(
              duration: VsMotion.ms(context, 300),
              curve: Curves.easeOut,
              widthFactor: value.clamp(0, 1),
              heightFactor: 1,
              child: Container(color: color),
            ),
          ],
        ),
      ),
    );
  }
}

// ============================================================
// 空态（.empty）
// ============================================================

class VsEmpty extends StatelessWidget {
  const VsEmpty({
    required this.icon,
    required this.title,
    required this.sub,
    super.key,
    this.action,
  });
  final String icon;
  final String title;
  final String sub;
  final Widget? action;

  @override
  Widget build(BuildContext context) {
    final v = context.vs;
    return Center(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Container(
            width: 76,
            height: 76,
            decoration: BoxDecoration(
              color: v.surface,
              borderRadius: BorderRadius.circular(22),
              border: Border.all(color: v.borderSoft),
            ),
            child: Center(child: AppIcon(icon, size: 34, color: v.text3)),
          ),
          const SizedBox(height: 18),
          Text(title, style: TextStyle(fontSize: 15.5, color: v.text)),
          const SizedBox(height: 6),
          Text(sub, style: TextStyle(fontSize: 12.5, color: v.text2)),
          if (action != null) ...[const SizedBox(height: 18), action!],
        ],
      ),
    );
  }
}

// ============================================================
// 页头（.page-h）
// ============================================================

class VsPageHeader extends StatelessWidget {
  const VsPageHeader({
    required this.title,
    required this.sub,
    super.key,
    this.actions,
  });
  final String title;
  final String sub;
  final List<Widget>? actions;

  @override
  Widget build(BuildContext context) {
    final v = context.vs;
    return Padding(
      padding: const EdgeInsets.only(bottom: DesignTokens.sp4),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(title,
                    style: TextStyle(
                        fontSize: 21,
                        fontWeight: FontWeight.w600,
                        letterSpacing: 0.01,
                        color: v.text)),
                const SizedBox(height: 3),
                Text(sub, style: TextStyle(fontSize: 12.5, color: v.text2)),
              ],
            ),
          ),
          if (actions != null) ...[
            const SizedBox(width: DesignTokens.sp4),
            Wrap(
              spacing: 8,
              runSpacing: 8,
              crossAxisAlignment: WrapCrossAlignment.center,
              children: actions!,
            ),
          ],
        ],
      ),
    );
  }
}

// ============================================================
// 弹窗（.overlay / .modal，modalin 动画 + backdrop blur）
// ============================================================

/// 打开原型风格弹窗：背景 rgba backdrop + 3px 模糊，
/// 弹窗体 240ms modalin（淡入 + 上移 14px + 缩放 .98）。
Future<T?> showVsModal<T>({
  required BuildContext context,
  required String title,
  String? sub,
  String? icon,
  VsTone micTone = VsTone.info,
  double width = 480,
  required WidgetBuilder body,
  List<Widget> Function(BuildContext ctx)? actions,
  bool dismissible = true,
}) {
  return showGeneralDialog<T>(
    context: context,
    barrierDismissible: dismissible,
    barrierLabel: title,
    barrierColor: Colors.transparent,
    transitionDuration: VsMotion.ms(context, 240),
    pageBuilder: (ctx, _, __) => VsModalScaffold(
      title: title,
      sub: sub,
      icon: icon,
      micTone: micTone,
      width: width,
      actions: actions,
      child: Builder(builder: body),
    ),
    transitionBuilder: (ctx, anim, _, child) {
      final t =
          CurvedAnimation(parent: anim, curve: const Cubic(.22, 1, .3, 1));
      // TextField 等组件要求 Material 祖先；showGeneralDialog 不提供，这里补透明层。
      child = Material(type: MaterialType.transparency, child: child);
      return Stack(
        children: [
          // 背景遮罩独立着色 + 模糊（backdrop-filter: blur(3px)）
          FadeTransition(
            opacity: t,
            child: BackdropFilter(
              filter: ImageFilter.blur(
                  sigmaX: 3 * anim.value, sigmaY: 3 * anim.value),
              child: ColoredBox(
                  color: ctx.vs.backdrop, child: const SizedBox.expand()),
            ),
          ),
          // modalin：淡入 + 上移 14px + scale .98
          FadeTransition(
            opacity: t,
            child: SlideTransition(
              position: Tween(begin: const Offset(0, 14 / 22), end: Offset.zero)
                  .animate(t),
              child: ScaleTransition(
                  scale: Tween(begin: .98, end: 1.0).animate(t), child: child),
            ),
          ),
        ],
      );
    },
  );
}

/// 弹窗骨架：head（mic 图标块 + 标题） / body / foot。
class VsModalScaffold extends StatelessWidget {
  const VsModalScaffold({
    required this.title,
    required this.child,
    super.key,
    this.sub,
    this.icon,
    this.micTone = VsTone.info,
    this.width = 480,
    this.actions,
  });

  final String title;
  final String? sub;
  final String? icon;
  final VsTone micTone;
  final double width;
  final Widget child;
  final List<Widget> Function(BuildContext ctx)? actions;

  @override
  Widget build(BuildContext context) {
    final v = context.vs;
    // actions 在弹窗树内构建：闭包拿到的是弹窗自己的 context，
    // 保证 Navigator.pop 弹掉的是弹窗路由而不是下层的页面路由。
    final footActions = actions?.call(context);
    return PageIn(
      duration: VsMotion.ms(context, 240),
      child: Center(
        child: Container(
          width: width,
          constraints:
              BoxConstraints(maxHeight: MediaQuery.heightOf(context) - 64),
          margin: const EdgeInsets.all(24),
          decoration: BoxDecoration(
            color: v.surface,
            borderRadius: BorderRadius.circular(DesignTokens.rLg),
            border: Border.all(color: v.border),
            boxShadow: v.shadow3,
          ),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Padding(
                padding: const EdgeInsets.fromLTRB(20, 16, 12, 0),
                child: Row(
                  children: [
                    VsMicTile(icon: icon ?? 'info', tone: micTone),
                    const SizedBox(width: 10),
                    Expanded(
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text(title,
                              style: TextStyle(
                                  fontSize: 15.5,
                                  fontWeight: FontWeight.w600,
                                  color: v.text)),
                          if (sub != null)
                            Text(sub!,
                                style: TextStyle(fontSize: 12, color: v.text2)),
                        ],
                      ),
                    ),
                    VsIconButton(
                        icon: 'close',
                        onPressed: () => Navigator.of(context).pop()),
                  ],
                ),
              ),
              Flexible(
                child: SingleChildScrollView(
                  padding:
                      const EdgeInsets.symmetric(vertical: DesignTokens.sp4),
                  child: Padding(
                    padding: const EdgeInsets.symmetric(horizontal: 20),
                    child: child,
                  ),
                ),
              ),
              if (footActions != null && footActions.isNotEmpty)
                Container(
                  padding: const EdgeInsets.fromLTRB(20, 14, 20, 18),
                  decoration: BoxDecoration(
                      border: Border(top: BorderSide(color: v.borderSoft))),
                  child: Row(
                    mainAxisAlignment: MainAxisAlignment.end,
                    children: [
                      for (var i = 0; i < footActions.length; i++) ...[
                        if (i > 0) const SizedBox(width: 8),
                        footActions[i],
                      ],
                    ],
                  ),
                ),
            ],
          ),
        ),
      ),
    );
  }
}

/// .mic：36×36 圆角色块图标。
class VsMicTile extends StatelessWidget {
  const VsMicTile(
      {required this.icon, this.tone = VsTone.info, super.key, this.size = 36});
  final String icon;
  final VsTone tone;
  final double size;

  @override
  Widget build(BuildContext context) {
    final v = context.vs;
    return Container(
      width: size,
      height: size,
      decoration: BoxDecoration(
        color: vsToneSoft(v, tone),
        borderRadius: BorderRadius.circular(10),
      ),
      child: Center(
          child: AppIcon(icon, size: size * 0.5, color: vsToneColor(v, tone))),
    );
  }
}

// ============================================================
// 向导步骤条（.steps）
// ============================================================

class VsSteps extends StatelessWidget {
  const VsSteps(
      {required this.count, required this.current, super.key, this.labels})
      : assert(current >= 0 && current < count);
  final int count;
  final int current;
  final List<String>? labels;

  @override
  Widget build(BuildContext context) {
    final v = context.vs;
    final l = AppLocalizations.of(context);
    return Padding(
      padding: const EdgeInsets.fromLTRB(20, 4, 20, 14),
      child: Row(
        children: [
          for (var i = 0; i < count; i++) ...[
            if (i > 0)
              Expanded(
                child: Container(
                    height: 1,
                    color: v.border,
                    margin: const EdgeInsets.symmetric(horizontal: 6)),
              ),
            Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                Container(
                  width: 21,
                  height: 21,
                  decoration: BoxDecoration(
                    shape: BoxShape.circle,
                    color: i < current
                        ? v.okSoft
                        : i == current
                            ? v.goldSoft
                            : Colors.transparent,
                    border: Border.all(
                        color: i < current
                            ? v.ok
                            : i == current
                                ? v.gold
                                : v.border,
                        width: 1.5),
                  ),
                  child: Center(
                    child: i < current
                        ? AppIcon('check', size: 11, color: v.ok)
                        : Text('${i + 1}',
                            style: TextStyle(
                                fontSize: 11,
                                fontFamily: DesignTokens.monoFamily,
                                color: i == current ? v.gold : v.text3)),
                  ),
                ),
                const SizedBox(width: 7),
                Text(
                  labels?[i] ?? l.vsStepsDefaultLabel(i + 1),
                  style: TextStyle(
                      fontSize: 12,
                      color: i == current ? v.text : v.text3,
                      fontWeight:
                          i == current ? FontWeight.w600 : FontWeight.w400),
                ),
              ],
            ),
          ],
        ],
      ),
    );
  }
}

// ============================================================
// 单选卡片（.radiocard）
// ============================================================

class VsRadioCard<T> extends StatelessWidget {
  const VsRadioCard({
    required this.value,
    required this.groupValue,
    required this.onChanged,
    required this.title,
    this.sub,
    super.key,
  });
  final T value;
  final T? groupValue;
  final ValueChanged<T?> onChanged;
  final String title;
  final String? sub;

  @override
  Widget build(BuildContext context) {
    final v = context.vs;
    final selected = value == groupValue;
    return GestureDetector(
      onTap: () => onChanged(value),
      child: MouseRegion(
        cursor: SystemMouseCursors.click,
        child: AnimatedContainer(
          duration: VsMotion.ms(context, 150),
          margin: const EdgeInsets.only(bottom: 8),
          padding: const EdgeInsets.symmetric(horizontal: 13, vertical: 12),
          decoration: BoxDecoration(
            color: selected ? v.blueSoft : Colors.transparent,
            borderRadius: BorderRadius.circular(DesignTokens.rSm),
            border: Border.all(color: selected ? v.blue : v.border),
          ),
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Container(
                width: 16,
                height: 16,
                margin: const EdgeInsets.only(top: 2),
                decoration: BoxDecoration(
                  shape: BoxShape.circle,
                  border: Border.all(
                      color: selected ? v.blue : v.border, width: 1.5),
                ),
                child: selected
                    ? Center(
                        child: Container(
                            width: 10,
                            height: 10,
                            decoration: BoxDecoration(
                                color: v.blue, shape: BoxShape.circle)),
                      )
                    : null,
              ),
              const SizedBox(width: 10),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(title,
                        style: TextStyle(
                            fontSize: 13,
                            fontWeight: FontWeight.w600,
                            color: v.text)),
                    if (sub != null)
                      Text(sub!,
                          style: TextStyle(fontSize: 12, color: v.text2)),
                  ],
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

// ============================================================
// 设置行（.row-setting）
// ============================================================

class VsSettingRow extends StatelessWidget {
  const VsSettingRow({
    required this.title,
    super.key,
    this.sub,
    this.trailing,
    this.last = false,
  });
  final String title;
  final String? sub;
  final Widget? trailing;
  final bool last;

  @override
  Widget build(BuildContext context) {
    final v = context.vs;
    return Container(
      padding: const EdgeInsets.symmetric(vertical: 11),
      decoration: BoxDecoration(
        border: last ? null : Border(bottom: BorderSide(color: v.borderSoft)),
      ),
      child: Row(
        children: [
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(title, style: TextStyle(fontSize: 13, color: v.text)),
                if (sub != null)
                  Text(sub!, style: TextStyle(fontSize: 12, color: v.text3)),
              ],
            ),
          ),
          if (trailing != null) ...[const SizedBox(width: 12), trailing!],
        ],
      ),
    );
  }
}

/// .switch：38×22 绿色开关（自定义，匹配原型轨道/滑块）。
class VsSwitch extends StatelessWidget {
  const VsSwitch({required this.value, required this.onChanged, super.key});
  final bool value;
  final ValueChanged<bool> onChanged;

  @override
  Widget build(BuildContext context) {
    final v = context.vs;
    return GestureDetector(
      onTap: () => onChanged(!value),
      child: MouseRegion(
        cursor: SystemMouseCursors.click,
        child: AnimatedContainer(
          duration: VsMotion.ms(context, 200),
          width: 38,
          height: 22,
          padding: const EdgeInsets.all(3),
          decoration: BoxDecoration(
            color: value ? v.ok : v.border,
            borderRadius: BorderRadius.circular(20),
          ),
          child: AnimatedAlign(
            duration: VsMotion.ms(context, 200),
            curve: Curves.easeOutCubic,
            alignment: value ? Alignment.centerRight : Alignment.centerLeft,
            child: Container(
              width: 16,
              height: 16,
              decoration: const BoxDecoration(
                color: Colors.white,
                shape: BoxShape.circle,
                boxShadow: [
                  BoxShadow(
                      offset: Offset(0, 1),
                      blurRadius: 3,
                      color: Color(0x4D000000)),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}

// ============================================================
// 右键菜单（.ctxmenu）
// ============================================================

class VsCtxEntry {
  const VsCtxEntry(this.label, this.icon, this.onTap, {this.danger = false});
  const VsCtxEntry.sep()
      : label = '',
        icon = null,
        onTap = null,
        danger = false;
  final String label;
  final String? icon;
  final VoidCallback? onTap;
  final bool danger;
}

/// 显示原型风格右键菜单（自动防溢出屏幕）。
Future<void> showVsCtxMenu(
  BuildContext context,
  Offset position,
  String? header,
  List<VsCtxEntry> entries,
) {
  final v = context.vs;
  final overlay = Overlay.of(context).context.findRenderObject() as RenderBox;
  return showGeneralDialog<void>(
    context: context,
    barrierDismissible: true,
    barrierLabel: 'ctx',
    barrierColor: Colors.transparent,
    pageBuilder: (ctx, _, __) => const SizedBox.shrink(),
    transitionBuilder: (ctx, anim, _, __) {
      const mw = 210.0;
      final mh = entries.length * 32.0 + 12 + (header != null ? 22.0 : 0);
      final left = (position.dx + mw > overlay.size.width - 8)
          ? overlay.size.width - mw - 8
          : position.dx;
      final top = (position.dy + mh > overlay.size.height - 8)
          ? overlay.size.height - mh - 8
          : position.dy;
      return Stack(
        children: [
          Positioned(
            left: left,
            top: top,
            child: FadeTransition(
              opacity: CurvedAnimation(parent: anim, curve: Curves.easeOut),
              child: SlideTransition(
                position: Tween(begin: const Offset(0, -0.02), end: Offset.zero)
                    .animate(anim),
                child: Material(
                  color: Colors.transparent,
                  child: Container(
                    width: mw,
                    padding: const EdgeInsets.all(5),
                    decoration: BoxDecoration(
                      color: v.surface,
                      borderRadius: BorderRadius.circular(DesignTokens.rMd),
                      border: Border.all(color: v.border),
                      boxShadow: v.shadow3,
                    ),
                    child: Column(
                      mainAxisSize: MainAxisSize.min,
                      crossAxisAlignment: CrossAxisAlignment.stretch,
                      children: [
                        if (header != null)
                          Padding(
                            padding: const EdgeInsets.fromLTRB(10, 6, 10, 4),
                            child: Text(
                              header,
                              style: TextStyle(
                                  fontSize: 11,
                                  fontWeight: FontWeight.w600,
                                  color: v.text3),
                              overflow: TextOverflow.ellipsis,
                            ),
                          ),
                        for (final e in entries)
                          if (e.icon == null && e.label.isEmpty)
                            Container(
                              height: 1,
                              margin: const EdgeInsets.symmetric(
                                  horizontal: 4, vertical: 5),
                              color: v.borderSoft,
                            )
                          else
                            _CtxItem(entry: e),
                      ],
                    ),
                  ),
                ),
              ),
            ),
          ),
        ],
      );
    },
  );
}

class _CtxItem extends StatefulWidget {
  const _CtxItem({required this.entry});
  final VsCtxEntry entry;

  @override
  State<_CtxItem> createState() => _CtxItemState();
}

class _CtxItemState extends State<_CtxItem> {
  bool _hover = false;

  @override
  Widget build(BuildContext context) {
    final v = context.vs;
    final c = widget.entry.danger ? v.danger : v.text;
    return MouseRegion(
      cursor: SystemMouseCursors.click,
      onEnter: (_) => setState(() => _hover = true),
      onExit: (_) => setState(() => _hover = false),
      child: GestureDetector(
        onTap: () {
          Navigator.of(context, rootNavigator: true).pop();
          widget.entry.onTap?.call();
        },
        child: AnimatedContainer(
          duration: VsMotion.ms(context, 120),
          padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 7),
          decoration: BoxDecoration(
            color: _hover
                ? (widget.entry.danger ? v.dangerSoft : v.surfaceAlt)
                : Colors.transparent,
            borderRadius: BorderRadius.circular(7),
          ),
          child: Row(
            children: [
              if (widget.entry.icon != null)
                AppIcon(widget.entry.icon!, size: 15, color: c),
              const SizedBox(width: 9),
              Expanded(
                child: Text(widget.entry.label,
                    style: TextStyle(fontSize: 13, color: c)),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

// ============================================================
// 分段控件（.seg）
// ============================================================

class VsSeg<T> extends StatefulWidget {
  const VsSeg({
    required this.segments,
    required this.selected,
    required this.onChanged,
    super.key,
  });
  final List<(T, String, String)> segments; // (value, label, icon)
  final T selected;
  final ValueChanged<T> onChanged;

  @override
  State<VsSeg<T>> createState() => _VsSegState<T>();
}

class _VsSegState<T> extends State<VsSeg<T>> {
  int? _hover;

  @override
  Widget build(BuildContext context) {
    final v = context.vs;
    return Container(
      padding: const EdgeInsets.all(2),
      decoration: BoxDecoration(
        color: v.surface,
        borderRadius: BorderRadius.circular(DesignTokens.rSm),
        border: Border.all(color: v.border),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          for (var i = 0; i < widget.segments.length; i++)
            (() {
              final (val, label, icon) = widget.segments[i];
              final hovered = _hover == i;
              final active = val == widget.selected;
              return GestureDetector(
                onTap: () => widget.onChanged(val),
                child: MouseRegion(
                  cursor: SystemMouseCursors.click,
                  onEnter: (_) => setState(() => _hover = i),
                  onExit: (_) => setState(() => _hover = null),
                  child: AnimatedContainer(
                    duration: VsMotion.ms(context, 140),
                    padding:
                        const EdgeInsets.symmetric(horizontal: 11, vertical: 5),
                    decoration: BoxDecoration(
                      color: active
                          ? v.surfaceHover
                          : hovered
                              ? v.surfaceAlt
                              : Colors.transparent,
                      borderRadius: BorderRadius.circular(6),
                      boxShadow: active ? v.shadow1 : null,
                    ),
                    child: Row(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        AppIcon(icon,
                            size: 14,
                            color: active
                                ? v.text
                                : hovered
                                    ? v.text
                                    : v.text2),
                        const SizedBox(width: 6),
                        Text(label,
                            style: TextStyle(
                                fontSize: 12.5,
                                fontWeight:
                                    active ? FontWeight.w500 : FontWeight.w400,
                                color: active
                                    ? v.text
                                    : hovered
                                        ? v.text
                                        : v.text2)),
                      ],
                    ),
                  ),
                ),
              );
            })(),
        ],
      ),
    );
  }
}

// ============================================================
// Toast 卡片（.toast，toastin 动画）
// ============================================================

class VsToastCard extends StatelessWidget {
  const VsToastCard({
    required this.grade,
    required this.message,
    super.key,
  });
  final NotifGrade grade;
  final String message;

  @override
  Widget build(BuildContext context) {
    final v = context.vs;
    final (icon, color) = switch (grade) {
      NotifGrade.ok => ('check', v.ok),
      NotifGrade.warning => ('warn', v.warn),
      NotifGrade.danger => ('oct', v.danger),
      NotifGrade.info => ('info', v.blue),
    };
    return Container(
      padding: const EdgeInsets.all(13),
      decoration: BoxDecoration(
        color: v.surface,
        borderRadius: BorderRadius.circular(DesignTokens.rMd),
        border: Border.all(color: v.border),
        boxShadow: v.shadow3,
      ),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          AppIcon(icon, size: 16, color: color),
          const SizedBox(width: 10),
          Expanded(
            child: Text(message,
                style: TextStyle(fontSize: 12.5, color: v.text, height: 1.5)),
          ),
        ],
      ),
    );
  }
}

// ============================================================
// 手风琴（.acc，设置页折叠区）
// ============================================================

class VsAccordion extends StatelessWidget {
  const VsAccordion({
    required this.icon,
    required this.title,
    required this.sub,
    required this.open,
    required this.onToggle,
    required this.child,
    super.key,
  });
  final String icon;
  final String title;
  final String sub;
  final bool open;
  final VoidCallback onToggle;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    final v = context.vs;
    return Container(
      margin: const EdgeInsets.only(bottom: 10),
      decoration: BoxDecoration(
        color: v.surface,
        borderRadius: BorderRadius.circular(DesignTokens.rMd),
        border: Border.all(color: v.borderSoft),
      ),
      child: Column(
        children: [
          MouseRegion(
            cursor: SystemMouseCursors.click,
            child: GestureDetector(
              onTap: onToggle,
              child: Container(
                padding:
                    const EdgeInsets.symmetric(horizontal: 16, vertical: 13),
                decoration: BoxDecoration(
                  color: Colors.transparent,
                  borderRadius: BorderRadius.circular(DesignTokens.rMd),
                ),
                child: Row(
                  children: [
                    Container(
                      width: 32,
                      height: 32,
                      decoration: BoxDecoration(
                        color: v.surfaceAlt,
                        borderRadius: BorderRadius.circular(9),
                      ),
                      child: Center(
                          child: AppIcon(icon, size: 15, color: v.text2)),
                    ),
                    const SizedBox(width: 11),
                    Expanded(
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text(title,
                              style: TextStyle(
                                  fontSize: 13.5,
                                  fontWeight: FontWeight.w600,
                                  color: v.text)),
                          Text(sub,
                              style: TextStyle(fontSize: 12, color: v.text3)),
                        ],
                      ),
                    ),
                    AnimatedRotation(
                      turns: open ? 0.25 : 0,
                      duration: VsMotion.ms(context, 200),
                      child: AppIcon('chev-r', size: 16, color: v.text3),
                    ),
                  ],
                ),
              ),
            ),
          ),
          // 展开动画：尺寸 + 淡入（acc-body pagein .2s）
          AnimatedCrossFade(
            duration: VsMotion.ms(context, 200),
            sizeCurve: Curves.easeOutCubic,
            crossFadeState:
                open ? CrossFadeState.showSecond : CrossFadeState.showFirst,
            firstChild: const SizedBox(width: double.infinity),
            secondChild: Container(
              padding: const EdgeInsets.fromLTRB(59, 4, 16, 16),
              decoration: BoxDecoration(
                  border: Border(top: BorderSide(color: v.borderSoft))),
              child: child,
            ),
          ),
        ],
      ),
    );
  }
}

// ============================================================
// 主题便捷读取
// ============================================================

extension VsSchemeContext on BuildContext {
  VsScheme get vs => Theme.of(this).vs;
}
