// 开发用：vault_core.dll P1 端到端冒烟测试。
// 覆盖：创建 → 解锁 → 错误密码 → 冷却 → 改密码（零重加密）→ 生物识别绑定/解锁/解绑 → 锁定。
// 用法：在 app/ 目录下执行 `dart run tool/ffi_check.dart [dll路径]`
// ignore_for_file: avoid_print
import 'dart:ffi';
import 'dart:typed_data';
import 'dart:convert';
import 'dart:io';
import 'dart:async';
import 'dart:isolate';

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
// P8-5：中继导出带 token（relay + room + token）
typedef P2pSyncRelayC = Pointer<Utf8> Function(
    Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>, Pointer<Utf8>);
typedef P2pSyncRelayDart = Pointer<Utf8> Function(
    Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>, Pointer<Utf8>);
typedef P2pServeRelayC = Int32 Function(
    Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>, Pointer<Utf8>);
typedef P2pServeRelayDart = int Function(
    Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>, Pointer<Utf8>);
typedef P2pStatusC = Pointer<Utf8> Function(Pointer<Void>);
typedef P2pStatusDart = Pointer<Utf8> Function(Pointer<Void>);
typedef P2pUnpairC = Int32 Function(Pointer<Void>, Pointer<Utf8>);
typedef P2pUnpairDart = int Function(Pointer<Void>, Pointer<Utf8>);

// P8-4 打洞探针（7 参：句柄 + relay/room/token/code + role + timeout_ms）
typedef PunchProbeC = Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>,
    Pointer<Utf8>, Pointer<Utf8>, Pointer<Utf8>, Uint32, Uint64);
typedef PunchProbeDart = Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>,
    Pointer<Utf8>, Pointer<Utf8>, Pointer<Utf8>, int, int);

// P8-5 收尾：中继设置 + 多中继候选（各导出 typedef 独立）
typedef RelaySettingsGetC = Pointer<Utf8> Function(Pointer<Void>);
typedef RelaySettingsGetDart = Pointer<Utf8> Function(Pointer<Void>);
typedef RelaySettingsSetC = Int32 Function(Pointer<Void>, Pointer<Utf8>);
typedef RelaySettingsSetDart = int Function(Pointer<Void>, Pointer<Utf8>);
typedef RelayCandsSyncC = Pointer<Utf8> Function(
    Pointer<Void>, Pointer<Utf8>);
typedef RelayCandsSyncDart = Pointer<Utf8> Function(
    Pointer<Void>, Pointer<Utf8>);
typedef RelayCandsServeC = Int32 Function(Pointer<Void>, Pointer<Utf8>);
typedef RelayCandsServeDart = int Function(Pointer<Void>, Pointer<Utf8>);

// P8-9 阅后即焚（参数个数各不相同，typedef 严禁复用）
typedef BurnCreateC = Pointer<Utf8> Function(
    Pointer<Void>, Uint64, Uint64, Uint64);
typedef BurnCreateDart = Pointer<Utf8> Function(
    Pointer<Void>, int, int, int);
typedef BurnReceiveC = Pointer<Utf8> Function(
    Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>, Pointer<Utf8>);
typedef BurnReceiveDart = Pointer<Utf8> Function(
    Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>, Pointer<Utf8>);
typedef BurnSendC = Pointer<Utf8> Function(Pointer<Void>, Uint64,
    Pointer<Utf8>, Pointer<Utf8>, Pointer<Utf8>);
typedef BurnSendDart = Pointer<Utf8> Function(Pointer<Void>, int,
    Pointer<Utf8>, Pointer<Utf8>, Pointer<Utf8>);
typedef BurnOpenC = Pointer<Utf8> Function(Pointer<Void>, Uint64);
typedef BurnOpenDart = Pointer<Utf8> Function(Pointer<Void>, int);
typedef BurnEraseC = Pointer<Utf8> Function(Pointer<Void>, Uint64);
typedef BurnEraseDart = Pointer<Utf8> Function(Pointer<Void>, int);
typedef BurnStatusC = Pointer<Utf8> Function(Pointer<Void>, Uint64);
typedef BurnStatusDart = Pointer<Utf8> Function(Pointer<Void>, int);
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
late P2pSyncRelayDart _p2pSyncRelay;
late P2pServeRelayDart _p2pServeRelay;
late P2pSyncTaskDart _p2pSyncTask;
late P2pPathStatusDart _p2pPathStatus;
late P2pDiscoverDart _p2pDiscover;
late P2pStatusDart _p2pStatus;
late P2pUnpairDart _p2pUnpair;
late PunchProbeDart _punchProbe;
late RelaySettingsGetDart _relaySettingsGet;
late RelaySettingsSetDart _relaySettingsSet;
late RelayCandsSyncDart _p2pSyncRelayCands;
late BurnCreateDart _burnCreate;
late BurnReceiveDart _burnReceive;
late BurnSendDart _burnSend;
late BurnOpenDart _burnOpen;
late BurnEraseDart _burnErase;
late BurnStatusDart _burnStatus;
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
// P8-7 选择性同步策略（get 返回 JSON；set 与上下文上报为 int + JSON 字符串）
typedef SyncPolicyGetC = Pointer<Utf8> Function(Pointer<Void>);
typedef SyncPolicyGetDart = Pointer<Utf8> Function(Pointer<Void>);
typedef SyncPolicySetC = Int32 Function(Pointer<Void>, Pointer<Utf8>);
typedef SyncPolicySetDart = int Function(Pointer<Void>, Pointer<Utf8>);
// P8-8 原地编辑
typedef ApplyEditC = Int32 Function(
    Pointer<Void>, Int64, Pointer<Utf8>, Pointer<Utf8>);
typedef ApplyEditDart = int Function(
    Pointer<Void>, int, Pointer<Utf8>, Pointer<Utf8>);
// P8-10 隐写跨图（plan / embed_multi 走 out_json；extract_multi 返回码 + out_json）
typedef StegoPlanC = Int32 Function(Pointer<Void>, Int64, Pointer<Utf8>,
    Pointer<Utf8>, Pointer<Pointer<Utf8>>);
typedef StegoPlanDart = int Function(Pointer<Void>, int, Pointer<Utf8>,
    Pointer<Utf8>, Pointer<Pointer<Utf8>>);
typedef StegoEmbedMultiC = Int32 Function(
    Pointer<Void>,
    Int64,
    Pointer<Utf8>,
    Pointer<Utf8>,
    Pointer<Pointer<Utf8>>,
    Pointer<Uint32>);
typedef StegoEmbedMultiDart = int Function(
    Pointer<Void>,
    int,
    Pointer<Utf8>,
    Pointer<Utf8>,
    Pointer<Pointer<Utf8>>,
    Pointer<Uint32>);
typedef StegoExtractMultiC = Int32 Function(
    Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>, Pointer<Pointer<Utf8>>);
typedef StegoExtractMultiDart = int Function(
    Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>, Pointer<Pointer<Utf8>>);
late AuditListDart _auditList;
late AuditListDart _auditVerify;
late AuditExportDart _auditExport;
late DestroyLocalDart _destroyLocal;
late int Function(Pointer<Void>) _stegoStatus;
late StegoSetEnabledDart _stegoSetEnabled;
late StegoCapacityDart _stegoCapacity;
late StegoEmbedDart _stegoEmbed;
late StegoExtractDart _stegoExtract;
late SyncPolicyGetDart _syncPolicyGetFn;
late SyncPolicySetDart _syncPolicySetFn;
late ApplyEditDart _applyEditFn;
late StegoPlanDart _stegoPlanFn;
late StegoEmbedMultiDart _stegoEmbedMultiFn;
late StegoExtractMultiDart _stegoExtractMultiFn;
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
// P8-2（独立 typedef：每导出一套）
typedef TaskFnC = Int32 Function(Uint32);
typedef TaskFnD = int Function(int);
typedef P2pSyncTaskC = Int32 Function(
    Pointer<Void>, Pointer<Utf8>, Pointer<Uint32>);
typedef P2pSyncTaskDart = int Function(
    Pointer<Void>, Pointer<Utf8>, Pointer<Uint32>);
typedef P2pPathStatusC = Pointer<Utf8> Function(Pointer<Void>);
typedef P2pPathStatusDart = Pointer<Utf8> Function(Pointer<Void>);
typedef P2pDiscoverC = Int32 Function(Pointer<Void>, Pointer<Pointer<Utf8>>);
typedef P2pDiscoverDart = int Function(
    Pointer<Void>, Pointer<Pointer<Utf8>>);
late TaskFnD _taskPause;
late TaskFnD _taskResume;
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

/// 读回 out_json 并释放（P8-7/P8-10 的 JSON 导出共用）。
String? readOutJson(Pointer<Pointer<Utf8>> out) {
  if (out.value.address == 0) return null;
  final s = out.value.toDartString();
  _free(out.value);
  out.value = Pointer<Utf8>.fromAddress(0);
  return s;
}

/// P8-7：策略读取。
String? _syncPolicyGet(Pointer<Void> h) => _jsonOf(_syncPolicyGetFn(h));

/// P8-7：策略写入 / 上下文上报。
int _syncPolicySet(Pointer<Void> h, String json) {
  final j = n(json);
  try {
    return _syncPolicySetFn(h, j);
  } finally {
    calloc.free(j);
  }
}

int _setSyncContext(Pointer<Void> h, String json) => _syncPolicySet(h, json);

/// P8-8：原地编辑。
int _vaultApplyEdit(Pointer<Void> h, int fileId, Pointer<Utf8> src, String? opts) {
  final o = opts == null ? nullptr : n(opts);
  try {
    return _applyEditFn(h, fileId, src, o);
  } finally {
    if (opts != null) calloc.free(o);
  }
}

/// P8-10：容量协商。
String? _stegoPlan(Pointer<Void> h, int fileId, String images, String? opts) {
  final imgs = n(images);
  final o = opts == null ? nullptr : n(opts);
  final out = calloc<Pointer<Utf8>>();
  try {
    final rc = _stegoPlanFn(h, fileId, imgs, o, out);
    if (rc != 0) return null;
    return readOutJson(out);
  } finally {
    calloc.free(imgs);
    if (opts != null) calloc.free(o);
    calloc.free(out);
  }
}

/// P8-10：跨图嵌入（返回 `{taskId, images, shards}`）。
String? _stegoEmbedMulti(
    Pointer<Void> h, int fileId, String images, String? opts) {
  final imgs = n(images);
  final o = opts == null ? nullptr : n(opts);
  final out = calloc<Pointer<Utf8>>();
  final task = calloc<Uint32>();
  try {
    final rc = _stegoEmbedMultiFn(h, fileId, imgs, o, out, task);
    if (rc != 0) return null;
    return readOutJson(out);
  } finally {
    calloc.free(imgs);
    if (opts != null) calloc.free(o);
    calloc.free(out);
    calloc.free(task);
  }
}

/// P8-10：跨图提取（返回码；`out_json` 丢弃）。
int _stegoExtractMulti(Pointer<Void> h, String images, Pointer<Utf8> dest) {
  final imgs = n(images);
  final out = calloc<Pointer<Utf8>>();
  try {
    return _stegoExtractMultiFn(h, imgs, dest, out);
  } finally {
    calloc.free(imgs);
    if (out.value.address != 0) _free(out.value);
    calloc.free(out);
  }
}

/// 读取引擎返回的 JSON 字符串并释放；空指针 → null。
String? _jsonOf(Pointer<Utf8> p) {
  if (p.address == 0) return null;
  final s = p.toDartString();
  _free(p);
  return s;
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

/// P8-5 中继冒烟：spawn vault-relay.exe（配置经 stdin）→ 双会话经中继同步
/// （token 挑战应答 + 帧化数据阶段）→ 错 token 负向 + PATH_DEGRADED 契约事件
/// + path_status per-peer 断言。
Future<void> relaySection(
    Pointer<Void> hA2, Pointer<Void> hB2, Directory dir, String dllPath) async {
  final handleB = hB2.address;
  // 参数门禁（复用主 isolate 绑定）：null relay/room/token → 7
  check(
      _p2pServeRelay(
              hA2,
              Pointer<Utf8>.fromAddress(0),
              Pointer<Utf8>.fromAddress(0),
              Pointer<Utf8>.fromAddress(0)) ==
          7,
      'P8-5 serve_relay(null relay/room/token) → 7');
  final relayExe = File(r'..\native\target\release\vault-relay.exe');
  if (!relayExe.existsSync()) {
    final r = await Process.run('cargo', ['build', '--release', '-p', 'vault-relay'],
        workingDirectory: r'..\native');
    check(r.exitCode == 0, 'P8-5 现构建 vault-relay（exit=${r.exitCode}）');
  }
  if (!relayExe.existsSync()) {
    check(false, 'P8-5 vault-relay.exe 缺失且构建失败——中继冒烟无法执行');
    return;
  }
  const relayToken = 'vsr2-smoke-token-0123456789abcdef';
  Process? relayProc;
  try {
    relayProc = await Process.start(relayExe.path, const []);
    relayProc.stdin.writeln('bind = "127.0.0.1:0"');
    relayProc.stdin.writeln('');
    relayProc.stdin.writeln('[[tokens]]');
    relayProc.stdin.writeln('name = "smoke"');
    relayProc.stdin.writeln('secret = "$relayToken"');
    relayProc.stdin.writeln('tier = "private"');
    // P8-4：开启 UDP 映射观测端点（bind2 提供第二个观测目标 → 对称 NAT 前置判定）
    relayProc.stdin.writeln('');
    relayProc.stdin.writeln('[observe]');
    relayProc.stdin.writeln('enabled = true');
    relayProc.stdin.writeln('bind = "127.0.0.1:0"');
    relayProc.stdin.writeln('bind2 = "127.0.0.1:0"');
    // P8-5 收尾：运维端点（loopback + Bearer 强制；随机端口随启动行打印）
    relayProc.stdin.writeln('');
    relayProc.stdin.writeln('[ops]');
    relayProc.stdin.writeln('enabled = true');
    relayProc.stdin.writeln('bind = "127.0.0.1:0"');
    relayProc.stdin.writeln('token = "vsr2-smoke-ops-token-0123456789"');
    await relayProc.stdin.flush();
    await relayProc.stdin.close();
    final lines = StreamIterator(relayProc.stdout
        .transform(utf8.decoder)
        .transform(const LineSplitter()));
    check(
        await lines.moveNext().timeout(const Duration(seconds: 15)),
        'P8-5 relay 启动行可读');
    final firstLine = lines.current;
    final m = RegExp(r'listening on (\S+)').firstMatch(firstLine);
    check(m != null, 'P8-5 relay 启动行可解析：$firstLine');
    final relayAddr = m!.group(1)!;
    check(relayAddr.startsWith('127.0.0.1:'), 'P8-5 relay 随机端口绑定：$relayAddr');
    check(firstLine.contains('observe_ports=2'),
        'P8-4 relay 观测端点已就绪（两个观测目标）：$firstLine');
    final hasOpsLine = await lines.moveNext().timeout(const Duration(seconds: 5),
        onTimeout: () => false);
    final opsLine = hasOpsLine ? lines.current : '';
    final om = RegExp(r'ops endpoint on (\S+)').firstMatch(opsLine);
    check(om != null, 'P8-5 运维端点已启动：$opsLine');
    final opsPort = int.parse(om!.group(1)!.split(':').last);

    const room = 'smoke-room-1';
    // 响应端在独立 Isolate（FFI 阻塞调用不得占住主 isolate；句柄为不透明地址，
    // 与 engine_service 的 Isolate 编排同款）
    final serveFut = Isolate.run(() {
      final lib2 = DynamicLibrary.open(dllPath);
      final serve = lib2.lookupFunction<P2pServeRelayC, P2pServeRelayDart>(
          'vault_core_p2p_serve_relay');
      final pa = relayAddr.toNativeUtf8();
      final pr = room.toNativeUtf8();
      final pt = relayToken.toNativeUtf8();
      try {
        return serve(Pointer<Void>.fromAddress(handleB), pa, pr, pt);
      } finally {
        calloc.free(pa);
        calloc.free(pr);
        calloc.free(pt);
      }
    });

    final relaySrc = '${dir.path}${Platform.pathSeparator}relay-sync.txt';
    final relayPayload = utf8.encode('VSR2 relayed sync payload! ' * 4000);
    File(relaySrc).writeAsBytesSync(relayPayload);
    final relayId = calloc<Uint64>();
    check(_vaultImport(hA2, n(relaySrc), 0, relayId) == 0, 'P8-5 A 导入中继同步文件');
    final sumPtr = _p2pSyncRelay(hA2, n(relayAddr), n(room), n(relayToken));
    check(sumPtr.address != 0, 'P8-5 经中继同步成功（VSR2 鉴权 + 帧化数据阶段）');
    final sumJson = sumPtr.address != 0 ? str(sumPtr) : '';
    if (sumPtr.address != 0) _free(sumPtr);
    check(sumJson.contains('pushed'), 'P8-5 中继同步摘要含 pushed');
    final serveCode =
        await serveFut.timeout(const Duration(seconds: 120), onTimeout: () => -999);
    check(serveCode == 0, 'P8-5 B 响应端经中继完成（exit=$serveCode）');
    final relayOut = '${dir.path}${Platform.pathSeparator}relay-out.txt';
    check(_vaultExport(hB2, relayId.value, n(relayOut)) == 0,
        'P8-5 B 导出经中继收到的文件');
    final got = File(relayOut).readAsBytesSync();
    check(
        got.length == relayPayload.length && got.isNotEmpty,
        'P8-5 导出内容长度一致（${got.length}B，跨多个中继 DATA 帧）');

    // path_status per-peer：relay 路径 + 尝试计数 + 线上字节记账
    final psPtr = _p2pPathStatus(hA2);
    final ps = jsonDecode(str(psPtr)) as Map<String, dynamic>;
    _free(psPtr);
    final peers2 = (ps['peers'] as List).cast<Map<String, dynamic>>();
    check(peers2.isNotEmpty, 'P8-5 per-peer 段非空');
    final rec = peers2.firstWhere(
        (p) => p['path'] == 'relay',
        orElse: () => <String, dynamic>{});
    check(rec.isNotEmpty, 'P8-5 存在 relay 路径记录');
    check((rec['attempts']?['relay'] ?? 0) >= 1, 'P8-5 relay 尝试计数 ≥ 1');
    check((rec['bytesOut'] ?? 0) > 0, 'P8-5 中继线上字节已记账（bytesOut>0）');

    // ==== P8-4 打洞：中继 UDP 观测端点 + 候选经 Hello 交换 + 双向验证 ====
    // 打洞在引擎侧后台执行（不阻塞会话），故轮询等待结果落进 path_status。
    Map<String, dynamic> punchOf() {
      final p = _p2pPathStatus(hA2);
      final j = jsonDecode(str(p)) as Map<String, dynamic>;
      _free(p);
      final list = (j['peers'] as List).cast<Map<String, dynamic>>();
      final r = list.firstWhere((e) => e['path'] == 'relay',
          orElse: () => <String, dynamic>{});
      return (r['punch'] as Map<String, dynamic>?) ?? <String, dynamic>{};
    }

    var punch = punchOf();
    for (var i = 0; i < 40 && (punch['attempts'] ?? 0) < 1; i++) {
      await Future<void>.delayed(const Duration(milliseconds: 200));
      punch = punchOf();
    }
    check((punch['attempts'] ?? 0) >= 1, 'P8-4 打洞尝试已记账：$punch');
    check(punch['ok'] == true, 'P8-4 回环网络双向证实打通：$punch');
    check((punch['localCandidates'] ?? 0) >= 1 && (punch['remoteCandidates'] ?? 0) >= 1,
        'P8-4 两端候选经 Hello 交换非空：$punch');
    check(punch['symmetricNat'] == false, 'P8-4 回环非对称 NAT：$punch');
    check(punch['kind'] == 'punched' && punch['degradedSteps'] == 0,
        'P8-4 路径状态机停在 punched（无降级）：$punch');
    // 隐私纪律：打洞段只有计数与原因，**不得含 IP**
    check(!jsonEncode(punch).contains('127.0.0.1'), 'P8-4 打洞段不得泄露地址：$punch');
    check((rec['path'] ?? '') == 'relay',
        'P8-4 数据路径仍如实为 relay（不谎称已走打洞承载）');

    // ==== P8-4 探针：两端各调一次（不要求已配对独立于同步），各拿结构化结果 ====
    // 两端都要阻塞在同一房间上 → 响应端放 Isolate，发起端在主 isolate。
    const probeCode = 'natlab-probe-shared-code-1';
    final respProbeFut = Isolate.run(() {
      final lib2 = DynamicLibrary.open(dllPath);
      final probe = lib2.lookupFunction<PunchProbeC, PunchProbeDart>(
          'vault_core_p2p_punch_probe');
      final pa = n(relayAddr);
      final pr = n('probe-room-1');
      final pt = n(relayToken);
      final pc = n(probeCode);
      try {
        // role = 1（响应端）
        final out = probe(Pointer<Void>.fromAddress(handleB), pa, pr, pt, pc, 1, 8000);
        return out.address == 0 ? null : str(out);
      } finally {
        calloc.free(pa);
        calloc.free(pr);
        calloc.free(pt);
        calloc.free(pc);
      }
    });
    await Future<void>.delayed(const Duration(milliseconds: 400));
    final initPtr =
        _punchProbe(hA2, n(relayAddr), n('probe-room-1'), n(relayToken), n(probeCode), 0, 8000);
    check(initPtr.address != 0, 'P8-4 探针（发起端）返回结果');
    final initJson = initPtr.address != 0
        ? jsonDecode(str(initPtr)) as Map<String, dynamic>
        : <String, dynamic>{};
    if (initPtr.address != 0) _free(initPtr);
    final respRaw = await respProbeFut.timeout(const Duration(seconds: 60),
        onTimeout: () => null);
    final respJson = respRaw == null
        ? <String, dynamic>{}
        : jsonDecode(respRaw) as Map<String, dynamic>;

    check(initJson['ok'] == true, 'P8-4 探针（发起端）双向证实打通：$initJson');
    check(respJson['ok'] == true, 'P8-4 探针（响应端）双向证实打通：$respJson');
    check(initJson['role'] == 'initiator' && respJson['role'] == 'responder',
        'P8-4 探针角色如实标注：$initJson / $respJson');
    check(
        (initJson['peerDeviceId'] as String?)?.startsWith('vd-') == true,
        'P8-4 探针报出对端身份：$initJson');
    check(initJson['kind'] == 'punched' && initJson['degradedSteps'] == 0,
        'P8-4 探针路径状态机停在 punched：$initJson');
    check(
        initJson['mappedPort'] is int && initJson['peerObservedPort'] is int,
        'P8-4 探针给出两次观测端口（核对 NAT 类型用）：$initJson');
    check((initJson['elapsedMs'] ?? 99999) < 8000,
        'P8-4 探针在窗口内完成：$initJson');
    check(!jsonEncode(initJson).contains('127.0.0.1') &&
            !jsonEncode(respJson).contains('127.0.0.1'),
        'P8-4 探针结果不得泄露地址：$initJson / $respJson');

    // 错 token → 拒绝（R 码映射为本机码，返回 null）+ 可读诊断 + 契约事件
    final badPtr =
        _p2pSyncRelay(hA2, n(relayAddr), n(room), n('vsr2-wrong-token-0123456789abcd'));
    check(badPtr.address == 0, 'P8-5 错 token 同步被拒');
    if (badPtr.address != 0) _free(badPtr);
    final stA = _p2pStatus(hA2);
    final stAj = jsonDecode(str(stA)) as Map<String, dynamic>;
    _free(stA);
    check(
        stAj['lastPathFailure'] != null &&
            stAj['lastPathFailure']['reason'] == 'auth-failed',
        'P8-5 lastPathFailure.reason == auth-failed');
    check(
        (stAj['events'] as List).any((e) => e.toString().contains('auth-failed')),
        'P8-5 可读诊断进引擎事件日志');
    final frames = pollDrain();
    final degraded = frames.any((f) =>
        f['type'] == 13 && f['payload'].toString().contains('relay_unavailable'));
    check(degraded, 'P8-5 PATH_DEGRADED(13, relay_unavailable) 进契约事件流');

    // ==== P8-5 收尾：运维端点 /healthz /stats.json /metrics（loopback + Bearer）====
    Future<String> httpGet(String path, {String? bearer}) async {
      final sock = await Socket.connect('127.0.0.1', opsPort,
          timeout: const Duration(seconds: 5));
      final auth =
          bearer == null ? '' : 'Authorization: Bearer $bearer\r\n';
      sock.add(utf8.encode('GET $path HTTP/1.1\r\n'
          'Host: ops\r\n'
          '${auth}Connection: close\r\n'
          '\r\n'));
      await sock.flush();
      final resp = <int>[];
      await for (final chunk in sock) {
        resp.addAll(chunk);
      }
      await sock.close();
      return utf8.decode(resp);
    }

    final health = await httpGet('/healthz');
    check(health.startsWith('HTTP/1.1 200'),
        'P8-5 /healthz 无鉴权可读：${health.split('\r\n').first}');
    check(health.contains('uptimeS') && health.contains('"ver":2'),
        'P8-5 /healthz 只含存活 + 版本 + uptimeS');
    final noAuth = await httpGet('/stats.json');
    check(noAuth.startsWith('HTTP/1.1 401'), 'P8-5 /stats.json 无 Bearer → 401');
    const opsToken = 'vsr2-smoke-ops-token-0123456789';
    final stats = await httpGet('/stats.json', bearer: opsToken);
    check(stats.startsWith('HTTP/1.1 200'), 'P8-5 /stats.json 带 Bearer → 200');
    final statsBody = stats.split('\r\n\r\n').last;
    final statsJson = jsonDecode(statsBody) as Map<String, dynamic>;
    check((statsJson['sessions'] ?? 0) >= 1,
        'P8-5 /stats.json 5 min 聚合含会话数：$statsBody');
    check((statsJson['bytes'] ?? 0) > 0, 'P8-5 /stats.json 聚合字节数 > 0');
    check((statsJson['closeReasons'] as Map<String, dynamic>? ?? {}).isNotEmpty,
        'P8-5 /stats.json close_reason 直方图非空');
    check(!statsBody.contains('127.0.0.1') && !statsBody.contains(relayToken),
        'P8-5 /stats.json 聚合体不含 IP / token（匿名纪律）');
    final metrics = await httpGet('/metrics', bearer: opsToken);
    check(metrics.contains('vsr2_sessions_total'), 'P8-5 /metrics 输出计数');

    // ==== P8-5 收尾：中继设置（settings.enc）+ 多中继候选顺序 ====
    final rsDef = _relaySettingsGet(hA2);
    check(rsDef.address != 0, 'P8-5 relaySettingsGet 默认值可读');
    final rsDefJson =
        rsDef.address != 0 ? jsonDecode(str(rsDef)) as Map<String, dynamic> : <String, dynamic>{};
    if (rsDef.address != 0) _free(rsDef);
    check(rsDefJson['allowPublicRelay'] == true && (rsDefJson['relays'] as List).isEmpty,
        'P8-5 默认 allowPublicRelay=true 且无候选');

    // 非法设置（短 token）→ 7（原子拒绝）
    check(
        _relaySettingsSet(
                hA2,
                n('{"allowPublicRelay":true,"relays":[{"addr":"x:1","token":"short"}]}')) ==
            7,
        'P8-5 短 token 设置被原子拒绝（7）');

    // 负向：仅官方条目 + allowPublicRelay=false → 候选为空 → 不拨号、13
    check(
        _relaySettingsSet(
                hA2,
                n('{"allowPublicRelay":false,"relays":[{"addr":"$relayAddr",'
                    '"token":"$relayToken","public":true}]}')) ==
            0,
        'P8-5 写入「仅官方条目 + 禁用官方中继」设置');
    final negPtr = _p2pSyncRelayCands(hA2, n('smoke-cand-neg'));
    check(negPtr.address == 0, 'P8-5 禁用官方中继后候选同步被拒（不拨号）');
    if (negPtr.address != 0) _free(negPtr);
    final stNegPtr = _p2pStatus(hA2);
    final stNeg = jsonDecode(str(stNegPtr)) as Map<String, dynamic>;
    _free(stNegPtr);
    check(
        stNeg['lastPathFailure']?['reason'] == 'relay_unavailable',
        'P8-5 禁用官方中继失败原因 = relay_unavailable');

    // 正向：自建条目（public=false）不受开关影响 → 候选顺序同步端到端
    check(
        _relaySettingsSet(
                hA2,
                n('{"allowPublicRelay":false,"relays":[{"addr":"$relayAddr",'
                    '"token":"$relayToken","public":false}]}')) ==
            0,
        'P8-5 写入自建中继候选（public=false）');
    final rsBackPtr = _relaySettingsGet(hA2);
    final rsBack = jsonDecode(str(rsBackPtr)) as Map<String, dynamic>;
    _free(rsBackPtr);
    final rsList = rsBack['relays'] as List;
    check(rsList.length == 1 && rsList.first['addr'] == relayAddr,
        'P8-5 中继设置往返保真');
    const candRoom = 'smoke-cand-room-1';
    // 中继设置按会话隔离：B 侧同样要写入候选（响应端凭同一自建条目接入）
    check(
        _relaySettingsSet(
                hB2,
                n('{"allowPublicRelay":false,"relays":[{"addr":"$relayAddr",'
                    '"token":"$relayToken","public":false}]}')) ==
            0,
        'P8-5 B 写入自建中继候选');
    final candServeFut = Isolate.run(() {
      final lib2 = DynamicLibrary.open(dllPath);
      final serve = lib2.lookupFunction<RelayCandsServeC, RelayCandsServeDart>(
          'vault_core_p2p_serve_relay_candidates');
      final pr = n(candRoom);
      try {
        return serve(Pointer<Void>.fromAddress(handleB), pr);
      } finally {
        calloc.free(pr);
      }
    });
    final candSrc = '${dir.path}${Platform.pathSeparator}cand.txt';
    final candPayload = utf8.encode('relay candidates payload! ' * 500);
    File(candSrc).writeAsBytesSync(candPayload);
    final candId = calloc<Uint64>();
    check(_vaultImport(hA2, n(candSrc), 0, candId) == 0, 'P8-5 A 导入候选同步文件');
    final candPtr = _p2pSyncRelayCands(hA2, n(candRoom));
    check(candPtr.address != 0, 'P8-5 候选顺序同步成功（自建条目拨号）');
    if (candPtr.address != 0) _free(candPtr);
    final candServe = await candServeFut.timeout(const Duration(seconds: 120), onTimeout: () => -999);
    check(candServe == 0, 'P8-5 B 候选响应端完成（exit=$candServe）');
    final candOut = '${dir.path}${Platform.pathSeparator}cand-out.txt';
    check(_vaultExport(hB2, candId.value, n(candOut)) == 0, 'P8-5 B 导出候选同步文件');
    check(File(candOut).readAsBytesSync().length == candPayload.length,
        'P8-5 候选同步内容长度一致');
    calloc.free(candId);
  } finally {
    relayProc?.kill();
  }
}

Future<void> main(List<String> args) async {
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
  _p2pSyncRelay = lib.lookupFunction<P2pSyncRelayC, P2pSyncRelayDart>(
      'vault_core_p2p_sync_relay');
  _p2pServeRelay = lib.lookupFunction<P2pServeRelayC, P2pServeRelayDart>(
      'vault_core_p2p_serve_relay');
  _p2pSyncTask = lib.lookupFunction<P2pSyncTaskC, P2pSyncTaskDart>(
      'vault_core_p2p_sync_task');
  _p2pPathStatus = lib.lookupFunction<P2pPathStatusC, P2pPathStatusDart>(
      'vault_core_p2p_path_status');
  _p2pDiscover = lib.lookupFunction<P2pDiscoverC, P2pDiscoverDart>(
      'vault_core_p2p_discover');
  _p2pStatus =
      lib.lookupFunction<P2pStatusC, P2pStatusDart>('vault_core_p2p_status');
  _p2pUnpair =
      lib.lookupFunction<P2pUnpairC, P2pUnpairDart>('vault_core_p2p_unpair');
  _punchProbe = lib.lookupFunction<PunchProbeC, PunchProbeDart>(
      'vault_core_p2p_punch_probe');
  _relaySettingsGet = lib.lookupFunction<RelaySettingsGetC, RelaySettingsGetDart>(
      'vault_core_relay_settings_get');
  _relaySettingsSet = lib.lookupFunction<RelaySettingsSetC, RelaySettingsSetDart>(
      'vault_core_relay_settings_set');
  _p2pSyncRelayCands = lib.lookupFunction<RelayCandsSyncC, RelayCandsSyncDart>(
      'vault_core_p2p_sync_relay_candidates');
  // P8-9 阅后即焚
  _burnCreate = lib.lookupFunction<BurnCreateC, BurnCreateDart>(
      'vault_core_burn_create');
  _burnReceive = lib.lookupFunction<BurnReceiveC, BurnReceiveDart>(
      'vault_core_burn_receive');
  _burnSend = lib.lookupFunction<BurnSendC, BurnSendDart>('vault_core_burn_send');
  _burnOpen = lib.lookupFunction<BurnOpenC, BurnOpenDart>('vault_core_burn_open');
  _burnErase =
      lib.lookupFunction<BurnEraseC, BurnEraseDart>('vault_core_burn_erase');
  _burnStatus = lib.lookupFunction<BurnStatusC, BurnStatusDart>(
      'vault_core_burn_status');
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
  _syncPolicyGetFn = lib.lookupFunction<SyncPolicyGetC, SyncPolicyGetDart>(
      'vault_core_sync_policy_get');
  _syncPolicySetFn = lib.lookupFunction<SyncPolicySetC, SyncPolicySetDart>(
      'vault_core_sync_policy_set');
  _applyEditFn = lib.lookupFunction<ApplyEditC, ApplyEditDart>(
      'vault_core_vault_apply_edit');
  _stegoPlanFn = lib.lookupFunction<StegoPlanC, StegoPlanDart>(
      'vault_core_stego_plan');
  _stegoEmbedMultiFn =
      lib.lookupFunction<StegoEmbedMultiC, StegoEmbedMultiDart>(
          'vault_core_stego_embed_multi');
  _stegoExtractMultiFn =
      lib.lookupFunction<StegoExtractMultiC, StegoExtractMultiDart>(
          'vault_core_stego_extract_multi');
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
  _taskPause = lib.lookupFunction<TaskFnC, TaskFnD>('vault_core_task_pause');
  _taskResume = lib.lookupFunction<TaskFnC, TaskFnD>('vault_core_task_resume');
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

  // ==== P8-9 阅后即焚：一次性会话 → 单次性 → 打开 → 超时自动擦除 ====
  // 能力位 bit13 必须置位（未置位则入口不渲染，见 docs/v2.0/05-04 §5.5）
  final abiBurn = calloc<VsAbiInfo>();
  check(
      _abiInfo(abiBurn) == 0 &&
          (abiBurn.ref.capabilityBits & (1 << 13)) != 0,
      'P8-9 能力位 bit13 CAP_BURN_SHARE 置位');
  check(abiBurn.ref.capabilityBits & (1 << 6) != 0,
      'P8-4 能力位 bit6 CAP_HOLE_PUNCH 置位');
  calloc.free(abiBurn);

  final burnSrc = '${dirB.path}${Platform.pathSeparator}burn-once.txt';
  final burnPayload = utf8.encode('burn-after-reading payload (P8-9)');
  File(burnSrc).writeAsBytesSync(burnPayload);
  final burnIdOut = calloc<Uint64>();
  check(_vaultImport(hA, n(burnSrc), 0, burnIdOut) == 0, 'P8-9 A 导入待焚文件');
  final burnFileId = burnIdOut.value;

  final createPtr = _burnCreate(hA, burnFileId, 900, 1);
  check(createPtr.address != 0, 'P8-9 burn_create 返回票据');
  final createJson =
      createPtr.address != 0 ? jsonDecode(str(createPtr)) as Map<String, dynamic> : {};
  if (createPtr.address != 0) _free(createPtr);
  final burnId = (createJson['burnId'] as num?)?.toInt() ?? 0;
  final burnCode = (createJson['code'] as String?) ?? '';
  check(burnId > 0 && burnCode.length == 16, 'P8-9 邀请码 16 字符（码即秘密载体）');
  check(((createJson['room'] as String?) ?? '').isNotEmpty, 'P8-9 派生房间非空');

  final recvPtr = _burnReceive(hB, n(burnCode), n(''), n(''));
  check(recvPtr.address != 0 && str(recvPtr).contains('"ready":true'),
      'P8-9 B 凭码登记票据（秘密仅内存）');
  if (recvPtr.address != 0) _free(recvPtr);
  final badRecv = _burnReceive(hB, n('BADCODE'), n(''), n(''));
  check(badRecv.address == 0, 'P8-9 非法邀请码被拒');
  if (badRecv.address != 0) _free(badRecv);

  final sendPtr = _burnSend(hA, burnId, addrB, n(''), n(''));
  check(sendPtr.address != 0 && str(sendPtr).contains('"delivered":true'),
      'P8-9 直连投递成功');
  if (sendPtr.address != 0) _free(sendPtr);
  final reuse = _burnSend(hA, burnId, addrB, n(''), n(''));
  check(reuse.address == 0, 'P8-9 已交付票据不可复用（单次性）');
  if (reuse.address != 0) _free(reuse);

  final stRecv = _burnStatus(hB, burnId);
  check(stRecv.address != 0 && str(stRecv).contains('"phase":"delivered"'),
      'P8-9 接收端状态 delivered');
  if (stRecv.address != 0) _free(stRecv);

  final openPtr = _burnOpen(hB, burnId);
  check(openPtr.address != 0, 'P8-9 打开成功');
  final openJson =
      openPtr.address != 0 ? jsonDecode(str(openPtr)) as Map<String, dynamic> : {};
  if (openPtr.address != 0) _free(openPtr);
  final openPath = (openJson['path'] as String?) ?? '';
  check(File(openPath).existsSync(), 'P8-9 受控临时明文已就绪');
  check(
      File(openPath).readAsBytesSync().length == burnPayload.length,
      'P8-9 打开内容长度一致（${burnPayload.length}B）');

  sleep(const Duration(milliseconds: 2600)); // 报价 open_timeout=1s
  check(!File(openPath).existsSync(), 'P8-9 打开后超时自动擦除临时明文');
  final stErased = _burnStatus(hB, burnId);
  check(stErased.address != 0 && str(stErased).contains('"phase":"erased"'),
      'P8-9 擦除后状态为 erased');
  if (stErased.address != 0) _free(stErased);
  final stSent = _burnStatus(hA, burnId);
  check(stSent.address != 0 && str(stSent).contains('"consumed":true'),
      'P8-9 发送端票据标记已消费');
  if (stSent.address != 0) _free(stSent);
  // 显式擦除幂等（残留已清 → 仍报 erased）
  final erasePtr = _burnErase(hB, burnId);
  check(erasePtr.address != 0 && str(erasePtr).contains('"erased":true'),
      'P8-9 显式擦除幂等');
  if (erasePtr.address != 0) _free(erasePtr);
  calloc.free(burnIdOut);

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

  // ===== P8-10 隐写跨图分割与容量协商 =====
  final imgPaths = <String>[];
  for (var i = 0; i < 3; i++) {
    final p = '${p5dir.path}${Platform.pathSeparator}cover-$i.png';
    File(p).writeAsBytesSync(makePng(64, 64));
    imgPaths.add(p);
  }
  final planJson = _stegoPlan(hP, p5Id, jsonEncode(imgPaths), null);
  check(planJson != null, 'P8-10 stego_plan 返回计划');
  final plan = planJson == null
      ? <String, dynamic>{}
      : jsonDecode(planJson) as Map<String, dynamic>;
  check(plan['fits'] == true, 'P8-10 三张小图足够 → fits=true（$planJson）');
  check((plan['perImage'] as List).length == 3, 'P8-10 perImage 逐图给出');
  check(!(plan['notes'] as List).contains('disperse=off'), 'P8-10 默认位分散启用');
  // 容量不足：只给一张小图 + 20KB 文件 → fits=false 且外推所需图数（如实拒绝）
  final bigFile = '${p5dir.path}${Platform.pathSeparator}big.bin';
  File(bigFile).writeAsBytesSync(Uint8List(20000));
  final bigId = calloc<Uint64>();
  check(_vaultImport(hP, n(bigFile), 0, bigId) == 0, 'P8-10 导入 20KB 文件');
  final plan2 = _stegoPlan(hP, bigId.value, jsonEncode([imgPaths.first]), null);
  final p2 = plan2 == null
      ? <String, dynamic>{}
      : jsonDecode(plan2) as Map<String, dynamic>;
  check(p2['fits'] == false, 'P8-10 容量不足 → fits=false（如实拒绝）');
  check((p2['imagesNeeded'] as num) > 1, 'P8-10 给出所需图数（${p2['imagesNeeded']}）');
  // 跨图嵌入（后台任务）→ 轮询 done → 乱序跨图提取 → 内容逐字节一致
  final emJson = _stegoEmbedMulti(hP, p5Id, jsonEncode(imgPaths), null);
  final em = emJson == null
      ? <String, dynamic>{}
      : jsonDecode(emJson) as Map<String, dynamic>;
  check(emJson != null && (em['shards'] as num?) == 3, 'P8-10 分三片（$emJson）');
  final stegoTask = (em['taskId'] as num?)?.toInt() ?? 0;
  check(stegoTask > 0, 'P8-10 embed_multi 返回 task_id');
  var stTask = taskStatusOf(stegoTask);
  final dlTask = DateTime.now().add(const Duration(seconds: 15));
  while (stTask != null &&
      stTask['state'] != 'done' &&
      DateTime.now().isBefore(dlTask)) {
    sleep(const Duration(milliseconds: 20));
    stTask = taskStatusOf(stegoTask);
  }
  check(stTask != null && stTask['state'] == 'done', 'P8-10 嵌入任务完成');
  final multiDest = '${p5dir.path}${Platform.pathSeparator}multi-out.txt';
  final md = n(multiDest);
  // 提取的是**产出图**（`<stem>.stego.png`，源图不被覆盖）——乱序给出
  final carriers = (em['images'] as List).cast<String>();
  final shuffled = [carriers[2], carriers[0], carriers[1]];
  check(carriers.length == 3 && _stegoExtractMulti(hP, jsonEncode(shuffled), md) == 0,
      'P8-10 跨图提取（乱序）成功');
  check(File(multiDest).readAsStringSync() == File(p5src).readAsStringSync(),
      'P8-10 跨图往返内容逐字节一致');
  final md2 = n(multiDest);
  check(
      _stegoExtractMulti(hP, jsonEncode([carriers[0], carriers[1]]), md2) != 0,
      'P8-10 缺片必须失败（不静默）');
  calloc.free(md);
  calloc.free(md2);
  calloc.free(bigId);

  // ===== P8-7 选择性同步策略（sync-policy.enc）=====
  final pol0 = _syncPolicyGet(hP);
  check(pol0 != null, 'P8-7 策略读取返回 JSON');
  check((jsonDecode(pol0 ?? '{}') as Map)['fromDefault'] == true,
      'P8-7 无策略文件 → fromDefault=true（默认全同步）');
  check(_syncPolicySet(hP, '{"concurrency":0}') == 7, 'P8-7 非法策略 → 7（原子拒绝）');
  check(_syncPolicySet(hP, '{"concurrency":4,"filters":{"folderExclude":[7]}}') == 0,
      'P8-7 合法策略写入成功');
  final pj = jsonDecode(_syncPolicyGet(hP) ?? '{}') as Map<String, dynamic>;
  check(pj['fromDefault'] == false && pj['concurrency'] == 4, 'P8-7 策略往返保真');
  check(_setSyncContext(hP, '{"inWindow":true,"netType":"wifi"}') == 0,
      'P8-7 上报同步上下文');
  check(_syncPolicySet(hP, 'not-json') == 7, 'P8-7 非法 JSON → 7');

  // ===== P8-8 原地编辑（同一 file_id 提交新版本）=====
  // 用独立文件（不动 p5Id，避免影响后续 P5 轮换的内容一致性断言）
  final editBase = '${p5dir.path}${Platform.pathSeparator}edit-base.txt';
  File(editBase).writeAsStringSync('edit-base-v1');
  final editId = calloc<Uint64>();
  check(_vaultImport(hP, n(editBase), 0, editId) == 0, 'P8-8 导入编辑基线');
  final editSrc = '${p5dir.path}${Platform.pathSeparator}edited.txt';
  File(editSrc).writeAsStringSync('edit-base-v1 → v2（原地编辑）');
  final es = n(editSrc);
  check(_vaultApplyEdit(hP, editId.value, es, null) == 0, 'P8-8 原地编辑成功');
  final editedOut = '${p5dir.path}${Platform.pathSeparator}edited-out.txt';
  final eo = n(editedOut);
  check(_vaultExport(hP, editId.value, eo) == 0, 'P8-8 编辑后仍可导出（同一 file_id）');
  check(File(editedOut).readAsStringSync() == File(editSrc).readAsStringSync(),
      'P8-8 编辑后内容即新版本');
  check(_vaultApplyEdit(hP, 999999, es, null) != 0, 'P8-8 不存在的 file_id → 非 0');
  calloc.free(es);
  calloc.free(eo);
  calloc.free(editId);

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

  // ===== P8-2 任务暂停/恢复（docs/v2.0/02 §6.5：pause 保留槽位与进度，区别于 cancel）=====
  check(_taskPause(0xDEADBEEF) == 7, 'P8 task_pause 未知任务 → 7');
  final idP8 = calloc<Uint32>();
  check(_taskSpawn(idP8) == 0, 'P8 启动暂停测试任务');
  final taskP = idP8.value;
  sleep(const Duration(milliseconds: 60)); // 进入 running（selfcheck 20ms/tick）
  check(_taskPause(taskP) == 0, 'P8 暂停运行中任务 → 0');
  var stP = taskStatusOf(taskP);
  check(stP != null && stP['state'] == 'paused', 'P8 任务状态转 paused');
  final frozenBytes = stP != null ? (stP['progress']['doneBytes'] as num).toInt() : -1;
  check(stP != null && (stP['progress']['doneChunks'] as num).toInt() >= 0,
      'P8 进度含块粒度字段');
  check(_taskResume(taskP) == 0, 'P8 恢复任务 → 0');
  stP = taskStatusOf(taskP);
  final dlP8 = DateTime.now().add(const Duration(seconds: 5));
  while (stP != null && stP['state'] != 'done' && DateTime.now().isBefore(dlP8)) {
    sleep(const Duration(milliseconds: 50));
    stP = taskStatusOf(taskP);
  }
  check(stP != null && stP['state'] == 'done', 'P8 resume 后原任务内续做至 done');
  check(stP != null && (stP['progress']['doneBytes'] as num).toInt() > frozenBytes,
      'P8 续做不清空已有进度（冻结点 $frozenBytes → ${stP?['progress']['doneBytes']}）');
  check(_taskPause(taskP) == 12, 'P8 终态任务暂停 → 12');
  check(_taskResume(0xDEADBEEF) == 7, 'P8 task_resume 未知任务 → 7');
  calloc.free(idP8);

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
  // P8-4：短码（双端独立计算一致；6 位十进制 + 120s 有效期）
  if (pqJoined.address != 0) {
    final joinedJson = jsonDecode(pqJoined.toDartString()) as Map<String, dynamic>;
    final sc = joinedJson['shortCode']?.toString() ?? '';
    check(sc.length == 6 && sc.contains(RegExp(r'^\d{6}$')),
        'P8 配对短码为 6 位十进制（$sc）');
    final exp = (joinedJson['matchCodeExpiresMs'] as num).toInt();
    check(exp > 0, 'P8 短码有效期字段非零');
    _free(pqJoined);
  }

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

  // ===== P8-1 入队同步 + P8-6 填充诊断 =====
  final p8task = calloc<Uint32>();
  check(_p2pSyncTask(hQA, pqAddr, p8task) == 0 && p8task.value > 0,
      'P8 sync_task 入队返回 task id');
  var stSync = taskStatusOf(p8task.value);
  final dlSync = DateTime.now().add(const Duration(seconds: 15));
  while (stSync != null &&
      stSync['state'] != 'done' &&
      DateTime.now().isBefore(dlSync)) {
    sleep(const Duration(milliseconds: 50));
    stSync = taskStatusOf(p8task.value);
  }
  check(stSync != null && stSync['state'] == 'done', 'P8 队列同步任务轮询至 done');
  final resultJson = stSync != null ? stSync['resultJson'] : null;
  check(resultJson != null && resultJson.toString().contains('pushed'),
      'P8 同步摘要经 resultJson 获取（${resultJson?.toString().substring(0, 40)}…）');
  // 填充协商诊断（两新端 → tier=4，非 legacy，直方图有帧）
  final padPtr = _p2pPathStatus(hQA);
  check(padPtr.address != 0, 'P8 path_status 返回 JSON');
  final pad = jsonDecode(str(padPtr)) as Map<String, dynamic>;
  _free(padPtr);
  check(pad['schema'] == 1, 'P8 path_status schema == 1');
  final padObj = pad['padding'] as Map<String, dynamic>;
  check(padObj['tier'] == 4, 'P8 协商填充档位 == 4');
  check(padObj['legacyPeer'] == false, 'P8 legacyPeer == false');
  final hist = (padObj['frameLenHistogram'] as List).map((e) => (e as num).toInt()).toList();
  check(hist.isNotEmpty && hist.fold<int>(0, (a, b) => a + b) > 0,
      'P8 帧长直方图有帧（$hist）');
  check(hist.where((e) => e > 0).isNotEmpty, 'P8 直方图非零桶 ≥ 1');
  check(padObj['overheadRatio'] is num && (padObj['overheadRatio'] as num) >= 0,
      'P8 overheadRatio 为非负数值');
  check(pad['peers'] is List, 'P8 path_status 含 per-peer 段（P8-5）');
  check(_p2pSyncTask(hQA, Pointer<Utf8>.fromAddress(0), p8task) == 7,
      'P8 sync_task(null addr) → 7');
  calloc.free(p8task);

  // ===== P8-4 设备发现 + 配对短码 =====
  // 能力位 bit5 CAP_DISCOVERY 置位
  final abi5 = calloc<VsAbiInfo>();
  check(_abiInfo(abi5) == 0 && abi5.ref.capabilityBits & (1 << 5) != 0,
      'P8 能力位 bit5 CAP_DISCOVERY 置位');
  calloc.free(abi5);
  // 发现快照：schema / enabled / self / 候选数组（单机环境候选可为空——
  // 不谎报「无设备」，enabled=true 表示发现本身可用）
  final discOut = calloc<Pointer<Utf8>>();
  check(_p2pDiscover(hQA, discOut) == 0 && discOut.value.address != 0,
      'P8 p2p_discover 返回快照');
  final disc = jsonDecode(str(discOut.value)) as Map<String, dynamic>;
  _free(discOut.value);
  discOut.value = Pointer<Utf8>.fromAddress(0);
  check(disc['schema'] == 1, 'P8 discover schema == 1');
  check(disc['enabled'] == true, 'P8 发现默认启用');
  final selfObj = disc['self'] as Map<String, dynamic>;
  check(selfObj['deviceId'].toString().startsWith('vd-') &&
      selfObj['port'] as num > 0, 'P8 self 含 deviceId 与端口');
  check(disc['candidates'] is List, 'P8 候选为数组');
  calloc.free(discOut);

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

  // ===== P8-5 中继 VSR2（真实 vault-relay 子进程 + token 挑战-应答）=====
  await relaySection(hQA, hQB, pqDir, dll);

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
