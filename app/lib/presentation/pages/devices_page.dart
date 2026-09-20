import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:qr_flutter/qr_flutter.dart';

import '../../core/theme/design_tokens.dart';
import '../../l10n/app_localizations.dart';
import '../../service/engine_service.dart';
import '../../state/device_controller.dart';
import '../../state/models/p2p.dart';
import '../../state/notification_controller.dart';
import '../../state/session_controller.dart';
import '../widgets/vs_ui.dart';

/// 设备页（P4）：本机设备信息、已配对设备列表、配对向导、远程销毁。
/// 视觉 1:1 对应原型 page-devices（dev-row / 配对向导 steps / 指纹核验卡）。
class DevicesPage extends ConsumerStatefulWidget {
  const DevicesPage({super.key});

  @override
  ConsumerState<DevicesPage> createState() => _DevicesPageState();
}

class _DevicesPageState extends ConsumerState<DevicesPage> {
  /// 状态轮询：引擎无推送通道，页面可见期间按周期拉取（销毁倒计时 / 在线态）。
  Timer? _watch;

  Object? get _handle {
    final s = ref.read(sessionProvider);
    return s is SessionUnlocked ? s.sessionHandle : null;
  }

  @override
  void initState() {
    super.initState();
    // 不能在 build 生命周期内同步修改 provider（Riverpod 断言），延后一拍。
    Future.microtask(() {
      if (!mounted) return;
      ref.read(deviceProvider.notifier).refresh();
      _watch = Timer.periodic(const Duration(seconds: 2), (_) {
        if (mounted) ref.read(deviceProvider.notifier).refresh();
      });
    });
  }

  @override
  void dispose() {
    _watch?.cancel();
    super.dispose();
  }

  /// 截短设备 ID：取头尾段便于阅读。
  static String _short(String id) {
    if (id.length <= 10) return id;
    return '${id.substring(0, 6)}…${id.substring(id.length - 4)}';
  }

  Future<void> _openPairWizard(SyncStatus status) async {
    final l = AppLocalizations.of(context);
    final ok = await showDialog<bool>(
      context: context,
      barrierColor: context.vs.backdrop,
      builder: (ctx) => _PairWizard(status: status),
    );
    if (ok != true) return;
    ref.read(notificationProvider.notifier).ok(l.devicesPageNotifPaired);
    ref.read(deviceProvider.notifier).refresh();
  }

  Future<void> _openDestroyDialog(Peer peer, SyncStatus status) async {
    final handle = _handle;
    if (handle == null) return;
    final ok = await showDialog<bool>(
      context: context,
      barrierColor: context.vs.backdrop,
      builder: (ctx) => _DestroyDialog(peer: peer, handle: handle),
    );
    if (ok != true) return;
    ref.read(deviceProvider.notifier).refresh();
  }

  Future<void> _cancelDestroy() async {
    final l = AppLocalizations.of(context);
    final handle = _handle;
    if (handle == null) return;
    final r = await ref.read(vaultEngineProvider).destroyCancel(handle);
    final notif = ref.read(notificationProvider.notifier);
    if (r == 0) {
      notif.ok(l.devicesPageNotifDestroyCancelled);
    } else {
      notif.danger(l.devicesPageNotifDestroyCancelFailed(r));
    }
    ref.read(deviceProvider.notifier).refresh();
  }

  /// 解除配对（P4-4）：只移除本机登记表中的对端记录，
  /// 对端数据不受影响；重新互信需再次交换一次性邀请码。
  Future<void> _unpair(Peer peer) async {
    final l = AppLocalizations.of(context);
    final handle = _handle;
    if (handle == null) {
      ref
          .read(notificationProvider.notifier)
          .warning(l.devicesPageNotifNeedUnlock);
      return;
    }
    final ok = await showVsModal<bool>(
      context: context,
      title: l.devicesPageUnpairTitle,
      sub: l.devicesPageUnpairSub(peer.name),
      icon: 'devices',
      micTone: VsTone.warn,
      width: 460,
      body: (ctx) => Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          VsKv.text([
            (l.devicesPageKvDevice, peer.name),
            ('ID', _short(peer.deviceId)),
            (
              l.devicesPageFingerprint,
              peer.fingerprint.isEmpty ? '--' : peer.fingerprint
            ),
          ]),
          VsNote(
            l.devicesPageUnpairNote,
            tone: VsTone.warn,
          ),
        ],
      ),
      actions: (ctx) => [
        VsButton(
            label: l.devicesPageCancel,
            tone: VsBtnTone.ghost,
            onPressed: () => Navigator.pop(ctx, false)),
        VsButton(
            label: l.devicesPageConfirmUnpair,
            tone: VsBtnTone.danger,
            onPressed: () => Navigator.pop(ctx, true)),
      ],
    );
    if (ok != true) return;
    final err =
        await ref.read(vaultEngineProvider).unpair(handle, peer.deviceId);
    final notif = ref.read(notificationProvider.notifier);
    if (err != null) {
      notif.danger(l.devicesPageUnpairFailed(err));
    } else {
      notif.ok(l.devicesPageUnpaired(peer.name));
    }
    ref.read(deviceProvider.notifier).refresh();
  }

  @override
  Widget build(BuildContext context) {
    final l = AppLocalizations.of(context);
    final state = ref.watch(deviceProvider);

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        VsPageHeader(
          title: l.devicesPageTitle,
          sub: state.status == null
              ? l.devicesPageHeaderSub
              : l.devicesPageHeaderSubCounts(
                  state.status!.peers.length, state.status!.alive ? 1 : 0),
          actions: [
            VsIconButton(
                icon: 'sync',
                tooltip: l.devicesPageRefresh,
                onPressed: () => ref.read(deviceProvider.notifier).refresh()),
            VsButton(
              label: l.devicesPageManualAdd,
              icon: 'key',
              tone: VsBtnTone.ghost,
              onPressed: state.status == null
                  ? null
                  : () => _openPairWizard(state.status!),
            ),
            VsButton(
              label: l.devicesPagePairNew,
              icon: 'plus',
              tone: VsBtnTone.primary,
              onPressed: state.status == null
                  ? null
                  : () => _openPairWizard(state.status!),
            ),
          ],
        ),
        if (state.loading)
          const SizedBox(
            height: 260,
            child: Center(child: CircularProgressIndicator(strokeWidth: 2.4)),
          )
        else if (state.error != null)
          SizedBox(
            height: 260,
            child: Center(
              child: Text(state.error!,
                  style: TextStyle(color: context.vs.danger)),
            ),
          )
        else if (state.status == null)
          SizedBox(
            height: 300,
            child: VsEmpty(
              icon: 'devices',
              title: l.devicesPageEmptyTitle,
              sub: l.devicesPageEmptySub,
            ),
          )
        else
          ..._buildContent(context.vs, l, state.status!),
      ],
    );
  }

  List<Widget> _buildContent(
      VsScheme v, AppLocalizations l, SyncStatus status) {
    return [
      if (status.armedDestroy != null) ...[
        _armedDestroyCard(v, l, status.armedDestroy!),
        const SizedBox(height: DesignTokens.sp4),
      ],
      _devRow(
        v,
        l,
        icon: 'pc',
        name: status.name.isEmpty ? l.devicesPageThisComputer : status.name,
        badgeLocal: true,
        sub: l.devicesPageSelfSub(
            _short(status.deviceId.isEmpty ? '--' : status.deviceId),
            status.port > 0 ? '${status.port}' : '--'),
        online: status.alive,
        sideTop: VsBadge(
            status.alive ? l.devicesPageOnline : l.devicesPageOffline,
            tone: status.alive ? VsTone.ok : VsTone.mute),
        sideSub: l.devicesPageSelfRole,
        fingerprint: status.fingerprint,
      ),
      const SizedBox(height: 10),
      if (status.peers.isEmpty)
        SizedBox(
          height: 180,
          child: VsEmpty(
            icon: 'devices',
            title: l.devicesPageNoPeersTitle,
            sub: l.devicesPageNoPeersSub,
          ),
        )
      else
        for (final peer in status.peers) ...[
          _devRow(
            v,
            l,
            icon: 'drive',
            name: peer.name,
            sub: 'ID ${_short(peer.deviceId)}',
            online: true,
            sideTop: VsBadge(l.devicesPageBadgePaired, tone: VsTone.ok),
            sideSub: l.devicesPageLastSyncJustNow,
            fingerprint: peer.fingerprint,
            onDestroy: () => _openDestroyDialog(peer, status),
            onUnpair: () => _unpair(peer),
          ),
          const SizedBox(height: 10),
        ],
    ];
  }

  // ---------- 设备行（.dev-row） ----------

  Widget _devRow(
    VsScheme v,
    AppLocalizations l, {
    required String icon,
    required String name,
    required String sub,
    required bool online,
    required Widget sideTop,
    required String sideSub,
    bool badgeLocal = false,
    String fingerprint = '',
    VoidCallback? onDestroy,
    VoidCallback? onUnpair,
  }) {
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 14),
      decoration: BoxDecoration(
        color: v.surface,
        borderRadius: BorderRadius.circular(DesignTokens.rMd),
        border: Border.all(color: v.borderSoft),
      ),
      child: Row(
        children: [
          // 头像 + 在线点
          Container(
            width: 40,
            height: 40,
            decoration: BoxDecoration(
              color: v.surfaceAlt,
              borderRadius: BorderRadius.circular(11),
            ),
            child: Stack(
              clipBehavior: Clip.none,
              children: [
                Center(child: AppIcon(icon, size: 19, color: v.text2)),
                Positioned(
                  right: -2,
                  bottom: -2,
                  child: Container(
                    width: 11,
                    height: 11,
                    decoration: BoxDecoration(
                      color: online ? v.ok : v.text3,
                      shape: BoxShape.circle,
                      border: Border.all(color: v.surface, width: 2),
                    ),
                  ),
                ),
              ],
            ),
          ),
          const SizedBox(width: 14),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(
                  children: [
                    Text(name,
                        style: TextStyle(
                            fontSize: 13.5,
                            fontWeight: FontWeight.w600,
                            color: v.text)),
                    if (badgeLocal) ...[
                      const SizedBox(width: 7),
                      VsBadge(l.devicesPageBadgeLocal,
                          tone: VsTone.gold, plain: true),
                    ],
                  ],
                ),
                Text(sub, style: TextStyle(fontSize: 12, color: v.text3)),
              ],
            ),
          ),
          Column(
            crossAxisAlignment: CrossAxisAlignment.end,
            children: [
              sideTop,
              const SizedBox(height: 4),
              Text(sideSub, style: TextStyle(fontSize: 12, color: v.text3)),
            ],
          ),
          if (onDestroy != null) ...[
            const SizedBox(width: 12),
            VsButton(
                label: l.devicesPageUnbind,
                icon: 'x',
                tone: VsBtnTone.ghost,
                small: true,
                onPressed: onUnpair),
            const SizedBox(width: 6),
            VsButton(
                label: l.devicesPageRemoteWipe,
                icon: 'oct',
                tone: VsBtnTone.dangerGhost,
                small: true,
                onPressed: onDestroy),
          ],
        ],
      ),
    );
  }

  Widget _armedDestroyCard(VsScheme v, AppLocalizations l, ArmedDestroy armed) {
    return Container(
      padding: const EdgeInsets.all(16),
      decoration: BoxDecoration(
        color: v.surface,
        borderRadius: BorderRadius.circular(DesignTokens.rMd),
        border: Border.all(color: v.dangerSoft),
      ),
      child: Row(
        children: [
          AppIcon('flame', size: 22, color: v.danger),
          const SizedBox(width: 12),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(l.devicesPageDestroyArmed,
                    style: TextStyle(
                        fontSize: 13,
                        fontWeight: FontWeight.w600,
                        color: v.danger)),
                const SizedBox(height: 4),
                Text(
                  l.devicesPageDestroyArmedSub(
                      _short(armed.target), (armed.remainingMs / 1000).round()),
                  style: TextStyle(
                      fontSize: 12,
                      fontFamily: DesignTokens.monoFamily,
                      color: v.text2),
                ),
              ],
            ),
          ),
          VsButton(
              label: l.devicesPageCancelDestroy,
              tone: VsBtnTone.ghost,
              small: true,
              onPressed: _cancelDestroy),
        ],
      ),
    );
  }
}

// ============================================================
// 配对向导：steps + 方式选择 / 邀请码 / 手动输入 / 完成
// ============================================================

class _PairWizard extends ConsumerStatefulWidget {
  const _PairWizard({required this.status});

  final SyncStatus status;

  @override
  ConsumerState<_PairWizard> createState() => _PairWizardState();
}

class _PairWizardState extends ConsumerState<_PairWizard> {
  int _step = 0; // 0 方式 1 指纹核验 2 完成
  String? _mode; // 'generate' | 'manual'
  PairInvite? _invite;
  String? _resultName;
  bool _busy = false;
  String? _error;
  final _addrCtl = TextEditingController();
  final _codeCtl = TextEditingController();

  Object? get _handle {
    final s = ref.read(sessionProvider);
    return s is SessionUnlocked ? s.sessionHandle : null;
  }

  @override
  void dispose() {
    _addrCtl.dispose();
    _codeCtl.dispose();
    super.dispose();
  }

  Future<void> _generate() async {
    final l = AppLocalizations.of(context);
    final handle = _handle;
    if (handle == null) return;
    setState(() => _busy = true);
    final json = await ref.read(vaultEngineProvider).pairBegin(handle);
    if (!mounted) return;
    setState(() {
      _busy = false;
      if (json == null) {
        _error = l.devicesPageInviteFailed;
      } else {
        _invite = PairInvite.fromJson(json);
        _mode = 'generate';
        _error = null;
        _step = 1;
      }
    });
  }

  Future<void> _verifyConfirm() async {
    final l = AppLocalizations.of(context);
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      if (_mode == 'manual') {
        final handle = _handle;
        if (handle == null) {
          setState(() {
            _busy = false;
            _error = l.devicesPageNeedUnlock;
          });
          return;
        }
        final addr = _addrCtl.text.trim();
        final code = _codeCtl.text.trim();
        final json =
            await ref.read(vaultEngineProvider).pairJoin(handle, addr, code);
        if (!mounted) return;
        if (json == null) {
          setState(() {
            _busy = false;
            _error = l.devicesPagePairFailedInvalid;
          });
          return;
        }
        _resultName = json['name'] as String?;
      }
      if (!mounted) return;
      setState(() {
        _busy = false;
        _step = 2;
      });
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _busy = false;
        _error = l.devicesPagePairFailed('$e');
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final v = context.vs;
    final l = AppLocalizations.of(context);
    return Dialog(
      backgroundColor: Colors.transparent,
      insetPadding: const EdgeInsets.all(24),
      child: Container(
        width: 560,
        constraints:
            BoxConstraints(maxHeight: MediaQuery.heightOf(context) - 64),
        decoration: BoxDecoration(
          color: v.surface,
          borderRadius: BorderRadius.circular(DesignTokens.rLg),
          border: Border.all(color: v.border),
          boxShadow: v.shadow3,
        ),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            // head
            Padding(
              padding: const EdgeInsets.fromLTRB(20, 16, 12, 0),
              child: Row(
                children: [
                  const VsMicTile(icon: 'qr', tone: VsTone.gold),
                  const SizedBox(width: 10),
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(l.devicesPagePairNew,
                            style: TextStyle(
                                fontSize: 15.5,
                                fontWeight: FontWeight.w600,
                                color: v.text)),
                        Text(l.devicesPageWizardSub,
                            style: TextStyle(fontSize: 12, color: v.text2)),
                      ],
                    ),
                  ),
                  VsIconButton(
                      icon: 'close', onPressed: () => Navigator.pop(context)),
                ],
              ),
            ),
            VsSteps(
              count: 3,
              current: _step,
              labels: [
                l.devicesPageStepChoose,
                l.devicesPageStepVerify,
                l.devicesPageStepDone,
              ],
            ),
            Flexible(
              child: SingleChildScrollView(
                padding:
                    const EdgeInsets.symmetric(horizontal: 20, vertical: 4),
                child: PageIn(
                  duration: const Duration(milliseconds: 220),
                  key: ValueKey(_step),
                  child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: _buildStep(context)),
                ),
              ),
            ),
            // foot
            Container(
              padding: const EdgeInsets.fromLTRB(20, 14, 20, 18),
              decoration: BoxDecoration(
                  border: Border(top: BorderSide(color: v.borderSoft))),
              child: Row(
                mainAxisAlignment: MainAxisAlignment.end,
                children: [
                  VsButton(
                      label: l.devicesPageCancel,
                      tone: VsBtnTone.ghost,
                      onPressed: _busy ? null : () => Navigator.pop(context)),
                  const SizedBox(width: 8),
                  if (_step == 0)
                    VsButton(
                        label: l.devicesPageGenerateInvite,
                        tone: VsBtnTone.primary,
                        onPressed: _busy ? null : _generate),
                  if (_step == 1)
                    VsButton(
                        label: _mode == 'manual'
                            ? l.devicesPageConnectPair
                            : l.devicesPageFingerprintConfirm,
                        tone: VsBtnTone.primary,
                        onPressed: _busy ? null : _verifyConfirm),
                  if (_step == 2)
                    VsButton(
                        label: l.devicesPageDone,
                        tone: VsBtnTone.primary,
                        onPressed: () => Navigator.pop(context, true)),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }

  List<Widget> _buildStep(BuildContext context) {
    final v = context.vs;
    final l = AppLocalizations.of(context);
    switch (_step) {
      case 0:
        return [
          VsSeg<String>(
            segments: [
              ('qr', l.devicesPageGenerateInvite, 'qr'),
              ('manual', l.devicesPageManualMode, 'edit'),
            ],
            selected: _mode ?? 'qr',
            onChanged: (t) => setState(() => _mode = t),
          ),
          const SizedBox(height: 14),
          if ((_mode ?? 'qr') == 'qr')
            Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(l.devicesPageQrHint),
                const SizedBox(height: 12),
                VsNote(
                  l.devicesPageInviteNote,
                  tone: VsTone.info,
                ),
              ],
            )
          else ...[
            _fieldLabel(l.devicesPagePeerAddr),
            TextField(
              controller: _addrCtl,
              autofocus: true,
              decoration: InputDecoration(hintText: l.devicesPageAddrHint),
            ),
            _fieldLabel(l.devicesPageInviteCode),
            TextField(
              controller: _codeCtl,
              decoration:
                  InputDecoration(hintText: l.devicesPageInviteCodePasteHint),
            ),
          ],
          if (_error != null) ...[
            const SizedBox(height: 12),
            Text(_error!, style: TextStyle(color: v.danger)),
          ],
        ];
      case 1:
        if (_mode == 'generate' && _invite != null) {
          return [
            Text(l.devicesPageVerifyHint),
            const SizedBox(height: 12),
            Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                // 二维码（白底 qbox）
                Container(
                  width: 168,
                  height: 168,
                  padding: const EdgeInsets.all(10),
                  decoration: BoxDecoration(
                    color: Colors.white,
                    borderRadius: BorderRadius.circular(DesignTokens.rMd),
                  ),
                  child: QrImageView(
                    data: _invite!.code,
                    size: 148,
                    backgroundColor: Colors.white,
                  ),
                ),
                const SizedBox(width: 16),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      _fieldLabel(l.devicesPageLocalFingerprint),
                      Container(
                        width: double.infinity,
                        padding: const EdgeInsets.all(12),
                        decoration: BoxDecoration(
                          color: v.bg,
                          borderRadius: BorderRadius.circular(DesignTokens.rSm),
                          border: Border.all(color: v.border),
                        ),
                        child: SelectableText(
                          _invite!.fingerprint,
                          style: TextStyle(
                              fontSize: 13.5,
                              fontFamily: DesignTokens.monoFamily,
                              letterSpacing: 0.06,
                              color: v.gold),
                        ),
                      ),
                      const SizedBox(height: 10),
                      _fieldLabel(l.devicesPageInviteCodeValid),
                      Container(
                        width: double.infinity,
                        padding: const EdgeInsets.all(12),
                        decoration: BoxDecoration(
                          color: v.bg,
                          borderRadius: BorderRadius.circular(DesignTokens.rSm),
                          border: Border.all(color: v.border),
                        ),
                        child: SelectableText(
                          _invite!.code,
                          style: TextStyle(
                              fontSize: 15,
                              fontFamily: DesignTokens.monoFamily,
                              letterSpacing: 0.12,
                              color: v.text),
                        ),
                      ),
                    ],
                  ),
                ),
              ],
            ),
            const SizedBox(height: 12),
            VsNote(l.devicesPageMitmNote, tone: VsTone.warn),
            if (_error != null) ...[
              const SizedBox(height: 12),
              Text(_error!, style: TextStyle(color: v.danger)),
            ],
          ];
        }
        // manual
        return [
          Text(l.devicesPageManualHint),
          const SizedBox(height: 12),
          _fieldLabel(l.devicesPagePeerAddr),
          TextField(
            controller: _addrCtl,
            autofocus: true,
            decoration: const InputDecoration(hintText: 'host:port'),
          ),
          _fieldLabel(l.devicesPageInviteCode),
          TextField(controller: _codeCtl),
          const SizedBox(height: 12),
          _fieldLabel(l.devicesPageLocalFingerprintShort),
          SelectableText(
            widget.status.fingerprint.isEmpty
                ? '--'
                : widget.status.fingerprint,
            style: TextStyle(
                fontSize: 13.5,
                fontFamily: DesignTokens.monoFamily,
                letterSpacing: 0.06,
                color: v.gold),
          ),
          if (_error != null) ...[
            const SizedBox(height: 12),
            Text(_error!, style: TextStyle(color: v.danger)),
          ],
        ];
      default:
        return [
          const SizedBox(height: 8),
          Center(
            child: Column(
              children: [
                Container(
                  width: 64,
                  height: 64,
                  decoration: BoxDecoration(
                    color: v.okSoft,
                    borderRadius: BorderRadius.circular(20),
                  ),
                  child: Center(child: AppIcon('check', size: 30, color: v.ok)),
                ),
                const SizedBox(height: 16),
                Text(l.devicesPagePairDone,
                    style: TextStyle(
                        fontSize: 16,
                        fontWeight: FontWeight.w600,
                        color: v.text)),
                const SizedBox(height: 6),
                Text(l.devicesPagePairDoneSub,
                    style: TextStyle(fontSize: 12, color: v.text2)),
                if (_mode == 'manual')
                  Padding(
                    padding: const EdgeInsets.only(top: 4),
                    child: Text(
                        l.devicesPagePeerDeviceName(
                            _resultName ?? l.devicesPagePeerDevice),
                        style: TextStyle(fontSize: 12, color: v.text2)),
                  ),
              ],
            ),
          ),
        ];
    }
  }

  Widget _fieldLabel(String text) => Padding(
        padding: const EdgeInsets.only(top: 12, bottom: 6),
        child: Text(text,
            style:
                const TextStyle(fontSize: 12.5, fontWeight: FontWeight.w500)),
      );
}

// ============================================================
// 远程销毁对话框：延时单选卡 + 确认语「销毁」
// ============================================================

class _DestroyDialog extends ConsumerStatefulWidget {
  const _DestroyDialog({required this.peer, required this.handle});

  final Peer peer;
  final Object handle;

  @override
  ConsumerState<_DestroyDialog> createState() => _DestroyDialogState();
}

class _DestroyDialogState extends ConsumerState<_DestroyDialog> {
  int _delaySecs = 0;
  bool _busy = false;
  String? _error;
  bool _confirmed = false;
  final _confirmCtl = TextEditingController();

  @override
  void dispose() {
    _confirmCtl.dispose();
    super.dispose();
  }

  Future<void> _arm() async {
    final l = AppLocalizations.of(context);
    // 确认语本身也是提示文案：与 l.devicesPageDestroyConfirmWord 同源，
    // 否则英文界面下用户无法输入被接受的确认语。
    if (_confirmCtl.text.trim() != l.devicesPageDestroyConfirmWord) {
      setState(() => _error =
          l.devicesPageDestroyConfirmError(l.devicesPageDestroyConfirmWord));
      return;
    }
    setState(() {
      _busy = true;
      _error = null;
    });
    final addr = widget.peer.addr;
    if (addr == null || addr.isEmpty) {
      // 没有可拨地址就不要假装已发送：让操作员填地址后重试
      setState(() {
        _busy = false;
        _error = '缺少对端地址：请在下方填写 host:port 后重试';
      });
      return;
    }
    final r = (await ref.read(vaultEngineProvider).destroyArm(
              widget.handle,
              addr,
              widget.peer.deviceId,
              _delaySecs,
            )) ??
        const {'sent': false};
    final sent = (r['sent'] as bool?) ?? false;
    if (!mounted) return;
    setState(() {
      _busy = false;
      _confirmed = sent;
    });
    final notif = ref.read(notificationProvider.notifier);
    if (sent) {
      notif.danger(l.devicesPageDestroySentTo(widget.peer.name));
    } else {
      notif.danger(l.devicesPageDestroySendFailed);
    }
  }

  @override
  Widget build(BuildContext context) {
    final v = context.vs;
    final l = AppLocalizations.of(context);
    return Dialog(
      backgroundColor: Colors.transparent,
      child: Container(
        width: 480,
        decoration: BoxDecoration(
          color: v.surface,
          borderRadius: BorderRadius.circular(DesignTokens.rLg),
          border: Border.all(color: v.dangerSoft),
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
                  const VsMicTile(icon: 'oct', tone: VsTone.danger),
                  const SizedBox(width: 10),
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(l.devicesPageRemoteWipe,
                            style: TextStyle(
                                fontSize: 15.5,
                                fontWeight: FontWeight.w600,
                                color: v.text)),
                        Text(l.devicesPageDestroyTarget(widget.peer.name),
                            style: TextStyle(fontSize: 12, color: v.text2)),
                      ],
                    ),
                  ),
                  VsIconButton(
                      icon: 'close',
                      onPressed: () => Navigator.pop(context, false)),
                ],
              ),
            ),
            Flexible(
              child: SingleChildScrollView(
                padding: const EdgeInsets.all(20),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    VsNote(
                      l.devicesPageDestroyNote,
                      tone: VsTone.danger,
                    ),
                    _fieldLabel(l.devicesPageDestroyDelay),
                    VsRadioCard(
                        value: 0,
                        groupValue: _delaySecs,
                        onChanged: (v) => setState(() => _delaySecs = v!),
                        title: l.devicesPageDelayImmediate,
                        sub: l.devicesPageDelayImmediateSub),
                    VsRadioCard(
                        value: 10,
                        groupValue: _delaySecs,
                        onChanged: (v) => setState(() => _delaySecs = v!),
                        title: l.devicesPageDelay10s,
                        sub: l.devicesPageDelay10sSub),
                    VsRadioCard(
                        value: 60,
                        groupValue: _delaySecs,
                        onChanged: (v) => setState(() => _delaySecs = v!),
                        title: l.devicesPageDelay60s,
                        sub: l.devicesPageDelay60sSub),
                    VsRadioCard(
                        value: 300,
                        groupValue: _delaySecs,
                        onChanged: (v) => setState(() => _delaySecs = v!),
                        title: l.devicesPageDelay5m,
                        sub: l.devicesPageDelay5mSub),
                    _fieldLabel(l.devicesPageFingerprint),
                    SelectableText(
                      widget.peer.fingerprint.isEmpty
                          ? '--'
                          : widget.peer.fingerprint,
                      style: TextStyle(
                          fontSize: 12,
                          fontFamily: DesignTokens.monoFamily,
                          color: v.gold),
                    ),
                    _fieldLabel(l.devicesPageDestroyConfirmInput(
                        l.devicesPageDestroyConfirmWord)),
                    TextField(
                      controller: _confirmCtl,
                      decoration: InputDecoration(
                          hintText: l.devicesPageDestroyConfirmWord),
                    ),
                    if (_error != null) ...[
                      const SizedBox(height: 8),
                      Text(_error!, style: TextStyle(color: v.danger)),
                    ],
                    if (_confirmed) ...[
                      const SizedBox(height: 8),
                      Row(
                        children: [
                          AppIcon('check', size: 16, color: v.ok),
                          const SizedBox(width: 6),
                          Text(l.devicesPageDestroySent,
                              style: TextStyle(color: v.ok)),
                        ],
                      ),
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
                      label: l.devicesPageCancel,
                      tone: VsBtnTone.ghost,
                      onPressed: () => Navigator.pop(context, false)),
                  const SizedBox(width: 8),
                  VsButton(
                      label: _busy
                          ? l.devicesPageSending
                          : l.devicesPageIssueDestroy,
                      tone: VsBtnTone.danger,
                      onPressed: _busy || _confirmed ? null : _arm),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }

  Widget _fieldLabel(String text) => Padding(
        padding: const EdgeInsets.only(top: 12, bottom: 6),
        child: Text(text,
            style:
                const TextStyle(fontSize: 12.5, fontWeight: FontWeight.w500)),
      );
}
