import 'dart:convert';
import 'dart:io';
import 'dart:isolate';

import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:path_provider/path_provider.dart';

import '../core/l10n/vs_l10n.dart';
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

  /// 首次运行：主密码设置即创建保险箱（F-01；强度策略见 PasswordPolicy）。
  Future<void> ensureCreated(String password) async {
    final primary = await primaryPath();
    if (_bridge.vaultExists(primary)) return;
    if (!isNative) throw StateError('原生引擎未加载，无法创建保险箱');
    final err = await Isolate.run(() => _createSync(primary, password));
    if (err != VaultStatus.ok) {
      throw StateError('创建保险箱失败: ${VaultStatus.message(err)}');
    }
  }

  /// 创建 / 重设伪装保险箱（F-06）：独立文件 + 独立 MK，与真实保险箱无任何共享。
  /// 已存在时不覆盖（改伪装密码请使用设置页的「重设伪装密码」）。
  Future<void> ensureDisguiseCreated(String password) async {
    final path = await disguisePath();
    if (_bridge.vaultExists(path)) return;
    if (!isNative) throw StateError('原生引擎未加载，无法创建伪装保险箱');
    final err = await Isolate.run(() => _createSync(path, password));
    if (err != VaultStatus.ok) {
      throw StateError('创建伪装保险箱失败: ${VaultStatus.message(err)}');
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
        ? await Isolate.run(
            () => _unlockSync(primary, '', disguise: false, bio: true))
        : _bridge.unlockBio(primary);
    return _map(r, disguise: false);
  }

  Future<bool> vaultExists() async => _bridge.vaultExists(await primaryPath());

  Future<bool> disguiseExists() async =>
      _bridge.vaultExists(await disguisePath());

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
        ? await Isolate.run(
            () => _changeSync(sessionHandle as int, oldPassword, newPassword))
        : _bridge.changePassword(sessionHandle, oldPassword, newPassword);
    return err == VaultStatus.ok ? null : VaultStatus.message(err);
  }

  void lock(Object sessionHandle) {
    if (isNative) _bridge.lock(sessionHandle);
  }

  /// 绑定/解绑生物识别（F-04）。返回 null=成功；非 null=用户可读错误。
  Future<String?> bindBio(Object sessionHandle, {bool bind = true}) async {
    final h = sessionHandle as int;
    final err = isNative
        ? await Isolate.run(() => bind ? _bindBioSync(h) : _unbindBioSync(h))
        : (bind
            ? _bridge.bindBio(sessionHandle)
            : _bridge.unbindBio(sessionHandle));
    return err == VaultStatus.ok ? null : VaultStatus.message(err);
  }

  // ==== P2 保险箱操作（docs/05-02）====
  // 返回 null = 成功；非 null = 用户可读错误消息。
  // isolate 不可发送桥对象：原生路径走静态函数（句柄为 int 可跨 isolate）；
  // id 输出改用返回记录（(err, id)），因 isolate 内对捕获 List 的修改不会传回。

  Future<(String? err, int id)> mkdir(
      Object sessionHandle, int parent, String name) async {
    final h = sessionHandle as int;
    if (!isNative) {
      final idOut = <int>[0];
      final err = _bridge.vaultMkdir(sessionHandle, parent, name, idOut);
      return (
        err == VaultStatus.ok ? null : VaultStatus.message(err),
        idOut[0]
      );
    }
    final (err, id) = await Isolate.run(() => _mkdirSync(h, parent, name));
    return (err == VaultStatus.ok ? null : VaultStatus.message(err), id);
  }

  /// 列出子项；失败返回 null。
  Future<Map<String, dynamic>?> list(Object sessionHandle, int folder) async {
    final h = sessionHandle as int;
    final json = isNative
        ? await Isolate.run(() => _jsonSync(h, folder, null))
        : _bridge.vaultList(sessionHandle, folder);
    if (json == null) return null;
    return jsonDecode(json) as Map<String, dynamic>;
  }

  Future<Map<String, dynamic>?> search(
      Object sessionHandle, String query) async {
    final h = sessionHandle as int;
    final json = isNative
        ? await Isolate.run(() => _jsonSync(h, -1, query))
        : _bridge.vaultSearch(sessionHandle, query);
    if (json == null) return null;
    return jsonDecode(json) as Map<String, dynamic>;
  }

  Future<(String? err, int id)> importFile(
      Object sessionHandle, String src, int folder) async {
    final h = sessionHandle as int;
    if (!isNative) {
      final idOut = <int>[0];
      final err = _bridge.vaultImport(sessionHandle, src, folder, idOut);
      return (
        err == VaultStatus.ok ? null : VaultStatus.message(err),
        idOut[0]
      );
    }
    final (err, id) = await Isolate.run(() => _importSync(h, src, folder));
    return (err == VaultStatus.ok ? null : VaultStatus.message(err), id);
  }

  Future<String?> exportFile(
      Object sessionHandle, int fileId, String dest) async {
    final h = sessionHandle as int;
    final err = isNative
        ? await Isolate.run(() => _statusSync(h, fileId, dest, Op.export))
        : _bridge.vaultExport(sessionHandle, fileId, dest);
    return err == VaultStatus.ok ? null : VaultStatus.message(err);
  }

  Future<String?> renameFile(
      Object sessionHandle, int fileId, String name) async {
    final h = sessionHandle as int;
    final err = isNative
        ? await Isolate.run(() => _statusSync(h, fileId, name, Op.rename))
        : _bridge.vaultRenameFile(sessionHandle, fileId, name);
    return err == VaultStatus.ok ? null : VaultStatus.message(err);
  }

  Future<String?> deleteFile(Object sessionHandle, int fileId,
      {bool secure = true}) async {
    final h = sessionHandle as int;
    final err = isNative
        ? await Isolate.run(() => _deleteSync(h, fileId, secure))
        : _bridge.vaultDeleteFile(sessionHandle, fileId, secure: secure);
    return err == VaultStatus.ok ? null : VaultStatus.message(err);
  }

  /// 重命名文件夹（根目录不可改名）。
  Future<String?> renameFolder(
      Object sessionHandle, int folderId, String name) async {
    final h = sessionHandle as int;
    final err = isNative
        ? await Isolate.run(
            () => _statusSync(h, folderId, name, Op.renameFolder))
        : _bridge.vaultRenameFolder(sessionHandle, folderId, name);
    return err == VaultStatus.ok ? null : VaultStatus.message(err);
  }

  /// 递归擦除文件夹；返回 (err, 删除的文件数)。
  Future<(String?, int)> deleteFolder(Object sessionHandle, int folderId,
      {bool secure = true}) async {
    final h = sessionHandle as int;
    final n = isNative
        ? await Isolate.run(() => _deleteFolderSync(h, folderId, secure))
        : _bridge.vaultDeleteFolder(sessionHandle, folderId, secure: secure);
    // FFI 约定：成功返回删除的文件数（≥0），失败返回负状态码。
    if (n < 0) return (VaultStatus.message(-n), 0);
    return (null, n);
  }

  Future<String?> setTags(
      Object sessionHandle, int targetId, List<String> tags) async {
    final h = sessionHandle as int;
    final err = isNative
        ? await Isolate.run(
            () => _statusSync(h, targetId, tags.join(','), Op.setTags))
        : _bridge.vaultSetTags(sessionHandle, targetId, tags);
    return err == VaultStatus.ok ? null : VaultStatus.message(err);
  }

  /// 阅后即焚分享；成功返回 {"shareId":…,"token":…}。
  Future<Map<String, dynamic>?> shareCreate(
      Object sessionHandle, int fileId, int ttlSecs, int maxOpens) async {
    final h = sessionHandle as int;
    final json = isNative
        ? await Isolate.run(
            () => _shareCreateSync(h, fileId, ttlSecs, maxOpens))
        : _bridge.vaultShareCreate(sessionHandle, fileId, ttlSecs, maxOpens);
    if (json == null) return null;
    return jsonDecode(json) as Map<String, dynamic>;
  }

  Future<String?> shareOpen(
      Object sessionHandle, int shareId, String token, String dest) async {
    final h = sessionHandle as int;
    final err = isNative
        ? await Isolate.run(
            () => _statusSync(h, shareId, '$token\u0000$dest', Op.shareOpen))
        : _bridge.vaultShareOpen(sessionHandle, shareId, token, dest);
    return err == VaultStatus.ok ? null : VaultStatus.message(err);
  }

  // ==== P3 P2P 同步（docs/05, docs/08）====
  // 10 个导出补封装入服务层，遵循既有 isolate 模式：
  // 句柄为 int 跨 isolate 安全；JSON 结果为引擎分配字符串，桥层 _takeJson 释放。

  /// 开始配对，返回 JSON（含设备身份与邀请信息）；失败返回 null。
  Future<Map<String, dynamic>?> pairBegin(Object sessionHandle) async {
    final h = sessionHandle as int;
    final json = isNative
        ? await Isolate.run(() => _p2pPairBeginSync(h))
        : _bridge.p2pPairBegin(sessionHandle);
    if (json == null) return null;
    return jsonDecode(json) as Map<String, dynamic>;
  }

  /// 加入配对（addr + 一次性邀请码）。返回 JSON 或 null。
  Future<Map<String, dynamic>?> pairJoin(
      Object sessionHandle, String addr, String code) async {
    final h = sessionHandle as int;
    final json = isNative
        ? await Isolate.run(() => _p2pPairJoinSync(h, addr, code))
        : _bridge.p2pPairJoin(sessionHandle, addr, code);
    if (json == null) return null;
    return jsonDecode(json) as Map<String, dynamic>;
  }

  /// 直连增量同步。返回 JSON 摘要 {pulled/pushed/deleted/merged/conflicts} 或 null。
  Future<Map<String, dynamic>?> sync(Object sessionHandle, String addr) async {
    final h = sessionHandle as int;
    final json = isNative
        ? await Isolate.run(() => _p2pSyncSync(h, addr))
        : _bridge.p2pSync(sessionHandle, addr);
    if (json == null) return null;
    return jsonDecode(json) as Map<String, dynamic>;
  }

  /// 经中继同步（relay + room）。
  Future<Map<String, dynamic>?> syncRelay(
      Object sessionHandle, String relay, String room) async {
    final h = sessionHandle as int;
    final json = isNative
        ? await Isolate.run(() => _p2pSyncRelaySync(h, relay, room))
        : _bridge.p2pSyncRelay(sessionHandle, relay, room);
    if (json == null) return null;
    return jsonDecode(json) as Map<String, dynamic>;
  }

  /// 《待定语义》serveRelay 返回 ERR 码 int（状态码语义见 AGENTS）。
  Future<int> serveRelay(
      Object sessionHandle, String relay, String room) async {
    final h = sessionHandle as int;
    return isNative
        ? await Isolate.run(() => _p2pServeRelaySync(h, relay, room))
        : _bridge.p2pServeRelay(sessionHandle, relay, room);
  }

  /// P2P 状态快照 JSON（设备身份 / peers / 部署销毁 / 事件时间线）。
  Future<Map<String, dynamic>?> status(Object sessionHandle) async {
    final h = sessionHandle as int;
    final json = isNative
        ? await Isolate.run(() => _p2pStatusSync(h))
        : _bridge.p2pStatus(sessionHandle);
    if (json == null) return null;
    return jsonDecode(json) as Map<String, dynamic>;
  }

  // ==== P5-1 审计日志 / P5-5 紧急销毁 / P5-3 隐写术 ====

  /// 审计条目列表：{"count","head","entries":[…]};失败返回 null。
  Future<Map<String, dynamic>?> auditList(Object sessionHandle) async {
    final h = sessionHandle as int;
    final json = isNative
        ? await Isolate.run(() => _auditListSync(h))
        : _bridge.auditList(sessionHandle);
    if (json == null) return null;
    return jsonDecode(json) as Map<String, dynamic>;
  }

  /// 链式校验：{"ok","checked","brokenAt","reason"};失败返回 null。
  Future<Map<String, dynamic>?> auditVerify(Object sessionHandle) async {
    final h = sessionHandle as int;
    final json = isNative
        ? await Isolate.run(() => _auditVerifySync(h))
        : _bridge.auditVerify(sessionHandle);
    if (json == null) return null;
    return jsonDecode(json) as Map<String, dynamic>;
  }

  /// 加密导出审计日志。成功返回 null，失败返回可读消息。
  Future<String?> auditExport(Object sessionHandle, String dest) async {
    final h = sessionHandle as int;
    final err = isNative
        ? await Isolate.run(() => _auditExportSync(h, dest))
        : _bridge.auditExport(sessionHandle, dest);
    return err == VaultStatus.ok ? null : VaultStatus.message(err);
  }

  /// 紧急销毁 · 本机：销毁后该会话对应的保险箱已不存在，调用方须立即锁定。
  Future<String?> destroyLocal(Object sessionHandle,
      {bool secure = true}) async {
    final h = sessionHandle as int;
    final err = isNative
        ? await Isolate.run(() => _destroyLocalSync(h, secure))
        : _bridge.destroyLocal(sessionHandle, secure: secure);
    return err == VaultStatus.ok ? null : VaultStatus.message(err);
  }

  /// 隐写引擎开关（会话级）
  Future<bool> stegoEnabled(Object sessionHandle) async {
    final h = sessionHandle as int;
    final r = isNative
        ? await Isolate.run(() => _stegoStatusSync(h))
        : _bridge.stegoStatus(sessionHandle);
    return r == 1;
  }

  Future<String?> stegoSetEnabled(Object sessionHandle, bool on) async {
    final h = sessionHandle as int;
    final err = isNative
        ? await Isolate.run(() => _stegoSetEnabledSync(h, on))
        : _bridge.stegoSetEnabled(sessionHandle, on);
    return err == VaultStatus.ok ? null : VaultStatus.message(err);
  }

  /// 图片容量（可嵌入字节数）；负值为错误码（-3 IO / -6 非受支持 PNG）。
  Future<int> stegoCapacity(Object sessionHandle, String imagePath) async {
    final h = sessionHandle as int;
    return isNative
        ? await Isolate.run(() => _stegoCapacitySync(h, imagePath))
        : _bridge.stegoCapacity(sessionHandle, imagePath);
  }

  /// 嵌入：把 fileId 的明文加密后写入图片 LSB。
  Future<String?> stegoEmbed(Object sessionHandle, int fileId, String imagePath,
      String outPath) async {
    final h = sessionHandle as int;
    final err = isNative
        ? await Isolate.run(
            () => _stegoEmbedSync(h, fileId, imagePath, outPath))
        : _bridge.stegoEmbed(sessionHandle, fileId, imagePath, outPath);
    // 引擎在未启用时返回「参数非法」，此处翻译为可读语义（UI 也会先做前置检查）
    if (err == VaultStatus.invalidArg) {
      return '隐写引擎未启用（安全中心开关）';
    }
    if (err == VaultStatus.format) {
      return '不支持该图片或容量不足（需 8 位 RGB/RGBA PNG）';
    }
    return err == VaultStatus.ok ? null : VaultStatus.message(err);
  }

  /// 提取：返回 {"name","size"}（供 UI 提示还原出的文件名）或 null。
  Future<Map<String, dynamic>?> stegoExtract(
      Object sessionHandle, String imagePath, String destPath) async {
    final h = sessionHandle as int;
    final json = isNative
        ? await Isolate.run(() => _stegoExtractSync(h, imagePath, destPath))
        : _bridge.stegoExtract(sessionHandle, imagePath, destPath);
    if (json == null) return null;
    return jsonDecode(json) as Map<String, dynamic>;
  }

  /// 解除与指定设备的配对（幂等：该设备本就不存在亦视为成功）。
  /// 返回 null = 成功；非 null = 用户可读错误消息。
  Future<String?> unpair(Object sessionHandle, String deviceId) async {
    final h = sessionHandle as int;
    final err = isNative
        ? await Isolate.run(() => _p2pUnpairSync(h, deviceId))
        : _bridge.p2pUnpair(sessionHandle, deviceId);
    return err == VaultStatus.ok ? null : VaultStatus.message(err);
  }

  /// 部署远程销毁；返回 JSON 或 null。
  Future<Map<String, dynamic>?> destroyArm(
      Object sessionHandle, String addr, String target, int delaySecs) async {
    final h = sessionHandle as int;
    final json = isNative
        ? await Isolate.run(() => _p2pJson3Sync(h, addr, target, delaySecs))
        : _bridge.p2pDestroyArm(sessionHandle, addr, target, delaySecs);
    if (json == null) return null;
    return jsonDecode(json) as Map<String, dynamic>;
  }

  /// 取消武装中的销毁；返回 ERR 码 int。
  Future<int> destroyCancel(Object sessionHandle) async {
    final h = sessionHandle as int;
    return isNative
        ? await Isolate.run(() => _p2pDestroyCancelSync(h))
        : _bridge.p2pDestroyCancel(sessionHandle);
  }

  /// 解决冲突：keep 保留，drop 丢弃（可选 secure）。
  Future<int> conflictResolve(Object sessionHandle, int keepId, int dropId,
      {bool secure = false}) async {
    final h = sessionHandle as int;
    return isNative
        ? await Isolate.run(
            () => _p2pConflictResolveSync(h, keepId, dropId, secure))
        : _bridge.p2pConflictResolve(sessionHandle, keepId, dropId,
            secure: secure);
  }

  Future<String> primaryPath() async => _join(await _vaultDir(), primaryName);

  Future<String> disguisePath() async => _join(await _vaultDir(), disguiseName);

  Future<UnlockOutcome> _unlockOn(String path, String password,
      {required bool disguise}) async {
    final r = isNative
        ? await Isolate.run(
            () => _unlockSync(path, password, disguise: disguise))
        : _bridge.unlock(path, password, disguise: disguise);
    return _map(r, disguise: disguise);
  }

  UnlockOutcome _map(EngineUnlock r, {required bool disguise}) {
    if (r.ok) {
      return UnlockSuccess(disguise: disguise, sessionHandle: r.sessionHandle!);
    }
    return switch (r.status) {
      VaultStatus.wrongPassword => UnlockWrongPassword(cooldownMs: r.waitMs),
      VaultStatus.cooldown => UnlockCooldown(r.waitMs),
      VaultStatus.bioUnavailable => UnlockFailure(
          VsL10n.orNull?.stateBioUnsupported ?? '此平台不支持生物识别，请使用主密码'),
      VaultStatus.bioNotBound =>
        UnlockFailure(VsL10n.orNull?.stateBioNotBound ?? '尚未绑定生物识别（设置中绑定后可用）'),
      _ => UnlockFailure(VaultStatus.message(r.status)),
    };
  }

  static String _join(String dir, String name) =>
      '$dir${Platform.pathSeparator}$name';

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

int _bindBioSync(int handle) => _handleFnSync((lib) => lib.bindBio(handle));

int _unbindBioSync(int handle) => _handleFnSync((lib) => lib.unbindBio(handle));

/// 一个 session 句柄的幂等原生调用（未链接 DLL 时返回内部错误）。
int _handleFnSync(int Function(VaultCoreBridgeFfi) fn) {
  final lib = VaultCoreBridgeFfi.tryOpen();
  if (lib == null) return VaultStatus.internal;
  return fn(lib);
}

// ---- P2 保险箱操作的 isolate 侧函数 ----

/// 统一的"状态码 + 可选字符串载荷"操作。shareOpen 复用 payload：'token\u0000dest'。
enum Op { export, rename, renameFolder, setTags, shareOpen }

int _statusSync(int handle, int id, String payload, Op op) {
  final lib = VaultCoreBridgeFfi.tryOpen();
  if (lib == null) return VaultStatus.internal;
  switch (op) {
    case Op.export:
      return lib.vaultExport(handle, id, payload);
    case Op.rename:
      return lib.vaultRenameFile(handle, id, payload);
    case Op.renameFolder:
      return lib.vaultRenameFolder(handle, id, payload);
    case Op.setTags:
      return lib.vaultSetTags(handle, id, payload.split(','));
    case Op.shareOpen:
      final parts = payload.split('\u0000');
      return lib.vaultShareOpen(
          handle, id, parts[0], parts.length > 1 ? parts[1] : '');
  }
}

(int, int) _mkdirSync(int handle, int parent, String name) {
  final lib = VaultCoreBridgeFfi.tryOpen();
  if (lib == null) return (VaultStatus.internal, 0);
  final idOut = <int>[0];
  final err = lib.vaultMkdir(handle, parent, name, idOut);
  return (err, idOut[0]);
}

(int, int) _importSync(int handle, String src, int folder) {
  final lib = VaultCoreBridgeFfi.tryOpen();
  if (lib == null) return (VaultStatus.internal, 0);
  final idOut = <int>[0];
  final err = lib.vaultImport(handle, src, folder, idOut);
  return (err, idOut[0]);
}

int _deleteSync(int handle, int fileId, bool secure) {
  final lib = VaultCoreBridgeFfi.tryOpen();
  if (lib == null) return VaultStatus.internal;
  return lib.vaultDeleteFile(handle, fileId, secure: secure);
}

int _deleteFolderSync(int handle, int folderId, bool secure) {
  final lib = VaultCoreBridgeFfi.tryOpen();
  if (lib == null) return -VaultStatus.internal;
  return lib.vaultDeleteFolder(handle, folderId, secure: secure);
}

String? _jsonSync(int handle, int folder, String? query) {
  final lib = VaultCoreBridgeFfi.tryOpen();
  if (lib == null) return null;
  if (query != null) return lib.vaultSearch(handle, query);
  return lib.vaultList(handle, folder);
}

String? _shareCreateSync(int handle, int fileId, int ttlSecs, int maxOpens) {
  final lib = VaultCoreBridgeFfi.tryOpen();
  if (lib == null) return null;
  return lib.vaultShareCreate(handle, fileId, ttlSecs, maxOpens);
}

// ---- P3 p2p isolate 侧函数（句柄为 int，桥对象不可跨 isolate）----

String? _p2pPairBeginSync(int handle) {
  final lib = VaultCoreBridgeFfi.tryOpen();
  if (lib == null) return null;
  return lib.p2pPairBegin(handle);
}

String? _p2pPairJoinSync(int handle, String addr, String code) {
  final lib = VaultCoreBridgeFfi.tryOpen();
  if (lib == null) return null;
  return lib.p2pPairJoin(handle, addr, code);
}

String? _p2pSyncSync(int handle, String addr) {
  final lib = VaultCoreBridgeFfi.tryOpen();
  if (lib == null) return null;
  return lib.p2pSync(handle, addr);
}

String? _p2pSyncRelaySync(int handle, String relay, String room) {
  final lib = VaultCoreBridgeFfi.tryOpen();
  if (lib == null) return null;
  return lib.p2pSyncRelay(handle, relay, room);
}

int _p2pServeRelaySync(int handle, String relay, String room) {
  final lib = VaultCoreBridgeFfi.tryOpen();
  if (lib == null) return VaultStatus.internal;
  return lib.p2pServeRelay(handle, relay, room);
}

String? _p2pStatusSync(int handle) {
  final lib = VaultCoreBridgeFfi.tryOpen();
  if (lib == null) return null;
  return lib.p2pStatus(handle);
}

String? _auditListSync(int handle) {
  final lib = VaultCoreBridgeFfi.tryOpen();
  if (lib == null) return null;
  return lib.auditList(handle);
}

String? _auditVerifySync(int handle) {
  final lib = VaultCoreBridgeFfi.tryOpen();
  if (lib == null) return null;
  return lib.auditVerify(handle);
}

int _auditExportSync(int handle, String dest) {
  final lib = VaultCoreBridgeFfi.tryOpen();
  if (lib == null) return VaultStatus.internal;
  return lib.auditExport(handle, dest);
}

int _destroyLocalSync(int handle, bool secure) {
  final lib = VaultCoreBridgeFfi.tryOpen();
  if (lib == null) return VaultStatus.internal;
  return lib.destroyLocal(handle, secure: secure);
}

int _stegoStatusSync(int handle) {
  final lib = VaultCoreBridgeFfi.tryOpen();
  if (lib == null) return 0;
  return lib.stegoStatus(handle);
}

int _stegoSetEnabledSync(int handle, bool on) {
  final lib = VaultCoreBridgeFfi.tryOpen();
  if (lib == null) return VaultStatus.internal;
  return lib.stegoSetEnabled(handle, on);
}

int _stegoCapacitySync(int handle, String imagePath) {
  final lib = VaultCoreBridgeFfi.tryOpen();
  if (lib == null) return -VaultStatus.internal;
  return lib.stegoCapacity(handle, imagePath);
}

int _stegoEmbedSync(int handle, int fileId, String imagePath, String outPath) {
  final lib = VaultCoreBridgeFfi.tryOpen();
  if (lib == null) return VaultStatus.internal;
  return lib.stegoEmbed(handle, fileId, imagePath, outPath);
}

String? _stegoExtractSync(int handle, String imagePath, String destPath) {
  final lib = VaultCoreBridgeFfi.tryOpen();
  if (lib == null) return null;
  return lib.stegoExtract(handle, imagePath, destPath);
}

int _p2pUnpairSync(int handle, String deviceId) {
  final lib = VaultCoreBridgeFfi.tryOpen();
  if (lib == null) return VaultStatus.internal;
  return lib.p2pUnpair(handle, deviceId);
}

String? _p2pJson3Sync(int handle, String a, String b, int delaySecs) {
  final lib = VaultCoreBridgeFfi.tryOpen();
  if (lib == null) return null;
  return lib.p2pDestroyArm(handle, a, b, delaySecs);
}

int _p2pDestroyCancelSync(int handle) {
  final lib = VaultCoreBridgeFfi.tryOpen();
  if (lib == null) return VaultStatus.internal;
  return lib.p2pDestroyCancel(handle);
}

int _p2pConflictResolveSync(int handle, int keepId, int dropId, bool secure) {
  final lib = VaultCoreBridgeFfi.tryOpen();
  if (lib == null) return VaultStatus.internal;
  return lib.p2pConflictResolve(handle, keepId, dropId, secure: secure);
}

/// Provider：优先原生引擎，缺失时回退 Stub（仅纯 Dart 测试 / CI 环境）。
final vaultCoreBridgeProvider = Provider<VaultCoreBridge>(
    (ref) => VaultCoreBridgeFfi.tryOpen() ?? VaultCoreBridgeStub());

final vaultEngineProvider = Provider<VaultEngine>(
    (ref) => VaultEngine(ref.watch(vaultCoreBridgeProvider)));
