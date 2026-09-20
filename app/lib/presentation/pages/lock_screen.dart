import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../core/security/password_policy.dart';
import '../../core/theme/design_tokens.dart';
import '../../l10n/app_localizations.dart';
import '../../state/notification_controller.dart';
import '../../state/session_controller.dart';
import '../shell/window_chrome.dart';
import '../widgets/vs_ui.dart';

/// 锁屏：1:1 还原原型转盘锁（双环旋转 / pw-ok 金色脉冲 / pw-err 抖动），
/// 暗室氛围网格 + 径向渐变背景；主密码解锁（F-02）+ 生物识别（F-04）+ 冷却（P1-5）。
class LockScreen extends ConsumerStatefulWidget {
  const LockScreen({super.key});

  @override
  ConsumerState<LockScreen> createState() => _LockScreenState();
}

class _LockScreenState extends ConsumerState<LockScreen>
    with TickerProviderStateMixin {
  final _controller = TextEditingController();
  bool _obscure = true;
  bool _busy = false;
  bool _ok = false; // pw-ok：金色快转 + 光晕
  bool _err = false; // pw-err：抖动 + 红色
  Timer? _ticker;

  late final AnimationController _shake = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 500),
  );

  @override
  void initState() {
    super.initState();
    _ticker = Timer.periodic(const Duration(milliseconds: 500), (_) {
      if (mounted) setState(() {});
    });
    // 首启探测：决定锁屏是「创建主密码」（带强度策略）还是「解锁」。
    Future.microtask(() {
      if (mounted) ref.read(sessionProvider.notifier).probeFirstRun();
    });
  }

  @override
  void dispose() {
    _ticker?.cancel();
    _shake.dispose();
    _controller.dispose();
    super.dispose();
  }

  Future<void> _unlock() async {
    if (_busy) return;
    setState(() {
      _busy = true;
      _ok = true;
      _err = false;
    });
    await ref.read(sessionProvider.notifier).unlock(_controller.text);
    if (!mounted) return;
    if (ref.read(sessionProvider) is SessionUnlocked) return; // 路由守卫放行
    setState(() {
      _busy = false;
      _ok = false;
      _err = true;
    });
    _shake.forward(from: 0);
  }

  Future<void> _unlockBio() async {
    if (_busy) return;
    setState(() => _busy = true);
    await ref.read(sessionProvider.notifier).unlockBio();
    if (mounted) setState(() => _busy = false);
  }

  /// 忘记主密码：MK 无法由密码反推，唯一路径是已配对设备授权的重置流程（P5）。
  /// 这里给出明确说明，避免成为「点了没反应」的死控件。
  void _explainRecovery() {
    final l = AppLocalizations.of(context);
    ref.read(notificationProvider.notifier).info(l.lockScreenRecoveryInfo);
  }

  @override
  Widget build(BuildContext context) {
    final v = context.vs;
    final l = AppLocalizations.of(context);
    final session = ref.watch(sessionProvider);
    final locked = session is SessionLocked ? session : const SessionLocked();
    // 首启形态：尚无保险箱 → 本次输入即创建，需通过强度门槛。
    final firstRun = locked.firstRun ?? false;

    // 解锁成功：延迟跳转由 _unlock 内控制；此兜底仅覆盖生物识别路径。
    if (session is SessionUnlocked) {
      WidgetsBinding.instance.addPostFrameCallback((_) => context.go('/vault'));
      return Scaffold(
          body: Center(child: AppIcon('logo', size: 48, color: v.gold)));
    }

    final cooling = locked.cooling;
    final remaining = locked.cooldownUntil?.difference(DateTime.now());
    final remainingText = remaining == null
        ? ''
        : '${remaining.inMinutes.toString().padLeft(2, '0')}:'
            '${(remaining.inSeconds % 60).toString().padLeft(2, '0')}';

    final String? errText;
    if (cooling) {
      errText = l.lockScreenCooldown(remainingText);
    } else if (locked.hint != null) {
      errText = locked.failedAttempts > 0
          ? l.lockScreenHintWithAttempts(locked.hint!, locked.failedAttempts)
          : locked.hint!;
    } else if (locked.failedAttempts > 0) {
      errText = l.lockScreenPwWrongAttempts(locked.failedAttempts);
    } else {
      errText = null;
    }

    return Scaffold(
      body: Stack(
        fit: StackFit.expand,
        children: [
          // 背景：径向金/蓝渐变 + 暗室网格
          ColoredBox(color: v.bg),
          const Positioned.fill(child: _LockBackdrop()),
          // 主列
          Center(
            child: ConstrainedBox(
              constraints: const BoxConstraints(maxWidth: 380),
              child: PageIn(
                duration: const Duration(milliseconds: 700),
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    AnimatedBuilder(
                      animation: _shake,
                      builder: (context, child) {
                        final t = _shake.value;
                        // shake @keyframes：20/60% -7px，40/80% +7px
                        final dx = _shake.isAnimating
                            ? (t < .2
                                ? -7 * (t / .2)
                                : t < .4
                                    ? 7 * ((t - .2) / .2)
                                    : t < .6
                                        ? -7 * ((t - .4) / .2)
                                        : t < .8
                                            ? 7 * ((t - .6) / .2)
                                            : -7 * ((t - .8) / .2))
                            : 0.0;
                        return Transform.translate(
                            offset: Offset(dx, 0), child: child);
                      },
                      child: _Dial(ok: _ok, err: _err, busy: _busy),
                    ),
                    const SizedBox(height: 0),
                    Text(l.lockScreenTitle,
                        style: TextStyle(
                            fontSize: 23,
                            fontWeight: FontWeight.w600,
                            letterSpacing: 0.04,
                            color: v.text)),
                    const SizedBox(height: 6),
                    Text(l.lockScreenTagline,
                        style: TextStyle(
                            fontSize: 12.5,
                            letterSpacing: 0.14,
                            color: v.text3)),
                    const SizedBox(height: 30),
                    // 密码输入
                    SizedBox(
                      height: 44,
                      child: TextField(
                        controller: _controller,
                        obscureText: _obscure,
                        autofocus: true,
                        enabled: !cooling && !_busy,
                        onChanged: (_) => setState(() {}),
                        onSubmitted: (_) => _unlock(),
                        textAlign: TextAlign.center,
                        style: TextStyle(
                            fontSize: 17,
                            fontFamily: DesignTokens.monoFamily,
                            letterSpacing: 6,
                            color: v.text),
                        decoration: InputDecoration(
                          hintText: firstRun
                              ? l.lockScreenPwSetHint(PasswordPolicy.minLength)
                              : l.lockScreenPwEnterHint,
                          hintStyle: TextStyle(
                              fontSize: 13,
                              letterSpacing: 0,
                              fontFamily: DesignTokens.fontFamily,
                              color: v.text3),
                          filled: true,
                          fillColor: v.bg,
                          contentPadding: const EdgeInsets.symmetric(
                              horizontal: 40, vertical: 13),
                          suffixIcon: Padding(
                            padding: const EdgeInsets.only(right: 12),
                            child: MouseRegion(
                              cursor: SystemMouseCursors.click,
                              child: GestureDetector(
                                onTap: () =>
                                    setState(() => _obscure = !_obscure),
                                child: Semantics(
                                  button: true,
                                  label: l.lockScreenTogglePwVisibility,
                                  child: AppIcon(_obscure ? 'eye' : 'eye-off',
                                      size: 17, color: v.text3),
                                ),
                              ),
                            ),
                          ),
                          suffixIconConstraints:
                              const BoxConstraints(minWidth: 0, minHeight: 0),
                        ),
                      ),
                    ),
                    const SizedBox(height: 10),
                    // 首启：强度条 + 门槛提示（P4-1）；非首启不占位。
                    if (firstRun) ...[
                      _StrengthMeter(password: _controller.text),
                      const SizedBox(height: 8),
                    ],
                    SizedBox(
                      height: 20,
                      child: Text(
                        errText ?? '',
                        textAlign: TextAlign.center,
                        style: TextStyle(fontSize: 12.5, color: v.danger),
                        overflow: TextOverflow.ellipsis,
                      ),
                    ),
                    const SizedBox(height: 8),
                    Row(
                      children: [
                        Expanded(
                          child: VsButton(
                            label: _busy
                                ? l.lockScreenBusy
                                : firstRun
                                    ? l.lockScreenCreateAndUnlock
                                    : l.lockScreenUnlock,
                            icon: 'unlock',
                            tone: VsBtnTone.primary,
                            onPressed:
                                cooling || _busy ? null : () => _unlock(),
                          ),
                        ),
                        const SizedBox(width: 10),
                        Expanded(
                          child: VsButton(
                            label: l.lockScreenBiometric,
                            icon: 'finger',
                            onPressed:
                                cooling || _busy ? null : () => _unlockBio(),
                          ),
                        ),
                      ],
                    ),
                    const SizedBox(height: 18),
                    Row(
                      mainAxisAlignment: MainAxisAlignment.spaceBetween,
                      children: [
                        MouseRegion(
                          cursor: SystemMouseCursors.click,
                          child: GestureDetector(
                            onTap: _explainRecovery,
                            child: Text(l.lockScreenForgotPassword,
                                style:
                                    TextStyle(fontSize: 12.5, color: v.text3)),
                          ),
                        ),
                        Text(l.lockScreenBindManage,
                            style: TextStyle(fontSize: 12.5, color: v.text3)),
                      ],
                    ),
                    const SizedBox(height: 34),
                    if (firstRun)
                      Container(
                        padding: const EdgeInsets.symmetric(
                            horizontal: 12, vertical: 5),
                        decoration: BoxDecoration(
                          color: v.surface,
                          borderRadius: BorderRadius.circular(20),
                          border: Border.all(
                              color: v.border,
                              strokeAlign: BorderSide.strokeAlignInside),
                        ),
                        child: Text(
                          l.lockScreenFirstRunNote(PasswordPolicy.minLength),
                          textAlign: TextAlign.center,
                          style: TextStyle(fontSize: 11.5, color: v.text3),
                        ),
                      ),
                  ],
                ),
              ),
            ),
          ),
          // 窗口外框：原生标题栏已隐藏，锁屏同样需要可拖动 / 可最小化关闭
          const Positioned(
            top: 0,
            left: 0,
            right: 0,
            child: WindowDragArea(
              child: SizedBox(
                height: DesignTokens.topbarH,
                child: Row(
                  children: [
                    Spacer(),
                    WindowCaptionButtons(),
                  ],
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }
}

// ============================================================
// 首启强度条（对应原型设置页 .strength，锁屏复用同一把尺子）
// ============================================================

class _StrengthMeter extends StatelessWidget {
  const _StrengthMeter({required this.password});

  final String password;

  static const _colors = [
    Color(0xFFE55252), // 弱
    Color(0xFFE55252),
    Color(0xFFF0A63C), // 一般
    Color(0xFFD9B25F), // 较强
    Color(0xFF34C77B), // 强
  ];

  @override
  Widget build(BuildContext context) {
    final v = context.vs;
    final l = AppLocalizations.of(context);
    final s = PasswordPolicy.evaluate(password);
    final ok = PasswordPolicy.validate(password) == null;
    final text = password.isEmpty
        ? l.lockScreenPwSuggestMix
        : ok
            ? l.lockScreenPwStrengthOk(s.label)
            : s.label.isEmpty
                ? l.lockScreenPwTooShort(PasswordPolicy.minLength)
                : l.lockScreenPwStrengthWeak(s.label, PasswordPolicy.minLength);
    return Column(
      children: [
        Row(
          children: [
            for (var i = 0; i < 4; i++) ...[
              if (i > 0) const SizedBox(width: 5),
              Expanded(
                child: AnimatedContainer(
                  duration: const Duration(milliseconds: 250),
                  height: 4,
                  decoration: BoxDecoration(
                    color: i < s.level ? _colors[s.level] : v.surfaceAlt,
                    borderRadius: BorderRadius.circular(3),
                  ),
                ),
              ),
            ],
          ],
        ),
        const SizedBox(height: 6),
        Align(
          alignment: Alignment.centerLeft,
          child: Text(text,
              style: TextStyle(
                  fontSize: 11.5,
                  color: password.isEmpty
                      ? v.text3
                      : ok
                          ? v.ok
                          : v.warn)),
        ),
      ],
    );
  }
}

// ============================================================
// 转盘锁（.dial）
// ============================================================

class _Dial extends StatelessWidget {
  const _Dial({required this.ok, required this.err, required this.busy});
  final bool ok;
  final bool err;
  final bool busy;

  @override
  Widget build(BuildContext context) {
    final v = context.vs;
    final ringColor = ok ? v.gold : v.border;
    final ring2Color = ok ? v.gold : v.text3;
    final coreColor = ok
        ? v.gold
        : err
            ? v.danger
            : v.text3;
    final coreBorder = ok
        ? v.gold
        : err
            ? v.danger
            : v.border;

    return SizedBox(
      width: 148,
      height: 158,
      child: Stack(
        clipBehavior: Clip.none,
        children: [
          // 外环刻度（dialspin 46s / pw-ok 2.2s）
          _TickRing(
            size: 148,
            inset: 0,
            innerRatio: .57,
            outerRatio: .79,
            color: ringColor,
            tickDeg: 2.2,
            gapDeg: 15,
            duration:
                ok ? const Duration(seconds: 2) : const Duration(seconds: 46),
          ),
          // 内环刻度（反向 30s / pw-ok 1.4s）
          _TickRing(
            size: 148,
            inset: 14,
            innerRatio: .60,
            outerRatio: .70,
            color: ring2Color,
            tickDeg: 2,
            gapDeg: 45,
            duration: ok
                ? const Duration(milliseconds: 1400)
                : const Duration(seconds: 30),
            reverse: true,
          ),
          // 顶部金色 notch
          Positioned(
            top: 0,
            left: (148 - 10) / 2,
            child: Container(
              width: 10,
              height: 10,
              decoration: BoxDecoration(
                color: v.gold,
                shape: BoxShape.circle,
                boxShadow: [
                  BoxShadow(
                      color: v.gold.withValues(alpha: .65), blurRadius: 12),
                ],
              ),
            ),
          ),
          // core
          Center(
            child: AnimatedContainer(
              duration: const Duration(milliseconds: 400),
              width: 88,
              height: 88,
              decoration: BoxDecoration(
                shape: BoxShape.circle,
                color: v.surface,
                border: Border.all(color: coreBorder),
                boxShadow: ok
                    ? [
                        BoxShadow(
                            color: v.gold.withValues(alpha: .14),
                            blurRadius: 0,
                            spreadRadius: 5),
                        BoxShadow(
                            color: v.gold.withValues(alpha: .28),
                            blurRadius: 34),
                      ]
                    : err
                        ? [
                            BoxShadow(
                                color: v.dangerSoft,
                                blurRadius: 0,
                                spreadRadius: 5),
                          ]
                        : const [],
              ),
              child: Center(
                child: AppIcon('logo',
                    size: 34, color: ok || err ? coreColor : v.text2),
              ),
            ),
          ),
        ],
      ),
    );
  }
}

/// repeating-conic-gradient 刻度环：绕圆心匀速旋转。
class _TickRing extends StatefulWidget {
  const _TickRing({
    required this.size,
    required this.inset,
    required this.innerRatio,
    required this.outerRatio,
    required this.color,
    required this.tickDeg,
    required this.gapDeg,
    required this.duration,
    this.reverse = false,
  });

  final double size;
  final double inset;
  final double innerRatio;
  final double outerRatio;
  final Color color;
  final double tickDeg;
  final double gapDeg;
  final Duration duration;
  final bool reverse;

  @override
  State<_TickRing> createState() => _TickRingState();
}

class _TickRingState extends State<_TickRing>
    with SingleTickerProviderStateMixin {
  late final AnimationController _c =
      AnimationController(vsync: this, duration: widget.duration, value: 0)
        ..repeat();

  @override
  void didUpdateWidget(_TickRing old) {
    super.didUpdateWidget(old);
    if (old.duration != widget.duration) {
      _c.duration = widget.duration;
    }
  }

  @override
  void dispose() {
    _c.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Positioned(
      left: widget.inset,
      top: widget.inset,
      right: widget.inset,
      bottom: widget.inset,
      child: AnimatedBuilder(
        animation: _c,
        builder: (context, _) => Transform.rotate(
          angle: (widget.reverse ? -1 : 1) * _c.value * 2 * math.pi,
          child: CustomPaint(
            size: Size.infinite,
            painter: _TickPainter(
              color: widget.color,
              innerRatio: widget.innerRatio,
              outerRatio: widget.outerRatio,
              tickDeg: widget.tickDeg,
              gapDeg: widget.gapDeg,
            ),
          ),
        ),
      ),
    );
  }
}

class _TickPainter extends CustomPainter {
  _TickPainter({
    required this.color,
    required this.innerRatio,
    required this.outerRatio,
    required this.tickDeg,
    required this.gapDeg,
  });

  final Color color;
  final double innerRatio;
  final double outerRatio;
  final double tickDeg;
  final double gapDeg;

  @override
  void paint(Canvas canvas, Size size) {
    final center = Offset(size.width / 2, size.height / 2);
    final r = size.width / 2;
    final rIn = r * innerRatio;
    final rOut = r * outerRatio;
    final paint = Paint()
      ..color = color
      ..style = PaintingStyle.stroke
      ..strokeWidth = rOut - rIn;

    final step = tickDeg + gapDeg;
    final path = Path();
    for (var a = 0.0; a < 360; a += step) {
      // 每个刻度为一段圆弧（近似 conic 条带）
      final rect = Rect.fromCircle(center: center, radius: (rIn + rOut) / 2);
      path.addArc(rect, _deg2rad(a), _deg2rad(tickDeg));
    }
    canvas.drawPath(path, paint);
  }

  static double _deg2rad(double d) => d * math.pi / 180;

  @override
  bool shouldRepaint(_TickPainter old) =>
      old.color != color || old.tickDeg != tickDeg || old.gapDeg != gapDeg;
}

// ============================================================
// 暗室氛围背景：径向渐变 + 56px 网格（径向遮罩渐隐）
// ============================================================

class _LockBackdrop extends StatelessWidget {
  const _LockBackdrop();

  @override
  Widget build(BuildContext context) {
    final v = context.vs;
    return IgnorePointer(
      child: CustomPaint(
        painter: _BackdropPainter(v: v),
        size: Size.infinite,
      ),
    );
  }
}

class _BackdropPainter extends CustomPainter {
  _BackdropPainter({required this.v});
  final VsScheme v;

  @override
  void paint(Canvas canvas, Size size) {
    // 顶部金色氛围
    final goldPaint = Paint()
      ..shader = RadialGradient(
        center: const Alignment(0, -1.12),
        radius: 1.4,
        colors: [v.gold.withValues(alpha: .07), v.gold.withValues(alpha: 0)],
      ).createShader(Offset.zero & size);
    canvas.drawRect(Offset.zero & size, goldPaint);

    // 右下蓝色氛围
    final bluePaint = Paint()
      ..shader = RadialGradient(
        center: const Alignment(.82, 1.1),
        radius: 1.3,
        colors: [v.blue.withValues(alpha: .06), v.blue.withValues(alpha: 0)],
      ).createShader(Offset.zero & size);
    canvas.drawRect(Offset.zero & size, bluePaint);

    // 网格 + 径向遮罩（mask-image: radial-gradient 720x560 @ 50% 46%）
    final gridPaint = Paint()
      ..color = v.borderSoft.withValues(alpha: .28)
      ..strokeWidth = 1;
    const cell = 56.0;
    final gridPath = Path();
    for (var x = 0.0; x <= size.width; x += cell) {
      gridPath.moveTo(x, 0);
      gridPath.lineTo(x, size.height);
    }
    for (var y = 0.0; y <= size.height; y += cell) {
      gridPath.moveTo(0, y);
      gridPath.lineTo(size.width, y);
    }
    canvas.saveLayer(
      Offset.zero & size,
      Paint()
        ..shader = RadialGradient(
          center: Alignment(0, -0.08),
          radius: 1.1,
          colors: [Colors.black, Colors.black.withValues(alpha: 0)],
          stops: const [.3, .78],
        ).createShader(Offset.zero & size),
    );
    canvas.drawPath(gridPath, gridPaint);
    canvas.restore();
  }

  @override
  bool shouldRepaint(_BackdropPainter old) => old.v != v;
}
