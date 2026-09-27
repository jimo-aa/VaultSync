/// FFI 绑定层：唯一允许触碰原生内存指针的层（docs/01 §二）。
///
/// P1 密钥体系绑定：原生库由 `app/windows/runner/CMakeLists.txt` 在构建时
/// 经 cargo 编译并拷贝到产物目录；加载失败时由上层回退 Stub。
library;

import 'dart:convert';
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
  // ==== V2.0 扩展码（docs/v2.0/02 §六.6；中继 R1–R17 另有独立错误空间，不得原样透传）====
  static const leaseBusy = 9;
  static const maintenance = 10;
  static const needsRecovery = 11;
  static const cancelled = 12;
  static const capability = 13;

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
        leaseBusy => '保险箱被他进程/会话占用（已降级为只读）',
        maintenance => '引擎维护中，业务写入暂不可用',
        needsRecovery => '保险箱需要恢复后才能继续',
        cancelled => '操作已取消',
        capability => '当前引擎不支持该能力',
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

  int changePassword(
      Object sessionHandle, String oldPassword, String newPassword);

  void lock(Object sessionHandle);

  int cooldownRemainingMs(String path);

  // ==== P2 保险箱操作（JSON 结果为引擎分配的字符串，由桥接层释放）====

  int vaultMkdir(
      Object sessionHandle, int parent, String name, List<int> idOut);

  String? vaultList(Object sessionHandle, int folder);

  String? vaultSearch(Object sessionHandle, String query);

  int vaultImport(
      Object sessionHandle, String src, int folder, List<int> idOut);

  int vaultExport(Object sessionHandle, int fileId, String dest);

  int vaultRenameFile(Object sessionHandle, int fileId, String name);

  int vaultDeleteFile(Object sessionHandle, int fileId, {bool secure = true});

  /// 重命名文件夹（根目录不可改名）。
  int vaultRenameFolder(Object sessionHandle, int folderId, String name);

  /// 递归删除文件夹；成功返回删除的文件数（≥0），失败返回负状态码。
  int vaultDeleteFolder(Object sessionHandle, int folderId,
      {bool secure = true});

  /// 设置标签（文件与文件夹通用）。
  int vaultSetTags(Object sessionHandle, int targetId, List<String> tags);

  String? vaultShareCreate(
      Object sessionHandle, int fileId, int ttlSecs, int maxOpens);

  int vaultShareOpen(
      Object sessionHandle, int shareId, String token, String dest);

  // ==== P3 P2P 同步（JSON 结果为引擎分配的字符串，由桥接层释放）====

  String? p2pPairBegin(Object sessionHandle);

  String? p2pPairJoin(Object sessionHandle, String addr, String code);

  String? p2pSync(Object sessionHandle, String addr);

  /// P8-5：中继 token（≥128 位；合规中继必拒短 token）。
  String? p2pSyncRelay(
      Object sessionHandle, String relay, String room, String token);

  int p2pServeRelay(
      Object sessionHandle, String relay, String room, String token);

  // ==== P8-5 收尾：中继设置 + 多中继候选顺序（docs/08 §3.6）====

  /// 中继设置读取（settings.enc 载体）：`{allowPublicRelay, relays:[{addr, token, public}]}`。
  String? relaySettingsGet(Object sessionHandle);

  /// 中继设置写入（原子拒绝：任一条不合法整体拒绝）。
  int relaySettingsSet(Object sessionHandle, String json);

  /// 多中继候选顺序同步（候选取自设置；`allowPublicRelay=false` 时官方条目拨号前剔除）。
  String? p2pSyncRelayCandidates(Object sessionHandle, String room);

  int p2pServeRelayCandidates(Object sessionHandle, String room);

  String? p2pStatus(Object sessionHandle);

  /// P8-7：选择性同步策略（`sync-policy.enc`）读取；返回 JSON（含 `fromDefault`）。
  String? syncPolicyGet(Object sessionHandle);

  /// P8-7：写入策略（非法 → 7 原子拒绝；只读 9 / 维护 10）。
  int syncPolicySet(Object sessionHandle, String json);

  /// P8-7：上报同步上下文 `{"inWindow":bool,"netType":"wifi"}`（出站前复核用）。
  int setSyncContext(Object sessionHandle, String json);

  /// P8-8：原地编辑（同一 file_id 提交新版本；rev 递增，旧版本保留为合并基线）。
  int vaultApplyEdit(
      Object sessionHandle, int fileId, String src, String? optsJson);

  /// P8-10：隐写容量协商（**容量唯一真值来源**）。返回 JSON（含 fits/imagesNeeded）。
  String? stegoPlan(
      Object sessionHandle, int fileId, String imagesJson, String? optsJson);

  /// P8-10：跨图嵌入（后台任务，可 task_cancel）。返回 JSON `{taskId, images, shards}`。
  String? stegoEmbedMulti(
      Object sessionHandle, int fileId, String imagesJson, String? optsJson);

  /// P8-10：跨图提取（乱序可；缺片 → null 并诊断）。返回 JSON `{name, size, shards}`。
  String? stegoExtractMulti(
      Object sessionHandle, String imagesJson, String dest);

  /// 解除与指定设备的配对（幂等：该设备本就不存在亦成功）。
  int p2pUnpair(Object sessionHandle, String deviceId);

  /// 为每个已配对设备签发并入队销毁指令；返回 JSON {"queued","peers"}。
  String? destroyQueueAll(Object sessionHandle, int delaySecs);

  /// 待投递队列快照 JSON {"count","items"}。
  String? pendingOrders(Object sessionHandle);

  /// 清空待投递队列，返回清除条数（负值为错误码）。
  int cancelOrders(Object sessionHandle);

  /// 记录 / 更新对端监听地址（配对时自动记录；手填兜底）。
  int rememberPeerAddr(Object sessionHandle, String deviceId, String addr);

  // ==== P5-1 审计日志 / P5-5 紧急销毁（JSON 结果为引擎分配字符串）====

  /// 审计条目列表 JSON：{"count","head","entries":[{seq,tsMs,kind,detail,prevHash,hash}]}。
  String? auditList(Object sessionHandle);

  /// 链式校验 JSON：{"ok","checked","brokenAt","reason"}。
  String? auditVerify(Object sessionHandle);

  /// 加密导出审计日志到 dest（导出事件本身也记审计）。
  int auditExport(Object sessionHandle, String dest);

  /// 紧急销毁 · 本机：销毁保险箱头部与整个数据目录；secure=1 先单次覆写。
  int destroyLocal(Object sessionHandle, {bool secure = true});

  /// 全库完整性校验 JSON：{"files","checked","failed":[{id,name,reason}]}。
  String? vaultVerifyAll(Object sessionHandle);

  /// 单文件 FSKey 轮换（docs/07 §二）：其余文件不受影响。
  int rotateFileKey(Object sessionHandle, int fileId);

  /// 文件夹 FSK 轮换：新 FSK 覆盖 + 重写该文件夹直属文件容器。
  int rotateFolderKeys(Object sessionHandle, int folderId);

  /// MK 全库轮换（需主密码重新包装；成功后必须立即 lock 并重新解锁）。
  int rotateMk(Object sessionHandle, String password);

  /// 校验更新清单签名（覆盖清单原始字节），返回 {"ok":bool}。
  Map<String, dynamic>? verifyUpdateManifest(
      String manifest, String sigHex, String publicKeyHex);

  /// 文件 SHA-256（hex）；失败返回 null。更新包下载后复核用。
  String? fileSha256(String path);

  // ==== P5-3 隐写术（docs/05-05）====

  /// 1 = 已启用，0 = 未启用，负值为错误码。
  int stegoStatus(Object sessionHandle);

  /// 启用 / 停用隐写引擎（状态变更记审计）。
  int stegoSetEnabled(Object sessionHandle, bool on);

  /// 图片可嵌入载荷字节数；负值为错误码（-3 IO / -6 格式）。
  int stegoCapacity(Object sessionHandle, String imagePath);

  /// 把保险箱内文件的明文加密后嵌入图片，写出到 outPath。
  int stegoEmbed(
      Object sessionHandle, int fileId, String imagePath, String outPath);

  /// 从图片提取并解密写出到 destPath，返回 JSON {"name","size"}。
  String? stegoExtract(Object sessionHandle, String imagePath, String destPath);

  String? p2pDestroyArm(
      Object sessionHandle, String addr, String target, int delaySecs);

  int p2pDestroyCancel(Object sessionHandle);

  int p2pConflictResolve(Object sessionHandle, int keepId, int dropId,
      {bool secure = false});

  // ==== P6 ABI 握手 / 事件流 / 任务框架 / 租约（docs/v2.0/02 §6.2–§6.5）====

  /// ABI v2 握手：调用一次并解析 VsAbiInfo。
  ///
  /// 返回键：structSize / abiVersion / capabilityBits / maxWriteVerVault /
  /// maxWriteVerContainer / maxWriteVerIndex / maxWriteVerAudit /
  /// minReadVerVault / featureFlags / engineVersion。
  /// 返回 null 表示调用失败（握手必须先于一切业务导出）。
  Map<String, dynamic>? abiInfo();

  /// 订阅引擎事件流，返回订阅 id（失败返回负状态码）。
  ///
  /// Dart 侧仅注册原生回调把事件投递回 isolate（内容以 pollEvents 轮询为准）；
  /// 推送通道到 UI 外壳的接线属 P6 后续工作，本层不承诺回调语义。
  int subscribe();

  /// 取消订阅；0 = 成功，7 = 未知订阅 id。
  void unsubscribe(int subId);

  /// 拉取并清空缓冲事件（轮询为主通道）。每帧含
  /// type / seq / tsMs / taskId / payload（JSON 对象）。
  List<Map<String, dynamic>> pollEvents();

  /// 任务列表 JSON：{"count","tasks":[{id,kind,state,doneBytes,totalBytes}]}。
  List<Map<String, dynamic>> taskList();

  /// 单任务状态，字段同 taskList 条目；未知 id 返回 null。
  Map<String, dynamic>? taskStatus(int id);

  /// 协作式取消：0 = 已请求 / 7 = 未知 / 12 = 已取消（幂等）。
  int taskCancel(int id);

  /// 任务暂停（P8-2）：下一检查点转 paused，槽位与已收块保留。
  /// 0 / 7（未知）/ 12（状态不允许）。
  int taskPause(int id);

  /// 任务恢复（P8-2）：paused → 原任务内续做。0 / 7 / 12。
  int taskResume(int id);

  /// 启动诊断自检任务（约 0.5 s，进度经 TASK_PROGRESS 事件上报），返回任务 id。
  int taskSpawnSelfcheck();

  /// 会话只读状态：1 = 只读（他进程/他会话持租约），0 = 可写；
  /// 负值为错误码。
  int sessionReadonly(Object sessionHandle);

  /// 缩略图能力探针：当前恒返回 13（CAP_THUMBNAIL 未置位，编解码归 P9）。
  int vaultThumbnailProbe(Object sessionHandle, int fileId);

  // ==== P7 轮换会话 / 传播 / 擦除分级 / 迁移 / 检索 V2（docs/v2.0/09）====

  /// MK 轮换会话 · begin（同步执行，偏差记 LOG）：out 末位为 task id（恒 0）。
  int rotateMkBegin(Object sessionHandle, String password, List<int> taskIdOut);

  /// MK 轮换会话 · resume（密码派生 KEK 解封 VSRR）：幂等。
  int rotateMkResume(Object sessionHandle, String password);

  /// MK 轮换会话 · 状态 JSON（rotationState / rotationId / autoResumeExhausted…）。
  String? rotateMkStatus(Object sessionHandle);

  /// 远程锁定指令（P7-10）：peerId 传 null 表示全部已配对设备。
  int p2pPushLock(Object sessionHandle, String? peerId);

  /// 轮换传播指令（P7-4，kind=3）：out 末位为 task id（恒 0）。
  int p2pPushRotation(Object sessionHandle, String? peerId, List<int> taskIdOut);

  /// 擦除强度分级探测（P7-5）：JSON
  /// {mediaKind, eraseClass, methodBits, degradations[], secureEraseClaim}。
  String? eraseClassProbe(String path);

  /// V2→V3 迁移 · begin（同步执行，偏差记 LOG）：out 末位为 task id（恒 0）。
  int migrateBegin(Object sessionHandle, String password, List<int> taskIdOut);

  /// V2→V3 迁移 · 状态 JSON：{"state","phase","done","total","failed"}。
  String? migrateStatus(Object sessionHandle);

  /// V2→V3 迁移 · resume（断点续做，密码派生 KEK）。
  int migrateResume(Object sessionHandle, String password);

  /// V2→V3 迁移 · cancel（幂等清理暂存）。
  int migrateCancel(Object sessionHandle);

  /// 检索 V2（P7-9）：optsJson 可为 null（默认 limit=32 / rankVer=1）。
  /// 返回 JSON {"schema":1,"total","rankVer","hits":[…]}；负值为状态码。
  String? vaultSearchV2(Object sessionHandle, String query, String? optsJson);

  /// 发起一次增量同步（P8-1 入队语义）：立即返回 task id（负值为状态码）。
  /// 摘要经 task_status 的 resultJson / TASK_DONE 获取；可 pause/cancel。
  int p2pSyncTask(Object sessionHandle, String addr, List<int> taskIdOut);

  /// 路径与填充诊断（P8-6）：JSON {"padding":{tier,legacyPeer,frameLenHistogram},
  /// "path":{directCandidates,punchedCandidates,relayCandidates}}。
  String? p2pPathStatus(Object sessionHandle);

  /// 局域网设备发现快照（P8-4）：JSON {schema,tsMs,enabled,self,matchCode,
  /// matchCodeExpiresMs,candidates[]}；mDNS 不可用 → enabled=false 不报错。
  String? p2pDiscover(Object sessionHandle);

  /// 按平台返回动态库文件名。
  static String get libraryName {
    if (Platform.isWindows) return 'vault_core.dll';
    if (Platform.isMacOS) return 'libvault_core.dylib';
    if (Platform.isLinux) return 'libvault_core.so';
    throw UnsupportedError(
        'VaultSync 核心引擎暂不支持该平台: ${Platform.operatingSystem}');
  }
}

typedef _HelloC = Int32 Function();
typedef _VersionC = Pointer<Utf8> Function();
typedef _FreeStringC = Void Function(Pointer<Utf8>);
typedef _PathFnC = Int32 Function(Pointer<Utf8>);
typedef _CreateC = Int32 Function(Pointer<Utf8>, Pointer<Utf8>, Int32);
typedef _UnlockC = Int32 Function(Pointer<Utf8>, Pointer<Utf8>, Int32,
    Pointer<Pointer<Void>>, Pointer<Uint64>);
typedef _UnlockBioC = Int32 Function(
    Pointer<Utf8>, Pointer<Pointer<Void>>, Pointer<Uint64>);
typedef _UnlockBioDart = int Function(
    Pointer<Utf8>, Pointer<Pointer<Void>>, Pointer<Uint64>);
typedef _HandleFnC = Int32 Function(Pointer<Void>);
typedef _ChangePwdC = Int32 Function(
    Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>);
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
typedef _RenameFolderC = Int32 Function(Pointer<Void>, Int64, Pointer<Utf8>);
typedef _DeleteFileC = Int32 Function(Pointer<Void>, Int64, Int32);
typedef _DeleteFolderC = Int32 Function(Pointer<Void>, Int64, Int32);
typedef _TagsC = Int32 Function(Pointer<Void>, Int64, Pointer<Utf8>);
typedef _ShareCreateC = Pointer<Utf8> Function(
    Pointer<Void>, Int64, Uint64, Uint32);
typedef _ShareOpenC = Int32 Function(
    Pointer<Void>, Int64, Pointer<Utf8>, Pointer<Utf8>);
// P3 P2P 同步（每函数独立 typedef：参数个数不同严禁复用）
typedef _P2pPairBeginC = Pointer<Utf8> Function(Pointer<Void>);
typedef _P2pStatusC = Pointer<Utf8> Function(Pointer<Void>);
// P8-10 隐写跨图（每函数独立 typedef）
typedef _StegoPlanC = Int32 Function(Pointer<Void>, Int64, Pointer<Utf8>,
    Pointer<Utf8>, Pointer<Pointer<Utf8>>);
typedef _StegoEmbedMultiC = Int32 Function(
    Pointer<Void>,
    Int64,
    Pointer<Utf8>,
    Pointer<Utf8>,
    Pointer<Pointer<Utf8>>,
    Pointer<Uint32>);
typedef _StegoExtractMultiC = Int32 Function(
    Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>, Pointer<Pointer<Utf8>>);
typedef _SyncPolicyGetC = Pointer<Utf8> Function(Pointer<Void>);
typedef _SyncPolicySetC = Int32 Function(Pointer<Void>, Pointer<Utf8>);
// P8-8 原地编辑
typedef _VaultApplyEditC = Int32 Function(
    Pointer<Void>, Int64, Pointer<Utf8>, Pointer<Utf8>);
typedef _P2pUnpairC = Int32 Function(Pointer<Void>, Pointer<Utf8>);
// P5 审计 / 销毁 / 隐写（每函数独立 typedef）
typedef _AuditListC = Pointer<Utf8> Function(Pointer<Void>);
typedef _AuditExportC = Int32 Function(Pointer<Void>, Pointer<Utf8>);
typedef _DestroyLocalC = Int32 Function(Pointer<Void>, Int32);
typedef _VerifyAllC = Pointer<Utf8> Function(Pointer<Void>);
typedef _VerifyManifestC = Pointer<Utf8> Function(
    Pointer<Utf8>, Pointer<Utf8>, Pointer<Utf8>);
typedef _FileSha256C = Pointer<Utf8> Function(Pointer<Utf8>);
typedef _DestroyQueueAllC = Pointer<Utf8> Function(Pointer<Void>, Uint64);
typedef _PendingOrdersC = Pointer<Utf8> Function(Pointer<Void>);
typedef _RememberAddrC = Int32 Function(
    Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>);
typedef _RotateKeyC = Int32 Function(Pointer<Void>, Int64);
typedef _RotateMkC = Int32 Function(Pointer<Void>, Pointer<Utf8>);
typedef _StegoStatusC = Int32 Function(Pointer<Void>);
typedef _StegoSetEnabledC = Int32 Function(Pointer<Void>, Int32);
typedef _StegoCapacityC = Int64 Function(Pointer<Void>, Pointer<Utf8>);
typedef _StegoEmbedC = Int32 Function(
    Pointer<Void>, Int64, Pointer<Utf8>, Pointer<Utf8>);
typedef _StegoExtractC = Pointer<Utf8> Function(
    Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>);
typedef _P2pPairJoinC = Pointer<Utf8> Function(
    Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>);
typedef _P2pSyncC = Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>);
typedef _P2pSyncRelayC = Pointer<Utf8> Function(
    Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>, Pointer<Utf8>);
typedef _P2pServeRelayC = Int32 Function(
    Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>, Pointer<Utf8>);

typedef _RelaySettingsGetC = Pointer<Utf8> Function(Pointer<Void>);

typedef _RelaySettingsSetC = Int32 Function(Pointer<Void>, Pointer<Utf8>);

typedef _P2pRelayCandsSyncC = Pointer<Utf8> Function(
    Pointer<Void>, Pointer<Utf8>);

typedef _P2pRelayCandsServeC = Int32 Function(Pointer<Void>, Pointer<Utf8>);
typedef _P2pDestroyArmC = Pointer<Utf8> Function(
    Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>, Uint64);
typedef _P2pConflictResolveC = Int32 Function(
    Pointer<Void>, Int64, Int64, Int32);
// P6 ABI 握手 / 事件流 / 任务框架 / 租约（每函数独立 typedef：参数个数不同严禁复用）

/// ABI v2 握手结构（与 native/vault-core/src/contract.rs 的 VsAbiInfo 逐字段对齐）。
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

typedef _AbiInfoC = Int32 Function(Pointer<VsAbiInfo>);
typedef _EventCallbackC = Void Function(Pointer<Void>, Pointer<Void>);
typedef _SubscribeC = Int32 Function(
    Pointer<NativeFunction<_EventCallbackC>>, Pointer<Void>, Pointer<Uint32>);
typedef _UnsubscribeC = Int32 Function(Uint32);
typedef _PollEventsC = Int32 Function(Pointer<Pointer<Utf8>>);
typedef _TaskListC = Int32 Function(Pointer<Pointer<Utf8>>);
typedef _TaskStatusC = Int32 Function(Uint32, Pointer<Pointer<Utf8>>);
typedef _TaskCancelC = Int32 Function(Uint32);
typedef _TaskSpawnC = Int32 Function(Pointer<Uint32>);
typedef _SessionReadonlyC = Int32 Function(Pointer<Void>, Pointer<Int32>);
typedef _ThumbnailC = Int32 Function(
    Pointer<Void>, Uint64, Pointer<Pointer<Uint8>>, Pointer<Uint64>);
// P7（每函数独立 typedef：参数个数不同严禁复用）
typedef _RotateMkBeginC = Int32 Function(
    Pointer<Void>, Pointer<Utf8>, Pointer<Uint32>);
typedef _RotateMkResumeC = Int32 Function(Pointer<Void>, Pointer<Utf8>);
typedef _RotateMkStatusC = Pointer<Utf8> Function(Pointer<Void>);
typedef _P2pPushLockC = Int32 Function(Pointer<Void>, Pointer<Utf8>);
typedef _P2pPushRotationC = Int32 Function(
    Pointer<Void>, Pointer<Utf8>, Pointer<Uint32>);
typedef _EraseClassProbeC = Pointer<Utf8> Function(Pointer<Utf8>);
typedef _MigrateBeginC = Int32 Function(
    Pointer<Void>, Pointer<Utf8>, Pointer<Uint32>);
typedef _MigrateStatusC = Pointer<Utf8> Function(Pointer<Void>);
typedef _MigrateResumeC = Int32 Function(Pointer<Void>, Pointer<Utf8>);
typedef _MigrateCancelC = Int32 Function(Pointer<Void>);
typedef _SearchV2C = Int32 Function(Pointer<Void>, Pointer<Utf8>,
    Pointer<Utf8>, Pointer<Pointer<Utf8>>);
typedef _P2pSyncTaskC = Int32 Function(
    Pointer<Void>, Pointer<Utf8>, Pointer<Uint32>);
typedef _P2pPathStatusC = Pointer<Utf8> Function(Pointer<Void>);
typedef _P2pDiscoverC = Int32 Function(Pointer<Void>, Pointer<Pointer<Utf8>>);


class VaultCoreBridgeFfi implements VaultCoreBridge {
  VaultCoreBridgeFfi._(this._lib) {
    _hello = _lib.lookupFunction<_HelloC, int Function()>('vault_core_hello');
    final version = _lib.lookupFunction<_VersionC, Pointer<Utf8> Function()>(
        'vault_core_version');
    final freeString =
        _lib.lookupFunction<_FreeStringC, void Function(Pointer<Utf8>)>(
            'vault_core_free_string');
    _vaultExists = _lib.lookupFunction<_PathFnC, int Function(Pointer<Utf8>)>(
        'vault_core_vault_exists');
    _bioBound = _lib.lookupFunction<_PathFnC, int Function(Pointer<Utf8>)>(
        'vault_core_bio_bound');
    _create = _lib.lookupFunction<
        _CreateC,
        int Function(
            Pointer<Utf8>, Pointer<Utf8>, int)>('vault_core_create_vault');
    _unlock = _lib.lookupFunction<
        _UnlockC,
        int Function(Pointer<Utf8>, Pointer<Utf8>, int, Pointer<Pointer<Void>>,
            Pointer<Uint64>)>('vault_core_unlock');
    _unlockBio = _lib
        .lookupFunction<_UnlockBioC, _UnlockBioDart>('vault_core_unlock_bio');
    _bindBio = _lib.lookupFunction<_HandleFnC, int Function(Pointer<Void>)>(
        'vault_core_bind_bio');
    _unbindBio = _lib.lookupFunction<_HandleFnC, int Function(Pointer<Void>)>(
        'vault_core_unbind_bio');
    _changePassword = _lib.lookupFunction<
        _ChangePwdC,
        int Function(Pointer<Void>, Pointer<Utf8>,
            Pointer<Utf8>)>('vault_core_change_password');
    _lock = _lib.lookupFunction<_LockC, void Function(Pointer<Void>)>(
        'vault_core_lock');
    _cooldown = _lib.lookupFunction<
        _CooldownC,
        int Function(Pointer<Utf8>,
            Pointer<Uint64>)>('vault_core_cooldown_remaining_ms');
    _vaultMkdir = _lib.lookupFunction<
        _MkdirC,
        int Function(Pointer<Void>, int, Pointer<Utf8>,
            Pointer<Uint64>)>('vault_core_vault_mkdir');
    _vaultList =
        _lib.lookupFunction<_ListC, Pointer<Utf8> Function(Pointer<Void>, int)>(
            'vault_core_vault_list');
    _vaultSearch = _lib.lookupFunction<
        _SearchC,
        Pointer<Utf8> Function(
            Pointer<Void>, Pointer<Utf8>)>('vault_core_vault_search');
    _vaultImport = _lib.lookupFunction<
        _ImportC,
        int Function(Pointer<Void>, Pointer<Utf8>, int,
            Pointer<Uint64>)>('vault_core_vault_import');
    _vaultExport = _lib.lookupFunction<
        _ExportC,
        int Function(
            Pointer<Void>, int, Pointer<Utf8>)>('vault_core_vault_export');
    _vaultRenameFile = _lib.lookupFunction<
        _RenameC,
        int Function(
            Pointer<Void>, int, Pointer<Utf8>)>('vault_core_vault_rename_file');
    _vaultRenameFolder = _lib.lookupFunction<
        _RenameFolderC,
        int Function(Pointer<Void>, int,
            Pointer<Utf8>)>('vault_core_vault_rename_folder');
    _vaultDeleteFile = _lib.lookupFunction<_DeleteFileC,
        int Function(Pointer<Void>, int, int)>('vault_core_vault_delete_file');
    _vaultDeleteFolder = _lib
        .lookupFunction<_DeleteFolderC, int Function(Pointer<Void>, int, int)>(
            'vault_core_vault_delete_folder');
    _vaultSetTags = _lib.lookupFunction<
        _TagsC,
        int Function(
            Pointer<Void>, int, Pointer<Utf8>)>('vault_core_vault_set_tags');
    _vaultShareCreate = _lib.lookupFunction<
        _ShareCreateC,
        Pointer<Utf8> Function(
            Pointer<Void>, int, int, int)>('vault_core_vault_share_create');
    _vaultShareOpen = _lib.lookupFunction<
        _ShareOpenC,
        int Function(Pointer<Void>, int, Pointer<Utf8>,
            Pointer<Utf8>)>('vault_core_vault_share_open');
    _p2pPairBegin = _lib.lookupFunction<_P2pPairBeginC,
        Pointer<Utf8> Function(Pointer<Void>)>('vault_core_p2p_pair_begin');
    _p2pPairJoin = _lib.lookupFunction<
        _P2pPairJoinC,
        Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>,
            Pointer<Utf8>)>('vault_core_p2p_pair_join');
    _p2pSync = _lib.lookupFunction<
        _P2pSyncC,
        Pointer<Utf8> Function(
            Pointer<Void>, Pointer<Utf8>)>('vault_core_p2p_sync');
    _p2pSyncRelay = _lib.lookupFunction<
        _P2pSyncRelayC,
        Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>,
            Pointer<Utf8>)>('vault_core_p2p_sync_relay');
    _p2pServeRelay = _lib.lookupFunction<
        _P2pServeRelayC,
        int Function(Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>,
            Pointer<Utf8>)>('vault_core_p2p_serve_relay');
    _relaySettingsGet = _lib.lookupFunction<_RelaySettingsGetC,
        Pointer<Utf8> Function(Pointer<Void>)>('vault_core_relay_settings_get');
    _relaySettingsSet = _lib.lookupFunction<_RelaySettingsSetC,
        int Function(Pointer<Void>, Pointer<Utf8>)>(
        'vault_core_relay_settings_set');
    _p2pSyncRelayCandidates = _lib.lookupFunction<_P2pRelayCandsSyncC,
        Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>)>(
        'vault_core_p2p_sync_relay_candidates');
    _p2pServeRelayCandidates = _lib.lookupFunction<_P2pRelayCandsServeC,
        int Function(Pointer<Void>, Pointer<Utf8>)>(
        'vault_core_p2p_serve_relay_candidates');
    _p2pStatus =
        _lib.lookupFunction<_P2pStatusC, Pointer<Utf8> Function(Pointer<Void>)>(
            'vault_core_p2p_status');
    _syncPolicyGet = _lib.lookupFunction<_SyncPolicyGetC,
        Pointer<Utf8> Function(Pointer<Void>)>('vault_core_sync_policy_get');
    _syncPolicySet = _lib.lookupFunction<_SyncPolicySetC,
        int Function(Pointer<Void>, Pointer<Utf8>)>('vault_core_sync_policy_set');
    _setSyncContext = _lib.lookupFunction<_SyncPolicySetC,
        int Function(Pointer<Void>, Pointer<Utf8>)>('vault_core_set_sync_context');
    _vaultApplyEdit = _lib.lookupFunction<_VaultApplyEditC,
        int Function(Pointer<Void>, int, Pointer<Utf8>, Pointer<Utf8>)>(
        'vault_core_vault_apply_edit');
    _stegoPlan = _lib.lookupFunction<_StegoPlanC,
        int Function(Pointer<Void>, int, Pointer<Utf8>, Pointer<Utf8>,
            Pointer<Pointer<Utf8>>)>('vault_core_stego_plan');
    _stegoEmbedMulti = _lib.lookupFunction<_StegoEmbedMultiC,
        int Function(Pointer<Void>, int, Pointer<Utf8>, Pointer<Utf8>,
            Pointer<Pointer<Utf8>>, Pointer<Uint32>)>(
        'vault_core_stego_embed_multi');
    _stegoExtractMulti = _lib.lookupFunction<_StegoExtractMultiC,
        int Function(Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>,
            Pointer<Pointer<Utf8>>)>('vault_core_stego_extract_multi');
    _auditList =
        _lib.lookupFunction<_AuditListC, Pointer<Utf8> Function(Pointer<Void>)>(
            'vault_core_audit_list');
    _auditVerify =
        _lib.lookupFunction<_AuditListC, Pointer<Utf8> Function(Pointer<Void>)>(
            'vault_core_audit_verify');
    _auditExport = _lib.lookupFunction<_AuditExportC,
        int Function(Pointer<Void>, Pointer<Utf8>)>('vault_core_audit_export');
    _destroyLocal =
        _lib.lookupFunction<_DestroyLocalC, int Function(Pointer<Void>, int)>(
            'vault_core_destroy_local');
    _destroyQueueAll = _lib.lookupFunction<
        _DestroyQueueAllC,
        Pointer<Utf8> Function(
            Pointer<Void>, int)>('vault_core_p2p_destroy_queue_all');
    _pendingOrders = _lib.lookupFunction<_PendingOrdersC,
        Pointer<Utf8> Function(Pointer<Void>)>('vault_core_p2p_pending_orders');
    _cancelOrders =
        _lib.lookupFunction<_HandleFnC, int Function(Pointer<Void>)>(
            'vault_core_p2p_cancel_orders');
    _rememberAddr = _lib.lookupFunction<
        _RememberAddrC,
        int Function(Pointer<Void>, Pointer<Utf8>,
            Pointer<Utf8>)>('vault_core_p2p_remember_addr');
    _rotateFileKey =
        _lib.lookupFunction<_RotateKeyC, int Function(Pointer<Void>, int)>(
            'vault_core_vault_rotate_file_key');
    _rotateFolderKeys =
        _lib.lookupFunction<_RotateKeyC, int Function(Pointer<Void>, int)>(
            'vault_core_vault_rotate_folder_keys');
    _rotateMk = _lib.lookupFunction<_RotateMkC,
        int Function(Pointer<Void>, Pointer<Utf8>)>('vault_core_rotate_mk');
    _verifyManifest = _lib.lookupFunction<
        _VerifyManifestC,
        Pointer<Utf8> Function(Pointer<Utf8>, Pointer<Utf8>,
            Pointer<Utf8>)>('vault_core_verify_update_manifest');
    _fileSha256 = _lib.lookupFunction<_FileSha256C,
        Pointer<Utf8> Function(Pointer<Utf8>)>('vault_core_file_sha256');
    _vaultVerifyAll =
        _lib.lookupFunction<_VerifyAllC, Pointer<Utf8> Function(Pointer<Void>)>(
            'vault_core_vault_verify_all');
    _stegoStatus =
        _lib.lookupFunction<_StegoStatusC, int Function(Pointer<Void>)>(
            'vault_core_stego_status');
    _stegoSetEnabled = _lib.lookupFunction<_StegoSetEnabledC,
        int Function(Pointer<Void>, int)>('vault_core_stego_set_enabled');
    _stegoCapacity = _lib.lookupFunction<
        _StegoCapacityC,
        int Function(
            Pointer<Void>, Pointer<Utf8>)>('vault_core_stego_capacity');
    _stegoEmbed = _lib.lookupFunction<
        _StegoEmbedC,
        int Function(Pointer<Void>, int, Pointer<Utf8>,
            Pointer<Utf8>)>('vault_core_stego_embed');
    _stegoExtract = _lib.lookupFunction<
        _StegoExtractC,
        Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>,
            Pointer<Utf8>)>('vault_core_stego_extract');
    _p2pUnpair = _lib.lookupFunction<_P2pUnpairC,
        int Function(Pointer<Void>, Pointer<Utf8>)>('vault_core_p2p_unpair');
    _p2pDestroyArm = _lib.lookupFunction<
        _P2pDestroyArmC,
        Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>,
            int)>('vault_core_p2p_destroy_arm');
    _p2pDestroyCancel =
        _lib.lookupFunction<_HandleFnC, int Function(Pointer<Void>)>(
            'vault_core_p2p_destroy_cancel');
    _p2pConflictResolve = _lib.lookupFunction<
        _P2pConflictResolveC,
        int Function(
            Pointer<Void>, int, int, int)>('vault_core_p2p_conflict_resolve');
    _abiInfo = _lib.lookupFunction<_AbiInfoC, int Function(Pointer<VsAbiInfo>)>(
        'vault_core_abi_info');
    _subscribe = _lib.lookupFunction<_SubscribeC,
        int Function(Pointer<NativeFunction<_EventCallbackC>>, Pointer<Void>,
            Pointer<Uint32>)>('vault_core_subscribe');
    _unsubscribe = _lib
        .lookupFunction<_UnsubscribeC, int Function(int)>('vault_core_unsubscribe');
    _pollEvents = _lib.lookupFunction<_PollEventsC,
        int Function(Pointer<Pointer<Utf8>>)>('vault_core_poll_events');
    _taskList = _lib.lookupFunction<_TaskListC, int Function(Pointer<Pointer<Utf8>>)>(
        'vault_core_task_list');
    _taskStatus = _lib.lookupFunction<_TaskStatusC,
        int Function(int, Pointer<Pointer<Utf8>>)>('vault_core_task_status');
    _taskCancel =
        _lib.lookupFunction<_TaskCancelC, int Function(int)>('vault_core_task_cancel');
    _taskPause =
        _lib.lookupFunction<_TaskCancelC, int Function(int)>('vault_core_task_pause');
    _taskResume =
        _lib.lookupFunction<_TaskCancelC, int Function(int)>('vault_core_task_resume');
    _taskSpawn = _lib.lookupFunction<_TaskSpawnC, int Function(Pointer<Uint32>)>(
        'vault_core_task_spawn_selfcheck');
    _sessionReadonly = _lib.lookupFunction<_SessionReadonlyC,
        int Function(Pointer<Void>, Pointer<Int32>)>('vault_core_session_readonly');
    _thumbnail = _lib.lookupFunction<_ThumbnailC,
        int Function(Pointer<Void>, int, Pointer<Pointer<Uint8>>,
            Pointer<Uint64>)>('vault_core_vault_thumbnail');
    _rotateMkBegin = _lib.lookupFunction<_RotateMkBeginC,
        int Function(Pointer<Void>, Pointer<Utf8>,
            Pointer<Uint32>)>('vault_core_rotate_mk_begin');
    _rotateMkResume = _lib.lookupFunction<_RotateMkResumeC,
        int Function(Pointer<Void>, Pointer<Utf8>)>('vault_core_rotate_mk_resume');
    _rotateMkStatus = _lib.lookupFunction<_RotateMkStatusC,
        Pointer<Utf8> Function(Pointer<Void>)>('vault_core_rotate_mk_status');
    _p2pPushLock = _lib.lookupFunction<_P2pPushLockC,
        int Function(Pointer<Void>, Pointer<Utf8>)>('vault_core_p2p_push_lock');
    _p2pPushRotation = _lib.lookupFunction<_P2pPushRotationC,
        int Function(Pointer<Void>, Pointer<Utf8>,
            Pointer<Uint32>)>('vault_core_p2p_push_rotation');
    _eraseClassProbe = _lib.lookupFunction<_EraseClassProbeC,
        Pointer<Utf8> Function(Pointer<Utf8>)>('vault_core_erase_class_probe');
    _migrateBegin = _lib.lookupFunction<_MigrateBeginC,
        int Function(Pointer<Void>, Pointer<Utf8>,
            Pointer<Uint32>)>('vault_core_migrate_begin');
    _migrateStatus = _lib.lookupFunction<_MigrateStatusC,
        Pointer<Utf8> Function(Pointer<Void>)>('vault_core_migrate_status');
    _migrateResume = _lib.lookupFunction<_MigrateResumeC,
        int Function(Pointer<Void>, Pointer<Utf8>)>('vault_core_migrate_resume');
    _migrateCancel = _lib.lookupFunction<_MigrateCancelC,
        int Function(Pointer<Void>)>('vault_core_migrate_cancel');
    _searchV2 = _lib.lookupFunction<_SearchV2C,
        int Function(Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>,
            Pointer<Pointer<Utf8>>)>('vault_core_vault_search_v2');
    _p2pSyncTask = _lib.lookupFunction<_P2pSyncTaskC,
        int Function(Pointer<Void>, Pointer<Utf8>,
            Pointer<Uint32>)>('vault_core_p2p_sync_task');
    _p2pPathStatus = _lib.lookupFunction<_P2pPathStatusC,
        Pointer<Utf8> Function(Pointer<Void>)>('vault_core_p2p_path_status');
    _p2pDiscover = _lib.lookupFunction<_P2pDiscoverC,
        int Function(Pointer<Void>,
            Pointer<Pointer<Utf8>>)>('vault_core_p2p_discover');
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
  late final int Function(Pointer<Utf8>, Pointer<Utf8>, int,
      Pointer<Pointer<Void>>, Pointer<Uint64>) _unlock;
  late final _UnlockBioDart _unlockBio;
  late final int Function(Pointer<Void>) _bindBio;
  late final int Function(Pointer<Void>) _unbindBio;
  late final int Function(Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>)
      _changePassword;
  late final void Function(Pointer<Void>) _lock;
  late final int Function(Pointer<Utf8>, Pointer<Uint64>) _cooldown;
  late final int Function(Pointer<Void>, int, Pointer<Utf8>, Pointer<Uint64>)
      _vaultMkdir;
  late final Pointer<Utf8> Function(Pointer<Void>, int) _vaultList;
  late final Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>) _vaultSearch;
  late final int Function(Pointer<Void>, Pointer<Utf8>, int, Pointer<Uint64>)
      _vaultImport;
  late final int Function(Pointer<Void>, int, Pointer<Utf8>) _vaultExport;
  late final int Function(Pointer<Void>, int, Pointer<Utf8>) _vaultRenameFile;
  late final int Function(Pointer<Void>, int, Pointer<Utf8>) _vaultRenameFolder;
  late final int Function(Pointer<Void>, int, int) _vaultDeleteFile;
  late final int Function(Pointer<Void>, int, int) _vaultDeleteFolder;
  late final int Function(Pointer<Void>, int, Pointer<Utf8>) _vaultSetTags;
  late final Pointer<Utf8> Function(Pointer<Void>, int, int, int)
      _vaultShareCreate;
  late final int Function(Pointer<Void>, int, Pointer<Utf8>, Pointer<Utf8>)
      _vaultShareOpen;
  late final Pointer<Utf8> Function(Pointer<Void>) _p2pPairBegin;
  late final Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>)
      _p2pPairJoin;
  late final Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>) _p2pSync;
  late final Pointer<Utf8> Function(
          Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>, Pointer<Utf8>)
      _p2pSyncRelay;
  late final int Function(
          Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>, Pointer<Utf8>)
      _p2pServeRelay;
  late final Pointer<Utf8> Function(Pointer<Void>) _relaySettingsGet;
  late final int Function(Pointer<Void>, Pointer<Utf8>) _relaySettingsSet;
  late final Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>)
      _p2pSyncRelayCandidates;
  late final int Function(Pointer<Void>, Pointer<Utf8>)
      _p2pServeRelayCandidates;
  late final Pointer<Utf8> Function(Pointer<Void>) _p2pStatus;
  late final Pointer<Utf8> Function(Pointer<Void>) _syncPolicyGet;
  late final int Function(Pointer<Void>, Pointer<Utf8>) _syncPolicySet;
  late final int Function(Pointer<Void>, Pointer<Utf8>) _setSyncContext;
  late final int Function(Pointer<Void>, int, Pointer<Utf8>, Pointer<Utf8>)
      _vaultApplyEdit;
  late final int Function(Pointer<Void>, int, Pointer<Utf8>, Pointer<Utf8>,
      Pointer<Pointer<Utf8>>) _stegoPlan;
  late final int Function(Pointer<Void>, int, Pointer<Utf8>, Pointer<Utf8>,
      Pointer<Pointer<Utf8>>, Pointer<Uint32>) _stegoEmbedMulti;
  late final int Function(Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>,
      Pointer<Pointer<Utf8>>) _stegoExtractMulti;
  late final int Function(Pointer<Void>, Pointer<Utf8>) _p2pUnpair;
  late final Pointer<Utf8> Function(Pointer<Void>) _auditList;
  late final Pointer<Utf8> Function(Pointer<Void>) _auditVerify;
  late final int Function(Pointer<Void>, Pointer<Utf8>) _auditExport;
  late final int Function(Pointer<Void>, int) _destroyLocal;
  late final Pointer<Utf8> Function(Pointer<Void>) _vaultVerifyAll;
  late final Pointer<Utf8> Function(Pointer<Utf8>, Pointer<Utf8>, Pointer<Utf8>)
      _verifyManifest;
  late final Pointer<Utf8> Function(Pointer<Utf8>) _fileSha256;
  late final Pointer<Utf8> Function(Pointer<Void>, int) _destroyQueueAll;
  late final Pointer<Utf8> Function(Pointer<Void>) _pendingOrders;
  late final int Function(Pointer<Void>) _cancelOrders;
  late final int Function(Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>)
      _rememberAddr;
  late final int Function(Pointer<Void>, int) _rotateFileKey;
  late final int Function(Pointer<Void>, int) _rotateFolderKeys;
  late final int Function(Pointer<Void>, Pointer<Utf8>) _rotateMk;
  late final int Function(Pointer<Void>) _stegoStatus;
  late final int Function(Pointer<Void>, int) _stegoSetEnabled;
  late final int Function(Pointer<Void>, Pointer<Utf8>) _stegoCapacity;
  late final int Function(Pointer<Void>, int, Pointer<Utf8>, Pointer<Utf8>)
      _stegoEmbed;
  late final Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>)
      _stegoExtract;
  late final Pointer<Utf8> Function(
      Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>, int) _p2pDestroyArm;
  late final int Function(Pointer<Void>) _p2pDestroyCancel;
  late final int Function(Pointer<Void>, int, int, int) _p2pConflictResolve;
  late final int Function(Pointer<VsAbiInfo>) _abiInfo;
  late final int Function(
      Pointer<NativeFunction<_EventCallbackC>>, Pointer<Void>, Pointer<Uint32>)
      _subscribe;
  late final int Function(int) _unsubscribe;
  late final int Function(Pointer<Pointer<Utf8>>) _pollEvents;
  late final int Function(Pointer<Pointer<Utf8>>) _taskList;
  late final int Function(int, Pointer<Pointer<Utf8>>) _taskStatus;
  late final int Function(int) _taskCancel;
  late final int Function(int) _taskPause;
  late final int Function(int) _taskResume;
  late final int Function(Pointer<Uint32>) _taskSpawn;
  late final int Function(Pointer<Void>, Pointer<Int32>) _sessionReadonly;
  late final int Function(
      Pointer<Void>, int, Pointer<Pointer<Uint8>>, Pointer<Uint64>) _thumbnail;
  late final int Function(Pointer<Void>, Pointer<Utf8>, Pointer<Uint32>)
      _rotateMkBegin;
  late final int Function(Pointer<Void>, Pointer<Utf8>) _rotateMkResume;
  late final Pointer<Utf8> Function(Pointer<Void>) _rotateMkStatus;
  late final int Function(Pointer<Void>, Pointer<Utf8>) _p2pPushLock;
  late final int Function(Pointer<Void>, Pointer<Utf8>, Pointer<Uint32>)
      _p2pPushRotation;
  late final Pointer<Utf8> Function(Pointer<Utf8>) _eraseClassProbe;
  late final int Function(Pointer<Void>, Pointer<Utf8>, Pointer<Uint32>)
      _migrateBegin;
  late final Pointer<Utf8> Function(Pointer<Void>) _migrateStatus;
  late final int Function(Pointer<Void>, Pointer<Utf8>) _migrateResume;
  late final int Function(Pointer<Void>) _migrateCancel;
  late final int Function(Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>,
      Pointer<Pointer<Utf8>>) _searchV2;
  late final int Function(Pointer<Void>, Pointer<Utf8>, Pointer<Uint32>)
      _p2pSyncTask;
  late final Pointer<Utf8> Function(Pointer<Void>) _p2pPathStatus;
  late final int Function(
      Pointer<Void>, Pointer<Pointer<Utf8>>) _p2pDiscover;

  /// 引擎事件回调：订阅进程内注册一次、随进程存活（引擎侧为全局事件总线）。
  NativeCallable<_EventCallbackC>? _eventCallable;

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
      return VaultCoreBridgeFfi._(
          DynamicLibrary.open(VaultCoreBridge.libraryName));
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
    int Function(Pointer<Utf8>, Pointer<Utf8>, int, Pointer<Pointer<Void>>,
            Pointer<Uint64>)
        fn,
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
  int changePassword(
      Object sessionHandle, String oldPassword, String newPassword) {
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
  int vaultMkdir(
      Object sessionHandle, int parent, String name, List<int> idOut) {
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
  int vaultImport(
      Object sessionHandle, String src, int folder, List<int> idOut) {
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
  int vaultRenameFolder(Object sessionHandle, int folderId, String name) {
    final n = _toNative(name);
    try {
      return _vaultRenameFolder(_handle(sessionHandle), folderId, n);
    } finally {
      calloc.free(n);
    }
  }

  @override
  int vaultDeleteFolder(Object sessionHandle, int folderId,
      {bool secure = true}) {
    return _vaultDeleteFolder(_handle(sessionHandle), folderId, secure ? 1 : 0);
  }

  @override
  int vaultSetTags(Object sessionHandle, int targetId, List<String> tags) {
    final csv = _toNative(tags.join(','));
    try {
      return _vaultSetTags(_handle(sessionHandle), targetId, csv);
    } finally {
      calloc.free(csv);
    }
  }

  @override
  String? vaultShareCreate(
      Object sessionHandle, int fileId, int ttlSecs, int maxOpens) {
    return _takeJson(
        _vaultShareCreate(_handle(sessionHandle), fileId, ttlSecs, maxOpens));
  }

  @override
  int vaultShareOpen(
      Object sessionHandle, int shareId, String token, String dest) {
    final t = _toNative(token);
    final d = _toNative(dest);
    try {
      return _vaultShareOpen(_handle(sessionHandle), shareId, t, d);
    } finally {
      calloc.free(t);
      calloc.free(d);
    }
  }

  @override
  String? p2pPairBegin(Object sessionHandle) =>
      _takeJson(_p2pPairBegin(_handle(sessionHandle)));

  @override
  String? p2pPairJoin(Object sessionHandle, String addr, String code) {
    final a = _toNative(addr);
    final c = _toNative(code);
    try {
      return _takeJson(_p2pPairJoin(_handle(sessionHandle), a, c));
    } finally {
      calloc.free(a);
      calloc.free(c);
    }
  }

  @override
  String? p2pSync(Object sessionHandle, String addr) {
    final a = _toNative(addr);
    try {
      return _takeJson(_p2pSync(_handle(sessionHandle), a));
    } finally {
      calloc.free(a);
    }
  }

  @override
  int p2pSyncTask(Object sessionHandle, String addr, List<int> taskIdOut) {
    final a = _toNative(addr);
    final taskId = calloc<Uint32>();
    try {
      final status = _p2pSyncTask(_handle(sessionHandle), a, taskId);
      if (taskIdOut.isNotEmpty) taskIdOut[0] = taskId.value;
      return status;
    } finally {
      calloc.free(taskId);
      calloc.free(a);
    }
  }

  @override
  String? p2pPathStatus(Object sessionHandle) =>
      _takeJson(_p2pPathStatus(_handle(sessionHandle)));

  @override
  String? p2pDiscover(Object sessionHandle) {
    final out = calloc<Pointer<Utf8>>();
    try {
      final status = _p2pDiscover(_handle(sessionHandle), out);
      if (status != VaultStatus.ok) return null;
      return _takeJson(out.value);
    } finally {
      calloc.free(out);
    }
  }

  @override
  String? p2pSyncRelay(
      Object sessionHandle, String relay, String room, String token) {
    final r = _toNative(relay);
    final rm = _toNative(room);
    final tk = _toNative(token);
    try {
      return _takeJson(_p2pSyncRelay(_handle(sessionHandle), r, rm, tk));
    } finally {
      calloc.free(r);
      calloc.free(rm);
      calloc.free(tk);
    }
  }

  @override
  int p2pServeRelay(
      Object sessionHandle, String relay, String room, String token) {
    final r = _toNative(relay);
    final rm = _toNative(room);
    final tk = _toNative(token);
    try {
      return _p2pServeRelay(_handle(sessionHandle), r, rm, tk);
    } finally {
      calloc.free(r);
      calloc.free(rm);
      calloc.free(tk);
    }
  }

  @override
  String? relaySettingsGet(Object sessionHandle) =>
      _takeJson(_relaySettingsGet(_handle(sessionHandle)));

  @override
  int relaySettingsSet(Object sessionHandle, String json) {
    final j = _toNative(json);
    try {
      return _relaySettingsSet(_handle(sessionHandle), j);
    } finally {
      calloc.free(j);
    }
  }

  @override
  String? p2pSyncRelayCandidates(Object sessionHandle, String room) {
    final rm = _toNative(room);
    try {
      return _takeJson(_p2pSyncRelayCandidates(_handle(sessionHandle), rm));
    } finally {
      calloc.free(rm);
    }
  }

  @override
  int p2pServeRelayCandidates(Object sessionHandle, String room) {
    final rm = _toNative(room);
    try {
      return _p2pServeRelayCandidates(_handle(sessionHandle), rm);
    } finally {
      calloc.free(rm);
    }
  }

  @override
  String? p2pStatus(Object sessionHandle) =>
      _takeJson(_p2pStatus(_handle(sessionHandle)));

  @override
  String? syncPolicyGet(Object sessionHandle) =>
      _takeJson(_syncPolicyGet(_handle(sessionHandle)));

  @override
  int syncPolicySet(Object sessionHandle, String json) {
    final j = _toNative(json);
    try {
      return _syncPolicySet(_handle(sessionHandle), j);
    } finally {
      calloc.free(j);
    }
  }

  @override
  int setSyncContext(Object sessionHandle, String json) {
    final j = _toNative(json);
    try {
      return _setSyncContext(_handle(sessionHandle), j);
    } finally {
      calloc.free(j);
    }
  }

  @override
  int vaultApplyEdit(
      Object sessionHandle, int fileId, String src, String? optsJson) {
    final s = _toNative(src);
    final o = optsJson == null ? nullptr : _toNative(optsJson);
    try {
      return _vaultApplyEdit(_handle(sessionHandle), fileId, s, o);
    } finally {
      calloc.free(s);
      if (optsJson != null) calloc.free(o);
    }
  }

  @override
  String? stegoPlan(Object sessionHandle, int fileId, String imagesJson,
      String? optsJson) {
    final imgs = _toNative(imagesJson);
    final opts = optsJson == null ? nullptr : _toNative(optsJson);
    final out = calloc<Pointer<Utf8>>();
    try {
      final rc = _stegoPlan(_handle(sessionHandle), fileId, imgs, opts, out);
      if (rc != 0) return null;
      return _takeJson(out.value);
    } finally {
      calloc.free(imgs);
      if (optsJson != null) calloc.free(opts);
      calloc.free(out);
    }
  }

  @override
  String? stegoEmbedMulti(Object sessionHandle, int fileId, String imagesJson,
      String? optsJson) {
    final imgs = _toNative(imagesJson);
    final opts = optsJson == null ? nullptr : _toNative(optsJson);
    final out = calloc<Pointer<Utf8>>();
    final task = calloc<Uint32>();
    try {
      final rc = _stegoEmbedMulti(
          _handle(sessionHandle), fileId, imgs, opts, out, task);
      if (rc != 0) return null;
      return _takeJson(out.value);
    } finally {
      calloc.free(imgs);
      if (optsJson != null) calloc.free(opts);
      calloc.free(out);
      calloc.free(task);
    }
  }

  @override
  String? stegoExtractMulti(Object sessionHandle, String imagesJson, String dest) {
    final imgs = _toNative(imagesJson);
    final d = _toNative(dest);
    final out = calloc<Pointer<Utf8>>();
    try {
      final rc = _stegoExtractMulti(_handle(sessionHandle), imgs, d, out);
      final raw = _takeJson(out.value);
      // 失败（6）时也可能带结构化诊断——调用方按 null 判定失败
      return rc == 0 ? raw : null;
    } finally {
      calloc.free(imgs);
      calloc.free(d);
      calloc.free(out);
    }
  }

  @override
  Map<String, dynamic>? verifyUpdateManifest(
      String manifest, String sigHex, String publicKeyHex) {
    final m = _toNative(manifest);
    final s = _toNative(sigHex);
    final k = _toNative(publicKeyHex);
    try {
      final j = _takeJson(_verifyManifest(m, s, k));
      if (j == null) return null;
      return jsonDecode(j) as Map<String, dynamic>;
    } finally {
      calloc.free(m);
      calloc.free(s);
      calloc.free(k);
    }
  }

  @override
  String? fileSha256(String path) {
    final p = _toNative(path);
    try {
      return _takeJson(_fileSha256(p));
    } finally {
      calloc.free(p);
    }
  }

  @override
  String? destroyQueueAll(Object sessionHandle, int delaySecs) =>
      _takeJson(_destroyQueueAll(_handle(sessionHandle), delaySecs));

  @override
  String? pendingOrders(Object sessionHandle) =>
      _takeJson(_pendingOrders(_handle(sessionHandle)));

  @override
  int cancelOrders(Object sessionHandle) =>
      _cancelOrders(_handle(sessionHandle));

  @override
  int rememberPeerAddr(Object sessionHandle, String deviceId, String addr) {
    final d = _toNative(deviceId);
    final a = _toNative(addr);
    try {
      return _rememberAddr(_handle(sessionHandle), d, a);
    } finally {
      calloc.free(d);
      calloc.free(a);
    }
  }

  @override
  int rotateFileKey(Object sessionHandle, int fileId) =>
      _rotateFileKey(_handle(sessionHandle), fileId);

  @override
  int rotateFolderKeys(Object sessionHandle, int folderId) =>
      _rotateFolderKeys(_handle(sessionHandle), folderId);

  @override
  int rotateMk(Object sessionHandle, String password) {
    final p = _toNative(password);
    try {
      return _rotateMk(_handle(sessionHandle), p);
    } finally {
      calloc.free(p);
    }
  }

  @override
  String? vaultVerifyAll(Object sessionHandle) =>
      _takeJson(_vaultVerifyAll(_handle(sessionHandle)));

  @override
  String? auditList(Object sessionHandle) =>
      _takeJson(_auditList(_handle(sessionHandle)));

  @override
  String? auditVerify(Object sessionHandle) =>
      _takeJson(_auditVerify(_handle(sessionHandle)));

  @override
  int auditExport(Object sessionHandle, String dest) {
    final d = _toNative(dest);
    try {
      return _auditExport(_handle(sessionHandle), d);
    } finally {
      calloc.free(d);
    }
  }

  @override
  int destroyLocal(Object sessionHandle, {bool secure = true}) =>
      _destroyLocal(_handle(sessionHandle), secure ? 1 : 0);

  @override
  int stegoStatus(Object sessionHandle) => _stegoStatus(_handle(sessionHandle));

  @override
  int stegoSetEnabled(Object sessionHandle, bool on) =>
      _stegoSetEnabled(_handle(sessionHandle), on ? 1 : 0);

  @override
  int stegoCapacity(Object sessionHandle, String imagePath) {
    final p = _toNative(imagePath);
    try {
      return _stegoCapacity(_handle(sessionHandle), p);
    } finally {
      calloc.free(p);
    }
  }

  @override
  int stegoEmbed(
      Object sessionHandle, int fileId, String imagePath, String outPath) {
    final i = _toNative(imagePath);
    final o = _toNative(outPath);
    try {
      return _stegoEmbed(_handle(sessionHandle), fileId, i, o);
    } finally {
      calloc.free(i);
      calloc.free(o);
    }
  }

  @override
  String? stegoExtract(
      Object sessionHandle, String imagePath, String destPath) {
    final i = _toNative(imagePath);
    final d = _toNative(destPath);
    try {
      return _takeJson(_stegoExtract(_handle(sessionHandle), i, d));
    } finally {
      calloc.free(i);
      calloc.free(d);
    }
  }

  @override
  int p2pUnpair(Object sessionHandle, String deviceId) {
    final d = _toNative(deviceId);
    try {
      return _p2pUnpair(_handle(sessionHandle), d);
    } finally {
      calloc.free(d);
    }
  }

  @override
  String? p2pDestroyArm(
      Object sessionHandle, String addr, String target, int delaySecs) {
    final a = _toNative(addr);
    final t = _toNative(target);
    try {
      return _takeJson(_p2pDestroyArm(_handle(sessionHandle), a, t, delaySecs));
    } finally {
      calloc.free(a);
      calloc.free(t);
    }
  }

  @override
  int p2pDestroyCancel(Object sessionHandle) =>
      _p2pDestroyCancel(_handle(sessionHandle));

  @override
  int p2pConflictResolve(Object sessionHandle, int keepId, int dropId,
      {bool secure = false}) {
    return _p2pConflictResolve(
        _handle(sessionHandle), keepId, dropId, secure ? 1 : 0);
  }

  @override
  Map<String, dynamic>? abiInfo() {
    final info = calloc<VsAbiInfo>();
    try {
      if (_abiInfo(info) != VaultStatus.ok) return null;
      // engineVersion：NUL 填充的 SemVer 字节，截到首个 0。
      final bytes = <int>[];
      for (var i = 0; i < 32; i++) {
        final b = info.ref.engineVersion[i];
        if (b == 0) break;
        bytes.add(b);
      }
      return <String, dynamic>{
        'structSize': info.ref.structSize,
        'abiVersion': info.ref.abiVersion,
        'capabilityBits': info.ref.capabilityBits,
        'maxWriteVerVault': info.ref.maxWriteVerVault,
        'maxWriteVerContainer': info.ref.maxWriteVerContainer,
        'maxWriteVerIndex': info.ref.maxWriteVerIndex,
        'maxWriteVerAudit': info.ref.maxWriteVerAudit,
        'minReadVerVault': info.ref.minReadVerVault,
        'featureFlags': info.ref.featureFlags,
        'engineVersion': latin1.decode(bytes),
      };
    } finally {
      calloc.free(info);
    }
  }

  /// 原生事件回调空实现：仅满足引擎「非 0 函数指针」要求；
  /// 事件消费走 pollEvents 轮询（推送通道到外壳的接线为 P6 后续工作）。
  static void _onEngineEvent(Pointer<Void> event, Pointer<Void> user) {}

  @override
  int subscribe() {
    final callable = _eventCallable ??=
        NativeCallable<_EventCallbackC>.listener(_onEngineEvent);
    final subId = calloc<Uint32>();
    try {
      final status =
          _subscribe(callable.nativeFunction, Pointer<Void>.fromAddress(0), subId);
      if (status != VaultStatus.ok) return -status;
      return subId.value;
    } finally {
      calloc.free(subId);
    }
  }

  @override
  void unsubscribe(int subId) {
    _unsubscribe(subId);
  }

  @override
  List<Map<String, dynamic>> pollEvents() {
    final out = calloc<Pointer<Utf8>>();
    try {
      if (_pollEvents(out) != VaultStatus.ok) return const [];
      if (out.value.address == 0) return const [];
      final raw = _takeJson(out.value);
      if (raw == null || raw.isEmpty) return const [];
      final list = jsonDecode(raw) as List<dynamic>;
      return list.cast<Map<String, dynamic>>();
    } finally {
      calloc.free(out);
    }
  }

  @override
  List<Map<String, dynamic>> taskList() {
    final out = calloc<Pointer<Utf8>>();
    try {
      if (_taskList(out) != VaultStatus.ok) return const [];
      if (out.value.address == 0) return const [];
      final raw = _takeJson(out.value);
      if (raw == null) return const [];
      final map = jsonDecode(raw) as Map<String, dynamic>;
      return (map['tasks'] as List<dynamic>? ?? const [])
          .cast<Map<String, dynamic>>();
    } finally {
      calloc.free(out);
    }
  }

  @override
  Map<String, dynamic>? taskStatus(int id) {
    final out = calloc<Pointer<Utf8>>();
    try {
      if (_taskStatus(id, out) != VaultStatus.ok) return null;
      if (out.value.address == 0) return null;
      final raw = _takeJson(out.value);
      if (raw == null) return null;
      return jsonDecode(raw) as Map<String, dynamic>;
    } finally {
      calloc.free(out);
    }
  }

  @override
  int taskCancel(int id) => _taskCancel(id);

  @override
  int taskPause(int id) => _taskPause(id);

  @override
  int taskResume(int id) => _taskResume(id);

  @override
  int taskSpawnSelfcheck() {
    final id = calloc<Uint32>();
    try {
      final status = _taskSpawn(id);
      if (status != VaultStatus.ok) return -status;
      return id.value;
    } finally {
      calloc.free(id);
    }
  }

  @override
  int sessionReadonly(Object sessionHandle) {
    final out = calloc<Int32>();
    try {
      final status = _sessionReadonly(_handle(sessionHandle), out);
      if (status != VaultStatus.ok) return -status;
      return out.value;
    } finally {
      calloc.free(out);
    }
  }

  @override
  int vaultThumbnailProbe(Object sessionHandle, int fileId) {
    final png = calloc<Pointer<Uint8>>();
    final len = calloc<Uint64>();
    try {
      return _thumbnail(_handle(sessionHandle), fileId, png, len);
    } finally {
      calloc.free(png);
      calloc.free(len);
    }
  }

  @override
  int rotateMkBegin(
      Object sessionHandle, String password, List<int> taskIdOut) {
    final p = _toNative(password);
    final taskId = calloc<Uint32>();
    try {
      final status = _rotateMkBegin(_handle(sessionHandle), p, taskId);
      if (taskIdOut.isNotEmpty) taskIdOut[0] = taskId.value;
      return status;
    } finally {
      calloc.free(taskId);
      calloc.free(p);
    }
  }

  @override
  int rotateMkResume(Object sessionHandle, String password) {
    final p = _toNative(password);
    try {
      return _rotateMkResume(_handle(sessionHandle), p);
    } finally {
      calloc.free(p);
    }
  }

  @override
  String? rotateMkStatus(Object sessionHandle) =>
      _takeJson(_rotateMkStatus(_handle(sessionHandle)));

  @override
  int p2pPushLock(Object sessionHandle, String? peerId) {
    final pid = peerId == null
        ? Pointer<Utf8>.fromAddress(0)
        : _toNative(peerId);
    try {
      return _p2pPushLock(_handle(sessionHandle), pid);
    } finally {
      if (peerId != null) calloc.free(pid);
    }
  }

  @override
  int p2pPushRotation(
      Object sessionHandle, String? peerId, List<int> taskIdOut) {
    final pid = peerId == null
        ? Pointer<Utf8>.fromAddress(0)
        : _toNative(peerId);
    final taskId = calloc<Uint32>();
    try {
      final status = _p2pPushRotation(_handle(sessionHandle), pid, taskId);
      if (taskIdOut.isNotEmpty) taskIdOut[0] = taskId.value;
      return status;
    } finally {
      calloc.free(taskId);
      if (peerId != null) calloc.free(pid);
    }
  }

  @override
  String? eraseClassProbe(String path) {
    final p = _toNative(path);
    try {
      return _takeJson(_eraseClassProbe(p));
    } finally {
      calloc.free(p);
    }
  }

  @override
  int migrateBegin(Object sessionHandle, String password, List<int> taskIdOut) {
    final p = _toNative(password);
    final taskId = calloc<Uint32>();
    try {
      final status = _migrateBegin(_handle(sessionHandle), p, taskId);
      if (taskIdOut.isNotEmpty) taskIdOut[0] = taskId.value;
      return status;
    } finally {
      calloc.free(taskId);
      calloc.free(p);
    }
  }

  @override
  String? migrateStatus(Object sessionHandle) =>
      _takeJson(_migrateStatus(_handle(sessionHandle)));

  @override
  int migrateResume(Object sessionHandle, String password) {
    final p = _toNative(password);
    try {
      return _migrateResume(_handle(sessionHandle), p);
    } finally {
      calloc.free(p);
    }
  }

  @override
  int migrateCancel(Object sessionHandle) =>
      _migrateCancel(_handle(sessionHandle));

  @override
  String? vaultSearchV2(Object sessionHandle, String query, String? optsJson) {
    final q = _toNative(query);
    final opts =
        optsJson == null ? Pointer<Utf8>.fromAddress(0) : _toNative(optsJson);
    final out = calloc<Pointer<Utf8>>();
    try {
      final status = _searchV2(_handle(sessionHandle), q, opts, out);
      if (status != VaultStatus.ok) return null;
      return _takeJson(out.value);
    } finally {
      calloc.free(out);
      calloc.free(q);
      if (optsJson != null) calloc.free(opts);
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
  bool vaultExists(String path) =>
      throw UnsupportedError('stub: native not linked');

  @override
  bool bioBound(String path) =>
      throw UnsupportedError('stub: native not linked');

  @override
  int createVault(String path, String password, {bool bindBio = false}) =>
      throw UnsupportedError('stub: native not linked');

  @override
  EngineUnlock unlock(String path, String password, {required bool disguise}) =>
      throw UnsupportedError('stub: native not linked');

  @override
  EngineUnlock unlockBio(String path) =>
      throw UnsupportedError('stub: native not linked');

  @override
  int bindBio(Object sessionHandle) =>
      throw UnsupportedError('stub: native not linked');

  @override
  int unbindBio(Object sessionHandle) =>
      throw UnsupportedError('stub: native not linked');

  @override
  int changePassword(
          Object sessionHandle, String oldPassword, String newPassword) =>
      throw UnsupportedError('stub: native not linked');

  @override
  void lock(Object sessionHandle) =>
      throw UnsupportedError('stub: native not linked');

  @override
  int cooldownRemainingMs(String path) =>
      throw UnsupportedError('stub: native not linked');

  @override
  int vaultMkdir(
          Object sessionHandle, int parent, String name, List<int> idOut) =>
      throw UnsupportedError('stub: native not linked');

  @override
  String? vaultList(Object sessionHandle, int folder) =>
      throw UnsupportedError('stub: native not linked');

  @override
  String? vaultSearch(Object sessionHandle, String query) =>
      throw UnsupportedError('stub: native not linked');

  @override
  int vaultImport(
          Object sessionHandle, String src, int folder, List<int> idOut) =>
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
  int vaultRenameFolder(Object sessionHandle, int folderId, String name) =>
      throw UnsupportedError('stub: native not linked');

  @override
  int vaultDeleteFolder(Object sessionHandle, int folderId,
          {bool secure = true}) =>
      throw UnsupportedError('stub: native not linked');

  @override
  int vaultSetTags(Object sessionHandle, int targetId, List<String> tags) =>
      throw UnsupportedError('stub: native not linked');

  @override
  String? vaultShareCreate(
          Object sessionHandle, int fileId, int ttlSecs, int maxOpens) =>
      throw UnsupportedError('stub: native not linked');

  @override
  int vaultShareOpen(
          Object sessionHandle, int shareId, String token, String dest) =>
      throw UnsupportedError('stub: native not linked');

  @override
  String? p2pPairBegin(Object sessionHandle) =>
      throw UnsupportedError('stub: native not linked');

  @override
  String? p2pPairJoin(Object sessionHandle, String addr, String code) =>
      throw UnsupportedError('stub: native not linked');

  @override
  String? p2pSync(Object sessionHandle, String addr) =>
      throw UnsupportedError('stub: native not linked');

  @override
  String? p2pSyncRelay(
          Object sessionHandle, String relay, String room, String token) =>
      throw UnsupportedError('stub: native not linked');

  @override
  int p2pServeRelay(
          Object sessionHandle, String relay, String room, String token) =>
      throw UnsupportedError('stub: native not linked');

  @override
  String? relaySettingsGet(Object sessionHandle) =>
      throw UnsupportedError('stub: native not linked');

  @override
  int relaySettingsSet(Object sessionHandle, String json) =>
      throw UnsupportedError('stub: native not linked');

  @override
  String? p2pSyncRelayCandidates(Object sessionHandle, String room) =>
      throw UnsupportedError('stub: native not linked');

  @override
  int p2pServeRelayCandidates(Object sessionHandle, String room) =>
      throw UnsupportedError('stub: native not linked');

  @override
  String? p2pStatus(Object sessionHandle) =>
      throw UnsupportedError('stub: native not linked');

  @override
  String? syncPolicyGet(Object sessionHandle) =>
      throw UnsupportedError('stub: native not linked');

  @override
  int syncPolicySet(Object sessionHandle, String json) =>
      throw UnsupportedError('stub: native not linked');

  @override
  int setSyncContext(Object sessionHandle, String json) =>
      throw UnsupportedError('stub: native not linked');

  @override
  String? stegoPlan(Object sessionHandle, int fileId, String imagesJson,
          String? optsJson) =>
      throw UnsupportedError('stub: native not linked');

  @override
  int vaultApplyEdit(
          Object sessionHandle, int fileId, String src, String? optsJson) =>
      throw UnsupportedError('stub: native not linked');

  @override
  String? stegoEmbedMulti(Object sessionHandle, int fileId, String imagesJson,
          String? optsJson) =>
      throw UnsupportedError('stub: native not linked');

  @override
  String? stegoExtractMulti(
          Object sessionHandle, String imagesJson, String dest) =>
      throw UnsupportedError('stub: native not linked');

  @override
  int p2pUnpair(Object sessionHandle, String deviceId) =>
      throw UnsupportedError('stub: native not linked');

  @override
  @override
  Map<String, dynamic>? verifyUpdateManifest(
          String manifest, String sigHex, String publicKeyHex) =>
      throw UnsupportedError('stub: native not linked');

  @override
  String? fileSha256(String path) =>
      throw UnsupportedError('stub: native not linked');

  @override
  String? destroyQueueAll(Object sessionHandle, int delaySecs) =>
      throw UnsupportedError('stub: native not linked');

  @override
  String? pendingOrders(Object sessionHandle) =>
      throw UnsupportedError('stub: native not linked');

  @override
  int cancelOrders(Object sessionHandle) =>
      throw UnsupportedError('stub: native not linked');

  @override
  int rememberPeerAddr(Object sessionHandle, String deviceId, String addr) =>
      throw UnsupportedError('stub: native not linked');

  @override
  int rotateFileKey(Object sessionHandle, int fileId) =>
      throw UnsupportedError('stub: native not linked');

  @override
  int rotateFolderKeys(Object sessionHandle, int folderId) =>
      throw UnsupportedError('stub: native not linked');

  @override
  int rotateMk(Object sessionHandle, String password) =>
      throw UnsupportedError('stub: native not linked');

  @override
  String? vaultVerifyAll(Object sessionHandle) =>
      throw UnsupportedError('stub: native not linked');

  @override
  String? auditList(Object sessionHandle) =>
      throw UnsupportedError('stub: native not linked');

  @override
  String? auditVerify(Object sessionHandle) =>
      throw UnsupportedError('stub: native not linked');

  @override
  int auditExport(Object sessionHandle, String dest) =>
      throw UnsupportedError('stub: native not linked');

  @override
  int destroyLocal(Object sessionHandle, {bool secure = true}) =>
      throw UnsupportedError('stub: native not linked');

  @override
  int stegoStatus(Object sessionHandle) =>
      throw UnsupportedError('stub: native not linked');

  @override
  int stegoSetEnabled(Object sessionHandle, bool on) =>
      throw UnsupportedError('stub: native not linked');

  @override
  int stegoCapacity(Object sessionHandle, String imagePath) =>
      throw UnsupportedError('stub: native not linked');

  @override
  int stegoEmbed(
          Object sessionHandle, int fileId, String imagePath, String outPath) =>
      throw UnsupportedError('stub: native not linked');

  @override
  String? stegoExtract(
          Object sessionHandle, String imagePath, String destPath) =>
      throw UnsupportedError('stub: native not linked');

  @override
  String? p2pDestroyArm(
          Object sessionHandle, String addr, String target, int delaySecs) =>
      throw UnsupportedError('stub: native not linked');

  @override
  int p2pDestroyCancel(Object sessionHandle) =>
      throw UnsupportedError('stub: native not linked');

  @override
  int p2pConflictResolve(Object sessionHandle, int keepId, int dropId,
          {bool secure = false}) =>
      throw UnsupportedError('stub: native not linked');

  @override
  Map<String, dynamic>? abiInfo() => <String, dynamic>{
        // 与引擎 ABI v2 同布局的假数据（供无 DLL 环境的 UI / 测试走通流程）。
        'structSize': 72,
        'abiVersion': 2,
        'capabilityBits': (1 << 0) | (1 << 1) | (1 << 14) | (1 << 15),
        'maxWriteVerVault': 2,
        'maxWriteVerContainer': 2,
        'maxWriteVerIndex': 3,
        'maxWriteVerAudit': 1,
        'minReadVerVault': 2,
        'featureFlags': 0,
        'engineVersion': '0.0.0-stub',
      };

  @override
  int subscribe() => 1;

  @override
  void unsubscribe(int subId) {}

  @override
  List<Map<String, dynamic>> pollEvents() => const [];

  @override
  List<Map<String, dynamic>> taskList() => const [];

  @override
  Map<String, dynamic>? taskStatus(int id) => null;

  @override
  int taskCancel(int id) => VaultStatus.invalidArg;

  @override
  int taskPause(int id) => VaultStatus.invalidArg;

  @override
  int taskResume(int id) => VaultStatus.invalidArg;

  @override
  int taskSpawnSelfcheck() => 1;

  @override
  int sessionReadonly(Object sessionHandle) => 0;

  @override
  int vaultThumbnailProbe(Object sessionHandle, int fileId) =>
      VaultStatus.capability;

  @override
  int rotateMkBegin(
          Object sessionHandle, String password, List<int> taskIdOut) =>
      throw UnsupportedError('stub: native not linked');

  @override
  int rotateMkResume(Object sessionHandle, String password) =>
      throw UnsupportedError('stub: native not linked');

  @override
  String? rotateMkStatus(Object sessionHandle) =>
      throw UnsupportedError('stub: native not linked');

  @override
  int p2pPushLock(Object sessionHandle, String? peerId) =>
      throw UnsupportedError('stub: native not linked');

  @override
  int p2pPushRotation(
          Object sessionHandle, String? peerId, List<int> taskIdOut) =>
      throw UnsupportedError('stub: native not linked');

  @override
  String? eraseClassProbe(String path) =>
      throw UnsupportedError('stub: native not linked');

  @override
  int migrateBegin(Object sessionHandle, String password, List<int> taskIdOut) =>
      throw UnsupportedError('stub: native not linked');

  @override
  String? migrateStatus(Object sessionHandle) =>
      throw UnsupportedError('stub: native not linked');

  @override
  int migrateResume(Object sessionHandle, String password) =>
      throw UnsupportedError('stub: native not linked');

  @override
  int migrateCancel(Object sessionHandle) =>
      throw UnsupportedError('stub: native not linked');

  @override
  String? vaultSearchV2(Object sessionHandle, String query, String? optsJson) =>
      throw UnsupportedError('stub: native not linked');

  @override
  int p2pSyncTask(Object sessionHandle, String addr, List<int> taskIdOut) =>
      throw UnsupportedError('stub: native not linked');

  @override
  String? p2pPathStatus(Object sessionHandle) =>
      throw UnsupportedError('stub: native not linked');

  @override
  String? p2pDiscover(Object sessionHandle) =>
      throw UnsupportedError('stub: native not linked');
}
