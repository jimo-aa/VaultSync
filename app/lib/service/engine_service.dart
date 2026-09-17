import 'dart:io';
import 'dart:isolate';

import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:path_provider/path_provider.dart';

import '../ffi/vault_core_bridge.dart';

/// 解锁结果（服务层语义，隔离 FFI 状态码细节）。
sealed class UnlockOutcome {
  const UnlockOutcome();
}

class UnlockSuccess extends UnlockOutcome {
  const UnlockSuccess({required this.disguise, required this.sessionHandle});

  final bool disguise;
  final Object sessionHandle;
}

class UnlockWrongPassword extends UnlockOutcome {
  const UnlockWrongPassword({required this.cooldownMs});

  /// 新增冷却毫秒（未达阈值为 0）。
  final int cooldownMs;
}

class UnlockCooldown extends UnlockOutcome {
  const UnlockCooldown(this.remainingMs);

  final int remainingMs;
}

class UnlockFailure extends UnlockOutcome {
  const UnlockFailure(this.message);

  final String message;
}

/// 引擎服务：路径管理 + 解封流程编排。
///
/// Argon2id（桌面 64 MiB）派生约 0.5–1s，原生调用在后台 Isolate 执行，
/// 避免锁死 UI 线程；Stub 桥（测试 / CI）直接同步调用。
class VaultEngine {
  VaultEngine(this._bridge);

  final VaultCoreBridge _bridge;

  static const primaryName = 'primary.vsvb';
  static const disguiseName = 'disguise.vsvb';

  bool get isNative => _bridge is VaultCoreBridgeFfi;

  /// 首次运行：主密码设置即创建保险箱（F-01；强度策略 UI 随 P4-1 落地）。
  Future<void> ensureCreated(String password) async {
    final primary = await primaryPath();
    if (_bridge.vaultExists(primary)) return;
    if (!isNative) throw StateError('原生引擎未加载，无法创建保险箱');
    final err = await Isolate.run(() => _createSync(primary, password));
    if (err != VaultStatus.ok) {
      throw StateError('创建保险箱失败: ${VaultStatus.message(err)}');
    }
  }

  /// 主密码解锁；主空间密码错误时自动尝试伪装空间（F-06，独立 MK 独立文件）。
  Future<UnlockOutcome> unlock(String password) async {
    final primary = await primaryPath();
    final r = await _unlockOn(primary, password, disguise: false);
    if (r is UnlockSuccess) return r;

    if (r is UnlockWrongPassword && _bridge.vaultExists(await disguisePath())) {
      final disguisePath = await this.disguisePath();
      final d = await _unlockOn(disguisePath, password, disguise: true);
      if (d is UnlockSuccess) return d;
    }
    return r;
  }

  Future<UnlockOutcome> unlockBio() async {
    final primary = await primaryPath();
    final r = isNative
        ? await Isolate.run(() => _unlockSync(primary, '', disguise: false, bio: true))
        : _bridge.unlockBio(primary);
    return _map(r, disguise: false);
  }

  Future<bool> vaultExists() async => _bridge.vaultExists(await primaryPath());

  Future<bool> disguiseExists() async => _bridge.vaultExists(await disguisePath());

  Future<bool> bioBound() async {
    final primary = await primaryPath();
    if (isNative) return await Isolate.run(() => _bioBoundSync(primary)) == 1;
    return false;
  }

  Future<int> cooldownRemainingMs() async {
    final primary = await primaryPath();
    if (isNative) return await Isolate.run(() => _cooldownSync(primary));
    return 0;
  }

  /// 修改主密码（F-07）。成功返回 null，失败返回错误消息。
  Future<String?> changePassword(
      Object sessionHandle, String oldPassword, String newPassword) async {
    final err = isNative
        ? await Isolate.run(() => _changeSync(sessionHandle as int, oldPassword, newPassword))
        : _bridge.changePassword(sessionHandle, oldPassword, newPassword);
    return err == VaultStatus.ok ? null : VaultStatus.message(err);
  }

  void lock(Object sessionHandle) {
    if (isNative) _bridge.lock(sessionHandle);
  }

  Future<String> primaryPath() async => _join(await _vaultDir(), primaryName);

  Future<String> disguisePath() async => _join(await _vaultDir(), disguiseName);

  Future<UnlockOutcome> _unlockOn(String path, String password,
      {required bool disguise}) async {
    final r = isNative
        ? await Isolate.run(() => _unlockSync(path, password, disguise: disguise))
        : _bridge.unlock(path, password, disguise: disguise);
    return _map(r, disguise: disguise);
  }

  UnlockOutcome _map(EngineUnlock r, {required bool disguise}) {
    if (r.ok) return UnlockSuccess(disguise: disguise, sessionHandle: r.sessionHandle!);
    return switch (r.status) {
      VaultStatus.wrongPassword => UnlockWrongPassword(cooldownMs: r.waitMs),
      VaultStatus.cooldown => UnlockCooldown(r.waitMs),
      VaultStatus.bioUnavailable => const UnlockFailure('此平台不支持生物识别，请使用主密码'),
      VaultStatus.bioNotBound => const UnlockFailure('尚未绑定生物识别（设置中绑定后可用）'),
      _ => UnlockFailure(VaultStatus.message(r.status)),
    };
  }

  static String _join(String dir, String name) => '$dir${Platform.pathSeparator}$name';

  Future<String> _vaultDir() async {
    final docs = await getApplicationDocumentsDirectory();
    final dir = '${docs.path}${Platform.pathSeparator}VaultSync';
    await Directory(dir).create(recursive: true);
    return dir;
  }
}

// ---- Isolate 侧纯函数（重新打开动态库，禁止捕获桥对象）----

int _createSync(String path, String password) {
  final lib = VaultCoreBridgeFfi.tryOpen();
  if (lib == null) return VaultStatus.internal;
  return lib.createVault(path, password);
}

EngineUnlock _unlockSync(String path, String password,
    {required bool disguise, bool bio = false}) {
  final lib = VaultCoreBridgeFfi.tryOpen();
  if (lib == null) return const EngineUnlock(VaultStatus.internal, 0, null);
  if (bio) return lib.unlockBio(path);
  return lib.unlock(path, password, disguise: disguise);
}

int _bioBoundSync(String path) {
  final lib = VaultCoreBridgeFfi.tryOpen();
  if (lib == null) return 0;
  return lib.bioBound(path) ? 1 : 0;
}

int _cooldownSync(String path) {
  final lib = VaultCoreBridgeFfi.tryOpen();
  if (lib == null) return 0;
  return lib.cooldownRemainingMs(path);
}

int _changeSync(int handle, String oldPassword, String newPassword) {
  final lib = VaultCoreBridgeFfi.tryOpen();
  if (lib == null) return VaultStatus.internal;
  return lib.changePassword(handle, oldPassword, newPassword);
}

/// Provider：优先原生引擎，缺失时回退 Stub（仅纯 Dart 测试 / CI 环境）。
final vaultCoreBridgeProvider =
    Provider<VaultCoreBridge>((ref) => VaultCoreBridgeFfi.tryOpen() ?? VaultCoreBridgeStub());

final vaultEngineProvider =
    Provider<VaultEngine>((ref) => VaultEngine(ref.watch(vaultCoreBridgeProvider)));
