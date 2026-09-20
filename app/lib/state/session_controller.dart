import 'dart:async';

import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../l10n/app_localizations.dart';

import '../core/l10n/vs_l10n.dart';
import '../core/security/password_policy.dart';
import '../service/engine_service.dart';
import 'settings_controller.dart';

/// 会话状态（P1：对接真实引擎；P4-1 补空闲自动落锁与首启强度策略）。
sealed class SessionState {
  const SessionState();
}

class SessionLocked extends SessionState {
  const SessionLocked({
    this.failedAttempts = 0,
    this.cooldownUntil,
    this.hint,
    this.firstRun,
  });

  /// 连续失败次数（展示用；延时策略由引擎按父目录域强制执行）。
  final int failedAttempts;

  /// 防护冷却截止时刻；非 null 时禁止解锁尝试。
  final DateTime? cooldownUntil;

  /// 锁屏提示（首次运行 / 错误 / 生物识别状态）。
  final String? hint;

  /// 是否尚无保险箱（首启）；null = 尚未探测完成。
  /// 为 true 时锁屏展示强度策略，并按 [PasswordPolicy.validate] 校验。
  final bool? firstRun;

  bool get cooling =>
      cooldownUntil != null && cooldownUntil!.isAfter(DateTime.now());

  SessionLocked copyWith({
    int? failedAttempts,
    DateTime? cooldownUntil,
    String? hint,
    bool? firstRun,
    bool clearCooldown = false,
    bool clearHint = false,
  }) =>
      SessionLocked(
        failedAttempts: failedAttempts ?? this.failedAttempts,
        cooldownUntil:
            clearCooldown ? null : (cooldownUntil ?? this.cooldownUntil),
        hint: clearHint ? null : (hint ?? this.hint),
        firstRun: firstRun ?? this.firstRun,
      );
}

class SessionUnlocked extends SessionState {
  const SessionUnlocked({required this.disguise, required this.sessionHandle});

  /// 伪装空间会话（docs/05-01 F-06）：真实保险箱操作一律拒绝。
  final bool disguise;

  /// 原生会话句柄（MK 不出引擎，Dart 只持有不透明句柄）。
  final Object sessionHandle;
}

class SessionController extends Notifier<SessionState> {
  /// 空闲自动落锁定时器（P4-1/P4-5）：每次用户活动后重新计时。
  Timer? _autoLockTimer;

  @override
  SessionState build() {
    ref.onDispose(() => _autoLockTimer?.cancel());
    // 启动即锁定：不缓存任何会话（docs/05-01）。
    return const SessionLocked();
  }

  VaultEngine get _engine => ref.read(vaultEngineProvider);

  /// 当前语言（未注入时下面的兜底中文保证仍可读）。
  AppLocalizations? get _l => VsL10n.orNull;

  static String _fallbackIssue(PasswordIssue issue) => switch (issue) {
        PasswordIssue.tooShort => '主密码至少 ${PasswordPolicy.minLength} 位',
        PasswordIssue.tooWeak => '强度不足：建议混合大小写并加入数字或符号',
      };

  /// 探测是否首启（尚无保险箱）。锁屏据此切换「创建主密码」与「解锁」两种形态。
  Future<void> probeFirstRun() async {
    final exists = await _engine.vaultExists();
    if (!ref.mounted) return;
    final cur = state;
    if (cur is SessionLocked) state = cur.copyWith(firstRun: !exists);
  }

  /// 记录一次用户活动：重置空闲自动落锁倒计时（0 = 关闭自动落锁）。
  ///
  /// 由壳层与锁屏根部的 Listener 在指针/键盘事件上调用。
  void notifyActivity() {
    _autoLockTimer?.cancel();
    _autoLockTimer = null;
    if (state is! SessionUnlocked) return;
    final minutes = ref.read(settingsProvider).autoLockMinutes;
    if (minutes <= 0) return;
    _autoLockTimer = Timer(Duration(minutes: minutes), () {
      if (ref.mounted) lock();
    });
  }

  /// 主密码解锁；首次运行时以输入密码创建保险箱并立即解封。
  ///
  /// 首启走 [PasswordPolicy.validate] 强度门槛（P4-1）：
  /// 不通过时不创建任何东西，也不计入失败次数（避免误触防护冷却）。
  Future<void> unlock(String password) async {
    if (state is SessionLocked && (state as SessionLocked).cooling) return;

    var firstRun =
        state is SessionLocked ? (state as SessionLocked).firstRun : null;
    if (firstRun == null) {
      firstRun = !await _engine.vaultExists();
      if (!ref.mounted) return;
    }

    if (firstRun) {
      final issue = PasswordPolicy.validate(password);
      if (issue != null) {
        final l = _l;
        _fail(l == null ? _fallbackIssue(issue) : issue.messageOf(l),
            count: false);
        return;
      }
      try {
        await _engine.ensureCreated(password);
      } catch (e) {
        _fail(_l?.stateCreateFailed('$e') ?? '创建保险箱失败：$e', count: false);
        return;
      }
    } else if (password.length < 4) {
      _fail(_l?.statePwMin4 ?? '密码至少 4 位', count: false);
      return;
    }

    final r = await _engine.unlock(password);
    if (!ref.mounted) return;
    switch (r) {
      case UnlockSuccess(:final disguise, :final sessionHandle):
        // 伪装入口关闭时即使命中伪装密码也不放行（设置页开关的真实语义）；
        // 对外与「密码错误」不可区分，避免暴露伪装空间的存在。
        if (disguise && !ref.read(settingsProvider).decoyEnabled) {
          _engine.lock(sessionHandle);
          _fail(_l?.statePwWrong ?? '密码错误');
          return;
        }
        state =
            SessionUnlocked(disguise: disguise, sessionHandle: sessionHandle);
        notifyActivity();
      case UnlockWrongPassword(:final cooldownMs):
        _fail(_l?.statePwWrong ?? '密码错误',
            cooldownUntil: cooldownMs > 0
                ? DateTime.now().add(Duration(milliseconds: cooldownMs))
                : null);
      case UnlockCooldown(:final remainingMs):
        // 冷却本是上一次失败的延续，不重复计数。
        _fail(_l?.stateCooldownHint ?? '尝试过于频繁，请等待冷却结束',
            count: false,
            cooldownUntil:
                DateTime.now().add(Duration(milliseconds: remainingMs)));
      case UnlockFailure(:final message):
        _fail(message, count: false);
    }
  }

  /// 生物识别解锁（F-04）。
  Future<void> unlockBio() async {
    final r = await _engine.unlockBio();
    final hint = switch (r) {
      UnlockFailure(:final message) => message,
      UnlockCooldown(:final remainingMs) => _l?.stateCooldownSeconds(
              Duration(milliseconds: remainingMs).inSeconds) ??
          '尝试过于频繁，请等待 ${Duration(milliseconds: remainingMs).inSeconds} 秒',
      _ => _l?.stateBioFailed ?? '生物识别解锁失败',
    };
    if (!ref.mounted) return;
    final current = state;
    if (r is UnlockSuccess) {
      state = SessionUnlocked(disguise: false, sessionHandle: r.sessionHandle);
      notifyActivity();
    } else if (current is SessionLocked) {
      state = current.copyWith(hint: hint);
    }
  }

  void lock() {
    _autoLockTimer?.cancel();
    _autoLockTimer = null;
    final current = state;
    if (current is SessionUnlocked) {
      _engine.lock(current.sessionHandle);
    }
    state = const SessionLocked(firstRun: false);
  }

  /// 失败态迁移：保留 firstRun，并按需累加失败计数。
  void _fail(String hint, {bool count = true, DateTime? cooldownUntil}) {
    final cur = state;
    final prev = cur is SessionLocked ? cur : const SessionLocked();
    state = prev.copyWith(
      failedAttempts: count ? prev.failedAttempts + 1 : prev.failedAttempts,
      cooldownUntil: cooldownUntil,
      clearCooldown: cooldownUntil == null,
      hint: hint,
    );
  }
}

final sessionProvider =
    NotifierProvider<SessionController, SessionState>(SessionController.new);
