/// FFI 绑定层：唯一允许触碰原生内存指针的层（docs/01 §二）。
///
/// P1 密钥体系绑定：原生库由 `app/windows/runner/CMakeLists.txt` 在构建时
/// 经 cargo 编译并拷贝到产物目录；加载失败时由上层回退 Stub。
library;

import 'dart:ffi';
import 'dart:io';

import 'package:ffi/ffi.dart';

/// vault_core ffi.rs 定义的状态码。
abstract final class VaultStatus {
  static const ok = 0;
  static const wrongPassword = 1;
  static const cooldown = 2;
  static const io = 3;
  static const bioUnavailable = 4;
  static const bioNotBound = 5;
  static const format = 6;
  static const invalidArg = 7;
  static const internal = 8;

  static String message(int code) => switch (code) {
        ok => 'OK',
        wrongPassword => '密码错误',
        cooldown => '防护冷却中',
        io => '保险箱文件读写失败',
        bioUnavailable => '平台安全存储不可用',
        bioNotBound => '未绑定生物识别',
        format => '保险箱格式不支持（引擎过旧或文件损坏）',
        invalidArg => '参数非法',
        internal => '引擎内部错误',
        _ => '未知状态 $code',
      };
}

/// 解锁调用结果。sessionHandle 为原生 Session 指针地址（int 跨 isolate 安全）。
class EngineUnlock {
  const EngineUnlock(this.status, this.waitMs, this.sessionHandle);

  final int status;
  final int waitMs;
  final Object? sessionHandle;

  bool get ok => status == VaultStatus.ok;
}

abstract class VaultCoreBridge {
  /// 引擎自检。
  bool hello();

  /// 引擎版本号（SemVer）。
  String coreVersion();

  bool vaultExists(String path);

  bool bioBound(String path);

  int createVault(String path, String password, {bool bindBio = false});

  EngineUnlock unlock(String path, String password, {required bool disguise});

  EngineUnlock unlockBio(String path);

  int bindBio(Object sessionHandle);

  int unbindBio(Object sessionHandle);

  int changePassword(Object sessionHandle, String oldPassword, String newPassword);

  void lock(Object sessionHandle);

  int cooldownRemainingMs(String path);

  /// 按平台返回动态库文件名。
  static String get libraryName {
    if (Platform.isWindows) return 'vault_core.dll';
    if (Platform.isMacOS) return 'libvault_core.dylib';
    if (Platform.isLinux) return 'libvault_core.so';
    throw UnsupportedError('VaultSync 核心引擎暂不支持该平台: ${Platform.operatingSystem}');
  }
}

typedef _HelloC = Int32 Function();
typedef _VersionC = Pointer<Utf8> Function();
typedef _FreeStringC = Void Function(Pointer<Utf8>);
typedef _PathFnC = Int32 Function(Pointer<Utf8>);
typedef _CreateC = Int32 Function(Pointer<Utf8>, Pointer<Utf8>, Int32);
typedef _UnlockC = Int32 Function(
    Pointer<Utf8>, Pointer<Utf8>, Int32, Pointer<Pointer<Void>>, Pointer<Uint64>);
typedef _UnlockBioC = Int32 Function(
    Pointer<Utf8>, Pointer<Pointer<Void>>, Pointer<Uint64>);
typedef _UnlockBioDart = int Function(
    Pointer<Utf8>, Pointer<Pointer<Void>>, Pointer<Uint64>);
typedef _HandleFnC = Int32 Function(Pointer<Void>);
typedef _ChangePwdC = Int32 Function(Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>);
typedef _LockC = Void Function(Pointer<Void>);
typedef _CooldownC = Int32 Function(Pointer<Utf8>, Pointer<Uint64>);

class VaultCoreBridgeFfi implements VaultCoreBridge {
  VaultCoreBridgeFfi._(this._lib) {
    _hello = _lib.lookupFunction<_HelloC, int Function()>('vault_core_hello');
    final version =
        _lib.lookupFunction<_VersionC, Pointer<Utf8> Function()>('vault_core_version');
    final freeString =
        _lib.lookupFunction<_FreeStringC, void Function(Pointer<Utf8>)>('vault_core_free_string');
    _vaultExists =
        _lib.lookupFunction<_PathFnC, int Function(Pointer<Utf8>)>('vault_core_vault_exists');
    _bioBound =
        _lib.lookupFunction<_PathFnC, int Function(Pointer<Utf8>)>('vault_core_bio_bound');
    _create = _lib.lookupFunction<_CreateC, int Function(
        Pointer<Utf8>, Pointer<Utf8>, int)>('vault_core_create_vault');
    _unlock = _lib.lookupFunction<_UnlockC, int Function(Pointer<Utf8>, Pointer<Utf8>, int,
        Pointer<Pointer<Void>>, Pointer<Uint64>)>('vault_core_unlock');
    _unlockBio = _lib
        .lookupFunction<_UnlockBioC, _UnlockBioDart>('vault_core_unlock_bio');
    _bindBio =
        _lib.lookupFunction<_HandleFnC, int Function(Pointer<Void>)>('vault_core_bind_bio');
    _unbindBio =
        _lib.lookupFunction<_HandleFnC, int Function(Pointer<Void>)>('vault_core_unbind_bio');
    _changePassword = _lib.lookupFunction<_ChangePwdC, int Function(
        Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>)>('vault_core_change_password');
    _lock = _lib.lookupFunction<_LockC, void Function(Pointer<Void>)>('vault_core_lock');
    _cooldown = _lib.lookupFunction<_CooldownC, int Function(
        Pointer<Utf8>, Pointer<Uint64>)>('vault_core_cooldown_remaining_ms');
    _version = version;
    _freeString = freeString;
  }

  final DynamicLibrary _lib;
  late final int Function() _hello;
  late final Pointer<Utf8> Function() _version;
  late final void Function(Pointer<Utf8>) _freeString;
  late final int Function(Pointer<Utf8>) _vaultExists;
  late final int Function(Pointer<Utf8>) _bioBound;
  late final int Function(Pointer<Utf8>, Pointer<Utf8>, int) _create;
  late final int Function(Pointer<Utf8>, Pointer<Utf8>, int, Pointer<Pointer<Void>>,
      Pointer<Uint64>) _unlock;
  late final _UnlockBioDart _unlockBio;
  late final int Function(Pointer<Void>) _bindBio;
  late final int Function(Pointer<Void>) _unbindBio;
  late final int Function(Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>) _changePassword;
  late final void Function(Pointer<Void>) _lock;
  late final int Function(Pointer<Utf8>, Pointer<Uint64>) _cooldown;

  /// 尝试加载原生引擎；失败返回 null（调用方回退 Stub）。
  static VaultCoreBridgeFfi? tryOpen() {
    try {
      return VaultCoreBridgeFfi._(DynamicLibrary.open(VaultCoreBridge.libraryName));
    } on Error {
      return null;
    }
  }

  Pointer<Utf8> _toNative(String s) => s.toNativeUtf8();

  @override
  bool hello() => _hello() == 0;

  @override
  String coreVersion() {
    final ptr = _version();
    try {
      return ptr.toDartString();
    } finally {
      _freeString(ptr);
    }
  }

  @override
  bool vaultExists(String path) {
    final p = _toNative(path);
    try {
      return _vaultExists(p) == 1;
    } finally {
      calloc.free(p);
    }
  }

  @override
  bool bioBound(String path) {
    final p = _toNative(path);
    try {
      return _bioBound(p) == 1;
    } finally {
      calloc.free(p);
    }
  }

  @override
  int createVault(String path, String password, {bool bindBio = false}) {
    final p = _toNative(path);
    final pw = _toNative(password);
    try {
      return _create(p, pw, bindBio ? 1 : 0);
    } finally {
      calloc.free(p);
      calloc.free(pw);
    }
  }

  EngineUnlock _runUnlock(
    int Function(Pointer<Utf8>, Pointer<Utf8>, int, Pointer<Pointer<Void>>, Pointer<Uint64>) fn,
    String path,
    String password,
    bool disguise,
  ) {
    final p = _toNative(path);
    final pw = _toNative(password);
    final handle = calloc<Pointer<Void>>();
    final wait = calloc<Uint64>();
    try {
      final status = fn(p, pw, disguise ? 1 : 0, handle, wait);
      final addr = handle.value.address;
      return EngineUnlock(status, wait.value, addr == 0 ? null : addr);
    } finally {
      calloc.free(p);
      calloc.free(pw);
      calloc.free(handle);
      calloc.free(wait);
    }
  }

  @override
  EngineUnlock unlock(String path, String password, {required bool disguise}) =>
      _runUnlock(_unlock, path, password, disguise);

  @override
  EngineUnlock unlockBio(String path) {
    final p = _toNative(path);
    final handle = calloc<Pointer<Void>>();
    final wait = calloc<Uint64>();
    try {
      final status = _unlockBio(p, handle, wait);
      final addr = handle.value.address;
      return EngineUnlock(status, wait.value, addr == 0 ? null : addr);
    } finally {
      calloc.free(p);
      calloc.free(handle);
      calloc.free(wait);
    }
  }

  @override
  int bindBio(Object sessionHandle) =>
      _bindBio(Pointer<Void>.fromAddress(sessionHandle as int));

  @override
  int unbindBio(Object sessionHandle) =>
      _unbindBio(Pointer<Void>.fromAddress(sessionHandle as int));

  @override
  int changePassword(Object sessionHandle, String oldPassword, String newPassword) {
    final oldPw = _toNative(oldPassword);
    final newPw = _toNative(newPassword);
    try {
      return _changePassword(
          Pointer<Void>.fromAddress(sessionHandle as int), oldPw, newPw);
    } finally {
      calloc.free(oldPw);
      calloc.free(newPw);
    }
  }

  @override
  void lock(Object sessionHandle) =>
      _lock(Pointer<Void>.fromAddress(sessionHandle as int));

  @override
  int cooldownRemainingMs(String path) {
    final p = _toNative(path);
    final wait = calloc<Uint64>();
    try {
      _cooldown(p, wait);
      return wait.value;
    } finally {
      calloc.free(p);
      calloc.free(wait);
    }
  }
}

/// 桩实现：原生库不可用时（纯 Dart 测试 / CI 无 DLL）。
/// 引擎操作一律抛错，会话流程由测试用 Fake 桩接管。
class VaultCoreBridgeStub implements VaultCoreBridge {
  @override
  bool hello() => false;

  @override
  String coreVersion() => 'vault-core (stub, native not linked)';

  @override
  bool vaultExists(String path) => throw UnsupportedError('stub: native not linked');

  @override
  bool bioBound(String path) => throw UnsupportedError('stub: native not linked');

  @override
  int createVault(String path, String password, {bool bindBio = false}) =>
      throw UnsupportedError('stub: native not linked');

  @override
  EngineUnlock unlock(String path, String password, {required bool disguise}) =>
      throw UnsupportedError('stub: native not linked');

  @override
  EngineUnlock unlockBio(String path) => throw UnsupportedError('stub: native not linked');

  @override
  int bindBio(Object sessionHandle) => throw UnsupportedError('stub: native not linked');

  @override
  int unbindBio(Object sessionHandle) => throw UnsupportedError('stub: native not linked');

  @override
  int changePassword(Object sessionHandle, String oldPassword, String newPassword) =>
      throw UnsupportedError('stub: native not linked');

  @override
  void lock(Object sessionHandle) => throw UnsupportedError('stub: native not linked');

  @override
  int cooldownRemainingMs(String path) => throw UnsupportedError('stub: native not linked');
}
