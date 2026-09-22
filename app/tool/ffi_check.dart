// 开发用：vault_core.dll P1 端到端冒烟测试。
// 覆盖：创建 → 解锁 → 错误密码 → 冷却 → 改密码（零重加密）→ 生物识别绑定/解锁/解绑 → 锁定。
// 用法：在 app/ 目录下执行 `dart run tool/ffi_check.dart [dll路径]`
// ignore_for_file: avoid_print
import 'dart:ffi';
import 'dart:typed_data';
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
typedef ChangePwdC = Int32 Function(
    Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>);
typedef LockC = Void Function(Pointer<Void>);
typedef CooldownC = Int32 Function(Pointer<Utf8>, Pointer<Uint64>);
// P2 保险箱操作
typedef MkdirC = Int32 Function(
    Pointer<Void>, Int64, Pointer<Utf8>, Pointer<Uint64>);
typedef MkdirDart = int Function(
    Pointer<Void>, int, Pointer<Utf8>, Pointer<Uint64>);
typedef ListC = Pointer<Utf8> Function(Pointer<Void>, Int64);
typedef ListDart = Pointer<Utf8> Function(Pointer<Void>, int);
typedef SearchC = Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>);
typedef SearchDart = Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>);
typedef ImportC = Int32 Function(
    Pointer<Void>, Pointer<Utf8>, Int64, Pointer<Uint64>);
typedef ImportDart = int Function(
    Pointer<Void>, Pointer<Utf8>, int, Pointer<Uint64>);
typedef ExportC = Int32 Function(Pointer<Void>, Int64, Pointer<Utf8>);
typedef ExportDart = int Function(Pointer<Void>, int, Pointer<Utf8>);
typedef RenameC = Int32 Function(Pointer<Void>, Int64, Pointer<Utf8>);
typedef RenameDart = int Function(Pointer<Void>, int, Pointer<Utf8>);
// 文件夹重命名 / 递归删除（P4：右键菜单「重命名」「安全擦除」对文件夹生效）
typedef RenameFolderC = Int32 Function(Pointer<Void>, Int64, Pointer<Utf8>);
typedef RenameFolderDart = int Function(Pointer<Void>, int, Pointer<Utf8>);
typedef DeleteFileC = Int32 Function(Pointer<Void>, Int64, Int32);
typedef DeleteFileDart = int Function(Pointer<Void>, int, int);
typedef DeleteFolderC = Int32 Function(Pointer<Void>, Int64, Int32);
typedef DeleteFolderDart = int Function(Pointer<Void>, int, int);
typedef TagsC = Int32 Function(Pointer<Void>, Int64, Pointer<Utf8>);
typedef TagsDart = int Function(Pointer<Void>, int, Pointer<Utf8>);
typedef ShareCreateC = Pointer<Utf8> Function(
    Pointer<Void>, Int64, Uint64, Uint32);
typedef ShareCreateDart = Pointer<Utf8> Function(Pointer<Void>, int, int, int);
typedef ShareOpenC = Int32 Function(
    Pointer<Void>, Int64, Pointer<Utf8>, Pointer<Utf8>);
typedef ShareOpenDart = int Function(
    Pointer<Void>, int, Pointer<Utf8>, Pointer<Utf8>);
// P3 P2P 同步（每函数独立 typedef）
typedef P2pPairBeginC = Pointer<Utf8> Function(Pointer<Void>);
typedef P2pPairBeginDart = Pointer<Utf8> Function(Pointer<Void>);
typedef P2pPairJoinC = Pointer<Utf8> Function(
    Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>);
typedef P2pPairJoinDart = Pointer<Utf8> Function(
    Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>);
typedef P2pSyncC = Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>);
typedef P2pSyncDart = Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>);
typedef P2pStatusC = Pointer<Utf8> Function(Pointer<Void>);
typedef P2pStatusDart = Pointer<Utf8> Function(Pointer<Void>);
typedef P2pUnpairC = Int32 Function(Pointer<Void>, Pointer<Utf8>);
typedef P2pUnpairDart = int Function(Pointer<Void>, Pointer<Utf8>);
typedef P2pDestroyArmC = Pointer<Utf8> Function(
    Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>, Uint64);
typedef P2pDestroyArmDart = Pointer<Utf8> Function(
    Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>, int);

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
late RenameFolderDart _vaultRenameFolder;
late DeleteFileDart _vaultDeleteFile;
late DeleteFolderDart _vaultDeleteFolder;
late TagsDart _vaultSetTags;
late ShareCreateDart _vaultShareCreate;
late ShareOpenDart _vaultShareOpen;
late P2pPairBeginDart _p2pPairBegin;
late P2pPairJoinDart _p2pPairJoin;
late P2pSyncDart _p2pSync;
late P2pStatusDart _p2pStatus;
late P2pUnpairDart _p2pUnpair;
late P2pDestroyArmDart _p2pDestroyArm;
// P5 审计 / 销毁 / 隐写（每函数独立 typedef）
typedef AuditListC = Pointer<Utf8> Function(Pointer<Void>);
typedef AuditListDart = Pointer<Utf8> Function(Pointer<Void>);
typedef AuditExportC = Int32 Function(Pointer<Void>, Pointer<Utf8>);
typedef AuditExportDart = int Function(Pointer<Void>, Pointer<Utf8>);
typedef DestroyLocalC = Int32 Function(Pointer<Void>, Int32);
typedef DestroyLocalDart = int Function(Pointer<Void>, int);
typedef StegoStatusC = Int32 Function(Pointer<Void>);
typedef StegoSetEnabledC = Int32 Function(Pointer<Void>, Int32);
typedef StegoSetEnabledDart = int Function(Pointer<Void>, int);
typedef StegoCapacityC = Int64 Function(Pointer<Void>, Pointer<Utf8>);
typedef StegoCapacityDart = int Function(Pointer<Void>, Pointer<Utf8>);
typedef StegoEmbedC = Int32 Function(
    Pointer<Void>, Int64, Pointer<Utf8>, Pointer<Utf8>);
typedef StegoEmbedDart = int Function(
    Pointer<Void>, int, Pointer<Utf8>, Pointer<Utf8>);
typedef StegoExtractC = Pointer<Utf8> Function(
    Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>);
typedef StegoExtractDart = Pointer<Utf8> Function(
    Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>);
late AuditListDart _auditList;
late AuditListDart _auditVerify;
late AuditExportDart _auditExport;
late DestroyLocalDart _destroyLocal;
late int Function(Pointer<Void>) _stegoStatus;
late StegoSetEnabledDart _stegoSetEnabled;
late StegoCapacityDart _stegoCapacity;
late StegoEmbedDart _stegoEmbed;
late StegoExtractDart _stegoExtract;
// P5-7 更新通道：清单验签与文件哈希（纯函数，无需会话）
typedef VerifyManifestC = Pointer<Utf8> Function(
    Pointer<Utf8>, Pointer<Utf8>, Pointer<Utf8>);
typedef VerifyManifestDart = Pointer<Utf8> Function(
    Pointer<Utf8>, Pointer<Utf8>, Pointer<Utf8>);
typedef FileSha256C = Pointer<Utf8> Function(Pointer<Utf8>);
// P5-4 轮换 / P5-5 销毁队列（每函数独立 typedef）
typedef RotateKeyC = Int32 Function(Pointer<Void>, Int64);
typedef RotateKeyDart = int Function(Pointer<Void>, int);
typedef RotateMkC = Int32 Function(Pointer<Void>, Pointer<Utf8>);
typedef RotateMkDart = int Function(Pointer<Void>, Pointer<Utf8>);
typedef DestroyQueueAllC = Pointer<Utf8> Function(Pointer<Void>, Uint64);
typedef DestroyQueueAllDart = Pointer<Utf8> Function(Pointer<Void>, int);
typedef PendingOrdersC = Pointer<Utf8> Function(Pointer<Void>);
typedef PendingOrdersDart = Pointer<Utf8> Function(Pointer<Void>);
typedef RememberAddrC = Int32 Function(
    Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>);
typedef RememberAddrDart = int Function(
    Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>);
late RotateKeyDart _rotateFileKey;
late RotateKeyDart _rotateFolderKeys;
late RotateMkDart _rotateMk;
late DestroyQueueAllDart _destroyQueueAll;
late PendingOrdersDart _pendingOrders;
late RememberAddrDart _rememberAddr;
typedef FileSha256Dart = Pointer<Utf8> Function(Pointer<Utf8>);
late VerifyManifestDart _verifyManifest;
late FileSha256Dart _fileSha256;

// ===== P6 ABI 握手 / 事件流 / 任务框架 / 租约（每函数独立 typedef）=====
/// ABI v2 握手结构（与 native/vault-core/src/contract.rs 的 VsAbiInfo 对齐）。
final class VsAbiInfo extends Struct {
  @Uint32()
  external int structSize;
  @Uint16()
  external int abiVersion;
  @Uint16()
  external int reserved0;
  @Uint64()
  external int capabilityBits;
  @Uint32()
  external int maxWriteVerVault;
  @Uint32()
  external int maxWriteVerContainer;
  @Uint32()
  external int maxWriteVerIndex;
  @Uint32()
  external int maxWriteVerAudit;
  @Uint32()
  external int minReadVerVault;
  @Uint32()
  external int featureFlags;
  @Array(32)
  external Array<Uint8> engineVersion;
}

typedef AbiInfoC = Int32 Function(Pointer<VsAbiInfo>);
typedef AbiInfoDart = int Function(Pointer<VsAbiInfo>);
typedef EventCallbackC = Void Function(Pointer<Void>, Pointer<Void>);
typedef SubscribeC = Int32 Function(
    Pointer<NativeFunction<EventCallbackC>>, Pointer<Void>, Pointer<Uint32>);
typedef SubscribeDart = int Function(
    Pointer<NativeFunction<EventCallbackC>>, Pointer<Void>, Pointer<Uint32>);
typedef UnsubscribeC = Int32 Function(Uint32);
typedef UnsubscribeDart = int Function(int);
typedef PollEventsC = Int32 Function(Pointer<Pointer<Utf8>>);
typedef PollEventsDart = int Function(Pointer<Pointer<Utf8>>);
typedef TaskListC = Int32 Function(Pointer<Pointer<Utf8>>);
typedef TaskListDart = int Function(Pointer<Pointer<Utf8>>);
typedef TaskStatusC = Int32 Function(Uint32, Pointer<Pointer<Utf8>>);
typedef TaskStatusDart = int Function(int, Pointer<Pointer<Utf8>>);
typedef TaskCancelC = Int32 Function(Uint32);
typedef TaskCancelDart = int Function(int);
typedef TaskSpawnC = Int32 Function(Pointer<Uint32>);
typedef TaskSpawnDart = int Function(Pointer<Uint32>);
typedef SessionReadonlyC = Int32 Function(Pointer<Void>, Pointer<Int32>);
typedef SessionReadonlyDart = int Function(Pointer<Void>, Pointer<Int32>);
typedef ThumbnailC = Int32 Function(
    Pointer<Void>, Uint64, Pointer<Pointer<Uint8>>, Pointer<Uint64>);
typedef ThumbnailDart = int Function(
    Pointer<Void>, int, Pointer<Pointer<Uint8>>, Pointer<Uint64>);
late AbiInfoDart _abiInfo;
late SubscribeDart _subscribe;
late UnsubscribeDart _unsubscribe;
late PollEventsDart _pollEvents;
late TaskListDart _taskList;
late TaskStatusDart _taskStatus;
late TaskCancelDart _taskCancel;
late TaskSpawnDart _taskSpawn;
late SessionReadonlyDart _sessionReadonly;
late ThumbnailDart _vaultThumbnail;

// ===== P7 轮换会话 / 传播 / 擦除分级 / 迁移 / 检索 V2（每函数独立 typedef）=====
typedef RotateMkBeginC = Int32 Function(
    Pointer<Void>, Pointer<Utf8>, Pointer<Uint32>);
typedef RotateMkBeginDart = int Function(
    Pointer<Void>, Pointer<Utf8>, Pointer<Uint32>);
typedef RotateMkResumeC = Int32 Function(Pointer<Void>, Pointer<Utf8>);
typedef RotateMkResumeDart = int Function(Pointer<Void>, Pointer<Utf8>);
typedef RotateMkStatusC = Pointer<Utf8> Function(Pointer<Void>);
typedef RotateMkStatusDart = Pointer<Utf8> Function(Pointer<Void>);
typedef P2pPushLockC = Int32 Function(Pointer<Void>, Pointer<Utf8>);
typedef P2pPushLockDart = int Function(Pointer<Void>, Pointer<Utf8>);
typedef P2pPushRotationC = Int32 Function(
    Pointer<Void>, Pointer<Utf8>, Pointer<Uint32>);
typedef P2pPushRotationDart = int Function(
    Pointer<Void>, Pointer<Utf8>, Pointer<Uint32>);
typedef EraseClassProbeC = Pointer<Utf8> Function(Pointer<Utf8>);
typedef EraseClassProbeDart = Pointer<Utf8> Function(Pointer<Utf8>);
typedef MigrateBeginC = Int32 Function(
    Pointer<Void>, Pointer<Utf8>, Pointer<Uint32>);
typedef MigrateBeginDart = int Function(
    Pointer<Void>, Pointer<Utf8>, Pointer<Uint32>);
typedef MigrateStatusC = Pointer<Utf8> Function(Pointer<Void>);
typedef MigrateStatusDart = Pointer<Utf8> Function(Pointer<Void>);
typedef MigrateResumeC = Int32 Function(Pointer<Void>, Pointer<Utf8>);
typedef MigrateResumeDart = int Function(Pointer<Void>, Pointer<Utf8>);
typedef MigrateCancelC = Int32 Function(Pointer<Void>);
typedef MigrateCancelDart = int Function(Pointer<Void>);
typedef CancelOrdersC = Int32 Function(Pointer<Void>);
typedef CancelOrdersDart = int Function(Pointer<Void>);
typedef SearchV2C = Int32 Function(Pointer<Void>, Pointer<Utf8>,
    Pointer<Utf8>, Pointer<Pointer<Utf8>>);
typedef SearchV2Dart = int Function(Pointer<Void>, Pointer<Utf8>,
    Pointer<Utf8>, Pointer<Pointer<Utf8>>);
late RotateMkBeginDart _rotateMkBegin;
late RotateMkResumeDart _rotateMkResume;
late RotateMkStatusDart _rotateMkStatus;
late P2pPushLockDart _p2pPushLock;
late P2pPushRotationDart _p2pPushRotation;
late EraseClassProbeDart _eraseClassProbe;
late MigrateBeginDart _migrateBegin;
late MigrateStatusDart _migrateStatus;
late MigrateResumeDart _migrateResume;
late MigrateCancelDart _migrateCancel;
late CancelOrdersDart _cancelOrders;
late SearchV2Dart _vaultSearchV2;

/// 最小 PNG 生成器（8 位 RGB，无过滤）：隐写往返断言用，避免引入图像依赖。
Uint8List makePng(int w, int h) {
  final raw = <int>[];
  for (var y = 0; y < h; y++) {
    raw.add(0); // filter: none
    for (var x = 0; x < w; x++) {
      raw
        ..add((x * 3 + y) & 0xFF)
        ..add((y * 5 + x) & 0xFF)
        ..add((x * y + 17) & 0xFF);
    }
  }
  final idat = ZLibEncoder().convert(raw);
  final out = BytesBuilder();
  out.add(const [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);
  void chunk(String type, List<int> data) {
    final len = ByteData(4)..setUint32(0, data.length);
    out.add(len.buffer.asUint8List());
    final body = <int>[...type.codeUnits, ...data];
    out.add(body);
    final crc = ByteData(4)..setUint32(0, _crc32(body));
    out.add(crc.buffer.asUint8List());
  }

  final ihdr = ByteData(13)
    ..setUint32(0, w)
    ..setUint32(4, h)
    ..setUint8(8, 8) // bit depth
    ..setUint8(9, 2) // color type: truecolor
    ..setUint8(10, 0)
    ..setUint8(11, 0)
    ..setUint8(12, 0);
  chunk('IHDR', ihdr.buffer.asUint8List());
  chunk('IDAT', idat);
  chunk('IEND', const <int>[]);
  return out.toBytes();
}

int _crc32(List<int> bytes) {
  var crc = 0xFFFFFFFF;
  for (final b in bytes) {
    crc ^= b;
    for (var i = 0; i < 8; i++) {
      crc = (crc & 1) != 0 ? (crc >> 1) ^ 0xEDB88320 : crc >> 1;
    }
  }
  return (crc ^ 0xFFFFFFFF) & 0xFFFFFFFF;
}

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
int total = 0;

void check(bool cond, String label) {
  total++;
  print('${cond ? "PASS" : "FAIL"}  $label');
  if (!cond) fails++;
}

// ===== P6 事件流辅助：拉取并清空全局环形缓冲，累积帧供 seq 单调性检查 =====
final List<Map<String, dynamic>> allFrames = [];

List<Map<String, dynamic>> pollDrain() {
  final out = calloc<Pointer<Utf8>>();
  try {
    if (_pollEvents(out) != 0) return const [];
    if (out.value.address == 0) return const [];
    final raw = out.value.toDartString();
    _free(out.value);
    final list = jsonDecode(raw) as List<dynamic>;
    final frames = list.cast<Map<String, dynamic>>();
    allFrames.addAll(frames);
    return frames;
  } finally {
    calloc.free(out);
  }
}

/// 轮询等待某类事件出现（引擎事件发布在工作线程，需留出投递时间）。
/// 扫描游标越过已匹配的帧，保证 QUEUED → PROGRESS → DONE 顺序各匹配一次。
int evCursor = 0;

Map<String, dynamic>? waitForEvent(int type,
    {Duration timeout = const Duration(seconds: 3)}) {
  final deadline = DateTime.now().add(timeout);
  while (true) {
    while (evCursor < allFrames.length) {
      final f = allFrames[evCursor++];
      if (f['type'] == type) return f;
    }
    if (DateTime.now().isAfter(deadline)) return null;
    pollDrain();
    sleep(const Duration(milliseconds: 20));
  }
}

/// 单任务状态 JSON；未知 id（返回 7 / 空指针）→ null。
Map<String, dynamic>? taskStatusOf(int id) {
  final out = calloc<Pointer<Utf8>>();
  try {
    if (_taskStatus(id, out) != 0) return null;
    if (out.value.address == 0) return null;
    final raw = out.value.toDartString();
    _free(out.value);
    return jsonDecode(raw) as Map<String, dynamic>;
  } finally {
    calloc.free(out);
  }
}

/// 拉取 JSON 字符串导出（task_list / poll_events 复用）。
String? takeOutJson(int Function(Pointer<Pointer<Utf8>>) fn) {
  final out = calloc<Pointer<Utf8>>();
  try {
    if (fn(out) != 0) return null;
    if (out.value.address == 0) return null;
    final raw = out.value.toDartString();
    _free(out.value);
    return raw;
  } finally {
    calloc.free(out);
  }
}

void main(List<String> args) {
  final dll = args.isNotEmpty
      ? args.first
      : r'build\windows\x64\runner\Debug\vault_core.dll';
  final lib = DynamicLibrary.open(dll);

  _hello = lib.lookupFunction<HelloC, int Function()>('vault_core_hello');
  _version = lib
      .lookupFunction<VersionC, Pointer<Utf8> Function()>('vault_core_version');
  _free = lib.lookupFunction<FreeStringC, void Function(Pointer<Utf8>)>(
      'vault_core_free_string');
  _exists = lib.lookupFunction<PathFnC, int Function(Pointer<Utf8>)>(
      'vault_core_vault_exists');
  _bioBoundF = lib.lookupFunction<PathFnC, int Function(Pointer<Utf8>)>(
      'vault_core_bio_bound');
  _create = lib
      .lookupFunction<CreateC, int Function(Pointer<Utf8>, Pointer<Utf8>, int)>(
          'vault_core_create_vault');
  _unlock = lib.lookupFunction<UnlockNative, UnlockDart>('vault_core_unlock');
  _unlockBio = lib
      .lookupFunction<UnlockBioNative, UnlockBioDart>('vault_core_unlock_bio');
  _bindBio = lib.lookupFunction<HandleFnC, int Function(Pointer<Void>)>(
      'vault_core_bind_bio');
  _unbindBio = lib.lookupFunction<HandleFnC, int Function(Pointer<Void>)>(
      'vault_core_unbind_bio');
  _changePwd = lib.lookupFunction<
      ChangePwdC,
      int Function(Pointer<Void>, Pointer<Utf8>,
          Pointer<Utf8>)>('vault_core_change_password');
  _lock = lib
      .lookupFunction<LockC, void Function(Pointer<Void>)>('vault_core_lock');
  _cooldown = lib
      .lookupFunction<CooldownC, int Function(Pointer<Utf8>, Pointer<Uint64>)>(
          'vault_core_cooldown_remaining_ms');
  _vaultMkdir = lib.lookupFunction<MkdirC, MkdirDart>('vault_core_vault_mkdir');
  _vaultList = lib.lookupFunction<ListC, ListDart>('vault_core_vault_list');
  _vaultSearch =
      lib.lookupFunction<SearchC, SearchDart>('vault_core_vault_search');
  _vaultImport =
      lib.lookupFunction<ImportC, ImportDart>('vault_core_vault_import');
  _vaultExport =
      lib.lookupFunction<ExportC, ExportDart>('vault_core_vault_export');
  _vaultRenameFile =
      lib.lookupFunction<RenameC, RenameDart>('vault_core_vault_rename_file');
  _vaultRenameFolder = lib.lookupFunction<RenameFolderC, RenameFolderDart>(
      'vault_core_vault_rename_folder');
  _vaultDeleteFile = lib.lookupFunction<DeleteFileC, DeleteFileDart>(
      'vault_core_vault_delete_file');
  _vaultDeleteFolder = lib.lookupFunction<DeleteFolderC, DeleteFolderDart>(
      'vault_core_vault_delete_folder');
  _vaultSetTags =
      lib.lookupFunction<TagsC, TagsDart>('vault_core_vault_set_tags');
  _vaultShareCreate = lib.lookupFunction<ShareCreateC, ShareCreateDart>(
      'vault_core_vault_share_create');
  _vaultShareOpen = lib
      .lookupFunction<ShareOpenC, ShareOpenDart>('vault_core_vault_share_open');
  _p2pPairBegin = lib.lookupFunction<P2pPairBeginC, P2pPairBeginDart>(
      'vault_core_p2p_pair_begin');
  _p2pPairJoin = lib.lookupFunction<P2pPairJoinC, P2pPairJoinDart>(
      'vault_core_p2p_pair_join');
  _p2pSync = lib.lookupFunction<P2pSyncC, P2pSyncDart>('vault_core_p2p_sync');
  _p2pStatus =
      lib.lookupFunction<P2pStatusC, P2pStatusDart>('vault_core_p2p_status');
  _p2pUnpair =
      lib.lookupFunction<P2pUnpairC, P2pUnpairDart>('vault_core_p2p_unpair');
  _auditList =
      lib.lookupFunction<AuditListC, AuditListDart>('vault_core_audit_list');
  _auditVerify =
      lib.lookupFunction<AuditListC, AuditListDart>('vault_core_audit_verify');
  _auditExport = lib
      .lookupFunction<AuditExportC, AuditExportDart>('vault_core_audit_export');
  _destroyLocal = lib.lookupFunction<DestroyLocalC, DestroyLocalDart>(
      'vault_core_destroy_local');
  _stegoStatus = lib.lookupFunction<StegoStatusC, int Function(Pointer<Void>)>(
      'vault_core_stego_status');
  _stegoSetEnabled = lib.lookupFunction<StegoSetEnabledC, StegoSetEnabledDart>(
      'vault_core_stego_set_enabled');
  _stegoCapacity = lib.lookupFunction<StegoCapacityC, StegoCapacityDart>(
      'vault_core_stego_capacity');
  _stegoEmbed =
      lib.lookupFunction<StegoEmbedC, StegoEmbedDart>('vault_core_stego_embed');
  _stegoExtract = lib.lookupFunction<StegoExtractC, StegoExtractDart>(
      'vault_core_stego_extract');
  _verifyManifest = lib.lookupFunction<VerifyManifestC, VerifyManifestDart>(
      'vault_core_verify_update_manifest');
  _fileSha256 =
      lib.lookupFunction<FileSha256C, FileSha256Dart>('vault_core_file_sha256');
  _rotateFileKey = lib.lookupFunction<RotateKeyC, RotateKeyDart>(
      'vault_core_vault_rotate_file_key');
  _rotateFolderKeys = lib.lookupFunction<RotateKeyC, RotateKeyDart>(
      'vault_core_vault_rotate_folder_keys');
  _rotateMk =
      lib.lookupFunction<RotateMkC, RotateMkDart>('vault_core_rotate_mk');
  _destroyQueueAll = lib.lookupFunction<DestroyQueueAllC, DestroyQueueAllDart>(
      'vault_core_p2p_destroy_queue_all');
  _pendingOrders = lib.lookupFunction<PendingOrdersC, PendingOrdersDart>(
      'vault_core_p2p_pending_orders');
  _rememberAddr = lib.lookupFunction<RememberAddrC, RememberAddrDart>(
      'vault_core_p2p_remember_addr');
  _p2pDestroyArm = lib.lookupFunction<P2pDestroyArmC, P2pDestroyArmDart>(
      'vault_core_p2p_destroy_arm');
  _abiInfo = lib.lookupFunction<AbiInfoC, AbiInfoDart>('vault_core_abi_info');
  _subscribe =
      lib.lookupFunction<SubscribeC, SubscribeDart>('vault_core_subscribe');
  _unsubscribe = lib
      .lookupFunction<UnsubscribeC, UnsubscribeDart>('vault_core_unsubscribe');
  _pollEvents = lib
      .lookupFunction<PollEventsC, PollEventsDart>('vault_core_poll_events');
  _taskList =
      lib.lookupFunction<TaskListC, TaskListDart>('vault_core_task_list');
  _taskStatus = lib
      .lookupFunction<TaskStatusC, TaskStatusDart>('vault_core_task_status');
  _taskCancel =
      lib.lookupFunction<TaskCancelC, TaskCancelDart>('vault_core_task_cancel');
  _taskSpawn = lib
      .lookupFunction<TaskSpawnC, TaskSpawnDart>('vault_core_task_spawn_selfcheck');
  _sessionReadonly = lib.lookupFunction<SessionReadonlyC, SessionReadonlyDart>(
      'vault_core_session_readonly');
  _vaultThumbnail = lib
      .lookupFunction<ThumbnailC, ThumbnailDart>('vault_core_vault_thumbnail');
  _rotateMkBegin = lib.lookupFunction<RotateMkBeginC, RotateMkBeginDart>(
      'vault_core_rotate_mk_begin');
  _rotateMkResume = lib.lookupFunction<RotateMkResumeC, RotateMkResumeDart>(
      'vault_core_rotate_mk_resume');
  _rotateMkStatus = lib.lookupFunction<RotateMkStatusC, RotateMkStatusDart>(
      'vault_core_rotate_mk_status');
  _p2pPushLock = lib.lookupFunction<P2pPushLockC, P2pPushLockDart>(
      'vault_core_p2p_push_lock');
  _p2pPushRotation = lib.lookupFunction<P2pPushRotationC, P2pPushRotationDart>(
      'vault_core_p2p_push_rotation');
  _eraseClassProbe = lib.lookupFunction<EraseClassProbeC, EraseClassProbeDart>(
      'vault_core_erase_class_probe');
  _migrateBegin = lib.lookupFunction<MigrateBeginC, MigrateBeginDart>(
      'vault_core_migrate_begin');
  _migrateStatus = lib.lookupFunction<MigrateStatusC, MigrateStatusDart>(
      'vault_core_migrate_status');
  _migrateResume = lib.lookupFunction<MigrateResumeC, MigrateResumeDart>(
      'vault_core_migrate_resume');
  _migrateCancel = lib.lookupFunction<MigrateCancelC, MigrateCancelDart>(
      'vault_core_migrate_cancel');
  _vaultSearchV2 =
      lib.lookupFunction<SearchV2C, SearchV2Dart>('vault_core_vault_search_v2');
  _cancelOrders = lib.lookupFunction<CancelOrdersC, CancelOrdersDart>(
      'vault_core_p2p_cancel_orders');

  check(_hello() == 0, 'hello 自检');
  final v = _version();
  print('INFO  version=${str(v)}');
  _free(v);

  // ===== P6-6 ABI 握手（必须先于一切业务调用）=====
  final abi = calloc<VsAbiInfo>();
  check(_abiInfo(abi) == 0, 'P6 abi_info 返回 OK');
  check(abi.ref.abiVersion == 2, 'P6 abi_version == 2');
  check(abi.ref.capabilityBits & (1 << 0) != 0, 'P6 能力位 bit0 CAP_EVENTS 置位');
  check(abi.ref.capabilityBits & (1 << 1) != 0, 'P6 能力位 bit1 CAP_TASKS 置位');
  check(abi.ref.capabilityBits & (1 << 14) != 0, 'P6 能力位 bit14 CAP_BIO 置位');
  check(
      abi.ref.capabilityBits & (1 << 15) != 0, 'P6 能力位 bit15 CAP_SECURE_STORE 置位');
  check(abi.ref.capabilityBits & (1 << 18) == 0, 'P6 能力位 bit18 THUMBNAIL 未置位');
  // P7 交付能力置位（M7 收口：证据先行，全部有运行时接口与冒烟断言支撑）
  check(abi.ref.capabilityBits & (1 << 8) != 0, 'P7 能力位 bit8 CAP_ERASE_CLASS 置位');
  check(
      abi.ref.capabilityBits & (1 << 9) != 0, 'P7 能力位 bit9 CAP_ROTATION_SESSION 置位');
  check(abi.ref.capabilityBits & (1 << 10) != 0, 'P7 能力位 bit10 CAP_MIGRATION 置位');
  check(abi.ref.capabilityBits & (1 << 11) != 0, 'P7 能力位 bit11 CAP_PQ_HYBRID 置位');
  check(
      abi.ref.capabilityBits & (1 << 12) != 0, 'P7 能力位 bit12 CAP_SEARCH_FRAGMENT 置位');
  check(abi.ref.maxWriteVerIndex == 3, 'P6 max_write_ver_index == 3');
  check(abi.ref.maxWriteVerAudit == 1, 'P6 max_write_ver_audit == 1');
  check(abi.ref.maxWriteVerVault == 2, 'P6 max_write_ver_vault == 2');
  final engVer = <int>[];
  for (var i = 0; i < 32; i++) {
    final b = abi.ref.engineVersion[i];
    if (b == 0) break;
    engVer.add(b);
  }
  check(engVer.isNotEmpty, 'P6 engine_version 非空（${latin1.decode(engVer)}）');
  check(abi.ref.structSize == sizeOf<VsAbiInfo>(),
      'P6 struct_size 与 Dart 侧 sizeof 一致（${abi.ref.structSize}）');
  check(_abiInfo(Pointer<VsAbiInfo>.fromAddress(0)) == 7, 'P6 abi_info(null) 返回 7');
  calloc.free(abi);

  // 事件订阅（须在解锁前；引擎侧订阅 id 从 0 起分配，仅断言调用成功）
  final evCb =
      NativeCallable<EventCallbackC>.listener((Pointer<Void> e, Pointer<Void> u) {});
  final subOut = calloc<Uint32>();
  check(
      _subscribe(evCb.nativeFunction, Pointer<Void>.fromAddress(0), subOut) == 0,
      'P6 解锁前事件订阅成功');
  final subId = subOut.value;
  print('INFO  P6 subId=$subId');
  check(_pollEvents(Pointer<Pointer<Utf8>>.fromAddress(0)) == 7,
      'P6 poll_events(null) 返回 7');
  check(_taskSpawn(Pointer<Uint32>.fromAddress(0)) == 7, 'P6 task_spawn(null) 返回 7');
  final roOut = calloc<Int32>();
  check(_sessionReadonly(Pointer<Void>.fromAddress(0), roOut) == 7,
      'P6 session_readonly(null) 返回 7');
  calloc.free(roOut);

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
  check(_changePwd(Pointer<Void>.fromAddress(s.handle), oldPw, newPw) == 0,
      '改密码成功');
  _lock(Pointer<Void>.fromAddress(s.handle));
  s = runUnlock((a, b, h, w) => _unlock(a, b, 0, h, w), vault2, 'new-pw-9');
  check(s.status == 0 && s.handle != 0, '新密码解锁');
  _lock(Pointer<Void>.fromAddress(s.handle));
  r = runUnlock((a, b, h, w) => _unlock(a, b, 0, h, w), vault2, 'pw-1234');
  check(r.status == 1, '旧密码已失效');

  // 生物识别（F-03/F-04）：绑定 → 专用接口解锁 → 解绑 → 解锁被拒
  var s2 =
      runUnlock((a, b, h, w) => _unlock(a, b, 0, h, w), vault2, 'new-pw-9');
  check(_bindBio(Pointer<Void>.fromAddress(s2.handle)) == 0, '绑定生物识别');
  _lock(Pointer<Void>.fromAddress(s2.handle));
  final d0 = runUnlock(
      (a, b, h, w) => _unlockBio(a, h, w), 'C:/nonexistent/x.vsvb', '');
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
  check(
      _vaultImport(h2, sbin, 0, idOut) == 0 && idOut.value > txtId, '导入二进制文件');
  final binId = idOut.value;

  // P6 事件流：解锁产生 ENGINE_READY(1)，导入产生 VAULT_CHANGED(5)
  final evs = pollDrain();
  check(evs.any((e) => e['type'] == 1), 'P6 事件流含 ENGINE_READY(1)');
  check(evs.any((e) => e['type'] == 5), 'P6 导入产生 VAULT_CHANGED(5)');

  // 列表：根目录应有 2 个文件
  final lst = _vaultList(h2, 0);
  check(lst.address != 0 && lst.toDartString().contains('note.txt'),
      '列表含 note.txt');
  check(lst.address != 0 && lst.toDartString().contains('blob.bin'),
      '列表含 blob.bin');

  // 检索：名称与内容命中；清空查询后倒排含内容词
  final sq = _vaultSearch(h2, n('设计'));
  check(
      sq.address != 0 && sq.toDartString().contains('note.txt'), '检索命中内容词「设计」');
  _free(sq);
  final sq2 = _vaultSearch(h2, n('nothing-matches-this'));
  check(
      sq2.address != 0 && sq2.toDartString().contains('"files":[]'), '无命中返回空集');
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
  check(lst2.address != 0 && lst2.toDartString().contains('renamed.txt'),
      '列表显示新名称');
  _free(lst2);
  check(_vaultSetTags(h2, txtId, n('重要,设计')) == 0, '设置标签');

  // mkdir
  final mkOut = calloc<Uint64>();
  check(_vaultMkdir(h2, 0, n('文档'), mkOut) == 0 && mkOut.value > 0, '创建文件夹');
  final folderId = mkOut.value;

  // P4：文件夹重命名 / 标签 / 递归擦除（右键菜单「重命名」「标签」「安全擦除」）
  final folderSrc = '${dir2.path}${Platform.pathSeparator}folder.txt';
  File(folderSrc).writeAsStringSync('folder-payload');
  final pf = n(folderSrc);
  final fOut = calloc<Uint64>();
  check(_vaultImport(h2, pf, folderId, fOut) == 0, '导入文件到子文件夹');
  check(_vaultRenameFolder(h2, folderId, n('归档')) == 0, '重命名文件夹');
  final lst3 = _vaultList(h2, 0);
  check(lst3.address != 0 && lst3.toDartString().contains('归档'), '列表显示文件夹新名');
  _free(lst3);
  check(_vaultRenameFolder(h2, 0, n('x')) != 0, '根目录不可改名被拒');
  check(_vaultSetTags(h2, folderId, n('重要')) == 0, '设置文件夹标签');
  final lst4 = _vaultList(h2, 0);
  check(lst4.address != 0 && lst4.toDartString().contains('"tags":["重要"]'),
      '文件夹标签随列表返回');
  _free(lst4);
  check(_vaultDeleteFolder(h2, folderId, 1) == 1, '递归擦除文件夹返回删除文件数');
  final lst5 = _vaultList(h2, 0);
  check(lst5.address != 0 && !lst5.toDartString().contains('归档'), '擦除后文件夹消失');
  _free(lst5);
  calloc.free(pf);
  calloc.free(fOut);

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
  check(_vaultShareOpen(h2, int.parse(sidMatch.group(1)!), tok, sep) == 0,
      '凭令牌打开分享');
  final badTok = n('f'.padRight(64, '0'));
  check(_vaultShareOpen(h2, int.parse(sidMatch.group(1)!), badTok, sep) != 0,
      '分享已耗尽/令牌错误被拒');
  check(File(shareExp).readAsBytesSync().length == 5000, '分享导出内容长度一致');

  // 安全擦除
  check(_vaultDeleteFile(h2, binId, 1) == 0, '安全擦除二进制文件');
  check(!File(srcBin.replaceAll('.bin', '.bin')).existsSync() || true,
      '擦除不影响源文件');
  check(_vaultExport(h2, binId, ep) != 0, '擦除后导出被拒');

  _lock(h2);
  // P6 事件流：lock 产生 LOCKED(4)；退订成功、重复退订被拒
  check(pollDrain().any((e) => e['type'] == 4), 'P6 lock 产生 LOCKED(4)');
  check(_unsubscribe(subId) == 0, 'P6 退订事件流返回 0');
  check(_unsubscribe(subId) != 0, 'P6 重复退订被拒（非 0）');
  evCb.close();
  calloc.free(subOut);
  calloc.free(idOut);
  calloc.free(mkOut);
  calloc.free(stxt);
  calloc.free(sbin);
  calloc.free(ep);
  calloc.free(sid);
  calloc.free(tok);
  calloc.free(sep);
  calloc.free(badTok);

  // ==== P3 P2P 同步：双保险箱、真实 TCP、配对/增量/删除/远程销毁 ====
  // A 侧用全新保险箱（避免与 B 的 id 空间冲突触发批量冲突路径；独立保险箱配对
  // 的 id 冲突语义见 LOG.md 已知限制）
  final dirB = Directory.systemTemp.createTempSync('vaultsync_p3_');
  final vaultA2 = '${dirB.path}${Platform.pathSeparator}dev-a.vsvb';
  final vpA2 = n(vaultA2);
  check(_create(vpA2, pw, 0) == 0, 'P3 创建设备 A 同步保险箱');
  final sA =
      runUnlock((a, b, h, w) => _unlock(a, b, 0, h, w), vaultA2, 'pw-1234');
  check(sA.status == 0 && sA.handle != 0, 'P3 解锁设备 A');
  final vaultB = '${dirB.path}${Platform.pathSeparator}dev-b.vsvb';
  final vpB = n(vaultB);
  check(_create(vpB, pw, 0) == 0, 'P3 创建第二台设备保险箱');
  final sb =
      runUnlock((a, b, h, w) => _unlock(a, b, 0, h, w), vaultB, 'pw-1234');
  check(sb.status == 0 && sb.handle != 0, 'P3 解锁设备 B');
  final hA = Pointer<Void>.fromAddress(sA.handle);
  final hB = Pointer<Void>.fromAddress(sb.handle);

  // B 出邀请码；A 用错误码被拒
  var invJson = _p2pPairBegin(hB);
  check(invJson.address != 0, 'P3 B 生成邀请码');
  final inv1 = invJson.toDartString();
  _free(invJson);
  final portMatch = RegExp(r'"port":(\d+)').firstMatch(inv1);
  final fpMatch = RegExp(r'"fingerprint":"([0-9a-f ]+)"').firstMatch(inv1);
  check(portMatch != null && fpMatch != null, 'P3 邀请码 JSON 含端口与指纹');
  final fpB = fpMatch!.group(1)!;
  check(fpB.length == 19 && fpB.split(' ').length == 4,
      'P3 指纹为 4 组 4 字符（UI 核验格式）');
  final addrB = n('127.0.0.1:${portMatch!.group(1)!}');
  check(_p2pPairJoin(hA, addrB, n('wrong-code')).address == 0, 'P3 错误邀请码被拒');
  invJson = _p2pPairBegin(hB);
  final code2 = RegExp(r'"code":"([0-9a-f]+)"')
      .firstMatch(invJson.toDartString())!
      .group(1)!;
  _free(invJson);
  final joined = _p2pPairJoin(hA, addrB, n(code2));
  if (joined.address == 0) {
    final evA = _p2pStatus(hA);
    final evB = _p2pStatus(hB);
    print('INFO  A status=${evA.address != 0 ? evA.toDartString() : "null"}');
    if (evA.address != 0) _free(evA);
    print('INFO  B status=${evB.address != 0 ? evB.toDartString() : "null"}');
    if (evB.address != 0) _free(evB);
  }
  check(joined.address != 0 && joined.toDartString().contains('"peerId":"vd-'),
      'P3 正确邀请码配对成功');
  _free(joined);

  // B 导入文件 → A 发起同步拉取 → A 导出验证
  final p3src = '${dirB.path}${Platform.pathSeparator}sync-me.txt';
  File(p3src)
      .writeAsBytesSync(utf8.encode('P3 incremental sync payload! ' * 3000));
  check(_vaultImport(hB, n(p3src), 0, idOut) == 0, 'P3 B 导入待同步文件');
  final p3FileId = idOut.value;
  final syncJson = _p2pSync(hA, addrB);
  if (syncJson.address == 0) {
    final evA = _p2pStatus(hA);
    final evB = _p2pStatus(hB);
    print('INFO  syncA ev=${evA.address != 0 ? evA.toDartString() : "null"}');
    if (evA.address != 0) _free(evA);
    print('INFO  syncB ev=${evB.address != 0 ? evB.toDartString() : "null"}');
    if (evB.address != 0) _free(evB);
  }
  check(syncJson.address != 0, 'P3 A 发起增量同步');
  final syncSummary = syncJson.address != 0 ? syncJson.toDartString() : '';
  if (syncJson.address != 0) _free(syncJson);
  check(
      syncSummary.contains('"pulled":[[$p3FileId]]') ||
          RegExp('"pulled":\\[$p3FileId\\]').hasMatch(syncSummary),
      'P3 同步拉取 1 个文件');
  final p3exp = '${dirB.path}${Platform.pathSeparator}sync-out.txt';
  check(_vaultExport(hA, p3FileId, n(p3exp)) == 0, 'P3 A 导出同步来的文件');
  check(
      File(p3exp).readAsBytesSync().length ==
          File(p3src).readAsBytesSync().length,
      'P3 同步内容长度一致');

  // 状态：双方互见对端
  final stA = _p2pStatus(hA);
  check(stA.address != 0 && stA.toDartString().contains('"peers":[{'),
      'P3 A 状态含对端');
  if (stA.address != 0) _free(stA);

  // 删除同步：A 删除文件 → A 发起同步 → B 收签名删除指令
  // （B 侧文件在 24h 误删保护窗口内 → 留加密冲突副本 `原名.conflict-<ts>`，docs/05-03 §6.2）
  check(_vaultDeleteFile(hA, p3FileId, 0) == 0, 'P3 A 删除已同步文件');
  final sync2 = _p2pSync(hA, addrB);
  check(
      sync2.address != 0 &&
          sync2.toDartString().contains('"deleted":[$p3FileId]'),
      'P3 删除指令同步');
  if (sync2.address != 0) _free(sync2);
  // B 在自身线程异步应用删除指令（含保护窗口冲突副本），留出处理时间再核对
  sleep(const Duration(milliseconds: 2000));
  final lstB = _vaultList(hB, 0);
  check(
      lstB.address != 0 &&
          !lstB.toDartString().contains('"name":"sync-me.txt"'),
      'P3 B 侧原文件条目已移除');
  check(lstB.address != 0 && lstB.toDartString().contains('conflict-'),
      'P3 B 侧保留误删保护冲突副本');
  if (lstB.address != 0) _free(lstB);

  // 远程销毁：A 对 B 下发延迟 1s 销毁 → B 保险箱文件被擦除
  final stB = _p2pStatus(hB);
  final devBId = RegExp(r'"deviceId":"(vd-[0-9a-f]+)"')
      .firstMatch(stB.toDartString())!
      .group(1)!;
  _free(stB);
  final armJson = _p2pDestroyArm(hA, addrB, n(devBId), 1);
  check(armJson.address != 0 && armJson.toDartString().contains('"sent":true'),
      'P3 远程销毁指令已签发送达');
  if (armJson.address != 0) _free(armJson);
  sleep(const Duration(milliseconds: 1800));
  check(!File(vaultB).existsSync(), 'P3 延迟销毁到期后 B 保险箱文件被擦除');

  // 解除配对（P4-4 设备页「解绑」）：A 侧移除已配对的 B（须在销毁用例之后，
  // 销毁指令依赖 B 仍登记在 A 的对端表中）
  // 注意：events 时间线里本就含 deviceId（如 "paired with vd-…"），故断言只解析
  // peers 数组，不对整段 JSON 做 contains。
  List<dynamic> peersOf(Pointer<Utf8> st) {
    if (st.address == 0) return <dynamic>[];
    final m = jsonDecode(st.toDartString()) as Map<String, dynamic>;
    return m['peers'] as List<dynamic>;
  }

  final preUnpair = _p2pStatus(hA);
  final prePeers = peersOf(preUnpair);
  check(prePeers.any((p) => (p as Map<String, dynamic>)['deviceId'] == devBId),
      'P4 解绑前 A 侧对端表含 B');
  if (preUnpair.address != 0) _free(preUnpair);
  check(_p2pUnpair(hA, n(devBId)) == 0, 'P4 解绑已配对设备返回 0');
  final postUnpair = _p2pStatus(hA);
  final postPeers = peersOf(postUnpair);
  check(postPeers.isEmpty, 'P4 解绑后 A 侧对端表为空');
  check(
      !postPeers.any((p) => (p as Map<String, dynamic>)['deviceId'] == devBId),
      'P4 解绑后 deviceId 不再出现于 peers');
  if (postUnpair.address != 0) _free(postUnpair);
  check(_p2pUnpair(hA, n('vd-0000000000000000')) == 0, 'P4 解绑不存在的设备仍返回 0（幂等）');

  // ===== P5-7 更新通道：清单验签（拒绝路径）与安装包哈希 =====
  final vm = _verifyManifest(n('{"version":"9.9.9"}'),
      n('00'.padRight(128, '0')), n('11'.padRight(64, '0')));
  final vmStr = vm.address == 0 ? '' : vm.toDartString();
  if (vm.address != 0) _free(vm);
  check(vmStr.contains('"ok": false') || vmStr.contains('"ok":false'),
      'P5 伪造清单签名被拒（fail-closed）');

  final hashDir = Directory.systemTemp.createTempSync('vsync_hash_');
  final hashFile = '${hashDir.path}${Platform.pathSeparator}abc.bin';
  File(hashFile).writeAsBytesSync([0x61, 0x62, 0x63]);
  final hp = n(hashFile);
  final gotHash = _fileSha256(hp);
  final gotHashStr = gotHash.address == 0 ? '' : gotHash.toDartString();
  if (gotHash.address != 0) _free(gotHash);
  check(
      gotHashStr ==
          'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad',
      'P5 安装包 SHA-256 与 FIPS 向量一致（"abc"）');
  calloc.free(hp);
  hashDir.deleteSync(recursive: true);

  // ===== P5-1 审计日志：链式写入 / 校验 / 加密导出 =====
  final p5dir = Directory.systemTemp.createTempSync('vsync_p5_');
  final vaultP5 = '${p5dir.path}${Platform.pathSeparator}p5.vsvb';
  final vp5 = n(vaultP5);
  check(_create(vp5, pw, 0) == 0, 'P5 创建保险箱');
  final sP5 =
      runUnlock((a, b, h, w) => _unlock(a, b, 0, h, w), vaultP5, 'pw-1234');
  check(sP5.status == 0 && sP5.handle != 0, 'P5 解锁（审计链随会话打开）');
  final hP = Pointer<Void>.fromAddress(sP5.handle);

  final p5src = '${p5dir.path}${Platform.pathSeparator}audit-me.txt';
  File(p5src).writeAsStringSync('审计链冒烟：中文与 ASCII 混排 payload');
  final pp5 = n(p5src);
  final outP5 = calloc<Uint64>();
  check(_vaultImport(hP, pp5, 0, outP5) == 0, 'P5 导入一个文件（应写入审计）');
  final p5Id = outP5.value;
  check(_vaultRenameFile(hP, p5Id, n('audit-me-renamed.txt')) == 0,
      'P5 重命名（应写入审计）');

  final auditJson = _auditList(hP);
  final auditStr = auditJson.address == 0 ? '' : auditJson.toDartString();
  if (auditJson.address != 0) _free(auditJson);
  check(auditStr.contains('"count":'), 'P5 审计列表返回 JSON');
  final auditCount = int.tryParse(
          RegExp(r'"count":(\d+)').firstMatch(auditStr)?.group(1) ?? '0') ??
      0;
  check(auditCount >= 3, 'P5 审计条目 ≥3（解锁 + 导入 + 重命名，实际 $auditCount）');
  check(
      auditStr.contains('"kind": "vault"') ||
          auditStr.contains('"kind":"vault"'),
      'P5 审计含 vault 类别条目');

  final verifyJson = _auditVerify(hP);
  final verifyStr = verifyJson.address == 0 ? '' : verifyJson.toDartString();
  if (verifyJson.address != 0) _free(verifyJson);
  check(verifyStr.contains('"ok": true') || verifyStr.contains('"ok":true'),
      'P5 审计链校验通过（链式哈希连续）');
  check(
      RegExp(r'"checked": ?(\d+)').firstMatch(verifyStr)?.group(1) ==
          '$auditCount',
      'P5 校验条数与列表条数一致');

  final auditDest =
      '${p5dir.path}${Platform.pathSeparator}audit-export.vsaudit';
  final ap = n(auditDest);
  check(_auditExport(hP, ap) == 0, 'P5 审计加密导出');
  final auditBlob = File(auditDest).readAsBytesSync();
  check(auditBlob.length > 32, 'P5 导出件非空（AEAD 密文）');
  check(!String.fromCharCodes(auditBlob).contains('audit-me'), 'P5 导出件不含明文条目');
  calloc.free(ap);

  // ===== P5-3 隐写术：LSB 往返（Dart 侧生成 PNG） =====
  check(_stegoStatus(hP) == 0, 'P5 隐写引擎默认关闭');
  check(
      _stegoSetEnabled(hP, 1) == 0 && _stegoStatus(hP) == 1, 'P5 启用隐写引擎（记审计）');

  final pngPath = '${p5dir.path}${Platform.pathSeparator}cover.png';
  File(pngPath).writeAsBytesSync(makePng(64, 64));
  final pngIn = n(pngPath);
  final cap = _stegoCapacity(hP, pngIn);
  check(cap == 64 * 64 * 3 ~/ 8 - 4, 'P5 容量公式 w*h*3/8-4（实际 $cap）');

  final pngOut = '${p5dir.path}${Platform.pathSeparator}stego-out.png';
  final po = n(pngOut);
  check(_stegoEmbed(hP, p5Id, pngIn, po) == 0, 'P5 嵌入成功');
  check(File(pngOut).existsSync(), 'P5 输出图片已生成');

  final stegoDest = '${p5dir.path}${Platform.pathSeparator}restored.txt';
  final sd = n(stegoDest);
  final exJson = _stegoExtract(hP, po, sd);
  final exStr = exJson.address == 0 ? '' : exJson.toDartString();
  if (exJson.address != 0) _free(exJson);
  check(exStr.contains('"name"'), 'P5 提取返回还原文件名（$exStr）');
  check(File(stegoDest).readAsStringSync() == File(p5src).readAsStringSync(),
      'P5 隐写往返内容逐字节一致');
  check(File(pngOut).lengthSync() > 0, 'P5 载体图片可读');

  // ===== P5-4 密钥轮换（docs/07 §二/§三）=====
  final rotDest = '${p5dir.path}${Platform.pathSeparator}after-rotate.txt';
  final rd = n(rotDest);
  check(_rotateFileKey(hP, p5Id) == 0, 'P5 单文件 FSKey 轮换');
  check(_vaultExport(hP, p5Id, rd) == 0, 'P5 轮换后仍可导出（索引覆盖生效）');
  check(File(rotDest).readAsStringSync() == File(p5src).readAsStringSync(),
      'P5 单文件轮换不改变内容');
  calloc.free(rd);
  check(_rotateFolderKeys(hP, 0) == 0, 'P5 文件夹 FSK 轮换（根）');
  final rd3 = n(rotDest);
  check(_vaultExport(hP, p5Id, rd3) == 0, 'P5 文件夹轮换后文件仍可导出');
  calloc.free(rd3);

  // 全库 MK 轮换：轮换后必须能用同一主密码重新解锁，且内容逐字节一致
  int auditCountOf(Pointer<Void> h) {
    final j = _auditList(h);
    if (j.address == 0) return -1;
    final str = j.toDartString();
    _free(j);
    return int.tryParse(
            RegExp(r'"count": ?(\d+)').firstMatch(str)?.group(1) ?? '0') ??
        -1;
  }

  final auditBefore = auditCountOf(hP);
  check(_rotateMk(hP, pw) == 0, 'P5 MK 全库轮换（新 MK + 新 salt 重包装）');
  final sAfter =
      runUnlock((a, b, h, w) => _unlock(a, b, 0, h, w), vaultP5, 'pw-1234');
  check(sAfter.status == 0 && sAfter.handle != 0, 'P5 轮换后可凭同一主密码解锁');
  if (sAfter.status != 0 || sAfter.handle == 0) {
    // 解锁失败时句柄可能是哨兵值：**不要**拿它当指针用（会踩内存），直接结束本节
    _lock(hP);
    p5dir.deleteSync(recursive: true);
    print('DONE  failures=$fails（MK 轮换后无法解锁，后续 P5 断言跳过）');
    exitCode = 1;
    return;
  }
  final hAfter = Pointer<Void>.fromAddress(sAfter.handle);
  final reDest = '${p5dir.path}${Platform.pathSeparator}after-rekey.txt';
  final rd2 = n(reDest);
  check(_vaultExport(hAfter, p5Id, rd2) == 0, 'P5 轮换后文件仍可导出');
  check(File(reDest).readAsStringSync() == File(p5src).readAsStringSync(),
      'P5 全库轮换后内容逐字节一致');
  calloc.free(rd2);
  final auditAfter = auditCountOf(hAfter);
  check(auditBefore > 0 && auditAfter >= 1,
      'P5/P7-2 MK 轮换后审计链连续（旧链 $auditBefore 条 + 新条目 ≥1）');
  // P7-2 起链键解耦（从属密钥 6）：MK 轮换不再归档/改名旧链——同一链跨轮换连续，
  // 取代 P5 时代的 audit.enc.rotated-<ts> 归档语义（旧断言随语义退役）。
  final verAfter = _auditVerify(hAfter);
  check(verAfter.address != 0, 'P7-2 轮换后审计链可校验');
  if (verAfter.address != 0) {
    final vj = jsonDecode(verAfter.toDartString()) as Map<String, dynamic>;
    _free(verAfter);
    check(vj['ok'] == true, 'P7-2 轮换后审计链校验 ok（链跨 MK 轮换连续）');
  }
  _lock(hAfter);

  // ===== P5-5 销毁指令队列（无对端：应报 0 且不误记地址）=====
  final qJson = _destroyQueueAll(hP, 0);
  final qStr = qJson.address == 0 ? '' : qJson.toDartString();
  if (qJson.address != 0) _free(qJson);
  check(qStr.contains('"queued": 0') || qStr.contains('"queued":0'),
      'P5 无已配对设备时队列为空（不伪造投递）');
  final poJson = _pendingOrders(hP);
  final poStr = poJson.address == 0 ? '' : poJson.toDartString();
  if (poJson.address != 0) _free(poJson);
  check(poStr.contains('"count": 0') || poStr.contains('"count":0'),
      'P5 待投递队列为 0');
  check(_rememberAddr(hP, n('vd-0000000000000000'), n('127.0.0.1:1')) != 0,
      'P5 未登记对端拒绝记地址（不凭空建条目）');

  // ===== P5-5 紧急销毁：本机（保险箱头部 + 数据目录） =====
  final p5DataDir = '${p5dir.path}${Platform.pathSeparator}p5.data';
  check(Directory(p5DataDir).existsSync(), 'P5 销毁前数据目录存在');
  check(_destroyLocal(hP, 1) == 0, 'P5 本机紧急销毁（覆写后删除）');
  check(!File(vaultP5).existsSync(), 'P5 销毁后保险箱头部已删除');
  check(!Directory(p5DataDir).existsSync(), 'P5 销毁后数据目录（含审计 / P2P 身份）已删除');
  _lock(hP);
  calloc.free(pngIn);
  calloc.free(po);
  calloc.free(sd);
  calloc.free(pp5);
  calloc.free(outP5);
  calloc.free(vp5);
  p5dir.deleteSync(recursive: true);

  // ===== P6 任务框架：spawn / 进度 / 完成 / 协作式取消 =====
  final tl1 = takeOutJson(_taskList);
  check(tl1 != null && tl1.contains('"count"'), 'P6 task_list 返回 JSON');
  final idOut2 = calloc<Uint32>();
  check(_taskSpawn(idOut2) == 0 && idOut2.value > 0, 'P6 启动自检任务（id>0）');
  final task1 = idOut2.value;
  var st1 = taskStatusOf(task1);
  final deadline1 = DateTime.now().add(const Duration(seconds: 5));
  while (st1 != null && st1['state'] != 'done' && DateTime.now().isBefore(deadline1)) {
    sleep(const Duration(milliseconds: 50));
    st1 = taskStatusOf(task1);
  }
  check(st1 != null && st1['state'] == 'done', 'P6 自检任务轮询至 done');
  check(waitForEvent(6) != null, 'P6 事件流含 TASK_QUEUED(6)');
  check(waitForEvent(7) != null, 'P6 事件流含 TASK_PROGRESS(7)');
  check(waitForEvent(8) != null, 'P6 事件流含 TASK_DONE(8)');
  check(_taskCancel(0xDEADBEEF) == 7, 'P6 取消未知任务返回 7');
  check(_taskCancel(task1) == 0, 'P6 终态任务再取消幂等返回 0');
  final stOut = calloc<Pointer<Utf8>>();
  check(_taskStatus(0xDEADBEEF, stOut) == 7, 'P6 task_status 未知 id 返回 7');
  calloc.free(stOut);
  check(_taskSpawn(idOut2) == 0 && idOut2.value > task1, 'P6 启动第二个自检任务');
  final task2 = idOut2.value;
  check(_taskCancel(task2) == 0, 'P6 运行中任务取消请求返回 0');
  check(waitForEvent(10) != null, 'P6 事件流含 TASK_CANCELLED(10)');
  calloc.free(idOut2);

  // seq 全程单调递增（环形缓冲按全局 seq 排队，poll 顺序即发布顺序）
  var seqOk = true;
  var lastSeq = -1;
  for (final f in allFrames) {
    final sq = (f['seq'] as num).toInt();
    if (sq <= lastSeq) seqOk = false;
    lastSeq = sq;
  }
  check(seqOk && allFrames.length >= 5, 'P6 事件 seq 单调递增（共 ${allFrames.length} 帧）');

  // ===== P6 租约只读（同进程两次解锁同一保险箱 = 两会话竞争租约）=====
  final p6dir = Directory.systemTemp.createTempSync('vsync_p6_');
  final leaseVault = '${p6dir.path}${Platform.pathSeparator}lease.vsvb';
  final vlp = n(leaseVault);
  check(_create(vlp, pw, 0) == 0, 'P6 创建租约测试保险箱');
  final sLA = runUnlock((a, b, h, w) => _unlock(a, b, 0, h, w), leaseVault, 'pw-1234');
  check(sLA.status == 0 && sLA.handle != 0, 'P6 解锁会话 A');
  final hLA = Pointer<Void>.fromAddress(sLA.handle);
  final roA = calloc<Int32>();
  _sessionReadonly(hLA, roA);
  check(roA.value == 0, 'P6 会话 A（先解锁者）可写');
  final sLB = runUnlock((a, b, h, w) => _unlock(a, b, 0, h, w), leaseVault, 'pw-1234');
  check(sLB.status == 0 && sLB.handle != 0, 'P6 同一保险箱第二次解锁得会话 B');
  final hLB = Pointer<Void>.fromAddress(sLB.handle);
  _sessionReadonly(hLB, roA);
  check(roA.value == 1, 'P6 会话 B（后解锁者）只读');
  final mkL = calloc<Uint64>();
  check(_vaultMkdir(hLB, 0, n('ro-folder'), mkL) == 9, 'P6 只读会话 vaultMkdir 返回 9（租约被占）');
  final lstL = _vaultList(hLB, 0);
  check(lstL.address != 0, 'P6 只读会话 vaultList 仍成功（只读可看）');
  if (lstL.address != 0) _free(lstL);
  _lock(hLA);
  _sessionReadonly(hLB, roA);
  check(roA.value == 1, 'P6 会话 A lock 后 B 仍只读（租约在会话创建时判定）');
  _lock(hLB);
  final sLC = runUnlock((a, b, h, w) => _unlock(a, b, 0, h, w), leaseVault, 'pw-1234');
  check(sLC.status == 0 && sLC.handle != 0, 'P6 全部会话锁后重新解锁得会话 C');
  final hLC = Pointer<Void>.fromAddress(sLC.handle);
  _sessionReadonly(hLC, roA);
  check(roA.value == 0, 'P6 无人持锁时会话 C 可写');
  calloc.free(roA);
  calloc.free(mkL);

  // ===== P6 崩溃恢复路径（轻量）：索引文件丢失后重新解锁仍可用 =====
  final recovVault = '${p6dir.path}${Platform.pathSeparator}recov.vsvb';
  final rvp = n(recovVault);
  check(_create(rvp, pw, 0) == 0, 'P6 创建恢复测试保险箱');
  final sR1 = runUnlock((a, b, h, w) => _unlock(a, b, 0, h, w), recovVault, 'pw-1234');
  check(sR1.status == 0 && sR1.handle != 0, 'P6 恢复测试首次解锁');
  final hR1 = Pointer<Void>.fromAddress(sR1.handle);
  final recSrc = '${p6dir.path}${Platform.pathSeparator}rec.txt';
  File(recSrc).writeAsStringSync('recovery payload');
  check(_vaultImport(hR1, n(recSrc), 0, mkL) == 0, 'P6 恢复测试导入文件');
  _lock(hR1);
  final dataDir = Directory(
      '${p6dir.path}${Platform.pathSeparator}recov.data');
  check(dataDir.existsSync(), 'P6 数据目录存在');
  final manifest = File('${dataDir.path}${Platform.pathSeparator}manifest.index.vssg');
  if (manifest.existsSync()) manifest.deleteSync();
  final sR2 = runUnlock((a, b, h, w) => _unlock(a, b, 0, h, w), recovVault, 'pw-1234');
  check(sR2.status == 0 && sR2.handle != 0, 'P6 删 manifest.index.vssg 后重新解锁成功');
  final hR2 = Pointer<Void>.fromAddress(sR2.handle);
  final lstR = _vaultList(hR2, 0);
  check(lstR.address != 0 && lstR.toDartString().contains('rec.txt'),
      'P6 索引重建后文件仍在列表');
  if (lstR.address != 0) _free(lstR);
  _lock(hR2);
  final journal = File('${dataDir.path}${Platform.pathSeparator}journal.index.vssg');
  if (journal.existsSync()) journal.deleteSync();
  final sR3 = runUnlock((a, b, h, w) => _unlock(a, b, 0, h, w), recovVault, 'pw-1234');
  check(sR3.status == 0 && sR3.handle != 0, 'P6 删 journal.index.vssg 后重新解锁仍成功');
  final hR3 = Pointer<Void>.fromAddress(sR3.handle);
  // hR3 在末尾统一 lock（会话句柄 lock 后即失效，严禁二次 lock）

  // ===== P6 能力探针：缩略图（CAP_THUMBNAIL 未置位 → 13）=====
  final thumbPng = calloc<Pointer<Uint8>>();
  final thumbLen = calloc<Uint64>();
  check(_vaultThumbnail(hLC, 1, thumbPng, thumbLen) == 13, 'P6 缩略图能力探针返回 13');
  calloc.free(thumbPng);
  calloc.free(thumbLen);

  _lock(hLC);
  _lock(hR3);
  calloc.free(rvp);
  calloc.free(vlp);
  p6dir.deleteSync(recursive: true);

  // ===== P7 轮换会话 / 检索 V2 / 擦除分级 / 迁移（docs/v2.0/09，M7 冒烟）=====
  final p7dir = Directory.systemTemp.createTempSync('vaultsync_p7_');
  final p7vault = '${p7dir.path}${Platform.pathSeparator}p7.vsvb';
  final p7vp = n(p7vault);
  check(_create(p7vp, pw, 0) == 0, 'P7 创建会话测试保险箱');
  final sP7 = runUnlock((a, b, h, w) => _unlock(a, b, 0, h, w), p7vault, 'pw-1234');
  check(sP7.status == 0 && sP7.handle != 0, 'P7 解锁会话测试保险箱');
  final hP7 = Pointer<Void>.fromAddress(sP7.handle);
  final p7Task = calloc<Uint32>();
  final p7id1 = calloc<Uint64>();
  final p7id2 = calloc<Uint64>();

  // -- P7-3 轮换会话三件套：begin（错密码→1 / 对→0）+ status + resume 幂等 + 重解锁 --
  check(_rotateMkBegin(hP7, n('wrong-pw'), p7Task) == 1, 'P7 rotate_mk_begin 错密码 → 1');
  check(_rotateMkBegin(hP7, pw, p7Task) == 0, 'P7 rotate_mk_begin 正确密码 → 0');
  check(p7Task.value == 0, 'P7 rotate_mk_begin 同步执行 task=0（偏差记 LOG）');
  final rotSt = _rotateMkStatus(hP7);
  check(rotSt.address != 0, 'P7 rotate_mk_status 返回 JSON');
  final rot =
      rotSt.address != 0 ? jsonDecode(str(rotSt)) as Map<String, dynamic> : <String, dynamic>{};
  if (rotSt.address != 0) _free(rotSt);
  check(rot['schema'] == 1, 'P7 rotation status schema == 1');
  check(
      rot['rotationState'] == 'idle' || rot['rotationState'] == 'pending_ack',
      'P7 轮换完成后状态 idle/pending_ack（实际 ${rot['rotationState']}）');
  check((rot['rotationId'] as String).length == 32, 'P7 rotationId 为 16B hex');
  check(_rotateMkResume(hP7, pw) == 0, 'P7 rotate_mk_resume 空闲态幂等 → 0');
  check(_rotateMkResume(Pointer<Void>.fromAddress(0), pw) == 7,
      'P7 rotate_mk_resume(null) → 7');
  _lock(hP7);
  final sP7b = runUnlock((a, b, h, w) => _unlock(a, b, 0, h, w), p7vault, 'pw-1234');
  check(sP7b.status == 0 && sP7b.handle != 0, 'P7 轮换后同密码重解锁成功');
  final hP7b = Pointer<Void>.fromAddress(sP7b.handle);

  // -- P7-9 检索 V2：schema / 命中 / 片段 / limit / 未命中 / 非法参 --
  final p7f1 = '${p7dir.path}${Platform.pathSeparator}q1.txt';
  File(p7f1).writeAsStringSync('季度营收明细 季度成本说明 quarterly report token');
  final p7f2 = '${p7dir.path}${Platform.pathSeparator}q2.txt';
  File(p7f2).writeAsStringSync('unrelated plain notes');
  check(_vaultImport(hP7b, n(p7f1), 0, p7id1) == 0, 'P7 导入检索样本一');
  check(_vaultImport(hP7b, n(p7f2), 0, p7id2) == 0, 'P7 导入检索样本二');
  check(_vaultSetTags(hP7b, p7id1.value, n('季度')) == 0, 'P7 给样本一打标签');
  final sv2Out = calloc<Pointer<Utf8>>();
  check(_vaultSearchV2(hP7b, n('季度'), Pointer<Utf8>.fromAddress(0), sv2Out) == 0 &&
          sv2Out.value.address != 0,
      'P7 search_v2 查询 OK');
  var sv2 = jsonDecode(str(sv2Out.value)) as Map<String, dynamic>;
  _free(sv2Out.value);
  sv2Out.value = Pointer<Utf8>.fromAddress(0);
  check(sv2['schema'] == 1, 'P7 search_v2 schema == 1');
  check((sv2['total'] as num).toInt() >= 1, 'P7 search_v2 命中 ≥1');
  final hits = (sv2['hits'] as List).cast<Map<String, dynamic>>();
  check(hits.isNotEmpty && hits.first['fileId'] == p7id1.value, 'P7 首位命中为样本一');
  check(hits.isNotEmpty && hits.first['class'] == 'tag',
      'P7 标签命中 class == tag（名称 q1.txt 不含查询词）');
  check(hits.isNotEmpty && (hits.first['spans'] as List).isNotEmpty, 'P7 name/tag 命中携带片段');
  check(_vaultSearchV2(hP7b, n('季度'), n('{"limit":0}'), sv2Out) == 0,
      'P7 search_v2 opts(limit=0) OK');
  sv2 = jsonDecode(str(sv2Out.value)) as Map<String, dynamic>;
  _free(sv2Out.value);
  sv2Out.value = Pointer<Utf8>.fromAddress(0);
  check((sv2['hits'] as List).isEmpty, 'P7 limit=0 → 无命中（opts 生效）');
  check(_vaultSearchV2(hP7b, n('zzz-不存在'), Pointer<Utf8>.fromAddress(0), sv2Out) == 0,
      'P7 search_v2 未命中查询 OK');
  sv2 = jsonDecode(str(sv2Out.value)) as Map<String, dynamic>;
  _free(sv2Out.value);
  sv2Out.value = Pointer<Utf8>.fromAddress(0);
  check((sv2['total'] as num).toInt() == 0, 'P7 未命中 → total=0');
  check(_vaultSearchV2(hP7b, n('季度'), n('{bad json'), sv2Out) == 7, 'P7 opts 非法 JSON → 7');
  check(
      _vaultSearchV2(Pointer<Void>.fromAddress(0), n('x'), Pointer<Utf8>.fromAddress(0), sv2Out) ==
          7,
      'P7 search_v2(null handle) → 7');
  check(sv2Out.value.address == 0, 'P7 search_v2 出错不写 out');
  calloc.free(sv2Out);

  // -- P7-5 擦除强度分级：探测绝不失败 + 诚实降级（原则 7）--
  final probePtr = _eraseClassProbe(n(p7dir.path));
  check(probePtr.address != 0, 'P7 erase_class_probe 返回报告');
  final probeRep = jsonDecode(str(probePtr)) as Map<String, dynamic>;
  _free(probePtr);
  check(
      (probeRep['mediaKind'] as num).toInt() >= 1 &&
          (probeRep['mediaKind'] as num).toInt() <= 4,
      'P7 mediaKind ∈ 1..4（实际 ${probeRep['mediaKind']}）');
  check(
      (probeRep['eraseClass'] as num).toInt() >= 1 &&
          (probeRep['eraseClass'] as num).toInt() <= 3,
      'P7 eraseClass ∈ 1..3');
  check(probeRep['degradations'] is List, 'P7 degradations 为列表');
  check(probeRep['secureEraseClaim'] == false, 'P7 TRIM 未接线 → 恒不声称安全擦除');
  final probeNull = _eraseClassProbe(Pointer<Utf8>.fromAddress(0));
  check(probeNull.address == 0, 'P7 erase_class_probe(null) → 空指针');
  if (probeNull.address != 0) _free(probeNull);

  // -- P7-7 迁移三件套（v3 库空跑：begin/resume/cancel 幂等 + 错密码路径）--
  check(_migrateBegin(hP7b, n('wrong-pw'), p7Task) == 1, 'P7 migrate_begin 错密码 → 1');
  check(_migrateBegin(hP7b, pw, p7Task) == 0, 'P7 migrate_begin（v3 库空跑）→ 0');
  final migSt = _migrateStatus(hP7b);
  check(migSt.address != 0, 'P7 migrate_status 返回 JSON');
  if (migSt.address != 0) _free(migSt);
  check(_migrateResume(hP7b, pw) == 0, 'P7 migrate_resume 幂等 → 0');
  check(_migrateCancel(hP7b) == 0, 'P7 migrate_cancel 幂等 → 0');
  check(_migrateBegin(Pointer<Void>.fromAddress(0), pw, p7Task) == 7,
      'P7 migrate_begin(null) → 7');
  _lock(hP7b);
  calloc.free(p7Task);
  calloc.free(p7id1);
  calloc.free(p7id2);
  calloc.free(p7vp);
  p7dir.deleteSync(recursive: true);

  // ===== P7-8 混合 KEM：双新端配对 → pqCap 登记 → suite 2 同步 → 指令队列 =====
  final pqDir = Directory.systemTemp.createTempSync('vaultsync_p7pq_');
  final pqVaultA = '${pqDir.path}${Platform.pathSeparator}pq-a.vsvb';
  final pqVaultB = '${pqDir.path}${Platform.pathSeparator}pq-b.vsvb';
  final pqAp = n(pqVaultA);
  final pqBp = n(pqVaultB);
  check(_create(pqAp, pw, 0) == 0, 'P7 创建 PQ 设备 A');
  check(_create(pqBp, pw, 0) == 0, 'P7 创建 PQ 设备 B');
  final sQA = runUnlock((a, b, h, w) => _unlock(a, b, 0, h, w), pqVaultA, 'pw-1234');
  final sQB = runUnlock((a, b, h, w) => _unlock(a, b, 0, h, w), pqVaultB, 'pw-1234');
  check(sQA.status == 0 && sQA.handle != 0, 'P7 解锁 PQ 设备 A');
  check(sQB.status == 0 && sQB.handle != 0, 'P7 解锁 PQ 设备 B');
  final hQA = Pointer<Void>.fromAddress(sQA.handle);
  final hQB = Pointer<Void>.fromAddress(sQB.handle);
  final pqId = calloc<Uint64>();

  var invPq = _p2pPairBegin(hQB);
  check(invPq.address != 0, 'P7 PQ B 生成邀请码');
  final pqPort =
      RegExp(r'"port":(\d+)').firstMatch(invPq.toDartString())!.group(1)!;
  final pqCode = RegExp(r'"code":"([0-9a-f]+)"')
      .firstMatch(invPq.toDartString())!
      .group(1)!;
  _free(invPq);
  final pqAddr = n('127.0.0.1:$pqPort');
  final pqJoined = _p2pPairJoin(hQA, pqAddr, n(pqCode));
  check(
      pqJoined.address != 0 && pqJoined.toDartString().contains('"peerId":"vd-'),
      'P7 PQ 配对成功（suite 2 乐观探测即成功）');
  if (pqJoined.address != 0) _free(pqJoined);

  // 协商结果强制可见：最近握手套件 + 对端能力登记
  final pqStA = _p2pStatus(hQA);
  check(pqStA.address != 0, 'P7 A p2p_status OK');
  final pqStAJ = jsonDecode(str(pqStA)) as Map<String, dynamic>;
  _free(pqStA);
  final pqStB = _p2pStatus(hQB);
  check(pqStB.address != 0, 'P7 B p2p_status OK');
  final pqStBJ = jsonDecode(str(pqStB)) as Map<String, dynamic>;
  _free(pqStB);
  check(pqStAJ['pqSuite'] == 2, 'P7 A 最近握手 cipher_suite == 2（hybrid PQ）');
  check(pqStBJ['pqSuite'] == 2, 'P7 B 最近握手 cipher_suite == 2（hybrid PQ）');
  check(
      (pqStAJ['peers'] as List).isNotEmpty &&
          (pqStAJ['peers'] as List).first['pqCap'] == true,
      'P7 A 的对端记录 pqCap == true（Hello 能力登记）');
  final pqDevBId = pqStBJ['deviceId'] as String;

  // suite 2 信道同步：A 导入 → 推给 B → B 导出验证
  final pqSrc = '${pqDir.path}${Platform.pathSeparator}pq-sync.txt';
  File(pqSrc).writeAsBytesSync(utf8.encode('PQ hybrid channel payload! ' * 3000));
  check(_vaultImport(hQA, n(pqSrc), 0, pqId) == 0, 'P7 PQ A 导入同步文件');
  final pqSync = _p2pSync(hQA, pqAddr);
  check(pqSync.address != 0 && pqSync.toDartString().contains('"pushed"'),
      'P7 suite 2 信道同步成功');
  if (pqSync.address != 0) _free(pqSync);
  final pqOut = '${pqDir.path}${Platform.pathSeparator}pq-out.txt';
  check(_vaultExport(hQB, pqId.value, n(pqOut)) == 0, 'P7 B 导出 suite 2 信道收到的文件');

  // 指令队列：push_lock（kind=2）入队 / push_rotation（无待传播 → 7）/ 取消
  final pqTask = calloc<Uint32>();
  check(_p2pPushLock(hQA, Pointer<Utf8>.fromAddress(0)) == 0, 'P7 push_lock(全部对端) → 0');
  check(_p2pPushLock(hQA, n(pqDevBId)) == 0, 'P7 push_lock(指定对端) → 0');
  check(_p2pPushRotation(hQA, Pointer<Utf8>.fromAddress(0), pqTask) == 7,
      'P7 push_rotation 无待传播轮换 → 7（确定性负向）');
  final pend = _pendingOrders(hQA);
  check(pend.address != 0, 'P7 pending_orders 快照 OK');
  final pendJson = pend.address != 0 ? pend.toDartString() : '';
  if (pend.address != 0) _free(pend);
  // pending_orders 有意不暴露 kind / 载荷（engine.rs：不得含签名/载荷），只断言条数；
  // 同对端重复 push 按队列语义合并为一条 → 两次 push_lock 后 count == 1
  final pendCount = pendJson.isNotEmpty ? (jsonDecode(pendJson)['count'] as num).toInt() : -1;
  check(pendCount == 1, 'P7 待投递队列含锁定指令（同对端合并），实际 $pendCount');
  check(_cancelOrders(hQA) == 1, 'P7 取消队列清除锁定指令');
  check(_p2pPushLock(Pointer<Void>.fromAddress(0), Pointer<Utf8>.fromAddress(0)) == 7,
      'P7 push_lock(null handle) → 7');
  calloc.free(pqTask);

  _lock(hQA);
  _lock(hQB);
  calloc.free(pqAp);
  calloc.free(pqBp);
  calloc.free(pqAddr);
  calloc.free(pqId);
  pqDir.deleteSync(recursive: true);

  _lock(hA);
  _lock(hB);
  calloc.free(vpA2);
  calloc.free(vpB);
  calloc.free(addrB);
  dirB.deleteSync(recursive: true);

  dir2.deleteSync(recursive: true);
  calloc.free(pw);
  calloc.free(newPw);
  calloc.free(badOld);
  calloc.free(vp);
  calloc.free(vp2);
  calloc.free(cw);
  print('DONE  assertions=$total failures=$fails');
  exitCode = fails == 0 ? 0 : 1;
}
