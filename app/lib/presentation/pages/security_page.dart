import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/theme/design_tokens.dart';
import '../../core/theme/vs_motion.dart';
import '../../l10n/app_localizations.dart';
import '../../service/engine_service.dart';
import '../../state/models/p2p.dart';
import '../../state/notification_controller.dart';
import '../../state/session_controller.dart';
import '../widgets/vs_ui.dart';

/// 审计日志导出默认文件名（docs/05-06 §3.3：专用导出密钥加密落盘）。
const _kAuditExportName = 'vaultsync-audit.vsaudit';

/// 安全中心（P5-2）：整体状态徽章 + 六项检测 + 审计日志 + 隐写引擎开关 + 紧急销毁。
/// 视觉 1:1 对应原型 page-security（.secgrid / .detect / .audit-table / .wipe-card）。
///
/// 六项检测的数据口径（不编造数据）：
/// - 暴力破解：`cooldownRemainingMs()` 真实冷却（引擎按保险箱父目录域计数）；
/// - 异常设备：`status()` 的 peers / events 真实快照；
/// - 文件完整性、内存泄漏扫描：引擎无运行时扫描接口，属**设计保证**，只描述机制；
/// - 审计链完整性：`auditVerify()` 真实链式校验；
/// - 信令防重放：`status()` 的 alive / 事件条数（序号连续性由 Noise 信道保证）。
class SecurityPage extends ConsumerStatefulWidget {
  const SecurityPage({super.key});

  @override
  ConsumerState<SecurityPage> createState() => _SecurityPageState();
}

class _SecurityPageState extends ConsumerState<SecurityPage> {
  bool _loading = true;
  String? _error;

  /// 冷却剩余毫秒（`cooldownRemainingMs()`）。
  int _cooldownMs = 0;

  /// P2P 状态快照（peers / events / alive）。
  SyncStatus? _status;

  /// 全库完整性扫描结果（P5-2：真实逐块重算，而非「设计保证」）。
  Map<String, dynamic>? _scan;
  bool _scanning = false;

  /// 审计条目（倒序展示：最新在前）与本机链头。
  List<_AuditEntry> _entries = const [];
  String _head = '';

  /// 最近一次链式校验结果（`auditVerify()` 原始 JSON）。
  Map<String, dynamic>? _verify;

  /// 审计类别筛选：all / vault / device / session / security。
  String _filter = 'all';

  bool _auditBusy = false;
  bool _stego = false;
  bool _stegoBusy = false;

  @override
  void initState() {
    super.initState();
    Future.microtask(() {
      if (mounted) _load(initial: true);
    });
  }

  /// 当前会话句柄（仅解锁态可用）。
  Object? _handle() {
    final s = ref.read(sessionProvider);
    return s is SessionUnlocked ? s.sessionHandle : null;
  }

  NotificationController get _notif => ref.read(notificationProvider.notifier);

  /// 一次性拉取本页全部真实数据源（引擎无推送通道，刷新按钮与操作后调用）。
  Future<void> _load({bool initial = false}) async {
    final handle = _handle();
    if (handle == null) {
      if (mounted) {
        setState(() {
          _loading = false;
          _error = AppLocalizations.of(context).securityNeedUnlock;
        });
      }
      return;
    }
    if (initial) setState(() => _loading = true);
    final engine = ref.read(vaultEngineProvider);
    final cooldown = await engine.cooldownRemainingMs();
    final statusJson = await engine.status(handle);
    final list = await engine.auditList(handle);
    final verify = await engine.auditVerify(handle);
    final stego = await engine.stegoEnabled(handle);
    if (!mounted) return;
    final l = AppLocalizations.of(context);
    setState(() {
      _loading = false;
      _cooldownMs = cooldown;
      _status = statusJson == null ? null : SyncStatus.fromJson(statusJson);
      _verify = verify;
      _stego = stego;
      if (list == null) {
        _error = l.securityAuditLoadFailed;
      } else {
        _error = null;
        _entries = _AuditEntry.parse(list['entries']);
        _head = (list['head'] as String?) ?? '';
      }
    });
  }

  // ---- 审计操作 ----

  Future<void> _verifyChain() async {
    final l = AppLocalizations.of(context);
    final handle = _handle();
    if (handle == null) {
      _notif.warning(l.securityNeedUnlock);
      return;
    }
    setState(() => _auditBusy = true);
    final v = await ref.read(vaultEngineProvider).auditVerify(handle);
    if (!mounted) return;
    setState(() {
      _auditBusy = false;
      _verify = v;
    });
    if (v == null) {
      _notif.warning(l.securityAuditVerifyUnavailable);
      return;
    }
    if (v['ok'] == true) {
      _notif.ok(l.securityAuditVerifyOk((v['checked'] as num?)?.toInt() ?? 0));
    } else {
      _notif.danger(l.securityAuditVerifyFailed(
        (v['brokenAt'] as num?)?.toInt() ?? 0,
        (v['reason'] as String?) ?? '--',
      ));
    }
  }

  Future<void> _exportAudit() async {
    final l = AppLocalizations.of(context);
    final handle = _handle();
    if (handle == null) {
      _notif.warning(l.securityNeedUnlock);
      return;
    }
    final loc = await getSaveLocation(suggestedName: _kAuditExportName);
    if (loc == null) return;
    setState(() => _auditBusy = true);
    final err =
        await ref.read(vaultEngineProvider).auditExport(handle, loc.path);
    if (!mounted) return;
    setState(() => _auditBusy = false);
    if (err != null) {
      _notif.danger(l.securityAuditExportFailed(err));
      return;
    }
    _notif.ok(l.securityAuditExportedTo(loc.path));
    // 导出事件本身也记审计（docs/05-06 §3.3）：重新拉取列表。
    await _load();
  }

  // ---- 隐写引擎开关 ----

  Future<void> _toggleStego() async {
    final l = AppLocalizations.of(context);
    final handle = _handle();
    if (handle == null) {
      _notif.warning(l.securityNeedUnlock);
      return;
    }
    setState(() => _stegoBusy = true);
    final engine = ref.read(vaultEngineProvider);
    final err = await engine.stegoSetEnabled(handle, !_stego);
    if (!mounted) return;
    if (err != null) {
      setState(() => _stegoBusy = false);
      _notif.danger(l.securityStegoFailed(err));
      return;
    }
    // 以引擎回读为准（变更本身已由引擎记审计）。
    final on = await engine.stegoEnabled(handle);
    if (!mounted) return;
    setState(() {
      _stego = on;
      _stegoBusy = false;
    });
    _notif.ok(on ? l.securityStegoEnabled : l.securityStegoDisabled);
    await _load();
  }

  // ---- 紧急销毁 ----

  Future<void> _openDestroy() async {
    final l = AppLocalizations.of(context);
    final handle = _handle();
    if (handle == null) {
      _notif.warning(l.securityNeedUnlock);
      return;
    }
    final out = await showDialog<_DestroyOutcome>(
      context: context,
      barrierColor: context.vs.backdrop,
      builder: (_) => _DestroyDialog(
        handle: handle,
        peerCount: _status?.peers.length ?? 0,
      ),
    );
    if (!mounted || out == null || !out.destroyed) return;
    // 销毁后本机保险箱已不存在：立即回锁屏（路由守卫自动跳 /lock）。
    _notif.danger(l.securityDestroyDone);
    if (out.linked) {
      // 指令已入队：对端下次建立信道时投递并执行（收到 ACK 才出队）
      _notif.warning(out.queued > 0
          ? l.securityDestroyQueued(out.queued)
          : l.securityDestroyPeersNone);
    }
    ref.read(sessionProvider.notifier).lock();
  }

  // ---- 检测项 ----

  /// 六项检测（图标 / 语气 / 标题 / 副文案）。
  /// 全库完整性扫描（P5-2）：逐块重算 + GCM 认证，放在引擎侧后台 Isolate。
  Future<void> _runIntegrityScan() async {
    final handle = _handle();
    if (handle == null) return;
    final l = AppLocalizations.of(context);
    setState(() => _scanning = true);
    final r = await ref.read(vaultEngineProvider).verifyAll(handle);
    if (!mounted) return;
    setState(() {
      _scanning = false;
      _scan = r ?? const {'files': 0, 'checked': 0, 'failed': <dynamic>[]};
    });
    final failed = (_scan?['failed'] as List?)?.length ?? 0;
    if (r == null) {
      ref
          .read(notificationProvider.notifier)
          .danger(l.securityDetectIntegrityFailedRead);
    } else if (failed == 0) {
      ref.read(notificationProvider.notifier).ok(l.securityDetectIntegrityOk(
          (_scan!['checked'] as num?)?.toInt() ?? 0,
          (_scan!['files'] as num?)?.toInt() ?? 0));
    } else {
      ref.read(notificationProvider.notifier).danger(
          l.securityDetectIntegrityFailed(
              failed, (_scan!['files'] as num?)?.toInt() ?? 0));
    }
  }

  List<_Detect> _detections(AppLocalizations l) {
    final status = _status;
    final events = status?.events ?? const <String>[];
    final rejected = _firstRejected(events);
    final verify = _verify;
    final checked = (verify?['checked'] as num?)?.toInt() ?? 0;

    return [
      _Detect(
        icon: 'lock',
        tone: _cooldownMs > 0 ? VsTone.warn : VsTone.ok,
        title: l.securityDetectBrute,
        sub: _cooldownMs > 0
            ? l.securityDetectBruteCooldown(
                _cooldownMs ~/ 60000, (_cooldownMs % 60000) ~/ 1000)
            : l.securityDetectBruteOk,
      ),
      _Detect(
        icon: 'devices',
        tone: rejected == null ? VsTone.ok : VsTone.warn,
        title: l.securityDetectDevice,
        // 先看告警事件（被拒绝的配对尝试即使未配对上也要暴露），再看配对数量。
        sub: rejected != null
            ? l.securityDetectDeviceWarn(rejected)
            : (status == null || status.peers.isEmpty)
                ? l.securityDetectDeviceNone
                : l.securityDetectDeviceOk(status.peers.length),
      ),
      _Detect(
        icon: 'shield',
        tone: _scanning
            ? VsTone.info
            : _scan == null
                ? VsTone.mute
                : ((_scan!['failed'] as List?)?.isEmpty ?? true)
                    ? VsTone.ok
                    : VsTone.danger,
        title: l.securityDetectIntegrity,
        sub: _scanning
            ? l.securityDetectIntegrityScanning
            : _scan == null
                ? l.securityDetectIntegrityNotScanned
                : ((_scan!['failed'] as List?)?.isEmpty ?? true)
                    ? l.securityDetectIntegrityOk(
                        (_scan!['checked'] as num?)?.toInt() ?? 0,
                        (_scan!['files'] as num?)?.toInt() ?? 0)
                    : l.securityDetectIntegrityFailed(
                        (_scan!['failed'] as List).length,
                        (_scan!['files'] as num?)?.toInt() ?? 0),
        badge: _scan == null ? l.securityBadgeGuarantee : null,
        onTap: _scanning ? null : _runIntegrityScan,
      ),
      _Detect(
        icon: 'key',
        tone: VsTone.ok,
        title: l.securityDetectMemory,
        sub: l.securityDetectMemorySub,
        badge: l.securityBadgeGuarantee,
      ),
      _Detect(
        icon: 'history',
        tone: verify == null
            ? VsTone.mute
            : verify['ok'] == true
                ? VsTone.ok
                : VsTone.danger,
        title: l.securityDetectAudit,
        sub: verify == null
            ? l.securityDetectAuditUnknown
            : verify['ok'] == true
                ? l.securityDetectAuditOk(checked, _entries.length)
                : l.securityDetectAuditBroken(
                    (verify['brokenAt'] as num?)?.toInt() ?? 0,
                    (verify['reason'] as String?) ?? '--',
                  ),
      ),
      _Detect(
        icon: 'wifi',
        tone: status == null
            ? VsTone.mute
            : status.alive
                ? VsTone.ok
                : VsTone.warn,
        title: l.securityDetectSignal,
        sub: status == null
            ? l.securityDetectSignalUnknown
            : status.alive
                ? l.securityDetectSignalOk(events.length)
                : l.securityDetectSignalDown,
      ),
    ];
  }

  /// 引擎事件行形如「毫秒时间戳 + 文本」：取第一条含 reject/fail 的事件文本。
  String? _firstRejected(List<String> events) {
    for (final e in events) {
      final low = e.toLowerCase();
      if (low.contains('reject') || low.contains('fail')) {
        // 去掉前导毫秒时间戳（仅做展示裁剪，不改变事件内容）。
        final sp = e.indexOf(' ');
        return sp > 0 && int.tryParse(e.substring(0, sp)) != null
            ? e.substring(sp + 1)
            : e;
      }
    }
    return null;
  }

  /// 整体状态：任一 danger → 异常；任一 warn / 无法判定 → 注意；否则正常。
  (String, VsTone) _overall(AppLocalizations l, List<_Detect> items) {
    final neutral = items
        .map((d) => switch (d.tone) {
              VsTone.danger => 2,
              VsTone.warn => 1,
              VsTone.mute => 1,
              _ => 0,
            })
        .fold(0, (a, b) => a > b ? a : b);
    return switch (neutral) {
      2 => (l.securityOverallDanger, VsTone.danger),
      1 => (l.securityOverallWarn, VsTone.warn),
      _ => (l.securityOverallOk, VsTone.ok),
    };
  }

  @override
  Widget build(BuildContext context) {
    final v = context.vs;
    final l = AppLocalizations.of(context);
    final items = _detections(l);
    final (overallText, overallTone) = _overall(l, items);

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        VsPageHeader(
          title: l.securityTitle,
          sub: l.securitySub,
          actions: [
            VsBadge(overallText, tone: overallTone),
            VsIconButton(
                icon: 'sync',
                tooltip: l.securityRefresh,
                onPressed: _loading ? null : () => _load()),
          ],
        ),
        // 六项检测网格（原型 .secgrid：两列三行，行内等高）
        for (var i = 0; i < items.length; i += 2) ...[
          if (i > 0) const SizedBox(height: DesignTokens.sp3),
          IntrinsicHeight(
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Expanded(child: _detectCard(v, items[i])),
                const SizedBox(width: DesignTokens.sp3),
                Expanded(
                  child: i + 1 < items.length
                      ? _detectCard(v, items[i + 1])
                      : const SizedBox(),
                ),
              ],
            ),
          ),
        ],
        const SizedBox(height: DesignTokens.sp4),
        _auditCard(v, l),
        const SizedBox(height: DesignTokens.sp4),
        // 底部两卡：隐写引擎开关 + 紧急销毁（原型 .stego-card / .wipe-card）
        IntrinsicHeight(
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Expanded(child: _stegoCard(v, l)),
              const SizedBox(width: DesignTokens.sp3),
              Expanded(child: _wipeCard(v, l)),
              const SizedBox(width: DesignTokens.sp3),
              Expanded(child: _rekeyCard(v, l)),
            ],
          ),
        ),
      ],
    );
  }

  /// 单张检测卡（原型 .detect：32px 圆角图标块 + 标题 + 副文案）。
  Widget _detectCard(VsScheme v, _Detect d) {
    final card = Container(
      padding: const EdgeInsets.symmetric(horizontal: 15, vertical: 13),
      decoration: BoxDecoration(
        color: v.surface,
        borderRadius: BorderRadius.circular(DesignTokens.rMd),
        border: Border.all(color: v.borderSoft),
      ),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          VsMicTile(icon: d.icon, tone: d.tone, size: 32),
          const SizedBox(width: 11),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(
                  children: [
                    Flexible(
                      child: Text(d.title,
                          style: TextStyle(
                              fontSize: 13,
                              fontWeight: FontWeight.w600,
                              color: v.text),
                          overflow: TextOverflow.ellipsis),
                    ),
                    if (d.badge != null) ...[
                      const SizedBox(width: 6),
                      VsBadge(d.badge!, plain: true, tone: VsTone.mute),
                    ],
                  ],
                ),
                const SizedBox(height: 2),
                Text(d.sub,
                    style:
                        TextStyle(fontSize: 12, color: v.text2, height: 1.45)),
              ],
            ),
          ),
        ],
      ),
    );
    // 可点击项（全库完整性扫描）：加悬停提示与语义标签，非可点击项原样返回。
    if (d.onTap == null) return card;
    return MouseRegion(
      cursor: SystemMouseCursors.click,
      child: GestureDetector(
        onTap: d.onTap,
        child: Semantics(
          button: true,
          label: '${d.title} · ${d.sub}',
          child: card,
        ),
      ),
    );
  }

  /// 审计日志卡：校验 / 导出 + 类别筛选 + 表格 + 口径说明。
  Widget _auditCard(VsScheme v, AppLocalizations l) {
    final rows = _filter == 'all'
        ? _entries
        : _entries.where((e) => e.kind == _filter).toList();

    return VsCard(
      header: l.securityAuditTitle,
      headerExtra: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          if (_verify != null) ...[
            VsBadge(
              _verify!['ok'] == true
                  ? l.securityAuditChainOk
                  : l.securityAuditChainBroken,
              tone: _verify!['ok'] == true ? VsTone.ok : VsTone.danger,
              plain: true,
            ),
            const SizedBox(width: 8),
          ],
          VsButton(
            label: l.securityAuditVerify,
            icon: 'check',
            tone: VsBtnTone.ghost,
            small: true,
            onPressed: _auditBusy ? null : _verifyChain,
          ),
          const SizedBox(width: 6),
          VsButton(
            label: l.securityAuditExport,
            icon: 'share',
            tone: VsBtnTone.ghost,
            small: true,
            onPressed: _auditBusy ? null : _exportAudit,
          ),
        ],
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Wrap(
            spacing: 6,
            runSpacing: 6,
            children: [
              for (final k in _kinds)
                VsChip(_filterLabel(l, k),
                    on: _filter == k, onTap: () => setState(() => _filter = k)),
            ],
          ),
          const SizedBox(height: DesignTokens.sp3),
          if (_loading)
            SizedBox(
              height: 120,
              child: Center(
                child: SizedBox(
                  width: 26,
                  height: 26,
                  child: CircularProgressIndicator(
                      strokeWidth: 2.4, color: v.gold),
                ),
              ),
            )
          else if (_error != null)
            Padding(
              padding: const EdgeInsets.symmetric(vertical: 8),
              child: Text(_error!,
                  style: TextStyle(fontSize: 12.5, color: v.danger)),
            )
          else ...[
            _auditHead(v, l),
            if (rows.isEmpty)
              Padding(
                padding: const EdgeInsets.symmetric(vertical: 18),
                child: Center(
                  child: Text(l.securityAuditEmpty,
                      style: TextStyle(fontSize: 12.5, color: v.text3)),
                ),
              )
            else
              for (final e in rows)
                _AuditRow(cells: [
                  SizedBox(
                    width: 92,
                    child: Text(_fmtTime(e.tsMs),
                        style: TextStyle(
                            fontSize: 12, color: v.text2, height: 1.5)),
                  ),
                  SizedBox(
                    width: 62,
                    child: Align(
                      alignment: Alignment.centerLeft,
                      child: VsBadge(_kindLabel(l, e.kind),
                          plain: true, tone: VsTone.mute),
                    ),
                  ),
                  Expanded(
                    child: Text(e.detail,
                        style: TextStyle(
                            fontSize: 12.5, color: v.text, height: 1.5),
                        overflow: TextOverflow.ellipsis),
                  ),
                  SizedBox(
                    width: 44,
                    child: Text('${e.seq}',
                        textAlign: TextAlign.right,
                        style: TextStyle(
                            fontSize: 12,
                            fontFamily: DesignTokens.monoFamily,
                            color: v.text3)),
                  ),
                  const SizedBox(width: 16),
                  SizedBox(
                    width: 96,
                    child: SelectableText(
                      _shortHash(e.hash),
                      style: TextStyle(
                          fontSize: 11.5,
                          fontFamily: DesignTokens.monoFamily,
                          color: v.text3),
                    ),
                  ),
                ]),
            if (_head.isNotEmpty) ...[
              const SizedBox(height: 8),
              Text(l.securityAuditHead(_shortHash(_head)),
                  style: TextStyle(
                      fontSize: 11.5,
                      fontFamily: DesignTokens.monoFamily,
                      color: v.text3)),
            ],
          ],
          const SizedBox(height: DesignTokens.sp3),
          VsNote(l.securityAuditNote, tone: VsTone.info),
        ],
      ),
    );
  }

  Widget _auditHead(VsScheme v, AppLocalizations l) {
    Widget th(String t, {TextAlign align = TextAlign.left}) => Text(t,
        textAlign: align,
        style: TextStyle(
            fontSize: 11.5,
            fontWeight: FontWeight.w600,
            letterSpacing: 0.05,
            color: v.text3));
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 4, vertical: 8),
      decoration: BoxDecoration(
          border: Border(bottom: BorderSide(color: v.borderSoft))),
      child: Row(
        children: [
          SizedBox(width: 92, child: th(l.securityColTime)),
          SizedBox(width: 62, child: th(l.securityColCategory)),
          Expanded(child: th(l.securityColAction)),
          SizedBox(
              width: 44, child: th(l.securityColSeq, align: TextAlign.right)),
          const SizedBox(width: 16),
          SizedBox(width: 96, child: th(l.securityColHash)),
        ],
      ),
    );
  }

  /// 隐写引擎开关卡（原型 .stego-card）。
  Widget _stegoCard(VsScheme v, AppLocalizations l) {
    return VsCard(
      borderColor: v.encSoft,
      child: Row(
        children: [
          const VsMicTile(icon: 'stego', tone: VsTone.enc),
          const SizedBox(width: 12),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(l.securityStegoTitle,
                    style: TextStyle(
                        fontSize: 13.5,
                        fontWeight: FontWeight.w600,
                        color: v.text)),
                const SizedBox(height: 2),
                Text(_stego ? l.securityStegoSubOn : l.securityStegoSubOff,
                    style:
                        TextStyle(fontSize: 12, color: v.text2, height: 1.45)),
              ],
            ),
          ),
          const SizedBox(width: 12),
          VsButton(
            label: _stego ? l.securityStegoDisable : l.securityStegoEnable,
            small: true,
            onPressed: _stegoBusy ? null : _toggleStego,
          ),
        ],
      ),
    );
  }

  /// 紧急销毁入口卡（原型 .wipe-card：danger 渐变 + 红边）。
  Widget _wipeCard(VsScheme v, AppLocalizations l) {
    return VsCard(
      borderColor: v.dangerSoft,
      gradient: LinearGradient(
        begin: Alignment.topCenter,
        end: Alignment.bottomCenter,
        colors: [v.dangerSoft, Colors.transparent],
        stops: const [0, 0.6],
      ),
      child: Row(
        children: [
          const VsMicTile(icon: 'oct', tone: VsTone.danger),
          const SizedBox(width: 12),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(l.securityDestroyTitle,
                    style: TextStyle(
                        fontSize: 13.5,
                        fontWeight: FontWeight.w600,
                        color: v.text)),
                const SizedBox(height: 2),
                Text(l.securityDestroySub,
                    style:
                        TextStyle(fontSize: 12, color: v.text2, height: 1.45)),
              ],
            ),
          ),
          const SizedBox(width: 12),
          VsButton(
            label: l.securityDestroyNow,
            tone: VsBtnTone.danger,
            small: true,
            onPressed: _openDestroy,
          ),
        ],
      ),
    );
  }

  /// 主密钥全库轮换卡（P5-4，docs/07 §三 的本地部分）。
  Widget _rekeyCard(VsScheme v, AppLocalizations l) {
    return VsCard(
      borderColor: v.goldSoft,
      child: Row(
        children: [
          const VsMicTile(icon: 'key', tone: VsTone.gold),
          const SizedBox(width: 12),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(l.securityRekeyTitle,
                    style: TextStyle(
                        fontSize: 13.5,
                        fontWeight: FontWeight.w600,
                        color: v.text)),
                const SizedBox(height: 2),
                Text(l.securityRekeySub,
                    style:
                        TextStyle(fontSize: 12, color: v.text2, height: 1.45)),
              ],
            ),
          ),
          const SizedBox(width: 12),
          VsButton(
            label: l.securityRekeyNow,
            small: true,
            onPressed: _openRekey,
          ),
        ],
      ),
    );
  }

  /// 主密钥轮换流程（强确认：主密码 + 确认短语；成功后立即回锁屏）。
  Future<void> _openRekey() async {
    final l = AppLocalizations.of(context);
    final handle = _handle();
    if (handle == null) {
      _notif.warning(l.securityNeedUnlock);
      return;
    }
    final ok = await showDialog<bool>(
      context: context,
      barrierColor: context.vs.backdrop,
      builder: (_) => _RekeyDialog(handle: handle),
    );
    if (!mounted || ok != true) return;
    _notif.danger(l.securityRekeyDone);
    // 会话内的 MK 副本已陈旧：必须立即锁定并重新解锁
    ref.read(sessionProvider.notifier).lock();
  }

  // ---- 展示辅助 ----

  static const _kinds = ['all', 'vault', 'device', 'session', 'security'];

  String _filterLabel(AppLocalizations l, String k) => switch (k) {
        'all' => l.securityFilterAll,
        _ => _kindLabel(l, k),
      };

  String _kindLabel(AppLocalizations l, String kind) => switch (kind) {
        'vault' => l.securityCatVault,
        'device' => l.securityCatDevice,
        'session' => l.securityCatSession,
        'security' => l.securityCatSecurity,
        _ => kind.isEmpty ? '--' : kind,
      };

  /// 链哈希前 10 位 + 省略号（原型 .mono 列）。
  static String _shortHash(String hash) =>
      hash.length > 10 ? '${hash.substring(0, 10)}…' : hash;

  /// tsMs → MM-DD HH:mm:ss（原型审计表时间列）。
  static String _fmtTime(int ms) {
    final d = DateTime.fromMillisecondsSinceEpoch(ms);
    String two(int x) => x.toString().padLeft(2, '0');
    return '${two(d.month)}-${two(d.day)} '
        '${two(d.hour)}:${two(d.minute)}:${two(d.second)}';
  }
}

/// 单项检测结果。
class _Detect {
  const _Detect({
    required this.icon,
    required this.tone,
    required this.title,
    required this.sub,
    this.badge,
    this.onTap,
  });

  final String icon;
  final VsTone tone;
  final String title;
  final String sub;

  /// 设计保证类检测的附加徽章（无运行时扫描接口，只能标注机制）。
  final String? badge;

  /// 可点击的检测项（目前只有全库完整性扫描）。
  final VoidCallback? onTap;
}

/// 主密钥轮换弹窗：主密码 + 确认短语双把关（docs/07 §三 的强确认要求）。
class _RekeyDialog extends ConsumerStatefulWidget {
  const _RekeyDialog({required this.handle});

  final Object handle;

  @override
  ConsumerState<_RekeyDialog> createState() => _RekeyDialogState();
}

class _RekeyDialogState extends ConsumerState<_RekeyDialog> {
  final _pwCtl = TextEditingController();
  final _phraseCtl = TextEditingController();
  bool _busy = false;
  String? _error;

  @override
  void dispose() {
    _pwCtl.dispose();
    _phraseCtl.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final v = context.vs;
    final l = AppLocalizations.of(context);
    final phraseOk = _phraseCtl.text.trim() == l.securityRekeyPhraseWord;

    return Dialog(
      backgroundColor: Colors.transparent,
      child: Container(
        width: 480,
        decoration: BoxDecoration(
          color: v.surface,
          borderRadius: BorderRadius.circular(DesignTokens.rLg),
          border: Border.all(color: v.goldSoft),
          boxShadow: v.shadow3,
        ),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Padding(
              padding: const EdgeInsets.fromLTRB(20, 16, 12, 0),
              child: Row(
                children: [
                  const VsMicTile(icon: 'key', tone: VsTone.gold),
                  const SizedBox(width: 10),
                  Expanded(
                    child: Text(l.securityRekeyTitle,
                        style: TextStyle(
                            fontSize: 15.5,
                            fontWeight: FontWeight.w600,
                            color: v.text)),
                  ),
                  VsIconButton(
                      icon: 'close',
                      tooltip: l.securityDestroyCancel,
                      onPressed: _busy
                          ? null
                          : () => Navigator.of(context).pop(false)),
                ],
              ),
            ),
            Flexible(
              child: SingleChildScrollView(
                padding: const EdgeInsets.all(20),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    VsNote(l.securityRekeyNote, tone: VsTone.warn),
                    const SizedBox(height: 14),
                    Text(l.securityRekeyPw,
                        style: const TextStyle(
                            fontSize: 12.5, fontWeight: FontWeight.w500)),
                    const SizedBox(height: 6),
                    TextField(
                      controller: _pwCtl,
                      obscureText: true,
                      autofocus: true,
                      onChanged: (_) => setState(() {}),
                    ),
                    const SizedBox(height: 14),
                    Text(l.securityRekeyPhrase,
                        style: const TextStyle(
                            fontSize: 12.5, fontWeight: FontWeight.w500)),
                    const SizedBox(height: 6),
                    TextField(
                      controller: _phraseCtl,
                      onChanged: (_) => setState(() {}),
                    ),
                    if (_error != null) ...[
                      const SizedBox(height: 12),
                      Text(_error!, style: TextStyle(color: v.danger)),
                    ],
                  ],
                ),
              ),
            ),
            Container(
              padding: const EdgeInsets.fromLTRB(20, 14, 20, 18),
              decoration: BoxDecoration(
                  border: Border(top: BorderSide(color: v.borderSoft))),
              child: Row(
                mainAxisAlignment: MainAxisAlignment.end,
                children: [
                  VsButton(
                      label: l.securityDestroyCancel,
                      tone: VsBtnTone.ghost,
                      onPressed: _busy
                          ? null
                          : () => Navigator.of(context).pop(false)),
                  const SizedBox(width: 8),
                  VsButton(
                    label: _busy ? l.settingsUpdateChecking : l.securityRekeyDo,
                    tone: VsBtnTone.gold,
                    onPressed: !phraseOk || _busy || _pwCtl.text.isEmpty
                        ? null
                        : _submit,
                  ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }

  Future<void> _submit() async {
    final l = AppLocalizations.of(context);
    setState(() {
      _busy = true;
      _error = null;
    });
    final err = await ref
        .read(vaultEngineProvider)
        .rotateMk(widget.handle, _pwCtl.text);
    if (!mounted) return;
    if (err != null) {
      setState(() {
        _busy = false;
        _error = l.securityRekeyFailed(err);
      });
      return;
    }
    Navigator.of(context).pop(true);
  }
}

/// 一条审计记录（引擎 `auditList` entries[]）。
class _AuditEntry {
  const _AuditEntry({
    required this.seq,
    required this.tsMs,
    required this.kind,
    required this.detail,
    required this.hash,
  });

  final int seq;
  final int tsMs;
  final String kind;
  final String detail;
  final String hash;

  /// 引擎按 seq 升序返回，这里倒序（最新在前）便于阅读。
  static List<_AuditEntry> parse(Object? raw) {
    if (raw is! List) return const [];
    final out = <_AuditEntry>[];
    for (final item in raw) {
      if (item is! Map) continue;
      out.add(_AuditEntry(
        seq: (item['seq'] as num?)?.toInt() ?? 0,
        tsMs: (item['tsMs'] as num?)?.toInt() ?? 0,
        kind: (item['kind'] as String?) ?? '',
        detail: (item['detail'] as String?) ?? '',
        hash: (item['hash'] as String?) ?? '',
      ));
    }
    return out.reversed.toList();
  }
}

/// 审计表格行（原型 `.audit-table tbody tr:hover` 的悬停底色）。
class _AuditRow extends StatefulWidget {
  const _AuditRow({required this.cells});
  final List<Widget> cells;

  @override
  State<_AuditRow> createState() => _AuditRowState();
}

class _AuditRowState extends State<_AuditRow> {
  bool _hover = false;

  @override
  Widget build(BuildContext context) {
    final v = context.vs;
    return MouseRegion(
      onEnter: (_) => setState(() => _hover = true),
      onExit: (_) => setState(() => _hover = false),
      child: AnimatedContainer(
        duration: VsMotion.ms(context, 120),
        padding: const EdgeInsets.symmetric(horizontal: 4, vertical: 8),
        decoration: BoxDecoration(
          color: _hover ? v.surfaceAlt : Colors.transparent,
          border: Border(bottom: BorderSide(color: v.borderSoft)),
        ),
        child: Row(children: widget.cells),
      ),
    );
  }
}

/// 销毁模式：本机 / 全设备联动（原型 #ov-gwipe 二选一）。
enum _WipeMode { local, all }

/// 销毁流程结果（通知与回锁由页面统一处理，弹窗只负责执行）。
class _DestroyOutcome {
  const _DestroyOutcome({
    required this.destroyed,
    required this.linked,
    this.peerCount = 0,
    this.queued = 0,
  });

  final bool destroyed;

  /// 是否选择了「全设备联动销毁」。
  final bool linked;
  final int peerCount;

  /// 已入队的销毁指令条数（每台已配对设备一条）。
  final int queued;
}

/// 紧急销毁弹窗（原型 #ov-gwipe）：模式二选一 + 延迟销毁说明 + 先导出审计 + 确认短语。
class _DestroyDialog extends ConsumerStatefulWidget {
  const _DestroyDialog({required this.handle, required this.peerCount});

  final Object handle;
  final int peerCount;

  @override
  ConsumerState<_DestroyDialog> createState() => _DestroyDialogState();
}

class _DestroyDialogState extends ConsumerState<_DestroyDialog> {
  _WipeMode _mode = _WipeMode.local;
  bool _exportFirst = false;
  bool _busy = false;
  String? _error;
  final _phraseCtl = TextEditingController();

  @override
  void dispose() {
    _phraseCtl.dispose();
    super.dispose();
  }

  /// 确认语本身取自本地化：英文界面下也要能输入被接受的词（同 devices_page）。
  bool _phraseOk(AppLocalizations l) =>
      _phraseCtl.text.trim() == l.securityDestroyPhrase;

  Future<void> _submit() async {
    final l = AppLocalizations.of(context);
    if (!_phraseOk(l)) return;
    setState(() {
      _busy = true;
      _error = null;
    });

    // 1) 可选：先导出审计日志（数据目录连同审计会被一并销毁）。
    if (_exportFirst) {
      final loc = await getSaveLocation(suggestedName: _kAuditExportName);
      if (!mounted) return;
      if (loc == null) {
        setState(() => _busy = false);
        ref
            .read(notificationProvider.notifier)
            .warning(l.securityDestroyExportCancelled);
        return;
      }
      final err = await ref
          .read(vaultEngineProvider)
          .auditExport(widget.handle, loc.path);
      if (!mounted) return;
      if (err != null) {
        setState(() {
          _busy = false;
          _error = l.securityAuditExportFailed(err);
        });
        return;
      }
      ref
          .read(notificationProvider.notifier)
          .ok(l.securityDestroyExported(loc.path));
    }

    // 2) 全设备联动：先把签名指令入队（离线设备下次上线时投递），再擦本机。
    var queued = 0;
    if (_mode == _WipeMode.all) {
      final ok = await showVsModal<bool>(
        context: context,
        title: l.securityDestroyAllConfirmTitle,
        icon: 'oct',
        micTone: VsTone.danger,
        width: 460,
        body: (_) => VsNote(
          l.securityDestroyAllConfirmBody(widget.peerCount),
          tone: VsTone.danger,
        ),
        actions: (ctx) => [
          VsButton(
              label: l.securityDestroyCancel,
              tone: VsBtnTone.ghost,
              onPressed: () => Navigator.of(ctx).pop(false)),
          VsButton(
              label: l.securityDestroyConfirm,
              tone: VsBtnTone.danger,
              onPressed: () => Navigator.of(ctx).pop(true)),
        ],
      );
      if (!mounted || ok != true) {
        if (mounted) setState(() => _busy = false);
        return;
      }
      // 入队：每台已配对设备一份签名指令，投递与执行由对端在建立信道时完成
      final r = await ref
          .read(vaultEngineProvider)
          // 延迟 0 = 对端下次上线即刻执行（docs/05-06 §5.1 高优先级队列语义）
          .destroyQueueAll(widget.handle, 0);
      queued = (r?['queued'] as num?)?.toInt() ?? 0;
      if (!mounted) return;
    }

    // 3) 本机加密擦除（销毁成功即保险箱不存在，调用方须立即回锁）。
    final err = await ref
        .read(vaultEngineProvider)
        .destroyLocal(widget.handle, secure: true);
    if (!mounted) return;
    if (err != null) {
      setState(() {
        _busy = false;
        _error = l.securityDestroyFailed(err);
      });
      return;
    }
    Navigator.of(context).pop(_DestroyOutcome(
      destroyed: true,
      linked: _mode == _WipeMode.all,
      peerCount: widget.peerCount,
      queued: queued,
    ));
  }

  @override
  Widget build(BuildContext context) {
    final v = context.vs;
    final l = AppLocalizations.of(context);
    return VsModalScaffold(
      title: l.securityDestroyDialogTitle,
      sub: l.securityDestroyDialogSub,
      icon: 'oct',
      micTone: VsTone.danger,
      width: 520,
      actions: (ctx) => [
        VsButton(
            label: l.securityDestroyCancel,
            tone: VsBtnTone.ghost,
            onPressed: _busy ? null : () => Navigator.of(ctx).pop()),
        VsButton(
            label: _busy ? l.securityDestroyRunning : l.securityDestroyEnter,
            tone: VsBtnTone.danger,
            onPressed: _busy || !_phraseOk(l) ? null : _submit),
      ],
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          VsRadioCard<_WipeMode>(
            value: _WipeMode.local,
            groupValue: _mode,
            onChanged: (m) => setState(() => _mode = m ?? _WipeMode.local),
            title: l.securityDestroyModeLocal,
            sub: l.securityDestroyModeLocalSub,
          ),
          VsRadioCard<_WipeMode>(
            value: _WipeMode.all,
            groupValue: _mode,
            onChanged: (m) => setState(() => _mode = m ?? _WipeMode.local),
            title: l.securityDestroyModeAll,
            sub: l.securityDestroyModeAllSub,
          ),
          if (_mode == _WipeMode.all) ...[
            const SizedBox(height: 4),
            VsNote(l.securityDestroyPeersHint, tone: VsTone.warn, icon: 'warn'),
          ],
          const SizedBox(height: 12),
          // 延迟销毁语义（docs/05-06 §五）
          VsNote(l.securityDestroyNote, tone: VsTone.danger),
          const SizedBox(height: 12),
          Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Padding(
                padding: const EdgeInsets.only(top: 1),
                child: Checkbox(
                  value: _exportFirst,
                  materialTapTargetSize: MaterialTapTargetSize.shrinkWrap,
                  visualDensity: VisualDensity.compact,
                  onChanged: _busy
                      ? null
                      : (val) => setState(() => _exportFirst = val ?? false),
                ),
              ),
              const SizedBox(width: 10),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(l.securityDestroyExportFirst,
                        style: TextStyle(fontSize: 13, color: v.text)),
                    Text(l.securityDestroyExportFirstSub,
                        style: TextStyle(fontSize: 12, color: v.text3)),
                  ],
                ),
              ),
            ],
          ),
          const SizedBox(height: 12),
          Padding(
            padding: const EdgeInsets.only(bottom: 6),
            child: Text(
              l.securityDestroyConfirmInput(l.securityDestroyPhrase),
              style:
                  const TextStyle(fontSize: 12.5, fontWeight: FontWeight.w500),
            ),
          ),
          TextField(
            controller: _phraseCtl,
            enabled: !_busy,
            onChanged: (_) => setState(() {}),
            decoration: InputDecoration(hintText: l.securityDestroyPhrase),
          ),
          if (_error != null) ...[
            const SizedBox(height: 10),
            Text(_error!, style: TextStyle(fontSize: 12.5, color: v.danger)),
          ],
        ],
      ),
    );
  }
}
