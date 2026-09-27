import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/theme/design_tokens.dart';
import '../../l10n/app_localizations.dart';
import '../../state/models/p2p.dart';
import '../../state/sync_controller.dart';
import '../widgets/vs_ui.dart';

/// 同步页（P4）：统计卡网格 + 事件时间线 + 最新摘要 + 冲突提示。
/// 视觉 1:1 对应原型 page-sync（statgrid / xfer 行样式）。
/// 引擎当前无逐项传输进度，事件时间线与摘要为真实数据，分块矩阵待 P5 提供。
class SyncPage extends ConsumerStatefulWidget {
  const SyncPage({super.key});

  @override
  ConsumerState<SyncPage> createState() => _SyncPageState();
}

class _SyncPageState extends ConsumerState<SyncPage> {
  @override
  void initState() {
    super.initState();
    Future.microtask(() {
      if (!mounted) return;
      final ctl = ref.read(syncProvider.notifier);
      ctl.refresh();
      ctl.refreshConflicts();
      // 引擎无推送通道：页面可见期间按周期拉取快照（P4-3 事件流）。
      ctl.startWatch();
    });
  }

  @override
  void dispose() {
    // 离开页面即停止轮询，避免后台空转。
    ref.read(syncProvider.notifier).stopWatch();
    super.dispose();
  }

  Future<void> _openSyncDialog() async {
    final mode = await showDialog<_SyncMode>(
      context: context,
      barrierColor: context.vs.backdrop,
      builder: (_) => const _SyncModeDialog(),
    );
    if (mode == null) return;
    final notifier = ref.read(syncProvider.notifier);
    if (mode.type == _SyncKind.direct) {
      await notifier.runSync(mode.addr);
    } else {
      await notifier.runSyncRelay(mode.relay, mode.room, mode.token);
    }
  }

  @override
  Widget build(BuildContext context) {
    final v = context.vs;
    final l = AppLocalizations.of(context);
    final state = ref.watch(syncProvider);
    final summary = state.lastSummary;

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        VsPageHeader(
          title: l.syncPageTitle,
          sub: l.syncPageHeaderSub,
          actions: [
            VsIconButton(
                icon: 'sync',
                tooltip: l.syncPageRefresh,
                onPressed: state.running
                    ? null
                    : () => ref.read(syncProvider.notifier).refresh()),
            VsButton(
              label: state.running ? l.syncPageSyncing : l.syncPageSyncNow,
              icon: 'sync',
              tone: VsBtnTone.primary,
              onPressed: state.running ? null : _openSyncDialog,
            ),
          ],
        ),
        // 统计卡网格（.statgrid）
        Row(
          children: [
            Expanded(
                child: _stat(v, 'down', VsTone.ok, summary?.pulled.length ?? 0,
                    l.syncPageStatPulledFiles)),
            const SizedBox(width: DesignTokens.sp3),
            Expanded(
                child: _stat(v, 'up', VsTone.gold, summary?.pushed.length ?? 0,
                    l.syncPageStatPushedFiles)),
            const SizedBox(width: DesignTokens.sp3),
            Expanded(
                child: _stat(v, 'x', VsTone.info, summary?.deleted.length ?? 0,
                    l.syncPageStatDeleted)),
            const SizedBox(width: DesignTokens.sp3),
            Expanded(
                child: _stat(v, 'warn', VsTone.danger,
                    summary?.conflicts.length ?? 0, l.syncPageStatConflicts)),
          ],
        ),
        const SizedBox(height: DesignTokens.sp4),
        if (summary != null && summary.conflicts.isNotEmpty) ...[
          _conflictBanner(v, l, summary.conflicts),
          const SizedBox(height: DesignTokens.sp4),
        ],
        // 事件卡（.card）
        VsCard(
          header: l.syncPageEvents,
          headerExtra: Text(l.syncPageEventsRecent(state.events.length),
              style: TextStyle(fontSize: 12, color: v.text3)),
          child: state.events.isEmpty
              ? Padding(
                  padding: const EdgeInsets.symmetric(vertical: 8),
                  child: Text(l.syncPageNoEvents,
                      style: TextStyle(fontSize: 12.5, color: v.text3)))
              : Column(
                  children: [
                    for (final e in state.events)
                      Container(
                        padding: const EdgeInsets.symmetric(
                            horizontal: 4, vertical: 8),
                        decoration: BoxDecoration(
                          border:
                              Border(bottom: BorderSide(color: v.borderSoft)),
                        ),
                        child: Row(
                          children: [
                            Container(
                              width: 6,
                              height: 6,
                              margin: const EdgeInsets.only(right: 10),
                              decoration: BoxDecoration(
                                color: v.blue,
                                shape: BoxShape.circle,
                              ),
                            ),
                            Expanded(
                              child: Text(e,
                                  style: TextStyle(
                                      fontSize: 12.5,
                                      fontFamily: DesignTokens.monoFamily,
                                      color: v.text2)),
                            ),
                          ],
                        ),
                      ),
                  ],
                ),
        ),
        const SizedBox(height: DesignTokens.sp4),
        // 摘要卡
        VsCard(
          header: l.syncPageLatestSummary,
          headerExtra: summary == null || summary.isEmpty
              ? null
              : Text(_summaryLine(l, summary),
                  style: TextStyle(fontSize: 12, color: v.text3)),
          child: summary == null || summary.isEmpty
              ? Padding(
                  padding: const EdgeInsets.symmetric(vertical: 8),
                  child: Text(l.syncPageNeverSynced,
                      style: TextStyle(fontSize: 12.5, color: v.text3)))
              : Wrap(
                  spacing: 24,
                  runSpacing: 12,
                  children: [
                    _summaryItem(v, 'check', VsTone.ok,
                        l.syncPageSummaryMerged(summary.merged.length)),
                    _summaryItem(v, 'down', VsTone.ok,
                        l.syncPageSummaryPulled(summary.pulled.length)),
                    _summaryItem(v, 'up', VsTone.gold,
                        l.syncPageSummaryPushed(summary.pushed.length)),
                    _summaryItem(v, 'x', VsTone.info,
                        l.syncPageSummaryDeleted(summary.deleted.length)),
                    _summaryItem(v, 'warn', VsTone.danger,
                        l.syncPageSummaryConflicts(summary.conflicts.length)),
                  ],
                ),
        ),
        const SizedBox(height: DesignTokens.sp4),
        // 同步状态（真实：引擎为阻塞式单次传输，无排队队列与逐块回调）
        VsCard(
          header: l.syncPageStatus,
          headerExtra: state.watching
              ? VsBadge(l.syncPageLive, tone: VsTone.ok, plain: true)
              : null,
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Row(
                children: [
                  Container(
                    width: 8,
                    height: 8,
                    margin: const EdgeInsets.only(right: 9),
                    decoration: BoxDecoration(
                      color: state.running ? v.gold : v.ok,
                      shape: BoxShape.circle,
                    ),
                  ),
                  Expanded(
                    child: Text(
                      state.running
                          ? l.syncPageStatusRunning
                          : l.syncPageStatusIdle,
                      style: TextStyle(
                          fontSize: 13,
                          fontWeight: FontWeight.w500,
                          color: v.text),
                    ),
                  ),
                  if (state.selfName.isNotEmpty)
                    Text(
                        l.syncPageSelfPeers(state.selfName, state.peers.length),
                        style: TextStyle(fontSize: 12, color: v.text3)),
                ],
              ),
              const SizedBox(height: 10),
              Text(
                l.syncPageEngineNote,
                style: TextStyle(fontSize: 12, color: v.text3),
              ),
            ],
          ),
        ),
        const SizedBox(height: DesignTokens.sp4),
        // 冲突处理（P4-3）：扫描保险箱内的 .conflict-<ts> 副本，逐个二选一
        VsCard(
          header: l.syncPageConflictSection,
          headerExtra: state.conflictsLoading
              ? Text(l.syncPageScanning,
                  style: TextStyle(fontSize: 12, color: v.text3))
              : Text(l.syncPagePendingCount(state.conflicts.length),
                  style: TextStyle(fontSize: 12, color: v.text3)),
          child: state.conflicts.isEmpty
              ? Padding(
                  padding: const EdgeInsets.symmetric(vertical: 8),
                  child: Text(l.syncPageNoConflicts,
                      style: TextStyle(fontSize: 12.5, color: v.text3)),
                )
              : Column(
                  children: [
                    for (final c in state.conflicts) _conflictRow(v, l, c),
                    const SizedBox(height: 10),
                    VsNote(
                      l.syncPageConflictNote,
                      tone: VsTone.info,
                    ),
                  ],
                ),
        ),
        const SizedBox(height: DesignTokens.sp4),
        // 分块详情占位（P5 提供逐块回调后的 blkgrid 位置）
        VsCard(
          header: l.syncPageChunkDetails,
          headerExtra: const VsBadge('P5', tone: VsTone.mute, plain: true),
          child: Text(l.syncPageChunkDetailsNote,
              style: TextStyle(fontSize: 12.5, color: v.text3)),
        ),
      ],
    );
  }

  /// 单个冲突对：文件名 + 时间戳 + 二选一操作。
  Widget _conflictRow(VsScheme v, AppLocalizations l, ConflictPair c) {
    final busy = ref.watch(syncProvider).running;
    return Container(
      margin: const EdgeInsets.only(bottom: 10),
      padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 12),
      decoration: BoxDecoration(
        color: v.bg,
        borderRadius: BorderRadius.circular(DesignTokens.rSm),
        border: Border.all(color: v.warnSoft),
      ),
      child: Row(
        children: [
          AppIcon('warn', size: 17, color: v.warn),
          const SizedBox(width: 12),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(c.originalName,
                    style: TextStyle(
                        fontSize: 13,
                        fontWeight: FontWeight.w600,
                        color: v.text),
                    overflow: TextOverflow.ellipsis),
                const SizedBox(height: 3),
                Text(
                  c.stamp.isEmpty
                      ? l.syncPageConflictCopyId(c.copyId)
                      : (c.originalId == null
                          ? l.syncPageConflictPeerGone(c.stamp)
                          : l.syncPageConflictStoredAt(c.stamp)),
                  style: TextStyle(fontSize: 11.5, color: v.text3),
                ),
              ],
            ),
          ),
          const SizedBox(width: 12),
          VsButton(
            label: l.syncPageKeepCopy,
            tone: VsBtnTone.ghost,
            small: true,
            onPressed: busy
                ? null
                : () => ref
                    .read(syncProvider.notifier)
                    .resolveConflict(c, keepCopy: true),
          ),
          const SizedBox(width: 8),
          VsButton(
            label: l.syncPageDropCopy,
            tone: VsBtnTone.dangerGhost,
            small: true,
            onPressed: busy
                ? null
                : () => ref
                    .read(syncProvider.notifier)
                    .resolveConflict(c, keepCopy: false),
          ),
        ],
      ),
    );
  }

  String _summaryLine(AppLocalizations l, SyncSummarySnapshot s) =>
      l.syncPageSummaryLine(s.pulled.length, s.pushed.length, s.deleted.length,
          s.conflicts.length);

  Widget _stat(VsScheme v, String icon, VsTone tone, int num, String label) {
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 15, vertical: 13),
      decoration: BoxDecoration(
        color: v.surface,
        borderRadius: BorderRadius.circular(DesignTokens.rMd),
        border: Border.all(color: v.borderSoft),
      ),
      child: Row(
        children: [
          VsMicTile(icon: icon, tone: tone),
          const SizedBox(width: 12),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text('$num',
                    style: TextStyle(
                        fontSize: 18,
                        fontWeight: FontWeight.w700,
                        height: 1.2,
                        color: v.text)),
                Text(label,
                    style: TextStyle(fontSize: 12, color: v.text2),
                    overflow: TextOverflow.ellipsis),
              ],
            ),
          ),
        ],
      ),
    );
  }

  Widget _summaryItem(VsScheme v, String icon, VsTone tone, String text) {
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        AppIcon(icon, size: 16, color: vsToneColor(v, tone)),
        const SizedBox(width: 6),
        Text(text,
            style: TextStyle(
                fontSize: 13.5,
                fontWeight: FontWeight.w600,
                color: vsToneColor(v, tone))),
      ],
    );
  }

  Widget _conflictBanner(VsScheme v, AppLocalizations l, List<int> conflicts) {
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 15, vertical: 11),
      decoration: BoxDecoration(
        color: v.warnSoft,
        borderRadius: BorderRadius.circular(DesignTokens.rMd),
      ),
      child: Row(
        children: [
          AppIcon('warn', size: 17, color: v.warn),
          const SizedBox(width: 12),
          Expanded(
            child: Text(
              l.syncPageConflictBanner(conflicts.length),
              style: TextStyle(fontSize: 13, color: v.warn),
            ),
          ),
        ],
      ),
    );
  }
}

// 同步方式：直连（需要 addr）或中继（需要 relay + room + token）。
enum _SyncKind { direct, relay }

class _SyncMode {
  const _SyncMode.direct(this.addr)
      : type = _SyncKind.direct,
        relay = '',
        room = '',
        token = '';
  const _SyncMode.relay(this.relay, this.room, this.token)
      : type = _SyncKind.relay,
        addr = '';

  final _SyncKind type;
  final String addr;
  final String relay;
  final String room;
  final String token;
}

/// “立即同步”弹窗：直连 / 中继切换 + 对应字段（VsSeg + 原型弹窗骨架）。
class _SyncModeDialog extends StatefulWidget {
  const _SyncModeDialog();

  @override
  State<_SyncModeDialog> createState() => _SyncModeDialogState();
}

class _SyncModeDialogState extends State<_SyncModeDialog> {
  _SyncKind _kind = _SyncKind.direct;
  final _addrCtl = TextEditingController();
  final _relayCtl = TextEditingController();
  final _roomCtl = TextEditingController();
  final _tokenCtl = TextEditingController();
  bool _tokenObscure = true;

  @override
  void dispose() {
    _addrCtl.dispose();
    _relayCtl.dispose();
    _roomCtl.dispose();
    _tokenCtl.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final l = AppLocalizations.of(context);
    return VsModalScaffold(
      title: l.syncPageSyncNow,
      sub: l.syncPageDialogSub,
      icon: 'sync',
      width: 480,
      actions: (ctx) => [
        VsButton(
            label: l.syncPageCancel,
            tone: VsBtnTone.ghost,
            onPressed: () => Navigator.of(ctx).pop()),
        VsButton(
          label: l.syncPageStartSync,
          tone: VsBtnTone.primary,
          onPressed: () {
            final ok = _kind == _SyncKind.direct
                ? _addrCtl.text.trim().isNotEmpty
                : _relayCtl.text.trim().isNotEmpty &&
                    _roomCtl.text.trim().isNotEmpty &&
                    _tokenCtl.text.trim().isNotEmpty;
            if (!ok) return;
            Navigator.of(ctx).pop(
              _kind == _SyncKind.direct
                  ? _SyncMode.direct(_addrCtl.text.trim())
                  : _SyncMode.relay(_relayCtl.text.trim(),
                      _roomCtl.text.trim(), _tokenCtl.text.trim()),
            );
          },
        ),
      ],
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          VsSeg<_SyncKind>(
            segments: [
              (_SyncKind.direct, l.syncPageDirect, 'up'),
              (_SyncKind.relay, l.syncPageRelay, 'globe'),
            ],
            selected: _kind,
            onChanged: (k) => setState(() => _kind = k),
          ),
          const SizedBox(height: 14),
          if (_kind == _SyncKind.direct)
            TextField(
              controller: _addrCtl,
              autofocus: true,
              decoration: InputDecoration(
                labelText: l.syncPagePeerAddr,
                hintText: l.syncPageAddrHint,
              ),
            )
          else ...[
            TextField(
              controller: _relayCtl,
              decoration: InputDecoration(
                labelText: l.syncPageRelayAddr,
                hintText: 'relay host:port',
              ),
            ),
            const SizedBox(height: 12),
            TextField(
              controller: _roomCtl,
              decoration: InputDecoration(
                labelText: l.syncPageRoomId,
                hintText: l.syncPageRoomHint,
              ),
            ),
            const SizedBox(height: 12),
            // P8-5：中继接入 token（≥128 位）。VSR2 强制鉴权——不提供
            // 「无 token」路径；输入框 obscure 且不写日志（docs/08 §3.2）。
            TextField(
              controller: _tokenCtl,
              obscureText: _tokenObscure,
              decoration: InputDecoration(
                labelText: l.syncPageRelayToken,
                hintText: l.syncPageRelayTokenHint,
                suffixIcon: IconButton(
                  onPressed: () =>
                      setState(() => _tokenObscure = !_tokenObscure),
                  icon: Icon(_tokenObscure
                      ? Icons.visibility_outlined
                      : Icons.visibility_off_outlined),
                  tooltip: l.syncPageTokenVisibility,
                ),
              ),
            ),
          ],
          const SizedBox(height: 14),
          VsNote(
            l.syncPageDialogNote,
            tone: VsTone.info,
            icon: 'lock',
          ),
        ],
      ),
    );
  }
}
