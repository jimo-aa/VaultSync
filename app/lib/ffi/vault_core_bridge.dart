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

  // ==== P2 保险箱操作（JSON 结果为引擎分配的字符串，由桥接层释放）====

  int vaultMkdir(Object sessionHandle, int parent, String name, List<int> idOut);

  String? vaultList(Object sessionHandle, int folder);

  String? vaultSearch(Object sessionHandle, String query);

  int vaultImport(Object sessionHandle, String src, int folder, List<int> idOut);

  int vaultExport(Object sessionHandle, int fileId, String dest);

  int vaultRenameFile(Object sessionHandle, int fileId, String name);

  int vaultDeleteFile(Object sessionHandle, int fileId, {bool secure = true});

  int vaultSetTags(Object sessionHandle, int fileId, List<String> tags);

  String? vaultShareCreate(Object sessionHandle, int fileId, int ttlSecs, int maxOpens);

  int vaultShareOpen(Object sessionHandle, int shareId, String token, String dest);

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
// P2 保险箱操作
typedef _MkdirC = Int32 Function(
    Pointer<Void>, Int64, Pointer<Utf8>, Pointer<Uint64>);
typedef _ListC = Pointer<Utf8> Function(Pointer<Void>, Int64);
typedef _SearchC = Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>);
typedef _ImportC = Int32 Function(
    Pointer<Void>, Pointer<Utf8>, Int64, Pointer<Uint64>);
typedef _ExportC = Int32 Function(Pointer<Void>, Int64, Pointer<Utf8>);
typedef _RenameC = Int32 Function(Pointer<Void>, Int64, Pointer<Utf8>);
typedef _DeleteFileC = Int32 Function(Pointer<Void>, Int64, Int32);
typedef _TagsC = Int32 Function(Pointer<Void>, Int64, Pointer<Utf8>);
typedef _ShareCreateC = Pointer<Utf8> Function(
    Pointer<Void>, Int64, Uint64, Uint32);
typedef _ShareOpenC = Int32 Function(
    Pointer<Void>, Int64, Pointer<Utf8>, Pointer<Utf8>);

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
    _vaultMkdir = _lib.lookupFunction<_MkdirC, int Function(
        Pointer<Void>, int, Pointer<Utf8>, Pointer<Uint64>)>('vault_core_vault_mkdir');
    _vaultList = _lib
        .lookupFunction<_ListC, Pointer<Utf8> Function(Pointer<Void>, int)>(
            'vault_core_vault_list');
    _vaultSearch = _lib.lookupFunction<_SearchC,
        Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>)>('vault_core_vault_search');
    _vaultImport = _lib.lookupFunction<_ImportC, int Function(
        Pointer<Void>, Pointer<Utf8>, int, Pointer<Uint64>)>('vault_core_vault_import');
    _vaultExport = _lib.lookupFunction<_ExportC, int Function(
        Pointer<Void>, int, Pointer<Utf8>)>('vault_core_vault_export');
    _vaultRenameFile = _lib.lookupFunction<_RenameC, int Function(
        Pointer<Void>, int, Pointer<Utf8>)>('vault_core_vault_rename_file');
    _vaultDeleteFile = _lib.lookupFunction<_DeleteFileC,
        int Function(Pointer<Void>, int, int)>('vault_core_vault_delete_file');
    _vaultSetTags = _lib.lookupFunction<_TagsC, int Function(
        Pointer<Void>, int, Pointer<Utf8>)>('vault_core_vault_set_tags');
    _vaultShareCreate = _lib.lookupFunction<_ShareCreateC,
        Pointer<Utf8> Function(Pointer<Void>, int, int, int)>('vault_core_vault_share_create');
    _vaultShareOpen = _lib.lookupFunction<_ShareOpenC, int Function(
        Pointer<Void>, int, Pointer<Utf8>, Pointer<Utf8>)>('vault_core_vault_share_open');
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
  late final int Function(Pointer<Void>, int, Pointer<Utf8>, Pointer<Uint64>) _vaultMkdir;
  late final Pointer<Utf8> Function(Pointer<Void>, int) _vaultList;
  late final Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>) _vaultSearch;
  late final int Function(Pointer<Void>, Pointer<Utf8>, int, Pointer<Uint64>) _vaultImport;
  late final int Function(Pointer<Void>, int, Pointer<Utf8>) _vaultExport;
  late final int Function(Pointer<Void>, int, Pointer<Utf8>) _vaultRenameFile;
  late final int Function(Pointer<Void>, int, int) _vaultDeleteFile;
  late final int Function(Pointer<Void>, int, Pointer<Utf8>) _vaultSetTags;
  late final Pointer<Utf8> Function(Pointer<Void>, int, int, int) _vaultShareCreate;
  late final int Function(Pointer<Void>, int, Pointer<Utf8>, Pointer<Utf8>) _vaultShareOpen;

  Pointer<Utf8> _toNative(String s) => s.toNativeUtf8();

  /// 读取并释放引擎返回的 JSON 字符串；空指针 → null。
  String? _takeJson(Pointer<Utf8> ptr) {
    if (ptr.address == 0) return null;
    try {
      return ptr.toDartString();
    } finally {
      _freeString(ptr);
    }
  }

  Pointer<Void> _handle(Object sessionHandle) =>
      Pointer<Void>.fromAddress(sessionHandle as int);

  /// 尝试加载原生引擎；失败返回 null（调用方回退 Stub）。
  static VaultCoreBridgeFfi? tryOpen() {
    try {
      return VaultCoreBridgeFfi._(DynamicLibrary.open(VaultCoreBridge.libraryName));
    } on Error {
      return null;
    }
  }

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

  @override
  int vaultMkdir(Object sessionHandle, int parent, String name, List<int> idOut) {
    final n = _toNative(name);
    final id = calloc<Uint64>();
    try {
      final status = _vaultMkdir(_handle(sessionHandle), parent, n, id);
      if (status == VaultStatus.ok) idOut[0] = id.value;
      return status;
    } finally {
      calloc.free(n);
      calloc.free(id);
    }
  }

  @override
  String? vaultList(Object sessionHandle, int folder) {
    final ptr = _vaultList(_handle(sessionHandle), folder);
    return _takeJson(ptr);
  }

  @override
  String? vaultSearch(Object sessionHandle, String query) {
    final q = _toNative(query);
    try {
      return _takeJson(_vaultSearch(_handle(sessionHandle), q));
    } finally {
      calloc.free(q);
    }
  }

  @override
  int vaultImport(Object sessionHandle, String src, int folder, List<int> idOut) {
    final s = _toNative(src);
    final id = calloc<Uint64>();
    try {
      final status = _vaultImport(_handle(sessionHandle), s, folder, id);
      if (status == VaultStatus.ok) idOut[0] = id.value;
      return status;
    } finally {
      calloc.free(s);
      calloc.free(id);
    }
  }

  @override
  int vaultExport(Object sessionHandle, int fileId, String dest) {
    final d = _toNative(dest);
    try {
      return _vaultExport(_handle(sessionHandle), fileId, d);
    } finally {
      calloc.free(d);
    }
  }

  @override
  int vaultRenameFile(Object sessionHandle, int fileId, String name) {
    final n = _toNative(name);
    try {
      return _vaultRenameFile(_handle(sessionHandle), fileId, n);
    } finally {
      calloc.free(n);
    }
  }

  @override
  int vaultDeleteFile(Object sessionHandle, int fileId, {bool secure = true}) {
    return _vaultDeleteFile(_handle(sessionHandle), fileId, secure ? 1 : 0);
  }

  @override
  int vaultSetTags(Object sessionHandle, int fileId, List<String> tags) {
    final csv = _toNative(tags.join(','));
    try {
      return _vaultSetTags(_handle(sessionHandle), fileId, csv);
    } finally {
      calloc.free(csv);
    }
  }

  @override
  String? vaultShareCreate(Object sessionHandle, int fileId, int ttlSecs, int maxOpens) {
    return _takeJson(_vaultShareCreate(_handle(sessionHandle), fileId, ttlSecs, maxOpens));
  }

  @override
  int vaultShareOpen(Object sessionHandle, int shareId, String token, String dest) {
    final t = _toNative(token);
    final d = _toNative(dest);
    try {
      return _vaultShareOpen(_handle(sessionHandle), shareId, t, d);
    } finally {
      calloc.free(t);
      calloc.free(d);
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

  @override
  int vaultMkdir(Object sessionHandle, int parent, String name, List<int> idOut) =>
      throw UnsupportedError('stub: native not linked');

  @override
  String? vaultList(Object sessionHandle, int folder) =>
      throw UnsupportedError('stub: native not linked');

  @override
  String? vaultSearch(Object sessionHandle, String query) =>
      throw UnsupportedError('stub: native not linked');

  @override
  int vaultImport(Object sessionHandle, String src, int folder, List<int> idOut) =>
      throw UnsupportedError('stub: native not linked');

  @override
  int vaultExport(Object sessionHandle, int fileId, String dest) =>
      throw UnsupportedError('stub: native not linked');

  @override
  int vaultRenameFile(Object sessionHandle, int fileId, String name) =>
      throw UnsupportedError('stub: native not linked');

  @override
  int vaultDeleteFile(Object sessionHandle, int fileId, {bool secure = true}) =>
      throw UnsupportedError('stub: native not linked');

  @override
  int vaultSetTags(Object sessionHandle, int fileId, List<String> tags) =>
      throw UnsupportedError('stub: native not linked');

  @override
  String? vaultShareCreate(Object sessionHandle, int fileId, int ttlSecs, int maxOpens) =>
      throw UnsupportedError('stub: native not linked');

  @override
  int vaultShareOpen(Object sessionHandle, int shareId, String token, String dest) =>
      throw UnsupportedError('stub: native not linked');
}
