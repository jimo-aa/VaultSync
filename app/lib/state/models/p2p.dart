/// P2P 状态/同步模型的容错解析。
///
/// 引擎侧字段基于 `native/vault-p2p/src/engine.rs` 的真实 `serde_json` 输出，
/// 这里全部做缺省容错（缺失 → 空值/默认），避免 schema 变更或 stub 数据导致 UI 崩溃。
library;

import '../../core/l10n/vs_l10n.dart';

/// 一次同步操作的计数摘要（来自 `p2p_sync` 返回 JSON）。
/// 字段为 id 数组：`{"pushed":[],"pulled":[],"deleted":[],"conflicts":[],"merged":[]}`。
class SyncSummarySnapshot {
  const SyncSummarySnapshot({
    this.pushed = const [],
    this.pulled = const [],
    this.deleted = const [],
    this.conflicts = const [],
    this.merged = const [],
  });

  final List<int> pushed;
  final List<int> pulled;
  final List<int> deleted;
  final List<int> conflicts;
  final List<int> merged;

  factory SyncSummarySnapshot.fromJson(Map<String, dynamic> json) {
    return SyncSummarySnapshot(
      pushed: _idList(json['pushed']),
      pulled: _idList(json['pulled']),
      deleted: _idList(json['deleted']),
      conflicts: _idList(json['conflicts']),
      merged: _idList(json['merged']),
    );
  }

  bool get isEmpty =>
      pushed.isEmpty && pulled.isEmpty && deleted.isEmpty && conflicts.isEmpty;

  /// 容错：字段可能是 id 数组，也可能意外是单个 int。
  static List<int> _idList(dynamic raw) {
    if (raw is List) {
      return raw.whereType<num>().map((e) => e.toInt()).toList();
    }
    if (raw is num) return [raw.toInt()];
    return const [];
  }

  SyncSummarySnapshot copyWith({List<int>? conflicts}) => SyncSummarySnapshot(
        pushed: pushed,
        pulled: pulled,
        deleted: deleted,
        conflicts: conflicts ?? this.conflicts,
        merged: merged,
      );
}

/// 已配对设备（`status().peers[]`）。
class Peer {
  const Peer({
    required this.deviceId,
    required this.name,
    this.fingerprint = '',
    this.addr,
  });

  final String deviceId;
  final String name;
  final String fingerprint;

  /// 对端上次可达的监听地址（配对/同步时记录；空表示未知，远程销毁需手填）。
  final String? addr;

  factory Peer.fromJson(Map<String, dynamic> json) => Peer(
        deviceId: (json['deviceId'] as String?) ?? '',
        name: (json['name'] as String?) ??
            (VsL10n.orNull?.stateUnknownDevice ?? '未知设备'),
        fingerprint: (json['fingerprint'] as String?) ?? '',
        addr: (json['addr'] as String?)?.trim().isEmpty ?? true
            ? null
            : (json['addr'] as String).trim(),
      );
}

/// 本机 P2P 状态（`p2p_status` JSON）。
class SyncStatus {
  const SyncStatus({
    this.deviceId = '',
    this.fingerprint = '',
    this.port = 0,
    this.name = '',
    this.alive = true,
    this.peers = const [],
    this.armedDestroy,
    this.events = const [],
  });

  final String deviceId;
  final String fingerprint;
  final int port;
  final String name;
  final bool alive;
  final List<Peer> peers;
  final ArmedDestroy? armedDestroy;
  final List<String> events;

  factory SyncStatus.fromJson(Map<String, dynamic> json) => SyncStatus(
        deviceId: (json['deviceId'] as String?) ?? '',
        fingerprint: (json['fingerprint'] as String?) ?? '',
        port: (json['port'] as num?)?.toInt() ?? 0,
        name: (json['name'] as String?) ?? '',
        alive: (json['alive'] as bool?) ?? true,
        peers: ((json['peers'] as List?) ?? const [])
            .whereType<Map>()
            .map((m) => Peer.fromJson(Map<String, dynamic>.from(m)))
            .toList(),
        armedDestroy: json['armedDestroy'] is Map
            ? ArmedDestroy.fromJson(
                Map<String, dynamic>.from(json['armedDestroy'] as Map))
            : null,
        events: ((json['events'] as List?) ?? const [])
            .whereType<String>()
            .toList(),
      );
}

/// 武装中的远程销毁（`status().armedDestroy`）。
class ArmedDestroy {
  const ArmedDestroy({required this.target, this.remainingMs = 0});

  final String target;
  final int remainingMs;

  factory ArmedDestroy.fromJson(Map<String, dynamic> json) => ArmedDestroy(
        target: (json['target'] as String?) ?? '',
        remainingMs: (json['remainingMs'] as num?)?.toInt() ?? 0,
      );
}

/// 配对起点（`pair_begin` 返回）：一次性邀请码 + 本机身份指纹。
class PairInvite {
  const PairInvite({
    required this.code,
    this.fingerprint = '',
    this.deviceId = '',
    this.port = 0,
    this.expiresMs,
  });

  final String code;
  final String fingerprint;
  final String deviceId;
  final int port;

  /// 引擎未返回过期时刻，客户端按邀请 TTL 保守估算（仅展示用途）。
  final int? expiresMs;

  factory PairInvite.fromJson(Map<String, dynamic> json) => PairInvite(
        code: (json['code'] as String?) ?? '',
        fingerprint: (json['fingerprint'] as String?) ?? '',
        deviceId: (json['deviceId'] as String?) ?? '',
        port: (json['port'] as num?)?.toInt() ?? 0,
      );
}

/// 远程销毁指令回执（`destroy_arm`）。
class DestroyReceipt {
  const DestroyReceipt({required this.sent, this.delayMs = 0});

  final bool sent;
  final int delayMs;

  factory DestroyReceipt.fromJson(Map<String, dynamic> json) => DestroyReceipt(
        sent: (json['sent'] as bool?) ?? false,
        delayMs: (json['delayMs'] as num?)?.toInt() ?? 0,
      );
}

/// 设备页展示模型：本机 + 已配对设备，附在线态与最近操作事件。
class DeviceOverview {
  const DeviceOverview({
    this.selfName = '',
    this.deviceId,
    this.onlinePeers = 0,
    this.totalPeers = 0,
    this.lastEvents = const [],
  });

  final String selfName;
  final String? deviceId;
  final int onlinePeers;
  final int totalPeers;
  final List<String> lastEvents;

  factory DeviceOverview.of(SyncStatus s) => DeviceOverview(
        selfName: s.name,
        deviceId: s.deviceId.isNotEmpty ? s.deviceId : null,
        onlinePeers: s.peers.length,
        totalPeers: s.peers.length,
        lastEvents: s.events,
      );
}
