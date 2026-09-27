// ignore: unused_import
import 'package:intl/intl.dart' as intl;
import 'app_localizations.dart';

// ignore_for_file: type=lint

/// The translations for Chinese (`zh`).
class AppLocalizationsZh extends AppLocalizations {
  AppLocalizationsZh([String locale = 'zh']) : super(locale);

  @override
  String get appName => 'VaultSync';

  @override
  String get decoyCancel => '取消';

  @override
  String get decoyClose => '关闭';

  @override
  String get decoyCreate => '创建伪装空间';

  @override
  String decoyCreateFailed(Object e) {
    return '创建失败：$e';
  }

  @override
  String get decoyCreating => '创建中…';

  @override
  String get decoyHintMissing => '尚未创建伪装空间：打开上方开关时设置伪装密码即可创建（独立文件、独立 MK）。';

  @override
  String get decoyHintReady => '伪装空间已就绪：锁屏输入伪装密码即进入伪空间（开关关闭后不再承认该密码）。';

  @override
  String get decoyMismatch => '两次输入的伪装密码不一致';

  @override
  String get decoyNote =>
      '伪装空间是独立加密容器，可以正常导入文件作为掩护；真实保险箱在伪空间内一律拒绝访问，且不记录「进入了哪个空间」（仅记录进入了挂锁门）。';

  @override
  String get decoyPwConfirmLabel => '确认伪装密码';

  @override
  String get decoyPwHint => '必须与主密码不同 · 胁迫场景下输入';

  @override
  String get decoyPwLabel => '伪装密码';

  @override
  String get decoySub => '独立保险箱文件 · 独立 MK · 与真实空间零共享';

  @override
  String get decoyTitleNew => '设置伪装入口';

  @override
  String get decoyTitleReset => '重设伪装密码';

  @override
  String get devicesPageAddrHint => 'host:port，如 192.168.1.10:9100';

  @override
  String get devicesPageBadgeLocal => '本机';

  @override
  String get devicesPageBadgePaired => '已配对';

  @override
  String get devicesPageCancel => '取消';

  @override
  String get devicesPageCancelDestroy => '取消销毁';

  @override
  String get devicesPageConfirmUnpair => '确认解绑';

  @override
  String get devicesPageConnectPair => '连接并配对';

  @override
  String get devicesPageDelay10s => '10 秒';

  @override
  String get devicesPageDelay10sSub => '给误操作留出撤回窗口';

  @override
  String get devicesPageDelay5m => '5 分钟';

  @override
  String get devicesPageDelay5mSub => '长延时 · 期间可随时取消';

  @override
  String get devicesPageDelay60s => '60 秒';

  @override
  String get devicesPageDelay60sSub => '可在设备页「取消销毁」';

  @override
  String get devicesPageDelayImmediate => '立即执行';

  @override
  String get devicesPageDelayImmediateSub => '签发后指令即进入信令队列';

  @override
  String get devicesPageDestroyArmed => '远程销毁已武装';

  @override
  String devicesPageDestroyArmedSub(String target, int seconds) {
    return '目标 $target · 还剩余 $seconds 秒';
  }

  @override
  String devicesPageDestroyConfirmError(String word) {
    return '请准确输入确认语「$word」';
  }

  @override
  String devicesPageDestroyConfirmInput(String word) {
    return '输入「$word」以确认';
  }

  @override
  String get devicesPageDestroyConfirmWord => '销毁';

  @override
  String get devicesPageDestroyDelay => '销毁延时';

  @override
  String get devicesPageDestroyNote =>
      '将向目标设备签发由本机密钥签名的销毁指令（高优先级信令队列）。设备离线时，将于下次上线后、处理任何其他消息之前执行。';

  @override
  String get devicesPageDestroySendFailed => '远程销毁指令发送失败';

  @override
  String get devicesPageDestroySent => '销毁指令已发送';

  @override
  String devicesPageDestroySentTo(String name) {
    return '远程销毁指令已发送至 $name';
  }

  @override
  String devicesPageDestroyTarget(String name) {
    return '目标：$name';
  }

  @override
  String get devicesPageDone => '完成';

  @override
  String get devicesPageEmptySub => '先解锁保险箱，或通过「配对新设备」建立第一个加密信道。';

  @override
  String get devicesPageEmptyTitle => '暂无设备数据';

  @override
  String get devicesPageFingerprint => '指纹';

  @override
  String get devicesPageFingerprintConfirm => '指纹一致，确认配对';

  @override
  String get devicesPageGenerateInvite => '生成邀请码';

  @override
  String get devicesPageHeaderSub => '通过端到端加密信道与已配对设备同步';

  @override
  String devicesPageHeaderSubCounts(int peers, int online) {
    return '$peers 台已配对设备 · $online 台本机在线';
  }

  @override
  String get devicesPageInviteCode => '一次性邀请码';

  @override
  String get devicesPageInviteCodePasteHint => '粘贴对方生成的邀请码';

  @override
  String get devicesPageInviteCodeValid => '一次性邀请码（10 分钟内有效）';

  @override
  String get devicesPageInviteFailed => '邀请码生成失败';

  @override
  String get devicesPageInviteNote => '邀请码 10 分钟内有效，用后即失效；内容不含敏感信息。';

  @override
  String get devicesPageIssueDestroy => '签发销毁指令';

  @override
  String get devicesPageKvDevice => '设备';

  @override
  String get devicesPageLastSyncJustNow => '上次同步 刚刚';

  @override
  String get devicesPageLocalFingerprint => '本机指纹（Fingerprint）';

  @override
  String get devicesPageLocalFingerprintShort => '本机指纹';

  @override
  String get devicesPageManualAdd => '手动添加';

  @override
  String get devicesPageManualHint => '输入对端地址与对方设备生成的一次性邀请码：';

  @override
  String get devicesPageManualMode => '手动输入';

  @override
  String get devicesPageMitmNote => '指纹不一致说明信道可能被劫持（MITM），请拒绝配对。';

  @override
  String get devicesPageNeedUnlock => '请先解锁';

  @override
  String get devicesPageNoPeersSub => '点击右上角「配对新设备」，扫码或手动输入邀请码建立信道。';

  @override
  String get devicesPageNoPeersTitle => '暂无已配对设备';

  @override
  String devicesPageNotifDestroyCancelFailed(int code) {
    return '取消销毁失败（错误码 $code）';
  }

  @override
  String get devicesPageNotifDestroyCancelled => '已取消远程销毁';

  @override
  String get devicesPageNotifNeedUnlock => '请先解锁保险箱';

  @override
  String get devicesPageNotifPaired => '配对完成 · 已加入节点清单';

  @override
  String get devicesPageOffline => '离线';

  @override
  String get devicesPageOnline => '在线';

  @override
  String get devicesPagePairDone => '配对完成';

  @override
  String get devicesPagePairDoneSub => '已建立长期加密信道 · 双方节点清单已更新 · 已记入审计';

  @override
  String devicesPagePairFailed(String e) {
    return '配对异常：$e';
  }

  @override
  String get devicesPagePairFailedInvalid => '配对失败：地址或邀请码无效';

  @override
  String get devicesPagePairNew => '配对新设备';

  @override
  String get devicesPagePeerAddr => '对端设备地址';

  @override
  String get devicesPagePeerDevice => '对方设备';

  @override
  String devicesPagePeerDeviceName(String name) {
    return '对方设备：$name';
  }

  @override
  String get devicesPageQrHint =>
      '点击下方「生成邀请码」生成一次性邀请码与二维码，供对方设备在「配对 → 手动输入」中连接。';

  @override
  String get devicesPageRefresh => '刷新';

  @override
  String get devicesPageRemoteWipe => '远程销毁';

  @override
  String get devicesPageSelfRole => '本机 · 主节点';

  @override
  String devicesPageSelfSub(String id, String port) {
    return '$id · 端口 $port';
  }

  @override
  String get devicesPageSending => '发送中…';

  @override
  String get devicesPageStepChoose => '选择方式';

  @override
  String get devicesPageStepDone => '完成配对';

  @override
  String get devicesPageStepVerify => '指纹核验';

  @override
  String get devicesPageThisComputer => '这台电脑';

  @override
  String get devicesPageTitle => '设备';

  @override
  String get devicesPageUnbind => '解绑';

  @override
  String devicesPageUnpairFailed(String err) {
    return '解绑失败：$err';
  }

  @override
  String get devicesPageUnpairNote =>
      '解绑只删除本机的对端登记：对方设备与其上的数据不受影响；本机文件也不会被删除。重新建立信道需再次交换一次性邀请码并核验指纹。';

  @override
  String devicesPageUnpairSub(String name) {
    return '移除「$name」的互信记录';
  }

  @override
  String get devicesPageUnpairTitle => '解除配对';

  @override
  String devicesPageUnpaired(String name) {
    return '已解除与「$name」的配对';
  }

  @override
  String get devicesPageVerifyHint => '请在两台设备上分别核对以下指纹是否一致（公钥哈希）：';

  @override
  String get devicesPageWizardSub => '交换公钥 · 校验指纹 · 双向确认';

  @override
  String get importDialogCancel => '取消';

  @override
  String get importDialogDropHere => '拖放文件到此处';

  @override
  String get importDialogHint => '支持多选 · 不限文件类型';

  @override
  String get importDialogImport => '导入';

  @override
  String importDialogImportCount(int n) {
    return '导入 $n 个文件';
  }

  @override
  String get importDialogNoFiles => '尚未选择文件';

  @override
  String get importDialogNote => '导入完成后立即生成密文缩略图与全文索引（明文不落盘）。';

  @override
  String get importDialogOrClick => '，或点击选择';

  @override
  String get importDialogRemove => '移除';

  @override
  String get importDialogSub => '流式读取 · CDC 分块 · AES-256-GCM 逐块加密';

  @override
  String get importDialogTitle => '导入文件';

  @override
  String get lockScreenBindManage => '绑定 / 管理';

  @override
  String get lockScreenBiometric => '生物识别';

  @override
  String get lockScreenBusy => '处理中…';

  @override
  String lockScreenCooldown(Object remaining) {
    return '尝试过于频繁，已触发暴力破解保护 · $remaining 后可重试';
  }

  @override
  String get lockScreenCreateAndUnlock => '创建并解锁';

  @override
  String lockScreenFirstRunNote(Object n) {
    return '首次启动 · 主密码 ≥$n 位并混合字符类型；MK 由 CSPRNG 生成，改密只需重新包装';
  }

  @override
  String get lockScreenForgotPassword => '忘记主密码？';

  @override
  String lockScreenHintWithAttempts(Object hint, Object n) {
    return '$hint（第 $n 次）';
  }

  @override
  String get lockScreenPwEnterHint => '输入主密码';

  @override
  String lockScreenPwSetHint(Object n) {
    return '设置主密码（至少 $n 位）';
  }

  @override
  String lockScreenPwStrengthOk(Object label) {
    return '$label · 可通过（Argon2id 派生 KEK）';
  }

  @override
  String lockScreenPwStrengthWeak(Object label, Object n) {
    return '$label · 需要至少 $n 位且混合字符类型';
  }

  @override
  String get lockScreenPwSuggestMix => '建议混合大小写并加入数字或符号';

  @override
  String lockScreenPwTooShort(Object n) {
    return '至少 $n 位';
  }

  @override
  String lockScreenPwWrongAttempts(Object n) {
    return '主密码错误（第 $n 次）';
  }

  @override
  String get lockScreenRecoveryInfo =>
      'MK 由 CSPRNG 生成且不被密码反推，无法凭记忆恢复；重置需已配对设备授权（P5 接入），或使用伪装入口进入伪空间。';

  @override
  String get lockScreenTagline => 'YOUR DATA · EVERYWHERE · ENCRYPTED';

  @override
  String get lockScreenTitle => 'VaultSync 保险箱';

  @override
  String get lockScreenTogglePwVisibility => '显示 / 隐藏主密码';

  @override
  String get lockScreenUnlock => '解锁';

  @override
  String get navDevices => '设备';

  @override
  String get navSecurity => '安全中心';

  @override
  String get navSettings => '设置';

  @override
  String get navStego => '隐写术';

  @override
  String get navSync => '同步';

  @override
  String get navVault => '保险箱';

  @override
  String get securityAuditChainBroken => '链校验失败';

  @override
  String get securityAuditChainOk => '链校验通过';

  @override
  String get securityAuditEmpty => '暂无审计记录 · 解锁/导入/同步等操作会写入';

  @override
  String get securityAuditExport => '导出';

  @override
  String securityAuditExportFailed(String err) {
    return '审计导出失败：$err';
  }

  @override
  String securityAuditExportedTo(String path) {
    return '审计日志已加密导出到 $path';
  }

  @override
  String securityAuditHead(String short) {
    return '本机链头 $short';
  }

  @override
  String get securityAuditLoadFailed => '审计日志读取失败（引擎不可用或会话已失效）';

  @override
  String get securityAuditNote =>
      '审计链按设备独立（每设备一条链，docs/05-06 §3.1），本页只显示本机链；未解锁时的失败尝试无法写入（日志密钥派生自 MK），故链上只有成功操作；日志为整体 AEAD 加密落盘，条目多时首次加载会略慢。';

  @override
  String get securityAuditTitle => '审计日志';

  @override
  String get securityAuditVerify => '校验审计链';

  @override
  String securityAuditVerifyFailed(int at, String reason) {
    return '审计链第 $at 条起校验失败：$reason';
  }

  @override
  String securityAuditVerifyOk(int checked) {
    return '审计链校验通过（$checked 条）';
  }

  @override
  String get securityAuditVerifyUnavailable => '未能读取审计链，校验未执行';

  @override
  String get securityBadgeGuarantee => '设计保证';

  @override
  String get securityCatDevice => '设备';

  @override
  String get securityCatSecurity => '安全';

  @override
  String get securityCatSession => '会话';

  @override
  String get securityCatVault => '保险箱';

  @override
  String get securityColAction => '动作 / 对象';

  @override
  String get securityColCategory => '类别';

  @override
  String get securityColHash => '链哈希';

  @override
  String get securityColSeq => '序号';

  @override
  String get securityColTime => '时间';

  @override
  String securityDestroyAllConfirmBody(int n) {
    return '引擎当前无离线指令队列，无法向离线设备投递销毁指令。确认后将只在本机执行加密擦除；已配对 $n 台设备需各自在线时由对端设备页发起。';
  }

  @override
  String get securityDestroyAllConfirmTitle => '确认全设备联动销毁';

  @override
  String get securityDestroyCancel => '取消';

  @override
  String get securityDestroyConfirm => '确认';

  @override
  String securityDestroyConfirmInput(String word) {
    return '输入「$word」以确认';
  }

  @override
  String get securityDestroyDialogSub => '不可撤销 · 需输入确认短语二次把关';

  @override
  String get securityDestroyDialogTitle => '紧急销毁';

  @override
  String get securityDestroyDone => '本机数据已加密擦除';

  @override
  String get securityDestroyEnter => '进入销毁流程';

  @override
  String get securityDestroyExportCancelled => '未选择导出路径，已取消销毁';

  @override
  String get securityDestroyExportFirst => '先导出审计日志';

  @override
  String get securityDestroyExportFirstSub => '数据目录连同审计日志会被一并销毁，勾选后先导出到所选路径';

  @override
  String securityDestroyExported(String path) {
    return '审计日志已导出到 $path';
  }

  @override
  String securityDestroyFailed(String err) {
    return '销毁失败：$err';
  }

  @override
  String get securityDestroyModeAll => '全设备联动销毁';

  @override
  String get securityDestroyModeAllSub => '签发签名指令，经 P2P / 中继传播至全部已配对设备';

  @override
  String get securityDestroyModeLocal => '本机销毁';

  @override
  String get securityDestroyModeLocalSub => '销毁本机 MK 与全部密钥槽，按介质兜底覆写';

  @override
  String get securityDestroyNote =>
      '离线设备无法即时收到销毁指令：指令进入各设备的高优先级信令队列，目标设备下次上线时先于任何其它消息校验并执行（指令由发起方设备密钥签名，防伪造）。对「永不上线」的设备，销毁无法保证。';

  @override
  String get securityDestroyNow => '立即销毁';

  @override
  String get securityDestroyPeersHint => '需先让对端在线，并由对端设备页发起';

  @override
  String get securityDestroyPeersNone => '全设备联动：当前无已配对设备，仅本机执行加密擦除';

  @override
  String securityDestroyPeersUndelivered(int n) {
    return '全设备联动未完成投递：本机已擦除，$n 台已配对设备需各自在线时由对端设备页发起销毁';
  }

  @override
  String get securityDestroyPhrase => '销毁';

  @override
  String securityDestroyQueued(int n) {
    return '已向 $n 台已配对设备签发销毁指令并进入投递队列：对端下次上线、处理任何其他消息之前执行。';
  }

  @override
  String get securityDestroyRunning => '销毁中…';

  @override
  String get securityDestroySub => '本地 / 远程 / 全设备联动 · 加密擦除';

  @override
  String get securityDestroyTitle => '紧急销毁';

  @override
  String get securityDetectAudit => '审计链完整性';

  @override
  String securityDetectAuditBroken(int at, String reason) {
    return '第 $at 条起校验失败：$reason';
  }

  @override
  String securityDetectAuditOk(int checked, int total) {
    return '正常 · $checked / $total 条校验通过';
  }

  @override
  String get securityDetectAuditUnknown => '未能读取审计链 · 无法校验';

  @override
  String get securityDetectBrute => '暴力破解检测';

  @override
  String securityDetectBruteCooldown(int m, int s) {
    return '冷却中 · 剩余 $m 分 $s 秒';
  }

  @override
  String get securityDetectBruteOk => '正常 · 当前无冷却';

  @override
  String get securityDetectDevice => '异常设备';

  @override
  String get securityDetectDeviceNone => '尚无已配对设备';

  @override
  String securityDetectDeviceOk(int n) {
    return '已配对 $n 台 · 无未授权访问记录';
  }

  @override
  String securityDetectDeviceWarn(String event) {
    return '注意 · $event';
  }

  @override
  String get securityDetectIntegrity => '文件完整性';

  @override
  String securityDetectIntegrityFailed(int failed, int files) {
    return '异常 · $failed / $files 个容器校验失败';
  }

  @override
  String get securityDetectIntegrityFailedRead => '完整性扫描失败：无法读取保险箱';

  @override
  String get securityDetectIntegrityNotScanned => '尚未扫描 · 点击卡片逐块重算';

  @override
  String securityDetectIntegrityOk(int checked, int files) {
    return '正常 · $checked / $files 个容器逐块校验通过';
  }

  @override
  String get securityDetectIntegrityScanning => '扫描中…（逐块重算，耗时与数据量成正比）';

  @override
  String get securityDetectIntegritySub => '随导出/同步逐块 GCM 校验 · 认证失败即拒绝';

  @override
  String get securityDetectMemory => '内存泄漏扫描';

  @override
  String get securityDetectMemorySub => '密钥与明文缓冲全链路 Zeroizing · MK 不出引擎';

  @override
  String get securityDetectSignal => '信令防重放';

  @override
  String get securityDetectSignalDown => '引擎已停止 · 无法确认信令序号连续性';

  @override
  String securityDetectSignalOk(int n) {
    return 'Noise 信道 + 应用层单调序号（docs/08 §4.1）· 本机引擎存活 · 信令事件 $n 条';
  }

  @override
  String get securityDetectSignalUnknown => '未取得 P2P 状态 · 无法判定';

  @override
  String get securityFilterAll => '全部';

  @override
  String get securityNeedUnlock => '请先解锁保险箱';

  @override
  String get securityOverallDanger => '整体状态：异常';

  @override
  String get securityOverallOk => '整体状态：正常';

  @override
  String get securityOverallWarn => '整体状态：注意';

  @override
  String get securityRefresh => '刷新';

  @override
  String get securityRekeyDo => '开始轮换';

  @override
  String get securityRekeyDone => '主密钥已轮换 · 请用主密码重新解锁（既有分享已作废）';

  @override
  String securityRekeyFailed(String err) {
    return '轮换失败：$err';
  }

  @override
  String get securityRekeyNote =>
      '轮换会全量重写索引与所有容器（耗时与数据量成正比），并以新 salt 重新包装 KEK_pwd（已绑定的生物识别副本同步重新包装）。向量时钟不推进，因此不会自动传播到已配对设备；既有阅后即焚分享令牌一并作废。完成后需用主密码重新解锁。';

  @override
  String get securityRekeyNow => '轮换 MK';

  @override
  String get securityRekeyPhrase => '输入「轮换」以确认';

  @override
  String get securityRekeyPhraseWord => '轮换';

  @override
  String get securityRekeyPw => '主密码';

  @override
  String get securityRekeySub => 'MK 泄露时的应急路径：新 MK 重写索引与全部容器（逐块）';

  @override
  String get securityRekeyTitle => '主密钥全库轮换';

  @override
  String get securityRekeyWrongPw => '主密码错误';

  @override
  String get securityStegoDisable => '停用';

  @override
  String get securityStegoDisabled => '隐写术引擎已停用';

  @override
  String get securityStegoEnable => '启用';

  @override
  String get securityStegoEnabled => '隐写术引擎已启用';

  @override
  String securityStegoFailed(String err) {
    return '隐写术引擎设置失败：$err';
  }

  @override
  String get securityStegoSubOff => '默认隐藏 · 图片 LSB 隐蔽存储 · 启用/停用均记审计';

  @override
  String get securityStegoSubOn => '已启用 · 图片 LSB 隐蔽存储 · 启用/停用均记审计';

  @override
  String get securityStegoTitle => '隐写术引擎';

  @override
  String get securitySub => '审计 · 检测 · 应急响应';

  @override
  String get securityTitle => '安全中心';

  @override
  String get settingsAbout => '关于';

  @override
  String get settingsAboutSub => '版本与设计文档';

  @override
  String get settingsAppearance => '外观与语言';

  @override
  String get settingsAppearanceSub => '主题 · 语言';

  @override
  String get settingsAutoLockSub => '空闲超时后自动锁定保险箱';

  @override
  String get settingsAutoLockTitle => '自动落锁';

  @override
  String get settingsAutoSync => '启用自动同步';

  @override
  String get settingsAutoSyncSub => '选择性同步：文件夹、大小、网络、设备（P5 细化）';

  @override
  String get settingsBioBound => '已绑定 · Windows Hello 指纹';

  @override
  String get settingsBioBoundDone => '生物识别已绑定 · 已追加 MK_wrap_bio';

  @override
  String get settingsBioHello => 'Windows Hello 指纹';

  @override
  String get settingsBioHelloSub => '密钥由平台安全区托管，本应用不接触生物特征';

  @override
  String get settingsBioLoading => '读取中…';

  @override
  String get settingsBioNote => '安全区重置后生物识别副本自动失效，主密码路径不受影响。';

  @override
  String get settingsBioTitle => '生物识别';

  @override
  String get settingsBioUnbound => '未绑定 · 主密码始终可用';

  @override
  String get settingsBioUnboundDone => '生物识别已解绑';

  @override
  String get settingsCancel => '取消';

  @override
  String get settingsChange => '修改';

  @override
  String get settingsChangePw => '修改主密码';

  @override
  String get settingsChangePwDialogSub => '仅重新包装 MK · 数据零重加密';

  @override
  String get settingsChangePwDone => '主密码已修改 · MK 重新包装完成，数据零重加密';

  @override
  String get settingsChangePwNote =>
      '修改后重新生成 MK_wrap_pwd 并更换 salt；已配对设备与密文不受影响。';

  @override
  String get settingsChangePwSub => '毫秒级完成，不影响已加密文件与同步';

  @override
  String get settingsConfirmChange => '确认修改';

  @override
  String get settingsConfirmPw => '确认新主密码';

  @override
  String get settingsCurrentPw => '当前主密码';

  @override
  String get settingsDecoyAllow => '允许伪装密码';

  @override
  String get settingsDecoyAllowSub => '锁屏输入伪装密码即进入伪空间，真实保险箱调用一律拒绝';

  @override
  String get settingsDecoySub => '胁迫场景下输入伪装密码进入独立伪空间';

  @override
  String get settingsDecoyTitle => '伪装入口';

  @override
  String get settingsDesignDocLabel => '设计文档';

  @override
  String get settingsEngineLabel => '引擎';

  @override
  String get settingsForgotPw => '忘记主密码';

  @override
  String get settingsForgotPwSub => '安全重置流程，需已配对设备授权（P5 接入）';

  @override
  String get settingsIdleTimeout => '空闲超时';

  @override
  String get settingsIdleTimeoutSub => '无操作后自动锁定（0 = 从不）';

  @override
  String get settingsLanguage => '语言';

  @override
  String get settingsLanguageEn => 'English';

  @override
  String get settingsLanguageSub => '界面语言 · 切换后立即生效';

  @override
  String get settingsLanguageSystem => '跟随系统';

  @override
  String get settingsLanguageZh => '中文';

  @override
  String settingsMinutes(int n) {
    return '$n 分钟';
  }

  @override
  String get settingsNeedUnlock => '请先解锁保险箱';

  @override
  String get settingsNever => '从不';

  @override
  String get settingsNewPw => '新主密码';

  @override
  String get settingsOpenDocsFailed => '无法打开文档链接';

  @override
  String get settingsPwMismatch => '两次输入的新密码不一致';

  @override
  String get settingsPwSection => '主密码管理';

  @override
  String get settingsPwSectionSub => '修改密码仅重新包装主密钥，数据零重加密';

  @override
  String settingsStrengthHint(String label) {
    return '$label · Argon2id 派生 KEK';
  }

  @override
  String get settingsSub => '安全 · 落锁 · 同步 · 外观';

  @override
  String get settingsSubmitting => '提交中…';

  @override
  String get settingsSyncOff => '自动同步已关闭';

  @override
  String get settingsSyncOn => '自动同步已开启 · 关闭后需手动触发';

  @override
  String get settingsSyncTitle => '同步策略';

  @override
  String get settingsTheme => '主题';

  @override
  String get settingsThemeDark => '深色（默认）';

  @override
  String get settingsThemeLight => '浅色';

  @override
  String get settingsThemeSub => '深色为默认 · 对应原型 data-theme';

  @override
  String get settingsThemeSystem => '跟随系统';

  @override
  String get settingsTitle => '设置';

  @override
  String get settingsUpdateCheck => '检查更新';

  @override
  String get settingsUpdateChecking => '检查中…';

  @override
  String get settingsUpdateDownload => '下载并安装';

  @override
  String get settingsUpdateNotConfigured => '未配置 · 需清单地址与发布公钥（fail-closed）';

  @override
  String get settingsUpdateNote =>
      '清单签名覆盖清单原始字节，安装包下载后再用清单内的 sha256 复核；任一校验失败即拒绝。当前仓库未附带发布服务器与签名密钥，配置前该功能保持禁用。';

  @override
  String get settingsUpdatePubkey => '发布公钥';

  @override
  String get settingsUpdatePubkeySub =>
      '内置的 Ed25519 公钥（hex，32 字节）——只接受该密钥签名的清单';

  @override
  String get settingsUpdateTitle => '更新通道';

  @override
  String get settingsUpdateUrl => '更新清单地址';

  @override
  String get settingsUpdateUrlSub =>
      '发布方托管的 latest.json（必须 https）；同目录 latest.json.sig 为 Ed25519 签名';

  @override
  String get settingsVersionLabel => '版本';

  @override
  String settingsVersionValue(String version) {
    return 'VaultSync $version · 端到端加密多设备同步';
  }

  @override
  String get settingsViewDocs => '查看在线文档';

  @override
  String get shellAllFiles => '全部文件';

  @override
  String get shellLockNow => '立即锁定';

  @override
  String get shellMarkAllRead => '全部已读';

  @override
  String get shellNavSectionControl => '管控';

  @override
  String get shellNavSectionSpace => '空间';

  @override
  String get shellNoNotifications => '暂无通知';

  @override
  String get shellNotifications => '通知';

  @override
  String get shellPlaceholderSub => '功能开发中 —— 计划见 docs/09-开发任务面板.md';

  @override
  String get shellSearchHint => '搜索文件、标签、全文…';

  @override
  String get shellSubDevices => '通过端到端加密信道与已配对设备同步';

  @override
  String get shellSubSecurity => '审计 · 检测 · 应急响应';

  @override
  String get shellSubSettings => '安全 · 落锁 · 同步 · 外观';

  @override
  String get shellSubStego => 'AES-256-GCM 密文嵌入图片 LSB';

  @override
  String get shellSubSync => '增量同步 · 仅传输差异块';

  @override
  String get shellTitleDevices => '设备';

  @override
  String get shellTitleSecurity => '安全中心';

  @override
  String get shellTitleSettings => '设置';

  @override
  String get shellTitleStego => '隐写术';

  @override
  String get shellTitleSync => '同步';

  @override
  String get shellTitleVault => '保险箱';

  @override
  String get shellVersionTagline => 'V0.2 · 桌面端';

  @override
  String get stateBioFailed => '生物识别解锁失败';

  @override
  String get stateBioNotBound => '尚未绑定生物识别（设置中绑定后可用）';

  @override
  String get stateBioUnsupported => '此平台不支持生物识别，请使用主密码';

  @override
  String get stateConflictDropped => '已丢弃冲突副本';

  @override
  String stateConflictKept(Object name) {
    return '已保留副本并替换原条目 · $name';
  }

  @override
  String stateConflictResolveFailed(Object err) {
    return '冲突解决失败：$err';
  }

  @override
  String get stateCooldownHint => '尝试过于频繁，请等待冷却结束';

  @override
  String stateCooldownSeconds(Object n) {
    return '尝试过于频繁，请等待 $n 秒';
  }

  @override
  String stateCreateFailed(Object e) {
    return '创建保险箱失败：$e';
  }

  @override
  String get stateDeviceLoadFailed => '设备状态读取失败';

  @override
  String get stateDeviceNeedUnlock => '请先解锁保险箱';

  @override
  String get stateNeedUnlock => '请先解锁保险箱';

  @override
  String get statePwMin4 => '密码至少 4 位';

  @override
  String get statePwRequired => '请输入主密码';

  @override
  String statePwTooShort(Object n) {
    return '主密码至少 $n 位';
  }

  @override
  String get statePwTooWeak => '强度不足：建议混合大小写并加入数字或符号';

  @override
  String get statePwWrong => '密码错误';

  @override
  String get stateStrengthFair => '一般';

  @override
  String get stateStrengthGood => '较强';

  @override
  String get stateStrengthStrong => '强';

  @override
  String get stateStrengthWeak => '弱';

  @override
  String stateSyncDone(Object pulled, Object pushed) {
    return '同步完成：拉取 $pulled，推送 $pushed';
  }

  @override
  String stateSyncDoneWithConflicts(
    Object conflicts,
    Object pulled,
    Object pushed,
  ) {
    return '同步完成：拉取 $pulled，推送 $pushed，冲突 $conflicts';
  }

  @override
  String get stateSyncFailed => '同步失败';

  @override
  String get stateSyncRelayFailed => '中继同步失败';

  @override
  String get stateUnknownDevice => '未知设备';

  @override
  String get stegoPageCancel => '取消';

  @override
  String get stegoPageCapacityCard => '容量参考';

  @override
  String get stegoPageCapacityFormat =>
      '不支持该图片：隐写仅接受 8 位 RGB / RGBA 的 PNG（有损压缩会破坏 LSB）';

  @override
  String get stegoPageCapacityFormula => '容量公式：宽 × 高 × 3 ÷ 8 − 4（4 字节为载荷长度前缀）';

  @override
  String get stegoPageCapacityIo => '图片读取失败：文件不存在或不可读';

  @override
  String get stegoPageCapacityLimit => '单图容量有限：更大文件需分割嵌入多张图片（docs/05-05 §3.2）。';

  @override
  String stegoPageCapacityLine(String cap, String need) {
    return '容量 $cap / 需要 $need';
  }

  @override
  String stegoPageCapacityShort(String need) {
    return '容量不足：还差 $need，请更换更大的图片（或分割嵌入多张图片）。';
  }

  @override
  String get stegoPageDisable => '停用';

  @override
  String get stegoPageDisableDone => '隐写引擎已停用';

  @override
  String get stegoPageDisabled => '未启用';

  @override
  String get stegoPageDisabledNote =>
      '隐写引擎默认关闭：启用后「嵌入 / 提取」才会被引擎接受；启用与停用都会写入审计链（docs/05-06 §七）。';

  @override
  String get stegoPageEmbedAction => '嵌入';

  @override
  String stegoPageEmbedDone(String path) {
    return '已嵌入 → $path';
  }

  @override
  String get stegoPageEmbedNote =>
      '载荷 = AEAD(长度前缀 ‖ 文件名 ‖ 明文) 的密文：4 字节长度前缀与 AEAD 开销（12 字节 nonce + 16 字节 tag）同样占用容量。';

  @override
  String get stegoPageEmbedSub => '保险箱文件 → AES-256-GCM 密文 → 写入 PNG LSB';

  @override
  String get stegoPageEmbedTitle => '隐写导入（嵌入）';

  @override
  String get stegoPageEmbedding => '嵌入中…';

  @override
  String get stegoPageEnable => '启用隐写引擎';

  @override
  String get stegoPageEnableDone => '隐写引擎已启用';

  @override
  String get stegoPageEnabled => '已启用';

  @override
  String get stegoPageEnabledNote =>
      '载荷是「先加密、再嵌入」：图片 LSB 里只有 AES-256-GCM 密文，密钥由本机 MK 经 HKDF 派生——提取必须持有同一保险箱的主密钥，因此这是本机隐蔽辅助，不是跨设备传输通道。';

  @override
  String get stegoPageExtractAction => '提取';

  @override
  String stegoPageExtractDone(String name, String size) {
    return '已还原 $name（$size）';
  }

  @override
  String get stegoPageExtractFailed =>
      '提取失败：图片中不含有效载荷、不是 8 位 RGB/RGBA PNG，或该载荷不属于当前保险箱（主密钥不匹配）。';

  @override
  String get stegoPageExtractNote => '提取只在同一保险箱内有效：载荷密钥派生自本机 MK，换库 / 换设备无法解密。';

  @override
  String get stegoPageExtractSub => 'PNG LSB → 密文帧 → AES-256-GCM 解密';

  @override
  String get stegoPageExtractTitle => '隐写提取（取出）';

  @override
  String get stegoPageExtracting => '提取中…';

  @override
  String get stegoPageFileEmpty => '保险箱内没有可选文件（请先导入文件）';

  @override
  String get stegoPageFileLoading => '正在读取保险箱文件…';

  @override
  String stegoPageFileOption(String name, String size, String folder) {
    return '$name · $size · $folder';
  }

  @override
  String get stegoPageFileRoot => '根目录';

  @override
  String get stegoPageNeedUnlock => '会话未解锁，无法执行隐写操作';

  @override
  String get stegoPageNotChosen => '未选择';

  @override
  String get stegoPagePickFile => '保险箱文件';

  @override
  String get stegoPagePickFileHint => '选择要嵌入的文件';

  @override
  String get stegoPagePickImage => '选择 PNG 图片';

  @override
  String get stegoPagePickOutput => '选择输出位置';

  @override
  String get stegoPagePickRestore => '选择还原位置';

  @override
  String get stegoPagePickStegoImage => '选择携带数据的 PNG';

  @override
  String stegoPagePlainSize(String size) {
    return '明文体量 ≈ $size';
  }

  @override
  String get stegoPageSafetyNote => '隐写是隐蔽辅助，不替代主保险箱；有损压缩（JPG）会破坏 LSB，请使用 PNG。';

  @override
  String stegoPageSizeB(int n) {
    return '$n B';
  }

  @override
  String stegoPageSizeBytes(int bytes, String size) {
    return '$bytes 字节 ≈ $size';
  }

  @override
  String stegoPageSizeKb(String n) {
    return '$n KB';
  }

  @override
  String stegoPageSizeMb(String n) {
    return '$n MB';
  }

  @override
  String get stegoPageSub => '加密之上的隐蔽 · AES-256-GCM 密文嵌入图片 LSB';

  @override
  String get stegoPageTileEmbed => '隐写导入（嵌入）';

  @override
  String get stegoPageTileEmbedSub => '将保险箱中已加密的文件嵌入普通图片，图片外观无可见变化。';

  @override
  String get stegoPageTileExtract => '隐写提取（取出）';

  @override
  String get stegoPageTileExtractSub => '从携带隐藏数据的图片中提取密文帧，解密还原原始文件。';

  @override
  String get stegoPageTileLocked => '需先启用隐写引擎';

  @override
  String get stegoPageTitle => '隐写术引擎';

  @override
  String stegoPageToggleFailed(String err) {
    return '操作失败：$err';
  }

  @override
  String get syncPageAddrHint => 'host:port，如 192.168.1.10:9100';

  @override
  String get syncPageCancel => '取消';

  @override
  String get syncPageChunkDetails => '分块详情';

  @override
  String get syncPageChunkDetailsNote =>
      '逐块传输进度矩阵（加密块 = 同步块 = 传输块）将在 P5 接入真实数据。';

  @override
  String syncPageConflictBanner(int n) {
    return '发现 $n 个冲突文件，冲突副本以 .conflict-<时间戳> 命名保留，可在保险箱列表中处理。';
  }

  @override
  String syncPageConflictCopyId(int id) {
    return '冲突副本 #$id';
  }

  @override
  String get syncPageConflictNote =>
      '冲突副本由引擎在两侧版本分叉时自动留存（本地版本另存为 <原名>.conflict-<时间戳>）。「保留副本」会把副本改回原名并替换对端版本，「丢弃副本」则只删副本——两者都会记入审计。';

  @override
  String syncPageConflictPeerGone(String stamp) {
    return '副本留存于 $stamp · 对端版本已不存在';
  }

  @override
  String get syncPageConflictSection => '冲突处理';

  @override
  String syncPageConflictStoredAt(String stamp) {
    return '副本留存于 $stamp';
  }

  @override
  String get syncPageDialogNote => '同步为块级增量：仅传输双方差异块，已到块经强哈希校验后复用。';

  @override
  String get syncPageDialogSub => '直连或经自建中继 · Noise 加密信道';

  @override
  String get syncPageDirect => '直连';

  @override
  String get syncPageDropCopy => '丢弃副本';

  @override
  String get syncPageEngineNote =>
      '引擎暂无传输队列与逐块进度回调：本页的统计、事件与冲突均为真实数据，逐块矩阵待引擎提供回调后接入（P5）。';

  @override
  String get syncPageEvents => '同步事件';

  @override
  String syncPageEventsRecent(int n) {
    return '最近 $n 条';
  }

  @override
  String get syncPageHeaderSub => '增量同步 · 仅传输差异块（CDC）';

  @override
  String get syncPageKeepCopy => '保留副本';

  @override
  String get syncPageLatestSummary => '最新同步摘要';

  @override
  String get syncPageLive => '实时';

  @override
  String get syncPageNeverSynced => '尚未同步';

  @override
  String get syncPageNoConflicts => '没有冲突副本 · 同步结果一致';

  @override
  String get syncPageNoEvents => '暂无事件 · 完成「立即同步」后在此记录';

  @override
  String get syncPagePeerAddr => '对端地址';

  @override
  String syncPagePendingCount(int n) {
    return '$n 项待解决';
  }

  @override
  String get syncPageRefresh => '刷新';

  @override
  String get syncPageRelay => '中继';

  @override
  String get syncPageRelayAddr => '中继地址';

  @override
  String get syncPageRoomHint => '共享房间 Room ID';

  @override
  String get syncPageRoomId => '房间号';

  @override
  String get syncPageRelayToken => '中继接入 Token';

  @override
  String get syncPageRelayTokenHint => '由中继部署方分发（≥16 字符）';

  @override
  String get syncPageTokenVisibility => '显示 / 隐藏 Token';

  @override
  String get syncPageScanning => '扫描中…';

  @override
  String syncPageSelfPeers(String name, int peers) {
    return '本机 $name · $peers 台已配对';
  }

  @override
  String get syncPageStartSync => '开始同步';

  @override
  String get syncPageStatConflicts => '冲突待解决';

  @override
  String get syncPageStatDeleted => '已删除项';

  @override
  String get syncPageStatPulledFiles => '拉取的文件';

  @override
  String get syncPageStatPushedFiles => '推送的文件';

  @override
  String get syncPageStatus => '同步状态';

  @override
  String get syncPageStatusIdle => '空闲 · 无进行中的传输';

  @override
  String get syncPageStatusRunning => '正在同步 · 块级增量传输中（阻塞式单次会话）';

  @override
  String syncPageSummaryConflicts(int n) {
    return '冲突 $n';
  }

  @override
  String syncPageSummaryDeleted(int n) {
    return '删除 $n';
  }

  @override
  String syncPageSummaryLine(
    int pulled,
    int pushed,
    int deleted,
    int conflicts,
  ) {
    return '拉取 $pulled · 推送 $pushed · 删除 $deleted · 冲突 $conflicts';
  }

  @override
  String syncPageSummaryMerged(int n) {
    return '合并 $n';
  }

  @override
  String syncPageSummaryPulled(int n) {
    return '拉取 $n';
  }

  @override
  String syncPageSummaryPushed(int n) {
    return '推送 $n';
  }

  @override
  String get syncPageSyncNow => '立即同步';

  @override
  String get syncPageSyncing => '同步中…';

  @override
  String get syncPageTitle => '同步';

  @override
  String get vaultPageAlgoLabel => '算法';

  @override
  String get vaultPageAlgoValue => 'AES-256-GCM · FastCDC 64KB';

  @override
  String get vaultPageBackToFolder => '返回当前文件夹';

  @override
  String get vaultPageBurn => '阅后即焚';

  @override
  String get vaultPageCancel => '取消';

  @override
  String get vaultPageCipherThumb => '密文缩略图';

  @override
  String get vaultPageClose => '关闭';

  @override
  String get vaultPageConflict => '冲突';

  @override
  String get vaultPageCreate => '创建';

  @override
  String vaultPageCreatedFolder(String name) {
    return '已创建「$name」· 独立派生子密钥 FSK';
  }

  @override
  String vaultPageDetailSub(String size, String kind) {
    return '$size · $kind';
  }

  @override
  String get vaultPageDetailThumb => '加密缩略图 · 解锁后解密预览';

  @override
  String get vaultPageDone => '完成';

  @override
  String get vaultPageEmptyFolderSub => '将文件拖入此处，或点击下方按钮导入（自动分块加密）。';

  @override
  String get vaultPageEmptyFolderTitle => '此文件夹为空';

  @override
  String get vaultPageEmptySearchSub => '换个关键词，或返回文件夹后重新搜索。';

  @override
  String get vaultPageEmptySearchTitle => '未找到匹配内容';

  @override
  String get vaultPageEmptyVaultSub => '导入的文件将自动分块加密，密钥仅存于本机。';

  @override
  String get vaultPageEmptyVaultTitle => '此保险箱为空';

  @override
  String get vaultPageEncInfo => '加密信息';

  @override
  String get vaultPageExport => '导出';

  @override
  String vaultPageExportCount(int n) {
    return '导出 $n 个文件';
  }

  @override
  String vaultPageExportFailedItem(String name, String error) {
    return '导出失败 $name：$error';
  }

  @override
  String get vaultPageExportNoneSelected => '所选项目中没有可导出的文件';

  @override
  String get vaultPageExportToHere => '导出到此处';

  @override
  String vaultPageExportedMany(int done, int total, String dir) {
    return '已导出 $done/$total 个文件到 $dir';
  }

  @override
  String vaultPageExportedTo(String path) {
    return '已导出到 $path · SHA-256 终验通过';
  }

  @override
  String get vaultPageFileLabel => '文件';

  @override
  String get vaultPageFolder => '文件夹';

  @override
  String get vaultPageFsKeyLabel => '文件密钥';

  @override
  String get vaultPageFsKeyValue => 'FSKey · 随机生成';

  @override
  String get vaultPageImport => '导入文件';

  @override
  String vaultPageImportFailed(String name) {
    return '导入失败 $name';
  }

  @override
  String vaultPageImportFileError(String name, String error) {
    return '$name：$error';
  }

  @override
  String vaultPageImportFolderUnsupported(String name) {
    return '$name（文件夹暂不支持导入）';
  }

  @override
  String vaultPageImportedCount(int n) {
    return '已导入 $n 个文件 · CDC 分块加密入箱';
  }

  @override
  String get vaultPageKindArchive => '压缩包';

  @override
  String get vaultPageKindDoc => '文档';

  @override
  String get vaultPageKindFile => '文件';

  @override
  String get vaultPageKindFolder => '加密文件夹';

  @override
  String get vaultPageKindImage => '图片';

  @override
  String get vaultPageKindVideo => '视频';

  @override
  String get vaultPageMkdirHint => '例如：合同归档';

  @override
  String get vaultPageMkdirSub => '文件夹独立派生子密钥 FSK';

  @override
  String get vaultPageMkdirTitle => '新建加密文件夹';

  @override
  String get vaultPageNameLabel => '名称';

  @override
  String get vaultPageNewFolder => '新建文件夹';

  @override
  String get vaultPageNoFiles => '无文件';

  @override
  String get vaultPageNoFilesEmptySub => '此文件夹为空 —— 将文件拖入此处，或点击下方「导入文件」。';

  @override
  String get vaultPageNoFilesSub => '此层级没有文件，可从左侧进入子文件夹，或点击下方「导入文件」。';

  @override
  String get vaultPageNoSubfolders => '无子文件夹';

  @override
  String get vaultPageNoTags => '暂无标签';

  @override
  String get vaultPageOpen => '打开';

  @override
  String get vaultPagePreview => '预览';

  @override
  String get vaultPagePreviewNote => '引擎未开放明文预览通道（明文不出引擎层）；查看内容请先「导出」。';

  @override
  String vaultPagePreviewSub(String size) {
    return '$size · 解锁会话内解密';
  }

  @override
  String vaultPagePreviewTitle(String name) {
    return '预览 · $name';
  }

  @override
  String get vaultPageRename => '重命名';

  @override
  String get vaultPageRenameFileSub => '修改加密元数据中的文件名';

  @override
  String get vaultPageRenameFolderSub => '修改加密元数据中的文件夹名';

  @override
  String get vaultPageRenamed => '已重命名 · 加密元数据已更新';

  @override
  String get vaultPageRotateFileDone => '密钥已轮换 · 容器已重写';

  @override
  String get vaultPageRotateFileSub => '以全新随机 FSKey 重写该文件容器';

  @override
  String get vaultPageRotateFileTitle => '轮换密钥';

  @override
  String get vaultPageRotateFolderDone => '文件夹密钥已轮换 · 直属文件容器已重写';

  @override
  String get vaultPageRotateFolderSub => '新 FSK 覆盖 + 逐文件重写直属文件容器';

  @override
  String get vaultPageRotateFolderTitle => '轮换文件夹密钥';

  @override
  String get vaultPageRotateKey => '轮换密钥';

  @override
  String get vaultPageRotateNote =>
      '轮换后旧密钥路径即失效（新密钥为随机值，不再由 MK 派生）。这是本机轮换：向量时钟未推进，不会自动传播到已配对设备，对端保留其自身可读副本；文件夹轮换只覆盖直属文件，不含子文件夹。';

  @override
  String get vaultPageRotateStart => '开始轮换';

  @override
  String get vaultPageRotateTargetFile => '文件';

  @override
  String get vaultPageRotateTargetFolder => '文件夹';

  @override
  String get vaultPageSave => '保存';

  @override
  String vaultPageSearching(String query) {
    return '搜索「$query」';
  }

  @override
  String vaultPageSelectedCount(int n) {
    return '已选 $n 项';
  }

  @override
  String get vaultPageShare => '阅后即焚分享';

  @override
  String get vaultPageShareFailed => '分享创建失败';

  @override
  String vaultPageShareIdAndToken(String id, String token) {
    return 'ID：$id\n令牌：$token';
  }

  @override
  String get vaultPageShareNote => '对方凭令牌打开后分享即失效，令牌不可复用。';

  @override
  String get vaultPageShareSub => '一次性 P2P 会话 · 对端打开后限时自动擦除';

  @override
  String get vaultPageShareTokenLabel => '一次性分享令牌（仅显示这一次）';

  @override
  String get vaultPageSizeLabel => '大小';

  @override
  String get vaultPageStatusLabel => '状态';

  @override
  String get vaultPageSynced => '已同步';

  @override
  String get vaultPageTags => '标签';

  @override
  String get vaultPageTagsHint => '逗号分隔，如: 工作,私密';

  @override
  String vaultPageTagsSub(String name) {
    return '管理「$name」的标签';
  }

  @override
  String get vaultPageTagsTitle => '标签管理';

  @override
  String get vaultPageTagsUpdated => '标签已更新';

  @override
  String get vaultPageTargetFile => '目标文件';

  @override
  String get vaultPageTargetFolder => '目标文件夹';

  @override
  String get vaultPageTreeAllFiles => '全部文件';

  @override
  String get vaultPageTreeCurrentPath => '当前路径';

  @override
  String get vaultPageTreeQuickAccess => '快速访问';

  @override
  String get vaultPageTypeLabel => '类型';

  @override
  String get vaultPageUnnamed => '未命名';

  @override
  String get vaultPageUnnamedFile => '未命名文件';

  @override
  String get vaultPageUnnamedFolder => '未命名文件夹';

  @override
  String vaultPageUnnamedWithId(int id) {
    return '未命名-$id';
  }

  @override
  String get vaultPageViewGrid => '网格';

  @override
  String get vaultPageViewList => '列表';

  @override
  String get vaultPageViewSplit => '分栏';

  @override
  String get vaultPageWipe => '安全擦除';

  @override
  String get vaultPageWipeAll => '全部擦除';

  @override
  String get vaultPageWipeConfirm => '确认擦除';

  @override
  String vaultPageWipeCount(int n) {
    return '安全擦除 $n 个文件';
  }

  @override
  String get vaultPageWipeFileSub => '销毁文件密钥 FSKey 并摘除块清单';

  @override
  String vaultPageWipeFolderFailed(String name, String error, int count) {
    return '「$name」$error（已销毁 $count 个文件）';
  }

  @override
  String get vaultPageWipeFolderSub => '递归销毁文件夹及其内文件的密钥与密文';

  @override
  String vaultPageWipeManyDone(int n) {
    return '已批量安全擦除 $n 个项目';
  }

  @override
  String vaultPageWipeManyFailed(int failed, int n) {
    return '批量擦除完成：$failed/$n 项失败';
  }

  @override
  String vaultPageWipeManyFilesSub(int n) {
    return '销毁 $n 个文件的密钥并删除密文';
  }

  @override
  String vaultPageWipeManyFoldersSub(int folders) {
    return '销毁密钥并删除密文（含 $folders 个文件夹，递归）';
  }

  @override
  String vaultPageWipeManyNote(int n) {
    return '将批量擦除 $n 个项目，不可恢复，且同步传播到已配对设备。';
  }

  @override
  String get vaultPageWipeManyTitle => '批量安全擦除';

  @override
  String get vaultPageWipeNote => '擦除将同步传播到所有已配对设备，操作会记入审计日志。';

  @override
  String get vaultPageWiped => '已安全擦除';

  @override
  String vsStepsDefaultLabel(Object n) {
    return '步骤 $n';
  }

  @override
  String get windowChromeClose => '关闭';

  @override
  String get windowChromeMaximize => '最大化';

  @override
  String get windowChromeMinimize => '最小化';

  @override
  String get windowChromeRestore => '向下还原';
}
