import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../service/engine_service.dart';
import 'app_prefs.dart';

/// 设置项状态（P4-5）。bioBound / disguiseReady 经引擎异步刷新。
class SettingsState {
  const SettingsState({
    this.autoLockMinutes = 5,
    this.decoyEnabled = false,
    this.autoSync = true,
    this.bioBound = false,
    this.bioLoading = false,
    this.disguiseReady = false,
    this.updateManifestUrl = '',
    this.updatePubkeyHex = '',
  });

  /// 自动落锁空闲分钟数；0 = 关闭自动落锁。
  final int autoLockMinutes;

  /// 伪装入口开关（伪装保险箱独立 MK）。关闭时锁屏不承认伪装密码。
  final bool decoyEnabled;

  /// 同步策略：true=自动同步，false=手动。
  final bool autoSync;

  final bool bioBound;
  final bool bioLoading;

  /// 伪装保险箱文件是否已存在（存在才可能凭伪装密码进入伪空间）。
  final bool disguiseReady;

  /// 更新通道（P5-7）：清单地址与内置发布公钥。任一为空即视为未配置（fail-closed）。
  final String updateManifestUrl;
  final String updatePubkeyHex;

  SettingsState copyWith({
    int? autoLockMinutes,
    bool? decoyEnabled,
    bool? autoSync,
    bool? bioBound,
    bool? bioLoading,
    bool? disguiseReady,
    String? updateManifestUrl,
    String? updatePubkeyHex,
  }) =>
      SettingsState(
        autoLockMinutes: autoLockMinutes ?? this.autoLockMinutes,
        decoyEnabled: decoyEnabled ?? this.decoyEnabled,
        autoSync: autoSync ?? this.autoSync,
        bioBound: bioBound ?? this.bioBound,
        bioLoading: bioLoading ?? this.bioLoading,
        disguiseReady: disguiseReady ?? this.disguiseReady,
        updateManifestUrl: updateManifestUrl ?? this.updateManifestUrl,
        updatePubkeyHex: updatePubkeyHex ?? this.updatePubkeyHex,
      );
}

class SettingsController extends Notifier<SettingsState> {
  static const kAutoLock = 'settings.autolock.minutes';
  static const kDecoy = 'settings.decoy.enabled';
  static const kAutoSync = 'settings.sync.autosync';
  static const kUpdateUrl = 'settings.update.url';
  static const kUpdatePubkey = 'settings.update.pubkey';

  @override
  SettingsState build() {
    final prefs = ref.watch(sharedPreferencesProvider);
    return SettingsState(
      autoLockMinutes: prefs.getInt(kAutoLock) ?? 5,
      decoyEnabled: prefs.getBool(kDecoy) ?? false,
      autoSync: prefs.getBool(kAutoSync) ?? true,
      updateManifestUrl: prefs.getString(kUpdateUrl) ?? '',
      updatePubkeyHex: prefs.getString(kUpdatePubkey) ?? '',
    );
  }

  Future<void> setAutoLock(int minutes) async {
    state = state.copyWith(autoLockMinutes: minutes);
    await ref.read(sharedPreferencesProvider).setInt(kAutoLock, minutes);
  }

  Future<void> setDecoy(bool enabled) async {
    state = state.copyWith(decoyEnabled: enabled);
    await ref.read(sharedPreferencesProvider).setBool(kDecoy, enabled);
  }

  Future<void> setAutoSync(bool enabled) async {
    state = state.copyWith(autoSync: enabled);
    await ref.read(sharedPreferencesProvider).setBool(kAutoSync, enabled);
  }

  Future<void> setUpdateChannel({String? url, String? pubkeyHex}) async {
    final prefs = ref.read(sharedPreferencesProvider);
    if (url != null) {
      state = state.copyWith(updateManifestUrl: url);
      await prefs.setString(kUpdateUrl, url);
    }
    if (pubkeyHex != null) {
      state = state.copyWith(updatePubkeyHex: pubkeyHex);
      await prefs.setString(kUpdatePubkey, pubkeyHex);
    }
  }

  /// 刷新生物识别绑定状态（需引擎可用）。
  Future<void> refreshBio() async {
    state = state.copyWith(bioLoading: true);
    final bound = await ref.read(vaultEngineProvider).bioBound();
    state = state.copyWith(bioBound: bound, bioLoading: false);
  }

  /// 刷新伪装保险箱是否已创建（伪装入口开关的前置条件）。
  Future<void> refreshDisguise() async {
    final ready = await ref.read(vaultEngineProvider).disguiseExists();
    state = state.copyWith(disguiseReady: ready);
  }
}

final settingsProvider =
    NotifierProvider<SettingsController, SettingsState>(SettingsController.new);
