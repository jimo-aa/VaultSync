import 'package:flutter/material.dart';
import 'package:window_manager/window_manager.dart';

import '../../core/theme/design_tokens.dart';
import '../../l10n/app_localizations.dart';
import '../widgets/vs_ui.dart';

/// 窗口外框：原生标题栏已隐藏（`TitleBarStyle.hidden`），拖动 / 双击最大化 /
/// 最小化 / 最大化还原 / 关闭全部由应用自绘。排列在顶栏通知铃铛右侧，
/// 对应 Windows 11 原生标题栏按钮的视觉与命中区（46×56）。

/// 窗口控制调用：桌面运行时生效；无插件环境（widget 测试 / 非桌面）静默忽略，
/// 避免 MissingPluginException 变成未捕获异步异常。
void _win(Future<void> Function() op) {
  op().catchError((Object _) {});
}

/// 可拖拽区：按住空白处拖动窗口，双击切换最大化 / 还原。
///
/// 子级若自行处理手势（按钮、输入框），手势竞技场内子级优先胜出，
/// 因此拖动不会干扰其上的交互控件。
class WindowDragArea extends StatelessWidget {
  const WindowDragArea({required this.child, super.key});

  final Widget child;

  @override
  Widget build(BuildContext context) {
    return GestureDetector(
      behavior: HitTestBehavior.translucent,
      onPanStart: (_) => _win(windowManager.startDragging),
      onDoubleTap: _toggleMaximize,
      child: child,
    );
  }
}

void _toggleMaximize() => _win(() async {
      if (await windowManager.isMaximized()) {
        await windowManager.unmaximize();
      } else {
        await windowManager.maximize();
      }
    });

/// 最小化 / 最大化-还原 / 关闭；最大化状态经 [WindowListener] 事件跟踪。
class WindowCaptionButtons extends StatefulWidget {
  const WindowCaptionButtons({this.height = DesignTokens.topbarH, super.key});

  final double height;

  @override
  State<WindowCaptionButtons> createState() => _WindowCaptionButtonsState();
}

class _WindowCaptionButtonsState extends State<WindowCaptionButtons>
    with WindowListener {
  bool _maximized = false;

  @override
  void initState() {
    super.initState();
    windowManager.addListener(this);
    _win(() async {
      final m = await windowManager.isMaximized();
      if (mounted) setState(() => _maximized = m);
    });
  }

  @override
  void dispose() {
    windowManager.removeListener(this);
    super.dispose();
  }

  @override
  void onWindowMaximize() => _setMax(true);

  @override
  void onWindowUnmaximize() => _setMax(false);

  void _setMax(bool v) {
    if (mounted) setState(() => _maximized = v);
  }

  @override
  Widget build(BuildContext context) {
    final l = AppLocalizations.of(context);
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        _CaptionButton(
          kind: _GlyphKind.min,
          height: widget.height,
          tooltip: l.windowChromeMinimize,
          onPressed: () => _win(windowManager.minimize),
        ),
        _CaptionButton(
          kind: _maximized ? _GlyphKind.restore : _GlyphKind.max,
          height: widget.height,
          tooltip: _maximized ? l.windowChromeRestore : l.windowChromeMaximize,
          onPressed: _toggleMaximize,
        ),
        _CaptionButton(
          kind: _GlyphKind.close,
          height: widget.height,
          tooltip: l.windowChromeClose,
          danger: true,
          onPressed: () => _win(windowManager.close),
        ),
      ],
    );
  }
}

enum _GlyphKind { min, max, restore, close }

class _CaptionButton extends StatefulWidget {
  const _CaptionButton({
    required this.kind,
    required this.height,
    required this.tooltip,
    required this.onPressed,
    this.danger = false,
  });

  final _GlyphKind kind;
  final double height;
  final String tooltip;
  final VoidCallback onPressed;
  final bool danger;

  @override
  State<_CaptionButton> createState() => _CaptionButtonState();
}

class _CaptionButtonState extends State<_CaptionButton> {
  bool _hover = false;

  @override
  Widget build(BuildContext context) {
    final v = context.vs;
    // 关闭键 hover 用系统红（#C42B1C）+ 白字形，与 Windows 原生一致。
    final bg = _hover
        ? (widget.danger ? const Color(0xFFC42B1C) : v.surfaceAlt)
        : Colors.transparent;
    final fg = _hover && widget.danger ? Colors.white : v.text2;

    return Tooltip(
      message: widget.tooltip,
      waitDuration: const Duration(milliseconds: 600),
      child: MouseRegion(
        cursor: SystemMouseCursors.click,
        onEnter: (_) => setState(() => _hover = true),
        onExit: (_) => setState(() => _hover = false),
        child: GestureDetector(
          onTap: widget.onPressed,
          child: AnimatedContainer(
            duration: const Duration(milliseconds: 100),
            width: 46,
            height: widget.height,
            color: bg,
            child: Center(
              child: CustomPaint(
                size: const Size(10, 10),
                painter: _GlyphPainter(kind: widget.kind, color: fg),
              ),
            ),
          ),
        ),
      ),
    );
  }
}

/// Windows 11 字形：10×10 命中格，1px 描边。
class _GlyphPainter extends CustomPainter {
  const _GlyphPainter({required this.kind, required this.color});

  final _GlyphKind kind;
  final Color color;

  @override
  void paint(Canvas canvas, Size size) {
    final p = Paint()
      ..color = color
      ..strokeWidth = 1
      ..style = PaintingStyle.stroke
      ..isAntiAlias = true;
    const s = 10.0;
    switch (kind) {
      case _GlyphKind.min:
        canvas.drawLine(const Offset(0, s / 2), const Offset(s, s / 2), p);
      case _GlyphKind.max:
        canvas.drawRect(
            const Rect.fromLTWH(.5, .5, s - 1, s - 1), p); // 内缩半像素防模糊
      case _GlyphKind.restore:
        canvas.drawRect(const Rect.fromLTWH(.5, 2.5, s - 3, s - 3), p);
        canvas.drawPath(
          Path()
            ..moveTo(2.5, 2.5)
            ..lineTo(2.5, .5)
            ..lineTo(s - .5, .5)
            ..lineTo(s - .5, s - 2.5)
            ..lineTo(s - 2.5, s - 2.5),
          p,
        );
      case _GlyphKind.close:
        canvas.drawLine(Offset.zero, const Offset(s, s), p);
        canvas.drawLine(const Offset(s, 0), const Offset(0, s), p);
    }
  }

  @override
  bool shouldRepaint(_GlyphPainter old) =>
      old.kind != kind || old.color != color;
}
