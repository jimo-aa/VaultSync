import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:url_launcher/url_launcher.dart';

import '../../core/l10n/vs_l10n.dart';
import '../../core/security/password_policy.dart';
import '../../l10n/app_localizations.dart';
import '../../core/theme/design_tokens.dart';
import '../../service/engine_service.dart';
import '../../service/update_service.dart';
import '../../state/app_theme_controller.dart';
import '../../state/locale_controller.dart';
import '../../state/notification_controller.dart';
import '../../state/session_controller.dart';
import '../../state/settings_controller.dart';
import '../widgets/decoy_setup_dialog.dart';
import '../widgets/vs_ui.dart';

/// 外部文档占位链接（P4-7 关于）。
const _kDocsUrl = 'https://github.com/';

/// 产品版本号（关于区展示；不进 ARB，只把描述文案本地化）。
const _kAppVersion = '0.3.0';

/// 设置页：1:1 对应原型 page-settings（acc 手风琴 + row-setting 行）。
class SettingsPage extends ConsumerStatefulWidget {
  const SettingsPage({super.key});

  @override
  ConsumerState<SettingsPage> createState() => _SettingsPageState();
}

class _SettingsPageState extends ConsumerState<SettingsPage> {
  int _open = -1; // 手风琴展开索引（-1 全收起；对应原型默认首项展开）

  // 更新通道（P5-7）：两项配置 + 检查结果
  late final TextEditingController _updateUrlCtl;
  late final TextEditingController _updatePkCtl;
  bool _checkingUpdate = false;
  bool _updateAvailable = false;
  bool _updateFailed = false;
  String? _updateMsg;
  UpdateManifest? _pendingUpdate;

  @override
  void initState() {
    super.initState();
    _open = 0;
    final cfg = ref.read(settingsProvider);
    _updateUrlCtl = TextEditingController(text: cfg.updateManifestUrl);
    _updatePkCtl = TextEditingController(text: cfg.updatePubkeyHex);
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) {
        ref.read(settingsProvider.notifier).refreshBio();
        ref.read(settingsProvider.notifier).refreshDisguise();
      }
    });
  }

  @override
  void dispose() {
    _updateUrlCtl.dispose();
    _updatePkCtl.dispose();
    super.dispose();
  }

  /// 检查更新（P5-7）：清单签名与安装包哈希都在引擎侧校验，未配置即禁用。
  Future<void> _checkUpdate() async {
    setState(() {
      _checkingUpdate = true;
      _updateMsg = null;
      _updateAvailable = false;
      _updateFailed = false;
    });
    final cfg = ref.read(settingsProvider);
    final r = await ref.read(updateServiceProvider).check(
          manifestUrl: cfg.updateManifestUrl,
          publicKeyHex: cfg.updatePubkeyHex,
          currentVersion: _kAppVersion,
        );
    if (!mounted) return;
    setState(() {
      _checkingUpdate = false;
      _updateAvailable = r.isAvailable;
      _updateFailed = r.status == 'error';
      _pendingUpdate = r.manifest;
      _updateMsg = r.isAvailable
          ? '发现 v${r.manifest!.version}${r.manifest!.notes.isEmpty ? '' : ' · ${r.manifest!.notes}'}'
          : r.message;
    });
  }

  Future<void> _downloadUpdate() async {
    final m = _pendingUpdate;
    if (m == null) return;
    final err = await ref.read(updateServiceProvider).downloadAndLaunch(m);
    _notify(err == null ? NotifGrade.ok : NotifGrade.danger,
        err ?? '安装包已校验并启动安装程序（v${m.version}）');
  }

  /// 当前会话句柄（仅解锁态可用；伪装空间返回 null 以禁止敏感操作）。
  Object? _handle() {
    final s = ref.read(sessionProvider);
    return s is SessionUnlocked ? s.sessionHandle : null;
  }

  void _notify(NotifGrade grade, String msg) =>
      ref.read(notificationProvider.notifier).add(grade, msg);

  @override
  Widget build(BuildContext context) {
    final v = context.vs;
    final l = AppLocalizations.of(context);
    final settings = ref.watch(settingsProvider);
    final mode = ref.watch(themeControllerProvider);
    final locale = ref.watch(localeControllerProvider);
    // 下拉项固定三选一：任何未知/未设置的语言都落到「跟随系统」。
    final langValue = switch (locale?.languageCode) {
      'zh' => 'zh',
      'en' => 'en',
      _ => 'system',
    };

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        VsPageHeader(title: l.settingsTitle, sub: l.settingsSub),
        // 主密码管理
        VsAccordion(
          icon: 'key',
          title: l.settingsPwSection,
          sub: l.settingsPwSectionSub,
          open: _open == 0,
          onToggle: () => setState(() => _open = _open == 0 ? -1 : 0),
          child: Column(
            children: [
              VsSettingRow(
                title: l.settingsChangePw,
                sub: l.settingsChangePwSub,
                trailing: VsButton(
                    label: l.settingsChange,
                    small: true,
                    onPressed: _showPasswordDialog),
              ),
              VsSettingRow(
                title: l.settingsForgotPw,
                sub: l.settingsForgotPwSub,
                last: true,
              ),
            ],
          ),
        ),
        // 生物识别
        VsAccordion(
          icon: 'finger',
          title: l.settingsBioTitle,
          sub: settings.bioLoading
              ? l.settingsBioLoading
              : settings.bioBound
                  ? l.settingsBioBound
                  : l.settingsBioUnbound,
          open: _open == 1,
          onToggle: () => setState(() => _open = _open == 1 ? -1 : 1),
          child: Column(
            children: [
              VsSettingRow(
                title: l.settingsBioHello,
                sub: l.settingsBioHelloSub,
                trailing: VsSwitch(
                  value: settings.bioBound,
                  onChanged: _toggleBio,
                ),
                last: true,
              ),
              const SizedBox(height: 10),
              Align(
                alignment: Alignment.centerLeft,
                child: VsNote(
                  l.settingsBioNote,
                  tone: VsTone.warn,
                ),
              ),
            ],
          ),
        ),
        // 自动落锁
        VsAccordion(
          icon: 'clock',
          title: l.settingsAutoLockTitle,
          sub: l.settingsAutoLockSub,
          open: _open == 2,
          onToggle: () => setState(() => _open = _open == 2 ? -1 : 2),
          child: VsSettingRow(
            title: l.settingsIdleTimeout,
            sub: l.settingsIdleTimeoutSub,
            trailing: SizedBox(
              width: 170,
              child: DropdownButtonFormField<int>(
                initialValue: settings.autoLockMinutes,
                style: TextStyle(fontSize: 13, color: v.text),
                dropdownColor: v.surface,
                decoration: const InputDecoration(),
                items: [
                  DropdownMenuItem(value: 0, child: Text(l.settingsNever)),
                  DropdownMenuItem(value: 1, child: Text(l.settingsMinutes(1))),
                  DropdownMenuItem(value: 5, child: Text(l.settingsMinutes(5))),
                  DropdownMenuItem(
                      value: 15, child: Text(l.settingsMinutes(15))),
                  DropdownMenuItem(
                      value: 60, child: Text(l.settingsMinutes(60))),
                ],
                onChanged: (val) {
                  if (val != null) {
                    ref.read(settingsProvider.notifier).setAutoLock(val);
                  }
                },
              ),
            ),
            last: true,
          ),
        ),
        // 伪装入口
        VsAccordion(
          icon: 'eye-off',
          title: l.settingsDecoyTitle,
          sub: l.settingsDecoySub,
          open: _open == 3,
          onToggle: () => setState(() => _open = _open == 3 ? -1 : 3),
          child: Column(
            children: [
              VsSettingRow(
                title: l.settingsDecoyAllow,
                sub: l.settingsDecoyAllowSub,
                trailing: VsSwitch(
                  value: settings.decoyEnabled,
                  onChanged: _toggleDecoy,
                ),
                last: true,
              ),
              const SizedBox(height: 10),
              DecoyHint(ready: settings.disguiseReady),
            ],
          ),
        ),
        // 同步策略
        VsAccordion(
          icon: 'sync',
          title: l.settingsSyncTitle,
          sub: settings.autoSync ? l.settingsSyncOn : l.settingsSyncOff,
          open: _open == 4,
          onToggle: () => setState(() => _open = _open == 4 ? -1 : 4),
          child: VsSettingRow(
            title: l.settingsAutoSync,
            sub: l.settingsAutoSyncSub,
            trailing: VsSwitch(
              value: settings.autoSync,
              onChanged: (val) =>
                  ref.read(settingsProvider.notifier).setAutoSync(val),
            ),
            last: true,
          ),
        ),
        // 外观与语言
        VsAccordion(
          icon: 'globe',
          title: l.settingsAppearance,
          sub: l.settingsAppearanceSub,
          open: _open == 5,
          onToggle: () => setState(() => _open = _open == 5 ? -1 : 5),
          child: Column(
            children: [
              VsSettingRow(
                title: l.settingsTheme,
                sub: l.settingsThemeSub,
                trailing: SizedBox(
                  width: 170,
                  child: DropdownButtonFormField<ThemeMode>(
                    initialValue: mode,
                    style: TextStyle(fontSize: 13, color: v.text),
                    dropdownColor: v.surface,
                    decoration: const InputDecoration(),
                    items: [
                      DropdownMenuItem(
                          value: ThemeMode.dark,
                          child: Text(l.settingsThemeDark)),
                      DropdownMenuItem(
                          value: ThemeMode.light,
                          child: Text(l.settingsThemeLight)),
                      DropdownMenuItem(
                          value: ThemeMode.system,
                          child: Text(l.settingsThemeSystem)),
                    ],
                    onChanged: (val) {
                      if (val != null) {
                        ref.read(themeControllerProvider.notifier).set(val);
                      }
                    },
                  ),
                ),
              ),
              VsSettingRow(
                title: l.settingsLanguage,
                sub: l.settingsLanguageSub,
                trailing: SizedBox(
                  width: 170,
                  child: DropdownButtonFormField<String>(
                    initialValue: langValue,
                    style: TextStyle(fontSize: 13, color: v.text),
                    dropdownColor: v.surface,
                    decoration: const InputDecoration(),
                    items: [
                      DropdownMenuItem(
                          value: 'system',
                          child: Text(l.settingsLanguageSystem)),
                      DropdownMenuItem(
                          value: 'zh', child: Text(l.settingsLanguageZh)),
                      DropdownMenuItem(
                          value: 'en', child: Text(l.settingsLanguageEn)),
                    ],
                    onChanged: (val) {
                      if (val == null) return;
                      ref
                          .read(localeControllerProvider.notifier)
                          .set(val == 'system' ? null : Locale(val));
                    },
                  ),
                ),
                last: true,
              ),
            ],
          ),
        ),
        // 更新通道（P5-7）：清单签名在引擎侧校验；未配置即 fail-closed
        VsAccordion(
          icon: 'down',
          title: l.settingsUpdateTitle,
          sub: (settings.updateManifestUrl.isEmpty ||
                  settings.updatePubkeyHex.isEmpty)
              ? l.settingsUpdateNotConfigured
              : settings.updateManifestUrl,
          open: _open == 6,
          onToggle: () => setState(() => _open = _open == 6 ? -1 : 6),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              VsSettingRow(
                title: l.settingsUpdateUrl,
                sub: l.settingsUpdateUrlSub,
                trailing: SizedBox(
                  width: 320,
                  child: TextField(
                    controller: _updateUrlCtl,
                    style: TextStyle(fontSize: 12.5, color: v.text),
                    decoration: const InputDecoration(
                        hintText: 'https://…/latest.json'),
                    onSubmitted: (t) => ref
                        .read(settingsProvider.notifier)
                        .setUpdateChannel(url: t.trim()),
                  ),
                ),
              ),
              VsSettingRow(
                title: l.settingsUpdatePubkey,
                sub: l.settingsUpdatePubkeySub,
                trailing: SizedBox(
                  width: 320,
                  child: TextField(
                    controller: _updatePkCtl,
                    style: TextStyle(
                        fontSize: 12,
                        fontFamily: DesignTokens.monoFamily,
                        color: v.text),
                    decoration: const InputDecoration(hintText: '64 位 hex'),
                    onSubmitted: (t) => ref
                        .read(settingsProvider.notifier)
                        .setUpdateChannel(pubkeyHex: t.trim()),
                  ),
                ),
                last: true,
              ),
              const SizedBox(height: 10),
              Row(
                children: [
                  VsButton(
                    label: _checkingUpdate
                        ? l.settingsUpdateChecking
                        : l.settingsUpdateCheck,
                    small: true,
                    onPressed: _checkingUpdate ? null : _checkUpdate,
                  ),
                  const SizedBox(width: 10),
                  if (_pendingUpdate != null && _updateAvailable)
                    VsButton(
                      label: l.settingsUpdateDownload,
                      tone: VsBtnTone.primary,
                      small: true,
                      onPressed: _downloadUpdate,
                    ),
                ],
              ),
              if (_updateMsg != null) ...[
                const SizedBox(height: 10),
                Text(_updateMsg!,
                    style: TextStyle(
                        fontSize: 12.5,
                        color: _updateAvailable
                            ? v.gold
                            : _updateFailed
                                ? v.danger
                                : v.text2)),
              ],
              const SizedBox(height: 12),
              VsNote(l.settingsUpdateNote, tone: VsTone.info),
            ],
          ),
        ),
        // 关于
        VsAccordion(
          icon: 'info',
          title: l.settingsAbout,
          sub: l.settingsAboutSub,
          open: _open == 7,
          onToggle: () => setState(() => _open = _open == 7 ? -1 : 7),
          child: Padding(
            padding: const EdgeInsets.only(bottom: 0),
            child: VsKv.text([
              (l.settingsVersionLabel, l.settingsVersionValue(_kAppVersion)),
              (
                l.settingsEngineLabel,
                ref.read(vaultCoreBridgeProvider).coreVersion()
              ),
              (l.settingsDesignDocLabel, 'docs/03-前端设计.md'),
            ]),
          ),
        ),
        const SizedBox(height: 8),
        Align(
          alignment: Alignment.centerLeft,
          child: VsButton(
            label: l.settingsViewDocs,
            icon: 'doc',
            tone: VsBtnTone.ghost,
            onPressed: _openDocs,
          ),
        ),
      ],
    );
  }

  Future<void> _openDocs() async {
    final l = AppLocalizations.of(context);
    final uri = Uri.parse(_kDocsUrl);
    if (await canLaunchUrl(uri)) {
      await launchUrl(uri, mode: LaunchMode.externalApplication);
    } else {
      _notify(NotifGrade.warning, l.settingsOpenDocsFailed);
    }
  }

  /// 生物识别开关：绑定 / 解绑，失败回滚并提示。
  Future<void> _toggleBio(bool value) async {
    final l = AppLocalizations.of(context);
    final handle = _handle();
    if (handle == null) {
      ref.read(settingsProvider.notifier).refreshBio();
      _notify(NotifGrade.warning, l.settingsNeedUnlock);
      return;
    }
    final err =
        await ref.read(vaultEngineProvider).bindBio(handle, bind: value);
    await ref.read(settingsProvider.notifier).refreshBio();
    if (err != null) {
      _notify(NotifGrade.danger, err);
    } else {
      _notify(NotifGrade.ok,
          value ? l.settingsBioBoundDone : l.settingsBioUnboundDone);
    }
  }

  /// 伪装入口开关（P4-5）：未就绪时先引导设置伪装密码，取消则保持关闭。
  Future<void> _toggleDecoy(bool value) async {
    final settings = ref.read(settingsProvider);
    if (value && !settings.disguiseReady) {
      final ok =
          await showDecoySetupDialog(context, exists: settings.disguiseReady);
      if (!mounted || !ok) return;
      await ref.read(settingsProvider.notifier).setDecoy(true);
      await ref.read(settingsProvider.notifier).refreshDisguise();
      return;
    }
    await ref.read(settingsProvider.notifier).setDecoy(value);
  }

  Future<void> _showPasswordDialog() async {
    final handle = _handle();
    if (handle == null) {
      _notify(
          NotifGrade.warning, AppLocalizations.of(context).settingsNeedUnlock);
      return;
    }
    await showDialog<void>(
      context: context,
      barrierColor: context.vs.backdrop,
      builder: (_) => _ChangePasswordDialog(handle: handle),
    );
  }
}

/// 修改主密码弹窗（3 个密码框 + 强度条，对应原型 ov-password）。
class _ChangePasswordDialog extends ConsumerStatefulWidget {
  const _ChangePasswordDialog({required this.handle});

  final Object handle;

  @override
  ConsumerState<_ChangePasswordDialog> createState() =>
      _ChangePasswordDialogState();
}

class _ChangePasswordDialogState extends ConsumerState<_ChangePasswordDialog> {
  final _oldCtl = TextEditingController();
  final _newCtl = TextEditingController();
  final _confirmCtl = TextEditingController();
  bool _submitting = false;
  String? _error;
  PasswordStrength _strength = PasswordStrength.empty;

  @override
  void dispose() {
    _oldCtl.dispose();
    _newCtl.dispose();
    _confirmCtl.dispose();
    super.dispose();
  }

  static const _strengthColors = [
    Color(0xFFE55252), // 弱
    Color(0xFFE55252),
    Color(0xFFF0A63C), // 一般
    Color(0xFFD9B25F), // 较强
    Color(0xFF34C77B), // 强
  ];

  Future<void> _submit() async {
    final l = AppLocalizations.of(context);
    final oldPw = _oldCtl.text;
    final newPw = _newCtl.text;
    final confirm = _confirmCtl.text;
    final issue = PasswordPolicy.validate(newPw);
    if (issue != null) {
      setState(() => _error = issue.messageOf(AppLocalizations.of(context)));
      return;
    }
    if (newPw != confirm) {
      setState(() => _error = l.settingsPwMismatch);
      return;
    }
    setState(() {
      _submitting = true;
      _error = null;
    });
    final err = await ref
        .read(vaultEngineProvider)
        .changePassword(widget.handle, oldPw, newPw);
    if (!mounted) return;
    if (err != null) {
      setState(() {
        _submitting = false;
        _error = err;
      });
      return;
    }
    ref.read(notificationProvider.notifier).ok(l.settingsChangePwDone);
    Navigator.of(context).pop();
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
                  const VsMicTile(icon: 'key', tone: VsTone.gold),
                  const SizedBox(width: 10),
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(l.settingsChangePw,
                            style: TextStyle(
                                fontSize: 15.5,
                                fontWeight: FontWeight.w600,
                                color: v.text)),
                        Text(l.settingsChangePwDialogSub,
                            style: TextStyle(fontSize: 12, color: v.text2)),
                      ],
                    ),
                  ),
                  VsIconButton(
                      icon: 'close',
                      onPressed: () => Navigator.of(context).pop()),
                ],
              ),
            ),
            Flexible(
              child: SingleChildScrollView(
                padding: const EdgeInsets.all(20),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    _fieldLabel(l.settingsCurrentPw),
                    TextField(
                        controller: _oldCtl,
                        obscureText: true,
                        autofocus: true),
                    _fieldLabel(l.settingsNewPw),
                    TextField(
                      controller: _newCtl,
                      obscureText: true,
                      onChanged: (val) => setState(
                          () => _strength = PasswordPolicy.evaluate(val)),
                    ),
                    const SizedBox(height: 7),
                    // 密码强度条（.strength）
                    Row(
                      children: [
                        for (var i = 0; i < 4; i++) ...[
                          if (i > 0) const SizedBox(width: 5),
                          Expanded(
                            child: AnimatedContainer(
                              duration: const Duration(milliseconds: 250),
                              height: 4,
                              decoration: BoxDecoration(
                                color: i < _strength.level
                                    ? _strengthColors[_strength.level]
                                    : v.surfaceAlt,
                                borderRadius: BorderRadius.circular(3),
                              ),
                            ),
                          ),
                        ],
                        const SizedBox(width: 8),
                        Text(
                          _strength == PasswordStrength.empty
                              ? ''
                              : l.settingsStrengthHint(_strength.labelOf(l)),
                          style: TextStyle(fontSize: 12, color: v.text3),
                        ),
                      ],
                    ),
                    _fieldLabel(l.settingsConfirmPw),
                    TextField(controller: _confirmCtl, obscureText: true),
                    const SizedBox(height: 14),
                    VsNote(
                      l.settingsChangePwNote,
                      tone: VsTone.info,
                    ),
                    if (_error != null) ...[
                      const SizedBox(height: 12),
                      Align(
                        alignment: Alignment.centerLeft,
                        child: Text(_error!, style: TextStyle(color: v.danger)),
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
                      label: l.settingsCancel,
                      tone: VsBtnTone.ghost,
                      onPressed: _submitting
                          ? null
                          : () => Navigator.of(context).pop()),
                  const SizedBox(width: 8),
                  VsButton(
                      label: _submitting
                          ? l.settingsSubmitting
                          : l.settingsConfirmChange,
                      tone: VsBtnTone.primary,
                      onPressed: _submitting ? null : _submit),
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
