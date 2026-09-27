import 'dart:async';

import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../core/l10n/vs_l10n.dart';
import '../l10n/app_localizations.dart';
import '../presentation/models/vault_entry.dart';
import '../service/engine_service.dart';
import 'models/p2p.dart';
import 'notification_controller.dart';
import 'session_controller.dart';
import 'vault_controller.dart';

/// 冲突副本对（P4-3）。引擎的冲突处理（docs/05-03 §6.2）是：把**本地**版本
/// 另存为 `<原名>.conflict-<ts>` 新条目，原 id 让给对端版本。因此「解决冲突」
/// 即在两者中留一个、删另一个。
class ConflictPair {
  const ConflictPair({
    required this.copyId,
    required this.copyName,
    required this.originalName,
    this.originalId,
    this.size = 0,
  });

  final int copyId;
  final String copyName;
  final String originalName;

  /// 同名兄弟条目的 id；null 表示对端版本已不存在，只剩副本。
  final int? originalId;
  final int size;

  /// 副本时间戳（`.conflict-<ms>` 后缀 → 可读时间）。
  String get stamp {
    final i = copyName.lastIndexOf('.conflict-');
    if (i < 0) return '';
    final ms = int.tryParse(copyName.substring(i + 10));
    if (ms == null) return '';
    final t = DateTime.fromMillisecondsSinceEpoch(ms);
    return '${t.year}-${_p(t.month)}-${_p(t.day)} ${_p(t.hour)}:${_p(t.minute)}';
  }

  static String _p(int n) => n.toString().padLeft(2, '0');
}

/// 同步页状态。引擎无推送通道：事件时间线 / 摘要 / 冲突清单均为真实数据，
/// 由本控制器轮询拉取；逐块传输矩阵待引擎提供回调（P5）。
class SyncState {
  const SyncState({
    this.events = const [],
    this.lastSummary,
    this.running = false,
    this.error,
    this.selfName = '',
    this.peers = const [],
    this.conflicts = const [],
    this.conflictsLoading = false,
    this.watching = false,
  });

  final List<String> events;
  final SyncSummarySnapshot? lastSummary;
  final bool running;
  final String? error;
  final String selfName;
  final List<Peer> peers;

  /// 保险箱内检出的冲突副本（真实扫描结果，而非摘要里的 id 列表）。
  final List<ConflictPair> conflicts;
  final bool conflictsLoading;

  /// 是否正在按周期拉取状态（同步页可见期间）。
  final bool watching;

  SyncState copyWith({
    List<String>? events,
    SyncSummarySnapshot? lastSummary,
    bool? running,
    String? error,
    String? selfName,
    List<Peer>? peers,
    List<ConflictPair>? conflicts,
    bool? conflictsLoading,
    bool? watching,
  }) =>
      SyncState(
        events: events ?? this.events,
        lastSummary: lastSummary ?? this.lastSummary,
        running: running ?? this.running,
        error: error,
        selfName: selfName ?? this.selfName,
        peers: peers ?? this.peers,
        conflicts: conflicts ?? this.conflicts,
        conflictsLoading: conflictsLoading ?? this.conflictsLoading,
        watching: watching ?? this.watching,
      );
}

class SyncController extends Notifier<SyncState> {
  Timer? _watchTimer;

  /// 当前语言（状态层不在组件树内，见 core/l10n/vs_l10n.dart）。
  AppLocalizations? get _l => VsL10n.orNull;

  @override
  SyncState build() {
    ref.onDispose(() => _watchTimer?.cancel());
    return const SyncState();
  }

  Object? get _handle {
    final s = ref.read(sessionProvider);
    return s is SessionUnlocked ? s.sessionHandle : null;
  }

  /// 刷新事件时间线 + 本机/设备信息（来自 p2p_status）。
  Future<void> refresh() async {
    final handle = _handle;
    if (handle == null) return;
    final json = await ref.read(vaultEngineProvider).status(handle);
    if (!ref.mounted || json == null) return;
    final status = SyncStatus.fromJson(json);
    state = state.copyWith(
      events: status.events,
      selfName: status.name,
      peers: status.peers,
    );
  }

  /// 同步页可见期间开启状态轮询（引擎无推送通道，按周期拉快照）。
  void startWatch({Duration interval = const Duration(seconds: 2)}) {
    if (state.watching) return;
    state = state.copyWith(watching: true);
    _watchTimer?.cancel();
    _watchTimer = Timer.periodic(interval, (_) => _tick());
  }

  void stopWatch() {
    _watchTimer?.cancel();
    _watchTimer = null;
    if (state.watching) state = state.copyWith(watching: false);
  }

  Future<void> _tick() async {
    await refresh();
    await refreshConflicts();
  }

  /// 扫描保险箱检出冲突副本及其同名兄弟（BFS 遍历全部文件夹）。
  Future<void> refreshConflicts() async {
    final handle = _handle;
    if (handle == null) return;
    final engine = ref.read(vaultEngineProvider);
    state = state.copyWith(conflictsLoading: true);
    final found = <ConflictPair>[];
    final queue = <int>[0];
    final seen = <int>{0};
    // 环 / 体量守卫：最多遍历 256 个文件夹。
    while (queue.isNotEmpty && seen.length <= 256) {
      final folder = queue.removeAt(0);
      final json = await engine.list(handle, folder);
      if (json == null) continue;
      final listing = VaultListing.fromJson(json);
      final byName = <String, ({int id, int size})>{
        for (final e in listing.files) e.name: (id: e.id, size: e.size),
      };
      for (final e in listing.files) {
        final i = e.name.lastIndexOf('.conflict-');
        if (i <= 0) continue;
        final original = e.name.substring(0, i);
        found.add(ConflictPair(
          copyId: e.id,
          copyName: e.name,
          originalName: original,
          originalId: byName[original]?.id,
          size: e.size,
        ));
      }
      for (final f in listing.folders) {
        if (seen.add(f.id)) queue.add(f.id);
      }
    }
    if (!ref.mounted) return;
    state = state.copyWith(conflicts: found, conflictsLoading: false);
  }

  /// 解决一个冲突对：[keepCopy] = 保留副本（丢弃对端版本），否则丢弃副本。
  ///
  /// 保留副本时先删原条目再把副本改回原名，避免同名并存。
  Future<void> resolveConflict(ConflictPair p, {required bool keepCopy}) async {
    final handle = _handle;
    if (handle == null) return;
    final engine = ref.read(vaultEngineProvider);
    final notif = ref.read(notificationProvider.notifier);
    String? err;
    if (!keepCopy || p.originalId == null) {
      err = await engine.deleteFile(handle, p.copyId);
    } else {
      err = await engine.deleteFile(handle, p.originalId!);
      err ??= await engine.renameFile(handle, p.copyId, p.originalName);
    }
    if (err != null) {
      notif.danger(_l?.stateConflictResolveFailed(err) ?? '冲突解决失败：$err');
    } else {
      notif.ok(keepCopy
          ? (_l?.stateConflictKept(p.originalName) ??
              '已保留副本并替换原条目 · ${p.originalName}')
          : (_l?.stateConflictDropped ?? '已丢弃冲突副本'));
    }
    // 跨页联动（P4-6）：保险箱列表与冲突清单都要反映结果。
    await ref.read(vaultUiProvider.notifier).refresh();
    await refreshConflicts();
  }

  /// 直连同步；成功后用摘要更新状态。
  Future<void> runSync(String addr) async {
    final handle = _handle;
    if (handle == null) return;
    final engine = ref.read(vaultEngineProvider);
    final notif = ref.read(notificationProvider.notifier);
    state = state.copyWith(running: true, error: null);
    final json = await engine.sync(handle, addr);
    if (!ref.mounted) return;
    if (json == null) {
      state =
          state.copyWith(running: false, error: _l?.stateSyncFailed ?? '同步失败');
      notif.danger(_l?.stateSyncFailed ?? '同步失败');
      return;
    }
    final summary = SyncSummarySnapshot.fromJson(json);
    state = state.copyWith(running: false, lastSummary: summary, error: null);
    final l = _l;
    notif.ok(summary.conflicts.isEmpty
        ? (l?.stateSyncDone(summary.pulled.length, summary.pushed.length) ??
            '同步完成：拉取 ${summary.pulled.length}，推送 ${summary.pushed.length}')
        : (l?.stateSyncDoneWithConflicts(summary.pulled.length,
                summary.pushed.length, summary.conflicts.length) ??
            '同步完成：拉取 ${summary.pulled.length}，推送 ${summary.pushed.length}'
                '，冲突 ${summary.conflicts.length}'));
    await _afterSync();
  }

  /// 经中继同步（P8-5 VSR2：relay + room + token）。
  Future<void> runSyncRelay(String relay, String room, String token) async {
    final handle = _handle;
    if (handle == null) return;
    final engine = ref.read(vaultEngineProvider);
    state = state.copyWith(running: true, error: null);
    final json = await engine.syncRelay(handle, relay, room, token);
    if (!ref.mounted) return;
    if (json == null) {
      state = state.copyWith(
          running: false, error: _l?.stateSyncRelayFailed ?? '中继同步失败');
      return;
    }
    final summary = SyncSummarySnapshot.fromJson(json);
    state = state.copyWith(running: false, lastSummary: summary, error: null);
    await _afterSync();
  }

  /// 同步后统一收尾：状态、冲突清单、保险箱列表（对端文件可能已入库）。
  Future<void> _afterSync() async {
    await refresh();
    await refreshConflicts();
    if (ref.mounted) await ref.read(vaultUiProvider.notifier).refresh();
  }
}

final syncProvider =
    NotifierProvider<SyncController, SyncState>(SyncController.new);
