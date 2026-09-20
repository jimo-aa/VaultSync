import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/l10n/vs_l10n.dart';
import '../../core/security/password_policy.dart';
import '../../core/theme/design_tokens.dart';
import '../../l10n/app_localizations.dart';
import '../../service/engine_service.dart';
import '../widgets/vs_ui.dart';

/// 伪装入口配置（P4-5 / F-06）：为伪装空间设置专属密码。
///
/// 伪装保险箱是**独立文件 + 独立 MK**：与真实保险箱无任何共享；锁屏输入伪装
/// 密码即进入伪空间，其中的任何操作都不会触碰真实数据。
/// 返回 true 表示伪装空间已就绪（新建成功或本就存在）。
Future<bool> showDecoySetupDialog(
  BuildContext context, {
  required bool exists,
}) async {
  final ok = await showDialog<bool>(
    context: context,
    barrierColor: context.vs.backdrop,
    builder: (_) => _DecoyDialog(exists: exists),
  );
  return ok == true;
}

class _DecoyDialog extends ConsumerStatefulWidget {
  const _DecoyDialog({required this.exists});

  /// 伪装保险箱是否已存在（已存在时文案为「重设」，引擎按不覆盖处理）。
  final bool exists;

  @override
  ConsumerState<_DecoyDialog> createState() => _DecoyDialogState();
}

class _DecoyDialogState extends ConsumerState<_DecoyDialog> {
  final _pwCtl = TextEditingController();
  final _confirmCtl = TextEditingController();
  bool _busy = false;
  String? _error;

  @override
  void dispose() {
    _pwCtl.dispose();
    _confirmCtl.dispose();
    super.dispose();
  }

  Future<void> _submit() async {
    final l = AppLocalizations.of(context);
    final issue = PasswordPolicy.validate(_pwCtl.text);
    if (issue != null) {
      setState(() => _error = issue.messageOf(l));
      return;
    }
    if (_pwCtl.text != _confirmCtl.text) {
      setState(() => _error = l.decoyMismatch);
      return;
    }
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      await ref.read(vaultEngineProvider).ensureDisguiseCreated(_pwCtl.text);
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _busy = false;
        _error = _failText(e);
      });
      return;
    }
    if (!mounted) return;
    Navigator.of(context).pop(true);
  }

  String _failText(Object e) =>
      VsL10n.orNull?.decoyCreateFailed('$e') ?? '创建失败：$e';

  @override
  Widget build(BuildContext context) {
    final v = context.vs;
    final l = AppLocalizations.of(context);
    final s = PasswordPolicy.evaluate(_pwCtl.text);
    return Dialog(
      backgroundColor: Colors.transparent,
      child: Container(
        width: 480,
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
            Padding(
              padding: const EdgeInsets.fromLTRB(20, 16, 12, 0),
              child: Row(
                children: [
                  const VsMicTile(icon: 'eye-off', tone: VsTone.enc),
                  const SizedBox(width: 10),
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                            widget.exists ? l.decoyTitleReset : l.decoyTitleNew,
                            style: TextStyle(
                                fontSize: 15.5,
                                fontWeight: FontWeight.w600,
                                color: v.text)),
                        Text(l.decoySub,
                            style: TextStyle(fontSize: 12, color: v.text2)),
                      ],
                    ),
                  ),
                  VsIconButton(
                      icon: 'close',
                      tooltip: l.decoyClose,
                      onPressed: () => Navigator.of(context).pop(false)),
                ],
              ),
            ),
            Flexible(
              child: SingleChildScrollView(
                padding: const EdgeInsets.all(20),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    _label(l.decoyPwLabel),
                    TextField(
                      controller: _pwCtl,
                      autofocus: true,
                      obscureText: true,
                      onChanged: (_) => setState(() {}),
                      decoration: InputDecoration(hintText: l.decoyPwHint),
                    ),
                    const SizedBox(height: 7),
                    Row(
                      children: [
                        for (var i = 0; i < 4; i++) ...[
                          if (i > 0) const SizedBox(width: 5),
                          Expanded(
                            child: AnimatedContainer(
                              duration: const Duration(milliseconds: 250),
                              height: 4,
                              decoration: BoxDecoration(
                                color: i < s.level
                                    ? (s.level <= 2 ? v.danger : v.gold)
                                    : v.surfaceAlt,
                                borderRadius: BorderRadius.circular(3),
                              ),
                            ),
                          ),
                        ],
                        const SizedBox(width: 8),
                        Text(s == PasswordStrength.empty ? '' : s.labelOf(l),
                            style: TextStyle(fontSize: 12, color: v.text3)),
                      ],
                    ),
                    _label(l.decoyPwConfirmLabel),
                    TextField(
                      controller: _confirmCtl,
                      obscureText: true,
                      onChanged: (_) => setState(() {}),
                    ),
                    const SizedBox(height: 14),
                    if (_error != null) ...[
                      Text(_error!, style: TextStyle(color: v.danger)),
                      const SizedBox(height: 10),
                    ],
                    VsNote(l.decoyNote, tone: VsTone.enc),
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
                      label: l.decoyCancel,
                      tone: VsBtnTone.ghost,
                      onPressed: _busy
                          ? null
                          : () => Navigator.of(context).pop(false)),
                  const SizedBox(width: 8),
                  VsButton(
                      label: _busy ? l.decoyCreating : l.decoyCreate,
                      tone: VsBtnTone.primary,
                      onPressed: _busy ? null : _submit),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }

  Widget _label(String text) => Padding(
        padding: const EdgeInsets.only(top: 12, bottom: 6),
        child: Text(text,
            style:
                const TextStyle(fontSize: 12.5, fontWeight: FontWeight.w500)),
      );
}

/// 伪装空间状态提示（设置页复用）。
class DecoyHint extends StatelessWidget {
  const DecoyHint({required this.ready, super.key});

  final bool ready;

  @override
  Widget build(BuildContext context) {
    final l = AppLocalizations.of(context);
    return Align(
      alignment: Alignment.centerLeft,
      child: VsNote(
        ready ? l.decoyHintReady : l.decoyHintMissing,
        tone: ready ? VsTone.enc : VsTone.warn,
      ),
    );
  }
}
