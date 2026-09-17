import 'dart:ffi';
import 'dart:io';

import 'package:ffi/ffi.dart';

/// FFI 绑定层：唯一允许触碰原生内存指针的层（docs/01 §二）。
///
/// P0-4 骨架：加载 vault-core 动态库并暴露引擎自检接口。
/// 原生库由 `app/windows/CMakeLists.txt` 在构建时经 cargo 编译并拷贝到产物目录；
/// 加载失败（如未构建原生库、CI 无 cargo）时由上层回退到 Stub 实现。
abstract class VaultCoreBridge {
  /// 引擎自检：true 表示原生库已加载且全部子 crate 可用。
  bool hello();

  /// 引擎版本号（SemVer，与 docs/10 版本规范对应）。
  String coreVersion();

  /// 按平台返回动态库文件名。
  static String get libraryName {
    if (Platform.isWindows) return 'vault_core.dll';
    if (Platform.isMacOS) return 'libvault_core.dylib';
    if (Platform.isLinux) return 'libvault_core.so';
    throw UnsupportedError('VaultSync 核心引擎暂不支持该平台: ${Platform.operatingSystem}');
  }
}

class VaultCoreBridgeFfi implements VaultCoreBridge {
  VaultCoreBridgeFfi._(this._lib);

  final DynamicLibrary _lib;
  late final _hello = _lib
      .lookupFunction<Int32 Function(), int Function()>('vault_core_hello');
  late final _version = _lib.lookupFunction<Pointer<Utf8> Function(),
      Pointer<Utf8> Function()>('vault_core_version');
  late final _freeString = _lib.lookupFunction<Void Function(Pointer<Utf8>),
      void Function(Pointer<Utf8>)>('vault_core_free_string');

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
}

/// 桩实现：原生库尚未链接（如纯 Dart 测试环境）。
class VaultCoreBridgeStub implements VaultCoreBridge {
  @override
  bool hello() => false;

  @override
  String coreVersion() => 'vault-core (stub, native not linked)';
}
