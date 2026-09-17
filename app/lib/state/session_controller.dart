import 'package:flutter_riverpod/flutter_riverpod.dart';

/// 会话状态（P1 阶段对接真实 FFI 引擎前的骨架桩）。
sealed class SessionState {
  const SessionState();
}

class SessionLocked extends SessionState {
  const SessionLocked({this.failedAttempts = 0, this.cooldownSeconds = 0});

  /// 连续失败次数（达到阈值触发暴力破解防护，见 docs/05-01）。
  final int failedAttempts;
  final int cooldownSeconds;
}

class SessionUnlocked extends SessionState {
  const SessionUnlocked({this.disguiseMode = false});

  /// 伪装密码解锁的伪空间（docs/05-01 伪装入口）。
  final bool disguiseMode;
}

class SessionController extends Notifier<SessionState> {
  @override
  SessionState build() {
    // 骨架阶段默认锁屏；启动后须直接进入锁屏，不缓存任何会话。
    return const SessionLocked();
  }

  /// TODO(P1-2/P1-3): 经 FFI 调用 vault-core 解封 MK。
  bool unlock(String password) {
    // 演示桩：与原型一致的演示口令。
    if (password == '1234') {
      state = const SessionUnlocked();
      return true;
    }
    if (password == '88888888') {
      state = const SessionUnlocked(disguiseMode: true);
      return true;
    }
    state = SessionLocked(
      failedAttempts: (state is SessionLocked
              ? (state as SessionLocked).failedAttempts
              : 0) +
          1,
    );
    return false;
  }

  void lock() => state = const SessionLocked();
}

final sessionProvider =
    NotifierProvider<SessionController, SessionState>(SessionController.new);
