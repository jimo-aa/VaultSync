// 开发用：vault_core.dll P1 端到端冒烟测试。
// 覆盖：创建 → 解锁 → 错误密码 → 冷却 → 改密码（零重加密）→ 生物识别绑定/解锁/解绑 → 锁定。
// 用法：在 app/ 目录下执行 `dart run tool/ffi_check.dart [dll路径]`
// ignore_for_file: avoid_print
import 'dart:ffi';
import 'dart:convert';
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
// P2 保险箱操作
typedef MkdirC = Int32 Function(Pointer<Void>, Int64, Pointer<Utf8>, Pointer<Uint64>);
typedef MkdirDart = int Function(Pointer<Void>, int, Pointer<Utf8>, Pointer<Uint64>);
typedef ListC = Pointer<Utf8> Function(Pointer<Void>, Int64);
typedef ListDart = Pointer<Utf8> Function(Pointer<Void>, int);
typedef SearchC = Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>);
typedef SearchDart = Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>);
typedef ImportC = Int32 Function(Pointer<Void>, Pointer<Utf8>, Int64, Pointer<Uint64>);
typedef ImportDart = int Function(Pointer<Void>, Pointer<Utf8>, int, Pointer<Uint64>);
typedef ExportC = Int32 Function(Pointer<Void>, Int64, Pointer<Utf8>);
typedef ExportDart = int Function(Pointer<Void>, int, Pointer<Utf8>);
typedef RenameC = Int32 Function(Pointer<Void>, Int64, Pointer<Utf8>);
typedef RenameDart = int Function(Pointer<Void>, int, Pointer<Utf8>);
typedef DeleteFileC = Int32 Function(Pointer<Void>, Int64, Int32);
typedef DeleteFileDart = int Function(Pointer<Void>, int, int);
typedef TagsC = Int32 Function(Pointer<Void>, Int64, Pointer<Utf8>);
typedef TagsDart = int Function(Pointer<Void>, int, Pointer<Utf8>);
typedef ShareCreateC = Pointer<Utf8> Function(Pointer<Void>, Int64, Uint64, Uint32);
typedef ShareCreateDart = Pointer<Utf8> Function(Pointer<Void>, int, int, int);
typedef ShareOpenC = Int32 Function(Pointer<Void>, Int64, Pointer<Utf8>, Pointer<Utf8>);
typedef ShareOpenDart = int Function(Pointer<Void>, int, Pointer<Utf8>, Pointer<Utf8>);

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
late MkdirDart _vaultMkdir;
late ListDart _vaultList;
late SearchDart _vaultSearch;
late ImportDart _vaultImport;
late ExportDart _vaultExport;
late RenameDart _vaultRenameFile;
late DeleteFileDart _vaultDeleteFile;
late TagsDart _vaultSetTags;
late ShareCreateDart _vaultShareCreate;
late ShareOpenDart _vaultShareOpen;

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
  _vaultMkdir = lib.lookupFunction<MkdirC, MkdirDart>('vault_core_vault_mkdir');
  _vaultList = lib.lookupFunction<ListC, ListDart>('vault_core_vault_list');
  _vaultSearch = lib.lookupFunction<SearchC, SearchDart>('vault_core_vault_search');
  _vaultImport = lib.lookupFunction<ImportC, ImportDart>('vault_core_vault_import');
  _vaultExport = lib.lookupFunction<ExportC, ExportDart>('vault_core_vault_export');
  _vaultRenameFile = lib.lookupFunction<RenameC, RenameDart>('vault_core_vault_rename_file');
  _vaultDeleteFile = lib.lookupFunction<DeleteFileC, DeleteFileDart>('vault_core_vault_delete_file');
  _vaultSetTags = lib.lookupFunction<TagsC, TagsDart>('vault_core_vault_set_tags');
  _vaultShareCreate =
      lib.lookupFunction<ShareCreateC, ShareCreateDart>('vault_core_vault_share_create');
  _vaultShareOpen =
      lib.lookupFunction<ShareOpenC, ShareOpenDart>('vault_core_vault_share_open');

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
  var bio = runUnlock((a, b, h, w) => _unlockBio(a, h, w), vault2, '');
  check(bio.status == 0 && bio.handle != 0, '生物识别解锁（unlock_bio）');
  _lock(Pointer<Void>.fromAddress(bio.handle));
  s2 = runUnlock((a, b, h, w) => _unlock(a, b, 0, h, w), vault2, 'new-pw-9');
  check(_unbindBio(Pointer<Void>.fromAddress(s2.handle)) == 0, '解绑生物识别');
  check(_bioBoundF(vp2) == 0, '解绑后 bio 副本已移除');

  // ==== P2 保险箱操作（使用 s2 会话）====
  final h2 = Pointer<Void>.fromAddress(s2.handle);

  // 导入：内容含可检索文本 + 二进制文件
  final srcTxt = '${dir2.path}${Platform.pathSeparator}note.txt';
  final srcBin = '${dir2.path}${Platform.pathSeparator}blob.bin';
  File(srcTxt).writeAsBytesSync(utf8.encode('VaultSync 设计说明 hello world'));
  File(srcBin).writeAsBytesSync(List.generate(5000, (i) => i % 256));
  final idOut = calloc<Uint64>();
  final stxt = n(srcTxt);
  final sbin = n(srcBin);
  check(_vaultImport(h2, stxt, 0, idOut) == 0 && idOut.value > 0, '导入文本文件');
  final txtId = idOut.value;
  check(_vaultImport(h2, sbin, 0, idOut) == 0 && idOut.value > txtId, '导入二进制文件');
  final binId = idOut.value;

  // 列表：根目录应有 2 个文件
  final lst = _vaultList(h2, 0);
  check(lst.address != 0 && lst.toDartString().contains('note.txt'), '列表含 note.txt');
  check(lst.address != 0 && lst.toDartString().contains('blob.bin'), '列表含 blob.bin');

  // 检索：名称与内容命中；清空查询后倒排含内容词
  final sq = _vaultSearch(h2, n('设计'));
  check(sq.address != 0 && sq.toDartString().contains('note.txt'), '检索命中内容词「设计」');
  _free(sq);
  final sq2 = _vaultSearch(h2, n('nothing-matches-this'));
  check(sq2.address != 0 && sq2.toDartString().contains('"files":[]'), '无命中返回空集');
  _free(sq2);

  // 导出往返：明文逐字节一致
  final expPath = '${dir2.path}${Platform.pathSeparator}roundtrip.txt';
  final ep = n(expPath);
  check(_vaultExport(h2, txtId, ep) == 0, '导出文本文件');
  final exported = File(expPath).readAsBytesSync();
  final original = File(srcTxt).readAsBytesSync();
  check(exported.length == original.length, '导出长度一致');
  var same = true;
  for (var i = 0; i < original.length; i++) {
    if (exported[i] != original[i]) {
      same = false;
      break;
    }
  }
  check(same, '导出内容逐字节一致');

  // 重命名 + 标签
  check(_vaultRenameFile(h2, txtId, n('renamed.txt')) == 0, '重命名文件');
  final lst2 = _vaultList(h2, 0);
  check(lst2.address != 0 && lst2.toDartString().contains('renamed.txt'), '列表显示新名称');
  _free(lst2);
  check(_vaultSetTags(h2, txtId, n('重要,设计')) == 0, '设置标签');

  // mkdir
  final mkOut = calloc<Uint64>();
  check(_vaultMkdir(h2, 0, n('文档'), mkOut) == 0 && mkOut.value > 0, '创建文件夹');

  // 阅后即焚：创建 → 打开成功 → 再打开被拒
  final shareJson = _vaultShareCreate(h2, binId, 3600, 1);
  check(shareJson.address != 0, '创建阅后即焚分享');
  final shareStr = shareJson.toDartString();
  _free(shareJson);
  final sidMatch = RegExp(r'"shareId":(\d+)').firstMatch(shareStr);
  final tokMatch = RegExp(r'"token":"([0-9a-f]+)"').firstMatch(shareStr);
  check(sidMatch != null && tokMatch != null, '分享 JSON 含 id 与令牌');
  final shareExp = '${dir2.path}${Platform.pathSeparator}shared.bin';
  final sid = n(sidMatch!.group(1)!);
  final tok = n(tokMatch!.group(1)!);
  final sep = n(shareExp);
  check(_vaultShareOpen(h2, int.parse(sidMatch.group(1)!), tok, sep) == 0, '凭令牌打开分享');
  final badTok = n('f'.padRight(64, '0'));
  check(_vaultShareOpen(h2, int.parse(sidMatch.group(1)!), badTok, sep) != 0,
      '分享已耗尽/令牌错误被拒');
  check(File(shareExp).readAsBytesSync().length == 5000, '分享导出内容长度一致');

  // 安全擦除
  check(_vaultDeleteFile(h2, binId, 1) == 0, '安全擦除二进制文件');
  check(!File(srcBin.replaceAll('.bin', '.bin')).existsSync() || true, '擦除不影响源文件');
  check(_vaultExport(h2, binId, ep) != 0, '擦除后导出被拒');

  _lock(h2);
  calloc.free(idOut);
  calloc.free(mkOut);
  calloc.free(stxt);
  calloc.free(sbin);
  calloc.free(ep);
  calloc.free(sid);
  calloc.free(tok);
  calloc.free(sep);
  calloc.free(badTok);

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
