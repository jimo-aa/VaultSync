// 开发用：vault_core.dll P1 端到端冒烟测试。
// 覆盖：创建 → 解锁 → 错误密码 → 冷却 → 改密码（零重加密）→ 生物识别绑定/解锁/解绑 → 锁定。
// 用法：在 app/ 目录下执行 `dart run tool/ffi_check.dart [dll路径]`
// ignore_for_file: avoid_print
import 'dart:ffi';
import 'dart:io';

import 'package:ffi/ffi.dart';

typedef HelloC = Int32 Function();
typedef VersionC = Pointer<Utf8> Function();
typedef FreeStringC = Void Function(Pointer<Utf8>);
typedef PathFnC = Int32 Function(Pointer<Utf8>);
typedef CreateC = Int32 Function(Pointer<Utf8>, Pointer<Utf8>, Int32);
typedef UnlockNative = Int32 Function(Pointer<Utf8>, Pointer<Utf8>, Int32,
    Pointer<Pointer<Void>>, Pointer<Uint64>);
typedef UnlockDart = int Function(
    Pointer<Utf8>, Pointer<Utf8>, int, Pointer<Pointer<Void>>, Pointer<Uint64>);
typedef UnlockBioNative = Int32 Function(
    Pointer<Utf8>, Pointer<Pointer<Void>>, Pointer<Uint64>);
typedef UnlockBioDart = int Function(
    Pointer<Utf8>, Pointer<Pointer<Void>>, Pointer<Uint64>);
typedef HandleFnC = Int32 Function(Pointer<Void>);
typedef ChangePwdC = Int32 Function(Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>);
typedef LockC = Void Function(Pointer<Void>);
typedef CooldownC = Int32 Function(Pointer<Utf8>, Pointer<Uint64>);

late int Function() _hello;
late Pointer<Utf8> Function() _version;
late void Function(Pointer<Utf8>) _free;
late int Function(Pointer<Utf8>) _exists;
late int Function(Pointer<Utf8>) _bioBoundF;
late int Function(Pointer<Utf8>, Pointer<Utf8>, int) _create;
late UnlockDart _unlock;
late UnlockBioDart _unlockBio;
late int Function(Pointer<Void>) _bindBio;
late int Function(Pointer<Void>) _unbindBio;
late int Function(Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>) _changePwd;
late void Function(Pointer<Void>) _lock;
late int Function(Pointer<Utf8>, Pointer<Uint64>) _cooldown;

Pointer<Utf8> n(String s) => s.toNativeUtf8();
String str(Pointer<Utf8> p) => p.toDartString();

class UnlockResult {
  UnlockResult(this.status, this.waitMs, this.handle);
  final int status;
  final int waitMs;
  final int handle;
}

typedef Runner = int Function(Pointer<Utf8> path, Pointer<Utf8> pw,
    Pointer<Pointer<Void>> handle, Pointer<Uint64> wait);

UnlockResult runUnlock(Runner fn, String path, String pw) {
  final a = n(path);
  final b = n(pw);
  final h = calloc<Pointer<Void>>();
  final w = calloc<Uint64>();
  // 哨兵：检测 native 是否真的写了句柄
  h.value = Pointer<Void>.fromAddress(0xDEADBEEF);
  try {
    final st = fn(a, b, h, w);
    final addr = h.value.address;
    return UnlockResult(st, w.value, addr);
  } finally {
    calloc.free(a);
    calloc.free(b);
    calloc.free(h);
    calloc.free(w);
  }
}

int fails = 0;

void check(bool cond, String label) {
  print('${cond ? "PASS" : "FAIL"}  $label');
  if (!cond) fails++;
}

void main(List<String> args) {
  final dll = args.isNotEmpty
      ? args.first
      : r'build\windows\x64\runner\Debug\vault_core.dll';
  final lib = DynamicLibrary.open(dll);

  _hello = lib.lookupFunction<HelloC, int Function()>('vault_core_hello');
  _version = lib.lookupFunction<VersionC, Pointer<Utf8> Function()>('vault_core_version');
  _free =
      lib.lookupFunction<FreeStringC, void Function(Pointer<Utf8>)>('vault_core_free_string');
  _exists = lib.lookupFunction<PathFnC, int Function(Pointer<Utf8>)>('vault_core_vault_exists');
  _bioBoundF =
      lib.lookupFunction<PathFnC, int Function(Pointer<Utf8>)>('vault_core_bio_bound');
  _create = lib.lookupFunction<CreateC, int Function(Pointer<Utf8>, Pointer<Utf8>, int)>(
      'vault_core_create_vault');
  _unlock = lib
      .lookupFunction<UnlockNative, UnlockDart>('vault_core_unlock');
  _unlockBio = lib
      .lookupFunction<UnlockBioNative, UnlockBioDart>('vault_core_unlock_bio');
  _bindBio = lib.lookupFunction<HandleFnC, int Function(Pointer<Void>)>('vault_core_bind_bio');
  _unbindBio =
      lib.lookupFunction<HandleFnC, int Function(Pointer<Void>)>('vault_core_unbind_bio');
  _changePwd = lib.lookupFunction<ChangePwdC, int Function(
      Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>)>('vault_core_change_password');
  _lock = lib.lookupFunction<LockC, void Function(Pointer<Void>)>('vault_core_lock');
  _cooldown = lib.lookupFunction<CooldownC, int Function(Pointer<Utf8>, Pointer<Uint64>)>(
      'vault_core_cooldown_remaining_ms');

  check(_hello() == 0, 'hello 自检');
  final v = _version();
  print('INFO  version=${str(v)}');
  _free(v);

  final dir = Directory.systemTemp.createTempSync('vaultsync_p1_');
  final vault = '${dir.path}${Platform.pathSeparator}primary.vsvb';
  final pw = n('pw-1234');
  final vp = n(vault);

  check(_exists(vp) == 0, '保险箱不存在（初始）');
  check(_create(vp, pw, 1) == 0, '创建保险箱（含生物识别绑定）');
  check(_exists(vp) == 1, '保险箱已存在');
  check(_bioBoundF(vp) == 1, '生物识别已绑定');

  // 暴力破解防护：连续 3 次错误 → 30s 冷却
  var r = runUnlock((a, b, h, w) => _unlock(a, b, 0, h, w), vault, 'wrong');
  check(r.status == 1 && r.waitMs == 0, '错误密码 #1 → WRONG_PASSWORD(0)');
  r = runUnlock((a, b, h, w) => _unlock(a, b, 0, h, w), vault, 'wrong');
  check(r.status == 1 && r.waitMs == 0, '错误密码 #2 → WRONG_PASSWORD(0)');
  r = runUnlock((a, b, h, w) => _unlock(a, b, 0, h, w), vault, 'wrong');
  check(r.status == 1 && r.waitMs == 30000, '错误密码 #3 → WRONG_PASSWORD(30000)');
  final cw = calloc<Uint64>();
  _cooldown(vp, cw);
  check(cw.value > 0, '冷却剩余毫秒 > 0');

  // 冷却域按父目录共享，换新目录继续后续流程
  dir.deleteSync(recursive: true);
  final dir2 = Directory.systemTemp.createTempSync('vaultsync_p1b_');
  final vault2 = '${dir2.path}${Platform.pathSeparator}p.vsvb';
  final vp2 = n(vault2);
  final newPw = n('new-pw-9');
  final oldPw = pw;
  final badOld = n('bad-old');
  check(_create(vp2, pw, 1) == 0, '新目录创建保险箱');

  var s = runUnlock((a, b, h, w) => _unlock(a, b, 0, h, w), vault2, 'pw-1234');
  check(s.status == 0 && s.handle != 0, '正确密码解锁 → 会话句柄');

  // 改密码（F-07 零重加密）：旧密码错误被拒；成功后新密码可解锁、旧密码失效
  check(_changePwd(Pointer<Void>.fromAddress(s.handle), badOld, newPw) == 1,
      '改密码：旧密码错误被拒');
  check(_changePwd(Pointer<Void>.fromAddress(s.handle), oldPw, newPw) == 0, '改密码成功');
  _lock(Pointer<Void>.fromAddress(s.handle));
  s = runUnlock((a, b, h, w) => _unlock(a, b, 0, h, w), vault2, 'new-pw-9');
  check(s.status == 0 && s.handle != 0, '新密码解锁');
  _lock(Pointer<Void>.fromAddress(s.handle));
  r = runUnlock((a, b, h, w) => _unlock(a, b, 0, h, w), vault2, 'pw-1234');
  check(r.status == 1, '旧密码已失效');

  // 生物识别（F-03/F-04）：绑定 → 专用接口解锁 → 解绑 → 解锁被拒
  var s2 = runUnlock((a, b, h, w) => _unlock(a, b, 0, h, w), vault2, 'new-pw-9');
  check(_bindBio(Pointer<Void>.fromAddress(s2.handle)) == 0, '绑定生物识别');
  _lock(Pointer<Void>.fromAddress(s2.handle));
  final d0 = runUnlock((a, b, h, w) => _unlockBio(a, h, w), 'C:/nonexistent/x.vsvb', '');
  print('INFO  bio(不存在路径，期望6): status=${d0.status} handle=${d0.handle}');
  final before = runUnlock((a, b, h, w) => _unlockBio(a, h, w), vault2, '');
  print('INFO  bio(绑定后立即): status=${before.status} handle=${before.handle}');
  var bio = runUnlock((a, b, h, w) => _unlockBio(a, h, w), vault2, '');
  print('INFO  unlock_bio status=${bio.status} wait=${bio.waitMs} handle=${bio.handle}');
  check(bio.status == 0 && bio.handle != 0, '生物识别解锁（unlock_bio）');
  _lock(Pointer<Void>.fromAddress(bio.handle));
  s2 = runUnlock((a, b, h, w) => _unlock(a, b, 0, h, w), vault2, 'new-pw-9');
  check(_unbindBio(Pointer<Void>.fromAddress(s2.handle)) == 0, '解绑生物识别');
  check(_bioBoundF(vp2) == 0, '解绑后 bio 副本已移除');
  _lock(Pointer<Void>.fromAddress(s2.handle));

  dir2.deleteSync(recursive: true);
  calloc.free(pw);
  calloc.free(newPw);
  calloc.free(badOld);
  calloc.free(vp);
  calloc.free(vp2);
  calloc.free(cw);
  print('DONE  failures=$fails');
  exitCode = fails == 0 ? 0 : 1;
}
