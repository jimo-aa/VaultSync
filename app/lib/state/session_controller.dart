import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../service/engine_service.dart';

/// 会话状态（P1：对接真实引擎；P4-1 补自动落锁）。
sealed class SessionState {
  const SessionState();
}

class SessionLocked extends SessionState {
  const SessionLocked({
    this.failedAttempts = 0,
    this.cooldownUntil,
    this.hint,
  });

  /// 连续失败次数（展示用；延时策略由引擎按父目录域强制执行）。
  final int failedAttempts;

  /// 防护冷却截止时刻；非 null 时禁止解锁尝试。
  final DateTime? cooldownUntil;

  /// 锁屏提示（首次运行 / 错误 / 生物识别状态）。
  final String? hint;

  bool get cooling => cooldownUntil != null && cooldownUntil!.isAfter(DateTime.now());
}

class SessionUnlocked extends SessionState {
  const SessionUnlocked({required this.disguise, required this.sessionHandle});

  /// 伪装空间会话（docs/05-01 F-06）：真实保险箱操作一律拒绝。
  final bool disguise;

  /// 原生会话句柄（MK 不出引擎，Dart 只持有不透明句柄）。
  final Object sessionHandle;
}

class SessionController extends Notifier<SessionState> {
  @override
  SessionState build() {
    // 启动即锁定：不缓存任何会话（docs/05-01）。
    return const SessionLocked();
  }

  VaultEngine get _engine => ref.read(vaultEngineProvider);

  /// 主密码解锁；首次运行时以输入密码创建保险箱并立即解封（强度策略 UI 随 P4-1 落地）。
  Future<void> unlock(String password) async {
    if (state is SessionLocked && (state as SessionLocked).cooling) return;
    if (password.length < 4) {
      state = SessionLocked(
        failedAttempts: _fails(state),
        hint: '密码至少 4 位',
      );
      return;
    }

    if (!await _engine.vaultExists()) {
      try {
        await _engine.ensureCreated(password);
      } catch (e) {
        state = SessionLocked(hint: '创建保险箱失败：$e');
        return;
      }
    }

    final r = await _engine.unlock(password);
    switch (r) {
      case UnlockSuccess(:final disguise, :final sessionHandle):
        state = SessionUnlocked(disguise: disguise, sessionHandle: sessionHandle);
      case UnlockWrongPassword(:final cooldownMs):
        state = SessionLocked(
          failedAttempts: _fails(state) + 1,
          cooldownUntil: cooldownMs > 0
              ? DateTime.now().add(Duration(milliseconds: cooldownMs))
              : null,
          hint: '密码错误',
        );
      case UnlockCooldown(:final remainingMs):
        state = SessionLocked(
          failedAttempts: _fails(state),
          cooldownUntil: DateTime.now().add(Duration(milliseconds: remainingMs)),
          hint: '尝试过于频繁，请等待冷却结束',
        );
      case UnlockFailure(:final message):
        state = SessionLocked(failedAttempts: _fails(state), hint: message);
    }
  }

  /// 生物识别解锁（F-04）。
  Future<void> unlockBio() async {
    final r = await _engine.unlockBio();
    final hint = switch (r) {
      UnlockFailure(:final message) => message,
      UnlockCooldown(:final remainingMs) =>
        '尝试过于频繁，请等待 ${Duration(milliseconds: remainingMs).inSeconds} 秒',
      _ => '生物识别解锁失败',
    };
    final current = state;
    if (r is UnlockSuccess) {
      state = SessionUnlocked(disguise: false, sessionHandle: r.sessionHandle);
    } else if (current is SessionLocked) {
      state = SessionLocked(failedAttempts: current.failedAttempts, hint: hint);
    }
  }

  void lock() {
    final current = state;
    if (current is SessionUnlocked) {
      _engine.lock(current.sessionHandle);
    }
    state = const SessionLocked();
  }

  static int _fails(SessionState s) => s is SessionLocked ? s.failedAttempts : 0;
}

final sessionProvider =
    NotifierProvider<SessionController, SessionState>(SessionController.new);
