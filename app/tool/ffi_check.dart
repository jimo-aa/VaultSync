// 开发用：验证 vault_core.dll 可加载且 FFI 自检接口可用。
// 用法：在 app/ 目录下执行 `dart run tool/ffi_check.dart <dll路径>`
// ignore_for_file: avoid_print
import 'dart:ffi';
import 'package:ffi/ffi.dart';

void main(List<String> args) {
  final path = args.isNotEmpty ? args.first : r'build\windows\x64\runner\Debug\vault_core.dll';
  final lib = DynamicLibrary.open(path);
  final hello =
      lib.lookupFunction<Int32 Function(), int Function()>('vault_core_hello');
  final version = lib.lookupFunction<Pointer<Utf8> Function(),
      Pointer<Utf8> Function()>('vault_core_version');
  final free = lib.lookupFunction<Void Function(Pointer<Utf8>),
      void Function(Pointer<Utf8>)>('vault_core_free_string');

  final vPtr = version();
  final versionStr = vPtr.toDartString();
  free(vPtr);
  print('hello=${hello() == 0 ? "OK" : "FAIL"} version=$versionStr');
}
