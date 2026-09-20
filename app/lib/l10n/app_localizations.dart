import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:intl/intl.dart' as intl;

import 'app_localizations_en.dart';
import 'app_localizations_zh.dart';

// ignore_for_file: type=lint

/// Callers can lookup localized strings with an instance of AppLocalizations
/// returned by `AppLocalizations.of(context)`.
///
/// Applications need to include `AppLocalizations.delegate()` in their app's
/// `localizationDelegates` list, and the locales they support in the app's
/// `supportedLocales` list. For example:
///
/// ```dart
/// import 'l10n/app_localizations.dart';
///
/// return MaterialApp(
///   localizationsDelegates: AppLocalizations.localizationsDelegates,
///   supportedLocales: AppLocalizations.supportedLocales,
///   home: MyApplicationHome(),
/// );
/// ```
///
/// ## Update pubspec.yaml
///
/// Please make sure to update your pubspec.yaml to include the following
/// packages:
///
/// ```yaml
/// dependencies:
///   # Internationalization support.
///   flutter_localizations:
///     sdk: flutter
///   intl: any # Use the pinned version from flutter_localizations
///
///   # Rest of dependencies
/// ```
///
/// ## iOS Applications
///
/// iOS applications define key application metadata, including supported
/// locales, in an Info.plist file that is built into the application bundle.
/// To configure the locales supported by your app, you’ll need to edit this
/// file.
///
/// First, open your project’s ios/Runner.xcworkspace Xcode workspace file.
/// Then, in the Project Navigator, open the Info.plist file under the Runner
/// project’s Runner folder.
///
/// Next, select the Information Property List item, select Add Item from the
/// Editor menu, then select Localizations from the pop-up menu.
///
/// Select and expand the newly-created Localizations item then, for each
/// locale your application supports, add a new item and select the locale
/// you wish to add from the pop-up menu in the Value field. This list should
/// be consistent with the languages listed in the AppLocalizations.supportedLocales
/// property.
abstract class AppLocalizations {
  AppLocalizations(String locale)
    : localeName = intl.Intl.canonicalizedLocale(locale.toString());

  final String localeName;

  static AppLocalizations of(BuildContext context) {
    return Localizations.of<AppLocalizations>(context, AppLocalizations)!;
  }

  static const LocalizationsDelegate<AppLocalizations> delegate =
      _AppLocalizationsDelegate();

  /// A list of this localizations delegate along with the default localizations
  /// delegates.
  ///
  /// Returns a list of localizations delegates containing this delegate along with
  /// GlobalMaterialLocalizations.delegate, GlobalCupertinoLocalizations.delegate,
  /// and GlobalWidgetsLocalizations.delegate.
  ///
  /// Additional delegates can be added by appending to this list in
  /// MaterialApp. This list does not have to be used at all if a custom list
  /// of delegates is preferred or required.
  static const List<LocalizationsDelegate<dynamic>> localizationsDelegates =
      <LocalizationsDelegate<dynamic>>[
        delegate,
        GlobalMaterialLocalizations.delegate,
        GlobalCupertinoLocalizations.delegate,
        GlobalWidgetsLocalizations.delegate,
      ];

  /// A list of this localizations delegate's supported locales.
  static const List<Locale> supportedLocales = <Locale>[
    Locale('en'),
    Locale('zh'),
  ];

  /// No description provided for @appName.
  ///
  /// In zh, this message translates to:
  /// **'VaultSync'**
  String get appName;

  /// No description provided for @decoyCancel.
  ///
  /// In zh, this message translates to:
  /// **'取消'**
  String get decoyCancel;

  /// No description provided for @decoyClose.
  ///
  /// In zh, this message translates to:
  /// **'关闭'**
  String get decoyClose;

  /// No description provided for @decoyCreate.
  ///
  /// In zh, this message translates to:
  /// **'创建伪装空间'**
  String get decoyCreate;

  /// No description provided for @decoyCreateFailed.
  ///
  /// In zh, this message translates to:
  /// **'创建失败：{e}'**
  String decoyCreateFailed(Object e);

  /// No description provided for @decoyCreating.
  ///
  /// In zh, this message translates to:
  /// **'创建中…'**
  String get decoyCreating;

  /// No description provided for @decoyHintMissing.
  ///
  /// In zh, this message translates to:
  /// **'尚未创建伪装空间：打开上方开关时设置伪装密码即可创建（独立文件、独立 MK）。'**
  String get decoyHintMissing;

  /// No description provided for @decoyHintReady.
  ///
  /// In zh, this message translates to:
  /// **'伪装空间已就绪：锁屏输入伪装密码即进入伪空间（开关关闭后不再承认该密码）。'**
  String get decoyHintReady;

  /// No description provided for @decoyMismatch.
  ///
  /// In zh, this message translates to:
  /// **'两次输入的伪装密码不一致'**
  String get decoyMismatch;

  /// No description provided for @decoyNote.
  ///
  /// In zh, this message translates to:
  /// **'伪装空间是独立加密容器，可以正常导入文件作为掩护；真实保险箱在伪空间内一律拒绝访问，且不记录「进入了哪个空间」（仅记录进入了挂锁门）。'**
  String get decoyNote;

  /// No description provided for @decoyPwConfirmLabel.
  ///
  /// In zh, this message translates to:
  /// **'确认伪装密码'**
  String get decoyPwConfirmLabel;

  /// No description provided for @decoyPwHint.
  ///
  /// In zh, this message translates to:
  /// **'必须与主密码不同 · 胁迫场景下输入'**
  String get decoyPwHint;

  /// No description provided for @decoyPwLabel.
  ///
  /// In zh, this message translates to:
  /// **'伪装密码'**
  String get decoyPwLabel;

  /// No description provided for @decoySub.
  ///
  /// In zh, this message translates to:
  /// **'独立保险箱文件 · 独立 MK · 与真实空间零共享'**
  String get decoySub;

  /// No description provided for @decoyTitleNew.
  ///
  /// In zh, this message translates to:
  /// **'设置伪装入口'**
  String get decoyTitleNew;

  /// No description provided for @decoyTitleReset.
  ///
  /// In zh, this message translates to:
  /// **'重设伪装密码'**
  String get decoyTitleReset;

  /// No description provided for @devicesPageAddrHint.
  ///
  /// In zh, this message translates to:
  /// **'host:port，如 192.168.1.10:9100'**
  String get devicesPageAddrHint;

  /// No description provided for @devicesPageBadgeLocal.
  ///
  /// In zh, this message translates to:
  /// **'本机'**
  String get devicesPageBadgeLocal;

  /// No description provided for @devicesPageBadgePaired.
  ///
  /// In zh, this message translates to:
  /// **'已配对'**
  String get devicesPageBadgePaired;

  /// No description provided for @devicesPageCancel.
  ///
  /// In zh, this message translates to:
  /// **'取消'**
  String get devicesPageCancel;

  /// No description provided for @devicesPageCancelDestroy.
  ///
  /// In zh, this message translates to:
  /// **'取消销毁'**
  String get devicesPageCancelDestroy;

  /// No description provided for @devicesPageConfirmUnpair.
  ///
  /// In zh, this message translates to:
  /// **'确认解绑'**
  String get devicesPageConfirmUnpair;

  /// No description provided for @devicesPageConnectPair.
  ///
  /// In zh, this message translates to:
  /// **'连接并配对'**
  String get devicesPageConnectPair;

  /// No description provided for @devicesPageDelay10s.
  ///
  /// In zh, this message translates to:
  /// **'10 秒'**
  String get devicesPageDelay10s;

  /// No description provided for @devicesPageDelay10sSub.
  ///
  /// In zh, this message translates to:
  /// **'给误操作留出撤回窗口'**
  String get devicesPageDelay10sSub;

  /// No description provided for @devicesPageDelay5m.
  ///
  /// In zh, this message translates to:
  /// **'5 分钟'**
  String get devicesPageDelay5m;

  /// No description provided for @devicesPageDelay5mSub.
  ///
  /// In zh, this message translates to:
  /// **'长延时 · 期间可随时取消'**
  String get devicesPageDelay5mSub;

  /// No description provided for @devicesPageDelay60s.
  ///
  /// In zh, this message translates to:
  /// **'60 秒'**
  String get devicesPageDelay60s;

  /// No description provided for @devicesPageDelay60sSub.
  ///
  /// In zh, this message translates to:
  /// **'可在设备页「取消销毁」'**
  String get devicesPageDelay60sSub;

  /// No description provided for @devicesPageDelayImmediate.
  ///
  /// In zh, this message translates to:
  /// **'立即执行'**
  String get devicesPageDelayImmediate;

  /// No description provided for @devicesPageDelayImmediateSub.
  ///
  /// In zh, this message translates to:
  /// **'签发后指令即进入信令队列'**
  String get devicesPageDelayImmediateSub;

  /// No description provided for @devicesPageDestroyArmed.
  ///
  /// In zh, this message translates to:
  /// **'远程销毁已武装'**
  String get devicesPageDestroyArmed;

  /// No description provided for @devicesPageDestroyArmedSub.
  ///
  /// In zh, this message translates to:
  /// **'目标 {target} · 还剩余 {seconds} 秒'**
  String devicesPageDestroyArmedSub(String target, int seconds);

  /// No description provided for @devicesPageDestroyConfirmError.
  ///
  /// In zh, this message translates to:
  /// **'请准确输入确认语「{word}」'**
  String devicesPageDestroyConfirmError(String word);

  /// No description provided for @devicesPageDestroyConfirmInput.
  ///
  /// In zh, this message translates to:
  /// **'输入「{word}」以确认'**
  String devicesPageDestroyConfirmInput(String word);

  /// No description provided for @devicesPageDestroyConfirmWord.
  ///
  /// In zh, this message translates to:
  /// **'销毁'**
  String get devicesPageDestroyConfirmWord;

  /// No description provided for @devicesPageDestroyDelay.
  ///
  /// In zh, this message translates to:
  /// **'销毁延时'**
  String get devicesPageDestroyDelay;

  /// No description provided for @devicesPageDestroyNote.
  ///
  /// In zh, this message translates to:
  /// **'将向目标设备签发由本机密钥签名的销毁指令（高优先级信令队列）。设备离线时，将于下次上线后、处理任何其他消息之前执行。'**
  String get devicesPageDestroyNote;

  /// No description provided for @devicesPageDestroySendFailed.
  ///
  /// In zh, this message translates to:
  /// **'远程销毁指令发送失败'**
  String get devicesPageDestroySendFailed;

  /// No description provided for @devicesPageDestroySent.
  ///
  /// In zh, this message translates to:
  /// **'销毁指令已发送'**
  String get devicesPageDestroySent;

  /// No description provided for @devicesPageDestroySentTo.
  ///
  /// In zh, this message translates to:
  /// **'远程销毁指令已发送至 {name}'**
  String devicesPageDestroySentTo(String name);

  /// No description provided for @devicesPageDestroyTarget.
  ///
  /// In zh, this message translates to:
  /// **'目标：{name}'**
  String devicesPageDestroyTarget(String name);

  /// No description provided for @devicesPageDone.
  ///
  /// In zh, this message translates to:
  /// **'完成'**
  String get devicesPageDone;

  /// No description provided for @devicesPageEmptySub.
  ///
  /// In zh, this message translates to:
  /// **'先解锁保险箱，或通过「配对新设备」建立第一个加密信道。'**
  String get devicesPageEmptySub;

  /// No description provided for @devicesPageEmptyTitle.
  ///
  /// In zh, this message translates to:
  /// **'暂无设备数据'**
  String get devicesPageEmptyTitle;

  /// No description provided for @devicesPageFingerprint.
  ///
  /// In zh, this message translates to:
  /// **'指纹'**
  String get devicesPageFingerprint;

  /// No description provided for @devicesPageFingerprintConfirm.
  ///
  /// In zh, this message translates to:
  /// **'指纹一致，确认配对'**
  String get devicesPageFingerprintConfirm;

  /// No description provided for @devicesPageGenerateInvite.
  ///
  /// In zh, this message translates to:
  /// **'生成邀请码'**
  String get devicesPageGenerateInvite;

  /// No description provided for @devicesPageHeaderSub.
  ///
  /// In zh, this message translates to:
  /// **'通过端到端加密信道与已配对设备同步'**
  String get devicesPageHeaderSub;

  /// No description provided for @devicesPageHeaderSubCounts.
  ///
  /// In zh, this message translates to:
  /// **'{peers} 台已配对设备 · {online} 台本机在线'**
  String devicesPageHeaderSubCounts(int peers, int online);

  /// No description provided for @devicesPageInviteCode.
  ///
  /// In zh, this message translates to:
  /// **'一次性邀请码'**
  String get devicesPageInviteCode;

  /// No description provided for @devicesPageInviteCodePasteHint.
  ///
  /// In zh, this message translates to:
  /// **'粘贴对方生成的邀请码'**
  String get devicesPageInviteCodePasteHint;

  /// No description provided for @devicesPageInviteCodeValid.
  ///
  /// In zh, this message translates to:
  /// **'一次性邀请码（10 分钟内有效）'**
  String get devicesPageInviteCodeValid;

  /// No description provided for @devicesPageInviteFailed.
  ///
  /// In zh, this message translates to:
  /// **'邀请码生成失败'**
  String get devicesPageInviteFailed;

  /// No description provided for @devicesPageInviteNote.
  ///
  /// In zh, this message translates to:
  /// **'邀请码 10 分钟内有效，用后即失效；内容不含敏感信息。'**
  String get devicesPageInviteNote;

  /// No description provided for @devicesPageIssueDestroy.
  ///
  /// In zh, this message translates to:
  /// **'签发销毁指令'**
  String get devicesPageIssueDestroy;

  /// No description provided for @devicesPageKvDevice.
  ///
  /// In zh, this message translates to:
  /// **'设备'**
  String get devicesPageKvDevice;

  /// No description provided for @devicesPageLastSyncJustNow.
  ///
  /// In zh, this message translates to:
  /// **'上次同步 刚刚'**
  String get devicesPageLastSyncJustNow;

  /// No description provided for @devicesPageLocalFingerprint.
  ///
  /// In zh, this message translates to:
  /// **'本机指纹（Fingerprint）'**
  String get devicesPageLocalFingerprint;

  /// No description provided for @devicesPageLocalFingerprintShort.
  ///
  /// In zh, this message translates to:
  /// **'本机指纹'**
  String get devicesPageLocalFingerprintShort;

  /// No description provided for @devicesPageManualAdd.
  ///
  /// In zh, this message translates to:
  /// **'手动添加'**
  String get devicesPageManualAdd;

  /// No description provided for @devicesPageManualHint.
  ///
  /// In zh, this message translates to:
  /// **'输入对端地址与对方设备生成的一次性邀请码：'**
  String get devicesPageManualHint;

  /// No description provided for @devicesPageManualMode.
  ///
  /// In zh, this message translates to:
  /// **'手动输入'**
  String get devicesPageManualMode;

  /// No description provided for @devicesPageMitmNote.
  ///
  /// In zh, this message translates to:
  /// **'指纹不一致说明信道可能被劫持（MITM），请拒绝配对。'**
  String get devicesPageMitmNote;

  /// No description provided for @devicesPageNeedUnlock.
  ///
  /// In zh, this message translates to:
  /// **'请先解锁'**
  String get devicesPageNeedUnlock;

  /// No description provided for @devicesPageNoPeersSub.
  ///
  /// In zh, this message translates to:
  /// **'点击右上角「配对新设备」，扫码或手动输入邀请码建立信道。'**
  String get devicesPageNoPeersSub;

  /// No description provided for @devicesPageNoPeersTitle.
  ///
  /// In zh, this message translates to:
  /// **'暂无已配对设备'**
  String get devicesPageNoPeersTitle;

  /// No description provided for @devicesPageNotifDestroyCancelFailed.
  ///
  /// In zh, this message translates to:
  /// **'取消销毁失败（错误码 {code}）'**
  String devicesPageNotifDestroyCancelFailed(int code);

  /// No description provided for @devicesPageNotifDestroyCancelled.
  ///
  /// In zh, this message translates to:
  /// **'已取消远程销毁'**
  String get devicesPageNotifDestroyCancelled;

  /// No description provided for @devicesPageNotifNeedUnlock.
  ///
  /// In zh, this message translates to:
  /// **'请先解锁保险箱'**
  String get devicesPageNotifNeedUnlock;

  /// No description provided for @devicesPageNotifPaired.
  ///
  /// In zh, this message translates to:
  /// **'配对完成 · 已加入节点清单'**
  String get devicesPageNotifPaired;

  /// No description provided for @devicesPageOffline.
  ///
  /// In zh, this message translates to:
  /// **'离线'**
  String get devicesPageOffline;

  /// No description provided for @devicesPageOnline.
  ///
  /// In zh, this message translates to:
  /// **'在线'**
  String get devicesPageOnline;

  /// No description provided for @devicesPagePairDone.
  ///
  /// In zh, this message translates to:
  /// **'配对完成'**
  String get devicesPagePairDone;

  /// No description provided for @devicesPagePairDoneSub.
  ///
  /// In zh, this message translates to:
  /// **'已建立长期加密信道 · 双方节点清单已更新 · 已记入审计'**
  String get devicesPagePairDoneSub;

  /// No description provided for @devicesPagePairFailed.
  ///
  /// In zh, this message translates to:
  /// **'配对异常：{e}'**
  String devicesPagePairFailed(String e);

  /// No description provided for @devicesPagePairFailedInvalid.
  ///
  /// In zh, this message translates to:
  /// **'配对失败：地址或邀请码无效'**
  String get devicesPagePairFailedInvalid;

  /// No description provided for @devicesPagePairNew.
  ///
  /// In zh, this message translates to:
  /// **'配对新设备'**
  String get devicesPagePairNew;

  /// No description provided for @devicesPagePeerAddr.
  ///
  /// In zh, this message translates to:
  /// **'对端设备地址'**
  String get devicesPagePeerAddr;

  /// No description provided for @devicesPagePeerDevice.
  ///
  /// In zh, this message translates to:
  /// **'对方设备'**
  String get devicesPagePeerDevice;

  /// No description provided for @devicesPagePeerDeviceName.
  ///
  /// In zh, this message translates to:
  /// **'对方设备：{name}'**
  String devicesPagePeerDeviceName(String name);

  /// No description provided for @devicesPageQrHint.
  ///
  /// In zh, this message translates to:
  /// **'点击下方「生成邀请码」生成一次性邀请码与二维码，供对方设备在「配对 → 手动输入」中连接。'**
  String get devicesPageQrHint;

  /// No description provided for @devicesPageRefresh.
  ///
  /// In zh, this message translates to:
  /// **'刷新'**
  String get devicesPageRefresh;

  /// No description provided for @devicesPageRemoteWipe.
  ///
  /// In zh, this message translates to:
  /// **'远程销毁'**
  String get devicesPageRemoteWipe;

  /// No description provided for @devicesPageSelfRole.
  ///
  /// In zh, this message translates to:
  /// **'本机 · 主节点'**
  String get devicesPageSelfRole;

  /// No description provided for @devicesPageSelfSub.
  ///
  /// In zh, this message translates to:
  /// **'{id} · 端口 {port}'**
  String devicesPageSelfSub(String id, String port);

  /// No description provided for @devicesPageSending.
  ///
  /// In zh, this message translates to:
  /// **'发送中…'**
  String get devicesPageSending;

  /// No description provided for @devicesPageStepChoose.
  ///
  /// In zh, this message translates to:
  /// **'选择方式'**
  String get devicesPageStepChoose;

  /// No description provided for @devicesPageStepDone.
  ///
  /// In zh, this message translates to:
  /// **'完成配对'**
  String get devicesPageStepDone;

  /// No description provided for @devicesPageStepVerify.
  ///
  /// In zh, this message translates to:
  /// **'指纹核验'**
  String get devicesPageStepVerify;

  /// No description provided for @devicesPageThisComputer.
  ///
  /// In zh, this message translates to:
  /// **'这台电脑'**
  String get devicesPageThisComputer;

  /// No description provided for @devicesPageTitle.
  ///
  /// In zh, this message translates to:
  /// **'设备'**
  String get devicesPageTitle;

  /// No description provided for @devicesPageUnbind.
  ///
  /// In zh, this message translates to:
  /// **'解绑'**
  String get devicesPageUnbind;

  /// No description provided for @devicesPageUnpairFailed.
  ///
  /// In zh, this message translates to:
  /// **'解绑失败：{err}'**
  String devicesPageUnpairFailed(String err);

  /// No description provided for @devicesPageUnpairNote.
  ///
  /// In zh, this message translates to:
  /// **'解绑只删除本机的对端登记：对方设备与其上的数据不受影响；本机文件也不会被删除。重新建立信道需再次交换一次性邀请码并核验指纹。'**
  String get devicesPageUnpairNote;

  /// No description provided for @devicesPageUnpairSub.
  ///
  /// In zh, this message translates to:
  /// **'移除「{name}」的互信记录'**
  String devicesPageUnpairSub(String name);

  /// No description provided for @devicesPageUnpairTitle.
  ///
  /// In zh, this message translates to:
  /// **'解除配对'**
  String get devicesPageUnpairTitle;

  /// No description provided for @devicesPageUnpaired.
  ///
  /// In zh, this message translates to:
  /// **'已解除与「{name}」的配对'**
  String devicesPageUnpaired(String name);

  /// No description provided for @devicesPageVerifyHint.
  ///
  /// In zh, this message translates to:
  /// **'请在两台设备上分别核对以下指纹是否一致（公钥哈希）：'**
  String get devicesPageVerifyHint;

  /// No description provided for @devicesPageWizardSub.
  ///
  /// In zh, this message translates to:
  /// **'交换公钥 · 校验指纹 · 双向确认'**
  String get devicesPageWizardSub;

  /// No description provided for @importDialogCancel.
  ///
  /// In zh, this message translates to:
  /// **'取消'**
  String get importDialogCancel;

  /// No description provided for @importDialogDropHere.
  ///
  /// In zh, this message translates to:
  /// **'拖放文件到此处'**
  String get importDialogDropHere;

  /// No description provided for @importDialogHint.
  ///
  /// In zh, this message translates to:
  /// **'支持多选 · 不限文件类型'**
  String get importDialogHint;

  /// No description provided for @importDialogImport.
  ///
  /// In zh, this message translates to:
  /// **'导入'**
  String get importDialogImport;

  /// No description provided for @importDialogImportCount.
  ///
  /// In zh, this message translates to:
  /// **'导入 {n} 个文件'**
  String importDialogImportCount(int n);

  /// No description provided for @importDialogNoFiles.
  ///
  /// In zh, this message translates to:
  /// **'尚未选择文件'**
  String get importDialogNoFiles;

  /// No description provided for @importDialogNote.
  ///
  /// In zh, this message translates to:
  /// **'导入完成后立即生成密文缩略图与全文索引（明文不落盘）。'**
  String get importDialogNote;

  /// No description provided for @importDialogOrClick.
  ///
  /// In zh, this message translates to:
  /// **'，或点击选择'**
  String get importDialogOrClick;

  /// No description provided for @importDialogRemove.
  ///
  /// In zh, this message translates to:
  /// **'移除'**
  String get importDialogRemove;

  /// No description provided for @importDialogSub.
  ///
  /// In zh, this message translates to:
  /// **'流式读取 · CDC 分块 · AES-256-GCM 逐块加密'**
  String get importDialogSub;

  /// No description provided for @importDialogTitle.
  ///
  /// In zh, this message translates to:
  /// **'导入文件'**
  String get importDialogTitle;

  /// No description provided for @lockScreenBindManage.
  ///
  /// In zh, this message translates to:
  /// **'绑定 / 管理'**
  String get lockScreenBindManage;

  /// No description provided for @lockScreenBiometric.
  ///
  /// In zh, this message translates to:
  /// **'生物识别'**
  String get lockScreenBiometric;

  /// No description provided for @lockScreenBusy.
  ///
  /// In zh, this message translates to:
  /// **'处理中…'**
  String get lockScreenBusy;

  /// No description provided for @lockScreenCooldown.
  ///
  /// In zh, this message translates to:
  /// **'尝试过于频繁，已触发暴力破解保护 · {remaining} 后可重试'**
  String lockScreenCooldown(Object remaining);

  /// No description provided for @lockScreenCreateAndUnlock.
  ///
  /// In zh, this message translates to:
  /// **'创建并解锁'**
  String get lockScreenCreateAndUnlock;

  /// No description provided for @lockScreenFirstRunNote.
  ///
  /// In zh, this message translates to:
  /// **'首次启动 · 主密码 ≥{n} 位并混合字符类型；MK 由 CSPRNG 生成，改密只需重新包装'**
  String lockScreenFirstRunNote(Object n);

  /// No description provided for @lockScreenForgotPassword.
  ///
  /// In zh, this message translates to:
  /// **'忘记主密码？'**
  String get lockScreenForgotPassword;

  /// No description provided for @lockScreenHintWithAttempts.
  ///
  /// In zh, this message translates to:
  /// **'{hint}（第 {n} 次）'**
  String lockScreenHintWithAttempts(Object hint, Object n);

  /// No description provided for @lockScreenPwEnterHint.
  ///
  /// In zh, this message translates to:
  /// **'输入主密码'**
  String get lockScreenPwEnterHint;

  /// No description provided for @lockScreenPwSetHint.
  ///
  /// In zh, this message translates to:
  /// **'设置主密码（至少 {n} 位）'**
  String lockScreenPwSetHint(Object n);

  /// No description provided for @lockScreenPwStrengthOk.
  ///
  /// In zh, this message translates to:
  /// **'{label} · 可通过（Argon2id 派生 KEK）'**
  String lockScreenPwStrengthOk(Object label);

  /// No description provided for @lockScreenPwStrengthWeak.
  ///
  /// In zh, this message translates to:
  /// **'{label} · 需要至少 {n} 位且混合字符类型'**
  String lockScreenPwStrengthWeak(Object label, Object n);

  /// No description provided for @lockScreenPwSuggestMix.
  ///
  /// In zh, this message translates to:
  /// **'建议混合大小写并加入数字或符号'**
  String get lockScreenPwSuggestMix;

  /// No description provided for @lockScreenPwTooShort.
  ///
  /// In zh, this message translates to:
  /// **'至少 {n} 位'**
  String lockScreenPwTooShort(Object n);

  /// No description provided for @lockScreenPwWrongAttempts.
  ///
  /// In zh, this message translates to:
  /// **'主密码错误（第 {n} 次）'**
  String lockScreenPwWrongAttempts(Object n);

  /// No description provided for @lockScreenRecoveryInfo.
  ///
  /// In zh, this message translates to:
  /// **'MK 由 CSPRNG 生成且不被密码反推，无法凭记忆恢复；重置需已配对设备授权（P5 接入），或使用伪装入口进入伪空间。'**
  String get lockScreenRecoveryInfo;

  /// No description provided for @lockScreenTagline.
  ///
  /// In zh, this message translates to:
  /// **'YOUR DATA · EVERYWHERE · ENCRYPTED'**
  String get lockScreenTagline;

  /// No description provided for @lockScreenTitle.
  ///
  /// In zh, this message translates to:
  /// **'VaultSync 保险箱'**
  String get lockScreenTitle;

  /// No description provided for @lockScreenUnlock.
  ///
  /// In zh, this message translates to:
  /// **'解锁'**
  String get lockScreenUnlock;

  /// No description provided for @navDevices.
  ///
  /// In zh, this message translates to:
  /// **'设备'**
  String get navDevices;

  /// No description provided for @navSecurity.
  ///
  /// In zh, this message translates to:
  /// **'安全中心'**
  String get navSecurity;

  /// No description provided for @navSettings.
  ///
  /// In zh, this message translates to:
  /// **'设置'**
  String get navSettings;

  /// No description provided for @navStego.
  ///
  /// In zh, this message translates to:
  /// **'隐写术'**
  String get navStego;

  /// No description provided for @navSync.
  ///
  /// In zh, this message translates to:
  /// **'同步'**
  String get navSync;

  /// No description provided for @navVault.
  ///
  /// In zh, this message translates to:
  /// **'保险箱'**
  String get navVault;

  /// No description provided for @securityAuditChainBroken.
  ///
  /// In zh, this message translates to:
  /// **'链校验失败'**
  String get securityAuditChainBroken;

  /// No description provided for @securityAuditChainOk.
  ///
  /// In zh, this message translates to:
  /// **'链校验通过'**
  String get securityAuditChainOk;

  /// No description provided for @securityAuditEmpty.
  ///
  /// In zh, this message translates to:
  /// **'暂无审计记录 · 解锁/导入/同步等操作会写入'**
  String get securityAuditEmpty;

  /// No description provided for @securityAuditExport.
  ///
  /// In zh, this message translates to:
  /// **'导出'**
  String get securityAuditExport;

  /// No description provided for @securityAuditExportFailed.
  ///
  /// In zh, this message translates to:
  /// **'审计导出失败：{err}'**
  String securityAuditExportFailed(String err);

  /// No description provided for @securityAuditExportedTo.
  ///
  /// In zh, this message translates to:
  /// **'审计日志已加密导出到 {path}'**
  String securityAuditExportedTo(String path);

  /// No description provided for @securityAuditHead.
  ///
  /// In zh, this message translates to:
  /// **'本机链头 {short}'**
  String securityAuditHead(String short);

  /// No description provided for @securityAuditLoadFailed.
  ///
  /// In zh, this message translates to:
  /// **'审计日志读取失败（引擎不可用或会话已失效）'**
  String get securityAuditLoadFailed;

  /// No description provided for @securityAuditNote.
  ///
  /// In zh, this message translates to:
  /// **'审计链按设备独立（每设备一条链，docs/05-06 §3.1），本页只显示本机链；未解锁时的失败尝试无法写入（日志密钥派生自 MK），故链上只有成功操作；日志为整体 AEAD 加密落盘，条目多时首次加载会略慢。'**
  String get securityAuditNote;

  /// No description provided for @securityAuditTitle.
  ///
  /// In zh, this message translates to:
  /// **'审计日志'**
  String get securityAuditTitle;

  /// No description provided for @securityAuditVerify.
  ///
  /// In zh, this message translates to:
  /// **'校验审计链'**
  String get securityAuditVerify;

  /// No description provided for @securityAuditVerifyFailed.
  ///
  /// In zh, this message translates to:
  /// **'审计链第 {at} 条起校验失败：{reason}'**
  String securityAuditVerifyFailed(int at, String reason);

  /// No description provided for @securityAuditVerifyOk.
  ///
  /// In zh, this message translates to:
  /// **'审计链校验通过（{checked} 条）'**
  String securityAuditVerifyOk(int checked);

  /// No description provided for @securityAuditVerifyUnavailable.
  ///
  /// In zh, this message translates to:
  /// **'未能读取审计链，校验未执行'**
  String get securityAuditVerifyUnavailable;

  /// No description provided for @securityBadgeGuarantee.
  ///
  /// In zh, this message translates to:
  /// **'设计保证'**
  String get securityBadgeGuarantee;

  /// No description provided for @securityCatDevice.
  ///
  /// In zh, this message translates to:
  /// **'设备'**
  String get securityCatDevice;

  /// No description provided for @securityCatSecurity.
  ///
  /// In zh, this message translates to:
  /// **'安全'**
  String get securityCatSecurity;

  /// No description provided for @securityCatSession.
  ///
  /// In zh, this message translates to:
  /// **'会话'**
  String get securityCatSession;

  /// No description provided for @securityCatVault.
  ///
  /// In zh, this message translates to:
  /// **'保险箱'**
  String get securityCatVault;

  /// No description provided for @securityColAction.
  ///
  /// In zh, this message translates to:
  /// **'动作 / 对象'**
  String get securityColAction;

  /// No description provided for @securityColCategory.
  ///
  /// In zh, this message translates to:
  /// **'类别'**
  String get securityColCategory;

  /// No description provided for @securityColHash.
  ///
  /// In zh, this message translates to:
  /// **'链哈希'**
  String get securityColHash;

  /// No description provided for @securityColSeq.
  ///
  /// In zh, this message translates to:
  /// **'序号'**
  String get securityColSeq;

  /// No description provided for @securityColTime.
  ///
  /// In zh, this message translates to:
  /// **'时间'**
  String get securityColTime;

  /// No description provided for @securityDestroyAllConfirmBody.
  ///
  /// In zh, this message translates to:
  /// **'引擎当前无离线指令队列，无法向离线设备投递销毁指令。确认后将只在本机执行加密擦除；已配对 {n} 台设备需各自在线时由对端设备页发起。'**
  String securityDestroyAllConfirmBody(int n);

  /// No description provided for @securityDestroyAllConfirmTitle.
  ///
  /// In zh, this message translates to:
  /// **'确认全设备联动销毁'**
  String get securityDestroyAllConfirmTitle;

  /// No description provided for @securityDestroyCancel.
  ///
  /// In zh, this message translates to:
  /// **'取消'**
  String get securityDestroyCancel;

  /// No description provided for @securityDestroyConfirm.
  ///
  /// In zh, this message translates to:
  /// **'确认'**
  String get securityDestroyConfirm;

  /// No description provided for @securityDestroyConfirmInput.
  ///
  /// In zh, this message translates to:
  /// **'输入「{word}」以确认'**
  String securityDestroyConfirmInput(String word);

  /// No description provided for @securityDestroyDialogSub.
  ///
  /// In zh, this message translates to:
  /// **'不可撤销 · 需输入确认短语二次把关'**
  String get securityDestroyDialogSub;

  /// No description provided for @securityDestroyDialogTitle.
  ///
  /// In zh, this message translates to:
  /// **'紧急销毁'**
  String get securityDestroyDialogTitle;

  /// No description provided for @securityDestroyDone.
  ///
  /// In zh, this message translates to:
  /// **'本机数据已加密擦除'**
  String get securityDestroyDone;

  /// No description provided for @securityDestroyEnter.
  ///
  /// In zh, this message translates to:
  /// **'进入销毁流程'**
  String get securityDestroyEnter;

  /// No description provided for @securityDestroyExportCancelled.
  ///
  /// In zh, this message translates to:
  /// **'未选择导出路径，已取消销毁'**
  String get securityDestroyExportCancelled;

  /// No description provided for @securityDestroyExportFirst.
  ///
  /// In zh, this message translates to:
  /// **'先导出审计日志'**
  String get securityDestroyExportFirst;

  /// No description provided for @securityDestroyExportFirstSub.
  ///
  /// In zh, this message translates to:
  /// **'数据目录连同审计日志会被一并销毁，勾选后先导出到所选路径'**
  String get securityDestroyExportFirstSub;

  /// No description provided for @securityDestroyExported.
  ///
  /// In zh, this message translates to:
  /// **'审计日志已导出到 {path}'**
  String securityDestroyExported(String path);

  /// No description provided for @securityDestroyFailed.
  ///
  /// In zh, this message translates to:
  /// **'销毁失败：{err}'**
  String securityDestroyFailed(String err);

  /// No description provided for @securityDestroyModeAll.
  ///
  /// In zh, this message translates to:
  /// **'全设备联动销毁'**
  String get securityDestroyModeAll;

  /// No description provided for @securityDestroyModeAllSub.
  ///
  /// In zh, this message translates to:
  /// **'签发签名指令，经 P2P / 中继传播至全部已配对设备'**
  String get securityDestroyModeAllSub;

  /// No description provided for @securityDestroyModeLocal.
  ///
  /// In zh, this message translates to:
  /// **'本机销毁'**
  String get securityDestroyModeLocal;

  /// No description provided for @securityDestroyModeLocalSub.
  ///
  /// In zh, this message translates to:
  /// **'销毁本机 MK 与全部密钥槽，按介质兜底覆写'**
  String get securityDestroyModeLocalSub;

  /// No description provided for @securityDestroyNote.
  ///
  /// In zh, this message translates to:
  /// **'离线设备无法即时收到销毁指令：指令进入各设备的高优先级信令队列，目标设备下次上线时先于任何其它消息校验并执行（指令由发起方设备密钥签名，防伪造）。对「永不上线」的设备，销毁无法保证。'**
  String get securityDestroyNote;

  /// No description provided for @securityDestroyNow.
  ///
  /// In zh, this message translates to:
  /// **'立即销毁'**
  String get securityDestroyNow;

  /// No description provided for @securityDestroyPeersHint.
  ///
  /// In zh, this message translates to:
  /// **'需先让对端在线，并由对端设备页发起'**
  String get securityDestroyPeersHint;

  /// No description provided for @securityDestroyPeersNone.
  ///
  /// In zh, this message translates to:
  /// **'全设备联动：当前无已配对设备，仅本机执行加密擦除'**
  String get securityDestroyPeersNone;

  /// No description provided for @securityDestroyPeersUndelivered.
  ///
  /// In zh, this message translates to:
  /// **'全设备联动未完成投递：本机已擦除，{n} 台已配对设备需各自在线时由对端设备页发起销毁'**
  String securityDestroyPeersUndelivered(int n);

  /// No description provided for @securityDestroyPhrase.
  ///
  /// In zh, this message translates to:
  /// **'销毁'**
  String get securityDestroyPhrase;

  /// No description provided for @securityDestroyRunning.
  ///
  /// In zh, this message translates to:
  /// **'销毁中…'**
  String get securityDestroyRunning;

  /// No description provided for @securityDestroySub.
  ///
  /// In zh, this message translates to:
  /// **'本地 / 远程 / 全设备联动 · 加密擦除'**
  String get securityDestroySub;

  /// No description provided for @securityDestroyTitle.
  ///
  /// In zh, this message translates to:
  /// **'紧急销毁'**
  String get securityDestroyTitle;

  /// No description provided for @securityDetectAudit.
  ///
  /// In zh, this message translates to:
  /// **'审计链完整性'**
  String get securityDetectAudit;

  /// No description provided for @securityDetectAuditBroken.
  ///
  /// In zh, this message translates to:
  /// **'第 {at} 条起校验失败：{reason}'**
  String securityDetectAuditBroken(int at, String reason);

  /// No description provided for @securityDetectAuditOk.
  ///
  /// In zh, this message translates to:
  /// **'正常 · {checked} / {total} 条校验通过'**
  String securityDetectAuditOk(int checked, int total);

  /// No description provided for @securityDetectAuditUnknown.
  ///
  /// In zh, this message translates to:
  /// **'未能读取审计链 · 无法校验'**
  String get securityDetectAuditUnknown;

  /// No description provided for @securityDetectBrute.
  ///
  /// In zh, this message translates to:
  /// **'暴力破解检测'**
  String get securityDetectBrute;

  /// No description provided for @securityDetectBruteCooldown.
  ///
  /// In zh, this message translates to:
  /// **'冷却中 · 剩余 {m} 分 {s} 秒'**
  String securityDetectBruteCooldown(int m, int s);

  /// No description provided for @securityDetectBruteOk.
  ///
  /// In zh, this message translates to:
  /// **'正常 · 当前无冷却'**
  String get securityDetectBruteOk;

  /// No description provided for @securityDetectDevice.
  ///
  /// In zh, this message translates to:
  /// **'异常设备'**
  String get securityDetectDevice;

  /// No description provided for @securityDetectDeviceNone.
  ///
  /// In zh, this message translates to:
  /// **'尚无已配对设备'**
  String get securityDetectDeviceNone;

  /// No description provided for @securityDetectDeviceOk.
  ///
  /// In zh, this message translates to:
  /// **'已配对 {n} 台 · 无未授权访问记录'**
  String securityDetectDeviceOk(int n);

  /// No description provided for @securityDetectDeviceWarn.
  ///
  /// In zh, this message translates to:
  /// **'注意 · {event}'**
  String securityDetectDeviceWarn(String event);

  /// No description provided for @securityDetectIntegrity.
  ///
  /// In zh, this message translates to:
  /// **'文件完整性'**
  String get securityDetectIntegrity;

  /// No description provided for @securityDetectIntegritySub.
  ///
  /// In zh, this message translates to:
  /// **'随导出/同步逐块 GCM 校验 · 认证失败即拒绝'**
  String get securityDetectIntegritySub;

  /// No description provided for @securityDetectMemory.
  ///
  /// In zh, this message translates to:
  /// **'内存泄漏扫描'**
  String get securityDetectMemory;

  /// No description provided for @securityDetectMemorySub.
  ///
  /// In zh, this message translates to:
  /// **'密钥与明文缓冲全链路 Zeroizing · MK 不出引擎'**
  String get securityDetectMemorySub;

  /// No description provided for @securityDetectSignal.
  ///
  /// In zh, this message translates to:
  /// **'信令防重放'**
  String get securityDetectSignal;

  /// No description provided for @securityDetectSignalDown.
  ///
  /// In zh, this message translates to:
  /// **'引擎已停止 · 无法确认信令序号连续性'**
  String get securityDetectSignalDown;

  /// No description provided for @securityDetectSignalOk.
  ///
  /// In zh, this message translates to:
  /// **'Noise 信道 + 应用层单调序号（docs/08 §4.1）· 本机引擎存活 · 信令事件 {n} 条'**
  String securityDetectSignalOk(int n);

  /// No description provided for @securityDetectSignalUnknown.
  ///
  /// In zh, this message translates to:
  /// **'未取得 P2P 状态 · 无法判定'**
  String get securityDetectSignalUnknown;

  /// No description provided for @securityFilterAll.
  ///
  /// In zh, this message translates to:
  /// **'全部'**
  String get securityFilterAll;

  /// No description provided for @securityNeedUnlock.
  ///
  /// In zh, this message translates to:
  /// **'请先解锁保险箱'**
  String get securityNeedUnlock;

  /// No description provided for @securityOverallDanger.
  ///
  /// In zh, this message translates to:
  /// **'整体状态：异常'**
  String get securityOverallDanger;

  /// No description provided for @securityOverallOk.
  ///
  /// In zh, this message translates to:
  /// **'整体状态：正常'**
  String get securityOverallOk;

  /// No description provided for @securityOverallWarn.
  ///
  /// In zh, this message translates to:
  /// **'整体状态：注意'**
  String get securityOverallWarn;

  /// No description provided for @securityRefresh.
  ///
  /// In zh, this message translates to:
  /// **'刷新'**
  String get securityRefresh;

  /// No description provided for @securityStegoDisable.
  ///
  /// In zh, this message translates to:
  /// **'停用'**
  String get securityStegoDisable;

  /// No description provided for @securityStegoDisabled.
  ///
  /// In zh, this message translates to:
  /// **'隐写术引擎已停用'**
  String get securityStegoDisabled;

  /// No description provided for @securityStegoEnable.
  ///
  /// In zh, this message translates to:
  /// **'启用'**
  String get securityStegoEnable;

  /// No description provided for @securityStegoEnabled.
  ///
  /// In zh, this message translates to:
  /// **'隐写术引擎已启用'**
  String get securityStegoEnabled;

  /// No description provided for @securityStegoFailed.
  ///
  /// In zh, this message translates to:
  /// **'隐写术引擎设置失败：{err}'**
  String securityStegoFailed(String err);

  /// No description provided for @securityStegoSubOff.
  ///
  /// In zh, this message translates to:
  /// **'默认隐藏 · 图片 LSB 隐蔽存储 · 启用/停用均记审计'**
  String get securityStegoSubOff;

  /// No description provided for @securityStegoSubOn.
  ///
  /// In zh, this message translates to:
  /// **'已启用 · 图片 LSB 隐蔽存储 · 启用/停用均记审计'**
  String get securityStegoSubOn;

  /// No description provided for @securityStegoTitle.
  ///
  /// In zh, this message translates to:
  /// **'隐写术引擎'**
  String get securityStegoTitle;

  /// No description provided for @securitySub.
  ///
  /// In zh, this message translates to:
  /// **'审计 · 检测 · 应急响应'**
  String get securitySub;

  /// No description provided for @securityTitle.
  ///
  /// In zh, this message translates to:
  /// **'安全中心'**
  String get securityTitle;

  /// No description provided for @settingsAbout.
  ///
  /// In zh, this message translates to:
  /// **'关于'**
  String get settingsAbout;

  /// No description provided for @settingsAboutSub.
  ///
  /// In zh, this message translates to:
  /// **'版本与设计文档'**
  String get settingsAboutSub;

  /// No description provided for @settingsAppearance.
  ///
  /// In zh, this message translates to:
  /// **'外观与语言'**
  String get settingsAppearance;

  /// No description provided for @settingsAppearanceSub.
  ///
  /// In zh, this message translates to:
  /// **'主题 · 语言'**
  String get settingsAppearanceSub;

  /// No description provided for @settingsAutoLockSub.
  ///
  /// In zh, this message translates to:
  /// **'空闲超时后自动锁定保险箱'**
  String get settingsAutoLockSub;

  /// No description provided for @settingsAutoLockTitle.
  ///
  /// In zh, this message translates to:
  /// **'自动落锁'**
  String get settingsAutoLockTitle;

  /// No description provided for @settingsAutoSync.
  ///
  /// In zh, this message translates to:
  /// **'启用自动同步'**
  String get settingsAutoSync;

  /// No description provided for @settingsAutoSyncSub.
  ///
  /// In zh, this message translates to:
  /// **'选择性同步：文件夹、大小、网络、设备（P5 细化）'**
  String get settingsAutoSyncSub;

  /// No description provided for @settingsBioBound.
  ///
  /// In zh, this message translates to:
  /// **'已绑定 · Windows Hello 指纹'**
  String get settingsBioBound;

  /// No description provided for @settingsBioBoundDone.
  ///
  /// In zh, this message translates to:
  /// **'生物识别已绑定 · 已追加 MK_wrap_bio'**
  String get settingsBioBoundDone;

  /// No description provided for @settingsBioHello.
  ///
  /// In zh, this message translates to:
  /// **'Windows Hello 指纹'**
  String get settingsBioHello;

  /// No description provided for @settingsBioHelloSub.
  ///
  /// In zh, this message translates to:
  /// **'密钥由平台安全区托管，本应用不接触生物特征'**
  String get settingsBioHelloSub;

  /// No description provided for @settingsBioLoading.
  ///
  /// In zh, this message translates to:
  /// **'读取中…'**
  String get settingsBioLoading;

  /// No description provided for @settingsBioNote.
  ///
  /// In zh, this message translates to:
  /// **'安全区重置后生物识别副本自动失效，主密码路径不受影响。'**
  String get settingsBioNote;

  /// No description provided for @settingsBioTitle.
  ///
  /// In zh, this message translates to:
  /// **'生物识别'**
  String get settingsBioTitle;

  /// No description provided for @settingsBioUnbound.
  ///
  /// In zh, this message translates to:
  /// **'未绑定 · 主密码始终可用'**
  String get settingsBioUnbound;

  /// No description provided for @settingsBioUnboundDone.
  ///
  /// In zh, this message translates to:
  /// **'生物识别已解绑'**
  String get settingsBioUnboundDone;

  /// No description provided for @settingsCancel.
  ///
  /// In zh, this message translates to:
  /// **'取消'**
  String get settingsCancel;

  /// No description provided for @settingsChange.
  ///
  /// In zh, this message translates to:
  /// **'修改'**
  String get settingsChange;

  /// No description provided for @settingsChangePw.
  ///
  /// In zh, this message translates to:
  /// **'修改主密码'**
  String get settingsChangePw;

  /// No description provided for @settingsChangePwDialogSub.
  ///
  /// In zh, this message translates to:
  /// **'仅重新包装 MK · 数据零重加密'**
  String get settingsChangePwDialogSub;

  /// No description provided for @settingsChangePwDone.
  ///
  /// In zh, this message translates to:
  /// **'主密码已修改 · MK 重新包装完成，数据零重加密'**
  String get settingsChangePwDone;

  /// No description provided for @settingsChangePwNote.
  ///
  /// In zh, this message translates to:
  /// **'修改后重新生成 MK_wrap_pwd 并更换 salt；已配对设备与密文不受影响。'**
  String get settingsChangePwNote;

  /// No description provided for @settingsChangePwSub.
  ///
  /// In zh, this message translates to:
  /// **'毫秒级完成，不影响已加密文件与同步'**
  String get settingsChangePwSub;

  /// No description provided for @settingsConfirmChange.
  ///
  /// In zh, this message translates to:
  /// **'确认修改'**
  String get settingsConfirmChange;

  /// No description provided for @settingsConfirmPw.
  ///
  /// In zh, this message translates to:
  /// **'确认新主密码'**
  String get settingsConfirmPw;

  /// No description provided for @settingsCurrentPw.
  ///
  /// In zh, this message translates to:
  /// **'当前主密码'**
  String get settingsCurrentPw;

  /// No description provided for @settingsDecoyAllow.
  ///
  /// In zh, this message translates to:
  /// **'允许伪装密码'**
  String get settingsDecoyAllow;

  /// No description provided for @settingsDecoyAllowSub.
  ///
  /// In zh, this message translates to:
  /// **'锁屏输入伪装密码即进入伪空间，真实保险箱调用一律拒绝'**
  String get settingsDecoyAllowSub;

  /// No description provided for @settingsDecoySub.
  ///
  /// In zh, this message translates to:
  /// **'胁迫场景下输入伪装密码进入独立伪空间'**
  String get settingsDecoySub;

  /// No description provided for @settingsDecoyTitle.
  ///
  /// In zh, this message translates to:
  /// **'伪装入口'**
  String get settingsDecoyTitle;

  /// No description provided for @settingsDesignDocLabel.
  ///
  /// In zh, this message translates to:
  /// **'设计文档'**
  String get settingsDesignDocLabel;

  /// No description provided for @settingsEngineLabel.
  ///
  /// In zh, this message translates to:
  /// **'引擎'**
  String get settingsEngineLabel;

  /// No description provided for @settingsForgotPw.
  ///
  /// In zh, this message translates to:
  /// **'忘记主密码'**
  String get settingsForgotPw;

  /// No description provided for @settingsForgotPwSub.
  ///
  /// In zh, this message translates to:
  /// **'安全重置流程，需已配对设备授权（P5 接入）'**
  String get settingsForgotPwSub;

  /// No description provided for @settingsIdleTimeout.
  ///
  /// In zh, this message translates to:
  /// **'空闲超时'**
  String get settingsIdleTimeout;

  /// No description provided for @settingsIdleTimeoutSub.
  ///
  /// In zh, this message translates to:
  /// **'无操作后自动锁定（0 = 从不）'**
  String get settingsIdleTimeoutSub;

  /// No description provided for @settingsLanguage.
  ///
  /// In zh, this message translates to:
  /// **'语言'**
  String get settingsLanguage;

  /// No description provided for @settingsLanguageEn.
  ///
  /// In zh, this message translates to:
  /// **'English'**
  String get settingsLanguageEn;

  /// No description provided for @settingsLanguageSub.
  ///
  /// In zh, this message translates to:
  /// **'界面语言 · 切换后立即生效'**
  String get settingsLanguageSub;

  /// No description provided for @settingsLanguageSystem.
  ///
  /// In zh, this message translates to:
  /// **'跟随系统'**
  String get settingsLanguageSystem;

  /// No description provided for @settingsLanguageZh.
  ///
  /// In zh, this message translates to:
  /// **'中文'**
  String get settingsLanguageZh;

  /// No description provided for @settingsMinutes.
  ///
  /// In zh, this message translates to:
  /// **'{n} 分钟'**
  String settingsMinutes(int n);

  /// No description provided for @settingsNeedUnlock.
  ///
  /// In zh, this message translates to:
  /// **'请先解锁保险箱'**
  String get settingsNeedUnlock;

  /// No description provided for @settingsNever.
  ///
  /// In zh, this message translates to:
  /// **'从不'**
  String get settingsNever;

  /// No description provided for @settingsNewPw.
  ///
  /// In zh, this message translates to:
  /// **'新主密码'**
  String get settingsNewPw;

  /// No description provided for @settingsOpenDocsFailed.
  ///
  /// In zh, this message translates to:
  /// **'无法打开文档链接'**
  String get settingsOpenDocsFailed;

  /// No description provided for @settingsPwMismatch.
  ///
  /// In zh, this message translates to:
  /// **'两次输入的新密码不一致'**
  String get settingsPwMismatch;

  /// No description provided for @settingsPwSection.
  ///
  /// In zh, this message translates to:
  /// **'主密码管理'**
  String get settingsPwSection;

  /// No description provided for @settingsPwSectionSub.
  ///
  /// In zh, this message translates to:
  /// **'修改密码仅重新包装主密钥，数据零重加密'**
  String get settingsPwSectionSub;

  /// No description provided for @settingsStrengthHint.
  ///
  /// In zh, this message translates to:
  /// **'{label} · Argon2id 派生 KEK'**
  String settingsStrengthHint(String label);

  /// No description provided for @settingsSub.
  ///
  /// In zh, this message translates to:
  /// **'安全 · 落锁 · 同步 · 外观'**
  String get settingsSub;

  /// No description provided for @settingsSubmitting.
  ///
  /// In zh, this message translates to:
  /// **'提交中…'**
  String get settingsSubmitting;

  /// No description provided for @settingsSyncOff.
  ///
  /// In zh, this message translates to:
  /// **'自动同步已关闭'**
  String get settingsSyncOff;

  /// No description provided for @settingsSyncOn.
  ///
  /// In zh, this message translates to:
  /// **'自动同步已开启 · 关闭后需手动触发'**
  String get settingsSyncOn;

  /// No description provided for @settingsSyncTitle.
  ///
  /// In zh, this message translates to:
  /// **'同步策略'**
  String get settingsSyncTitle;

  /// No description provided for @settingsTheme.
  ///
  /// In zh, this message translates to:
  /// **'主题'**
  String get settingsTheme;

  /// No description provided for @settingsThemeDark.
  ///
  /// In zh, this message translates to:
  /// **'深色（默认）'**
  String get settingsThemeDark;

  /// No description provided for @settingsThemeLight.
  ///
  /// In zh, this message translates to:
  /// **'浅色'**
  String get settingsThemeLight;

  /// No description provided for @settingsThemeSub.
  ///
  /// In zh, this message translates to:
  /// **'深色为默认 · 对应原型 data-theme'**
  String get settingsThemeSub;

  /// No description provided for @settingsThemeSystem.
  ///
  /// In zh, this message translates to:
  /// **'跟随系统'**
  String get settingsThemeSystem;

  /// No description provided for @settingsTitle.
  ///
  /// In zh, this message translates to:
  /// **'设置'**
  String get settingsTitle;

  /// No description provided for @settingsVersionLabel.
  ///
  /// In zh, this message translates to:
  /// **'版本'**
  String get settingsVersionLabel;

  /// No description provided for @settingsVersionValue.
  ///
  /// In zh, this message translates to:
  /// **'VaultSync {version} · 端到端加密多设备同步'**
  String settingsVersionValue(String version);

  /// No description provided for @settingsViewDocs.
  ///
  /// In zh, this message translates to:
  /// **'查看在线文档'**
  String get settingsViewDocs;

  /// No description provided for @shellAllFiles.
  ///
  /// In zh, this message translates to:
  /// **'全部文件'**
  String get shellAllFiles;

  /// No description provided for @shellLockNow.
  ///
  /// In zh, this message translates to:
  /// **'立即锁定'**
  String get shellLockNow;

  /// No description provided for @shellMarkAllRead.
  ///
  /// In zh, this message translates to:
  /// **'全部已读'**
  String get shellMarkAllRead;

  /// No description provided for @shellNavSectionControl.
  ///
  /// In zh, this message translates to:
  /// **'管控'**
  String get shellNavSectionControl;

  /// No description provided for @shellNavSectionSpace.
  ///
  /// In zh, this message translates to:
  /// **'空间'**
  String get shellNavSectionSpace;

  /// No description provided for @shellNoNotifications.
  ///
  /// In zh, this message translates to:
  /// **'暂无通知'**
  String get shellNoNotifications;

  /// No description provided for @shellNotifications.
  ///
  /// In zh, this message translates to:
  /// **'通知'**
  String get shellNotifications;

  /// No description provided for @shellPlaceholderSub.
  ///
  /// In zh, this message translates to:
  /// **'功能开发中 —— 计划见 docs/09-开发任务面板.md'**
  String get shellPlaceholderSub;

  /// No description provided for @shellSearchHint.
  ///
  /// In zh, this message translates to:
  /// **'搜索文件、标签、全文…'**
  String get shellSearchHint;

  /// No description provided for @shellSubDevices.
  ///
  /// In zh, this message translates to:
  /// **'通过端到端加密信道与已配对设备同步'**
  String get shellSubDevices;

  /// No description provided for @shellSubSecurity.
  ///
  /// In zh, this message translates to:
  /// **'审计 · 检测 · 应急响应'**
  String get shellSubSecurity;

  /// No description provided for @shellSubSettings.
  ///
  /// In zh, this message translates to:
  /// **'安全 · 落锁 · 同步 · 外观'**
  String get shellSubSettings;

  /// No description provided for @shellSubStego.
  ///
  /// In zh, this message translates to:
  /// **'AES-256-GCM 密文嵌入图片 LSB'**
  String get shellSubStego;

  /// No description provided for @shellSubSync.
  ///
  /// In zh, this message translates to:
  /// **'增量同步 · 仅传输差异块'**
  String get shellSubSync;

  /// No description provided for @shellTitleDevices.
  ///
  /// In zh, this message translates to:
  /// **'设备'**
  String get shellTitleDevices;

  /// No description provided for @shellTitleSecurity.
  ///
  /// In zh, this message translates to:
  /// **'安全中心'**
  String get shellTitleSecurity;

  /// No description provided for @shellTitleSettings.
  ///
  /// In zh, this message translates to:
  /// **'设置'**
  String get shellTitleSettings;

  /// No description provided for @shellTitleStego.
  ///
  /// In zh, this message translates to:
  /// **'隐写术'**
  String get shellTitleStego;

  /// No description provided for @shellTitleSync.
  ///
  /// In zh, this message translates to:
  /// **'同步'**
  String get shellTitleSync;

  /// No description provided for @shellTitleVault.
  ///
  /// In zh, this message translates to:
  /// **'保险箱'**
  String get shellTitleVault;

  /// No description provided for @shellVersionTagline.
  ///
  /// In zh, this message translates to:
  /// **'V0.2 · 桌面端'**
  String get shellVersionTagline;

  /// No description provided for @stateBioFailed.
  ///
  /// In zh, this message translates to:
  /// **'生物识别解锁失败'**
  String get stateBioFailed;

  /// No description provided for @stateBioNotBound.
  ///
  /// In zh, this message translates to:
  /// **'尚未绑定生物识别（设置中绑定后可用）'**
  String get stateBioNotBound;

  /// No description provided for @stateBioUnsupported.
  ///
  /// In zh, this message translates to:
  /// **'此平台不支持生物识别，请使用主密码'**
  String get stateBioUnsupported;

  /// No description provided for @stateConflictDropped.
  ///
  /// In zh, this message translates to:
  /// **'已丢弃冲突副本'**
  String get stateConflictDropped;

  /// No description provided for @stateConflictKept.
  ///
  /// In zh, this message translates to:
  /// **'已保留副本并替换原条目 · {name}'**
  String stateConflictKept(Object name);

  /// No description provided for @stateConflictResolveFailed.
  ///
  /// In zh, this message translates to:
  /// **'冲突解决失败：{err}'**
  String stateConflictResolveFailed(Object err);

  /// No description provided for @stateCooldownHint.
  ///
  /// In zh, this message translates to:
  /// **'尝试过于频繁，请等待冷却结束'**
  String get stateCooldownHint;

  /// No description provided for @stateCooldownSeconds.
  ///
  /// In zh, this message translates to:
  /// **'尝试过于频繁，请等待 {n} 秒'**
  String stateCooldownSeconds(Object n);

  /// No description provided for @stateCreateFailed.
  ///
  /// In zh, this message translates to:
  /// **'创建保险箱失败：{e}'**
  String stateCreateFailed(Object e);

  /// No description provided for @stateDeviceLoadFailed.
  ///
  /// In zh, this message translates to:
  /// **'设备状态读取失败'**
  String get stateDeviceLoadFailed;

  /// No description provided for @stateDeviceNeedUnlock.
  ///
  /// In zh, this message translates to:
  /// **'请先解锁保险箱'**
  String get stateDeviceNeedUnlock;

  /// No description provided for @stateNeedUnlock.
  ///
  /// In zh, this message translates to:
  /// **'请先解锁保险箱'**
  String get stateNeedUnlock;

  /// No description provided for @statePwMin4.
  ///
  /// In zh, this message translates to:
  /// **'密码至少 4 位'**
  String get statePwMin4;

  /// No description provided for @statePwRequired.
  ///
  /// In zh, this message translates to:
  /// **'请输入主密码'**
  String get statePwRequired;

  /// No description provided for @statePwTooShort.
  ///
  /// In zh, this message translates to:
  /// **'主密码至少 {n} 位'**
  String statePwTooShort(Object n);

  /// No description provided for @statePwTooWeak.
  ///
  /// In zh, this message translates to:
  /// **'强度不足：建议混合大小写并加入数字或符号'**
  String get statePwTooWeak;

  /// No description provided for @statePwWrong.
  ///
  /// In zh, this message translates to:
  /// **'密码错误'**
  String get statePwWrong;

  /// No description provided for @stateStrengthFair.
  ///
  /// In zh, this message translates to:
  /// **'一般'**
  String get stateStrengthFair;

  /// No description provided for @stateStrengthGood.
  ///
  /// In zh, this message translates to:
  /// **'较强'**
  String get stateStrengthGood;

  /// No description provided for @stateStrengthStrong.
  ///
  /// In zh, this message translates to:
  /// **'强'**
  String get stateStrengthStrong;

  /// No description provided for @stateStrengthWeak.
  ///
  /// In zh, this message translates to:
  /// **'弱'**
  String get stateStrengthWeak;

  /// No description provided for @stateSyncDone.
  ///
  /// In zh, this message translates to:
  /// **'同步完成：拉取 {pulled}，推送 {pushed}'**
  String stateSyncDone(Object pulled, Object pushed);

  /// No description provided for @stateSyncDoneWithConflicts.
  ///
  /// In zh, this message translates to:
  /// **'同步完成：拉取 {pulled}，推送 {pushed}，冲突 {conflicts}'**
  String stateSyncDoneWithConflicts(
    Object conflicts,
    Object pulled,
    Object pushed,
  );

  /// No description provided for @stateSyncFailed.
  ///
  /// In zh, this message translates to:
  /// **'同步失败'**
  String get stateSyncFailed;

  /// No description provided for @stateSyncRelayFailed.
  ///
  /// In zh, this message translates to:
  /// **'中继同步失败'**
  String get stateSyncRelayFailed;

  /// No description provided for @stateUnknownDevice.
  ///
  /// In zh, this message translates to:
  /// **'未知设备'**
  String get stateUnknownDevice;

  /// No description provided for @stegoPageCancel.
  ///
  /// In zh, this message translates to:
  /// **'取消'**
  String get stegoPageCancel;

  /// No description provided for @stegoPageCapacityCard.
  ///
  /// In zh, this message translates to:
  /// **'容量参考'**
  String get stegoPageCapacityCard;

  /// No description provided for @stegoPageCapacityFormat.
  ///
  /// In zh, this message translates to:
  /// **'不支持该图片：隐写仅接受 8 位 RGB / RGBA 的 PNG（有损压缩会破坏 LSB）'**
  String get stegoPageCapacityFormat;

  /// No description provided for @stegoPageCapacityFormula.
  ///
  /// In zh, this message translates to:
  /// **'容量公式：宽 × 高 × 3 ÷ 8 − 4（4 字节为载荷长度前缀）'**
  String get stegoPageCapacityFormula;

  /// No description provided for @stegoPageCapacityIo.
  ///
  /// In zh, this message translates to:
  /// **'图片读取失败：文件不存在或不可读'**
  String get stegoPageCapacityIo;

  /// No description provided for @stegoPageCapacityLimit.
  ///
  /// In zh, this message translates to:
  /// **'单图容量有限：更大文件需分割嵌入多张图片（docs/05-05 §3.2）。'**
  String get stegoPageCapacityLimit;

  /// No description provided for @stegoPageCapacityLine.
  ///
  /// In zh, this message translates to:
  /// **'容量 {cap} / 需要 {need}'**
  String stegoPageCapacityLine(String cap, String need);

  /// No description provided for @stegoPageCapacityShort.
  ///
  /// In zh, this message translates to:
  /// **'容量不足：还差 {need}，请更换更大的图片（或分割嵌入多张图片）。'**
  String stegoPageCapacityShort(String need);

  /// No description provided for @stegoPageDisable.
  ///
  /// In zh, this message translates to:
  /// **'停用'**
  String get stegoPageDisable;

  /// No description provided for @stegoPageDisableDone.
  ///
  /// In zh, this message translates to:
  /// **'隐写引擎已停用'**
  String get stegoPageDisableDone;

  /// No description provided for @stegoPageDisabled.
  ///
  /// In zh, this message translates to:
  /// **'未启用'**
  String get stegoPageDisabled;

  /// No description provided for @stegoPageDisabledNote.
  ///
  /// In zh, this message translates to:
  /// **'隐写引擎默认关闭：启用后「嵌入 / 提取」才会被引擎接受；启用与停用都会写入审计链（docs/05-06 §七）。'**
  String get stegoPageDisabledNote;

  /// No description provided for @stegoPageEmbedAction.
  ///
  /// In zh, this message translates to:
  /// **'嵌入'**
  String get stegoPageEmbedAction;

  /// No description provided for @stegoPageEmbedDone.
  ///
  /// In zh, this message translates to:
  /// **'已嵌入 → {path}'**
  String stegoPageEmbedDone(String path);

  /// No description provided for @stegoPageEmbedNote.
  ///
  /// In zh, this message translates to:
  /// **'载荷 = AEAD(长度前缀 ‖ 文件名 ‖ 明文) 的密文：4 字节长度前缀与 AEAD 开销（12 字节 nonce + 16 字节 tag）同样占用容量。'**
  String get stegoPageEmbedNote;

  /// No description provided for @stegoPageEmbedSub.
  ///
  /// In zh, this message translates to:
  /// **'保险箱文件 → AES-256-GCM 密文 → 写入 PNG LSB'**
  String get stegoPageEmbedSub;

  /// No description provided for @stegoPageEmbedTitle.
  ///
  /// In zh, this message translates to:
  /// **'隐写导入（嵌入）'**
  String get stegoPageEmbedTitle;

  /// No description provided for @stegoPageEmbedding.
  ///
  /// In zh, this message translates to:
  /// **'嵌入中…'**
  String get stegoPageEmbedding;

  /// No description provided for @stegoPageEnable.
  ///
  /// In zh, this message translates to:
  /// **'启用隐写引擎'**
  String get stegoPageEnable;

  /// No description provided for @stegoPageEnableDone.
  ///
  /// In zh, this message translates to:
  /// **'隐写引擎已启用'**
  String get stegoPageEnableDone;

  /// No description provided for @stegoPageEnabled.
  ///
  /// In zh, this message translates to:
  /// **'已启用'**
  String get stegoPageEnabled;

  /// No description provided for @stegoPageEnabledNote.
  ///
  /// In zh, this message translates to:
  /// **'载荷是「先加密、再嵌入」：图片 LSB 里只有 AES-256-GCM 密文，密钥由本机 MK 经 HKDF 派生——提取必须持有同一保险箱的主密钥，因此这是本机隐蔽辅助，不是跨设备传输通道。'**
  String get stegoPageEnabledNote;

  /// No description provided for @stegoPageExtractAction.
  ///
  /// In zh, this message translates to:
  /// **'提取'**
  String get stegoPageExtractAction;

  /// No description provided for @stegoPageExtractDone.
  ///
  /// In zh, this message translates to:
  /// **'已还原 {name}（{size}）'**
  String stegoPageExtractDone(String name, String size);

  /// No description provided for @stegoPageExtractFailed.
  ///
  /// In zh, this message translates to:
  /// **'提取失败：图片中不含有效载荷、不是 8 位 RGB/RGBA PNG，或该载荷不属于当前保险箱（主密钥不匹配）。'**
  String get stegoPageExtractFailed;

  /// No description provided for @stegoPageExtractNote.
  ///
  /// In zh, this message translates to:
  /// **'提取只在同一保险箱内有效：载荷密钥派生自本机 MK，换库 / 换设备无法解密。'**
  String get stegoPageExtractNote;

  /// No description provided for @stegoPageExtractSub.
  ///
  /// In zh, this message translates to:
  /// **'PNG LSB → 密文帧 → AES-256-GCM 解密'**
  String get stegoPageExtractSub;

  /// No description provided for @stegoPageExtractTitle.
  ///
  /// In zh, this message translates to:
  /// **'隐写提取（取出）'**
  String get stegoPageExtractTitle;

  /// No description provided for @stegoPageExtracting.
  ///
  /// In zh, this message translates to:
  /// **'提取中…'**
  String get stegoPageExtracting;

  /// No description provided for @stegoPageFileEmpty.
  ///
  /// In zh, this message translates to:
  /// **'保险箱内没有可选文件（请先导入文件）'**
  String get stegoPageFileEmpty;

  /// No description provided for @stegoPageFileLoading.
  ///
  /// In zh, this message translates to:
  /// **'正在读取保险箱文件…'**
  String get stegoPageFileLoading;

  /// No description provided for @stegoPageFileOption.
  ///
  /// In zh, this message translates to:
  /// **'{name} · {size} · {folder}'**
  String stegoPageFileOption(String name, String size, String folder);

  /// No description provided for @stegoPageFileRoot.
  ///
  /// In zh, this message translates to:
  /// **'根目录'**
  String get stegoPageFileRoot;

  /// No description provided for @stegoPageNeedUnlock.
  ///
  /// In zh, this message translates to:
  /// **'会话未解锁，无法执行隐写操作'**
  String get stegoPageNeedUnlock;

  /// No description provided for @stegoPageNotChosen.
  ///
  /// In zh, this message translates to:
  /// **'未选择'**
  String get stegoPageNotChosen;

  /// No description provided for @stegoPagePickFile.
  ///
  /// In zh, this message translates to:
  /// **'保险箱文件'**
  String get stegoPagePickFile;

  /// No description provided for @stegoPagePickFileHint.
  ///
  /// In zh, this message translates to:
  /// **'选择要嵌入的文件'**
  String get stegoPagePickFileHint;

  /// No description provided for @stegoPagePickImage.
  ///
  /// In zh, this message translates to:
  /// **'选择 PNG 图片'**
  String get stegoPagePickImage;

  /// No description provided for @stegoPagePickOutput.
  ///
  /// In zh, this message translates to:
  /// **'选择输出位置'**
  String get stegoPagePickOutput;

  /// No description provided for @stegoPagePickRestore.
  ///
  /// In zh, this message translates to:
  /// **'选择还原位置'**
  String get stegoPagePickRestore;

  /// No description provided for @stegoPagePickStegoImage.
  ///
  /// In zh, this message translates to:
  /// **'选择携带数据的 PNG'**
  String get stegoPagePickStegoImage;

  /// No description provided for @stegoPagePlainSize.
  ///
  /// In zh, this message translates to:
  /// **'明文体量 ≈ {size}'**
  String stegoPagePlainSize(String size);

  /// No description provided for @stegoPageSafetyNote.
  ///
  /// In zh, this message translates to:
  /// **'隐写是隐蔽辅助，不替代主保险箱；有损压缩（JPG）会破坏 LSB，请使用 PNG。'**
  String get stegoPageSafetyNote;

  /// No description provided for @stegoPageSizeB.
  ///
  /// In zh, this message translates to:
  /// **'{n} B'**
  String stegoPageSizeB(int n);

  /// No description provided for @stegoPageSizeBytes.
  ///
  /// In zh, this message translates to:
  /// **'{bytes} 字节 ≈ {size}'**
  String stegoPageSizeBytes(int bytes, String size);

  /// No description provided for @stegoPageSizeKb.
  ///
  /// In zh, this message translates to:
  /// **'{n} KB'**
  String stegoPageSizeKb(String n);

  /// No description provided for @stegoPageSizeMb.
  ///
  /// In zh, this message translates to:
  /// **'{n} MB'**
  String stegoPageSizeMb(String n);

  /// No description provided for @stegoPageSub.
  ///
  /// In zh, this message translates to:
  /// **'加密之上的隐蔽 · AES-256-GCM 密文嵌入图片 LSB'**
  String get stegoPageSub;

  /// No description provided for @stegoPageTileEmbed.
  ///
  /// In zh, this message translates to:
  /// **'隐写导入（嵌入）'**
  String get stegoPageTileEmbed;

  /// No description provided for @stegoPageTileEmbedSub.
  ///
  /// In zh, this message translates to:
  /// **'将保险箱中已加密的文件嵌入普通图片，图片外观无可见变化。'**
  String get stegoPageTileEmbedSub;

  /// No description provided for @stegoPageTileExtract.
  ///
  /// In zh, this message translates to:
  /// **'隐写提取（取出）'**
  String get stegoPageTileExtract;

  /// No description provided for @stegoPageTileExtractSub.
  ///
  /// In zh, this message translates to:
  /// **'从携带隐藏数据的图片中提取密文帧，解密还原原始文件。'**
  String get stegoPageTileExtractSub;

  /// No description provided for @stegoPageTileLocked.
  ///
  /// In zh, this message translates to:
  /// **'需先启用隐写引擎'**
  String get stegoPageTileLocked;

  /// No description provided for @stegoPageTitle.
  ///
  /// In zh, this message translates to:
  /// **'隐写术引擎'**
  String get stegoPageTitle;

  /// No description provided for @stegoPageToggleFailed.
  ///
  /// In zh, this message translates to:
  /// **'操作失败：{err}'**
  String stegoPageToggleFailed(String err);

  /// No description provided for @syncPageAddrHint.
  ///
  /// In zh, this message translates to:
  /// **'host:port，如 192.168.1.10:9100'**
  String get syncPageAddrHint;

  /// No description provided for @syncPageCancel.
  ///
  /// In zh, this message translates to:
  /// **'取消'**
  String get syncPageCancel;

  /// No description provided for @syncPageChunkDetails.
  ///
  /// In zh, this message translates to:
  /// **'分块详情'**
  String get syncPageChunkDetails;

  /// No description provided for @syncPageChunkDetailsNote.
  ///
  /// In zh, this message translates to:
  /// **'逐块传输进度矩阵（加密块 = 同步块 = 传输块）将在 P5 接入真实数据。'**
  String get syncPageChunkDetailsNote;

  /// No description provided for @syncPageConflictBanner.
  ///
  /// In zh, this message translates to:
  /// **'发现 {n} 个冲突文件，冲突副本以 .conflict-<时间戳> 命名保留，可在保险箱列表中处理。'**
  String syncPageConflictBanner(int n);

  /// No description provided for @syncPageConflictCopyId.
  ///
  /// In zh, this message translates to:
  /// **'冲突副本 #{id}'**
  String syncPageConflictCopyId(int id);

  /// No description provided for @syncPageConflictNote.
  ///
  /// In zh, this message translates to:
  /// **'冲突副本由引擎在两侧版本分叉时自动留存（本地版本另存为 <原名>.conflict-<时间戳>）。「保留副本」会把副本改回原名并替换对端版本，「丢弃副本」则只删副本——两者都会记入审计。'**
  String get syncPageConflictNote;

  /// No description provided for @syncPageConflictPeerGone.
  ///
  /// In zh, this message translates to:
  /// **'副本留存于 {stamp} · 对端版本已不存在'**
  String syncPageConflictPeerGone(String stamp);

  /// No description provided for @syncPageConflictSection.
  ///
  /// In zh, this message translates to:
  /// **'冲突处理'**
  String get syncPageConflictSection;

  /// No description provided for @syncPageConflictStoredAt.
  ///
  /// In zh, this message translates to:
  /// **'副本留存于 {stamp}'**
  String syncPageConflictStoredAt(String stamp);

  /// No description provided for @syncPageDialogNote.
  ///
  /// In zh, this message translates to:
  /// **'同步为块级增量：仅传输双方差异块，已到块经强哈希校验后复用。'**
  String get syncPageDialogNote;

  /// No description provided for @syncPageDialogSub.
  ///
  /// In zh, this message translates to:
  /// **'直连或经自建中继 · Noise 加密信道'**
  String get syncPageDialogSub;

  /// No description provided for @syncPageDirect.
  ///
  /// In zh, this message translates to:
  /// **'直连'**
  String get syncPageDirect;

  /// No description provided for @syncPageDropCopy.
  ///
  /// In zh, this message translates to:
  /// **'丢弃副本'**
  String get syncPageDropCopy;

  /// No description provided for @syncPageEngineNote.
  ///
  /// In zh, this message translates to:
  /// **'引擎暂无传输队列与逐块进度回调：本页的统计、事件与冲突均为真实数据，逐块矩阵待引擎提供回调后接入（P5）。'**
  String get syncPageEngineNote;

  /// No description provided for @syncPageEvents.
  ///
  /// In zh, this message translates to:
  /// **'同步事件'**
  String get syncPageEvents;

  /// No description provided for @syncPageEventsRecent.
  ///
  /// In zh, this message translates to:
  /// **'最近 {n} 条'**
  String syncPageEventsRecent(int n);

  /// No description provided for @syncPageHeaderSub.
  ///
  /// In zh, this message translates to:
  /// **'增量同步 · 仅传输差异块（CDC）'**
  String get syncPageHeaderSub;

  /// No description provided for @syncPageKeepCopy.
  ///
  /// In zh, this message translates to:
  /// **'保留副本'**
  String get syncPageKeepCopy;

  /// No description provided for @syncPageLatestSummary.
  ///
  /// In zh, this message translates to:
  /// **'最新同步摘要'**
  String get syncPageLatestSummary;

  /// No description provided for @syncPageLive.
  ///
  /// In zh, this message translates to:
  /// **'实时'**
  String get syncPageLive;

  /// No description provided for @syncPageNeverSynced.
  ///
  /// In zh, this message translates to:
  /// **'尚未同步'**
  String get syncPageNeverSynced;

  /// No description provided for @syncPageNoConflicts.
  ///
  /// In zh, this message translates to:
  /// **'没有冲突副本 · 同步结果一致'**
  String get syncPageNoConflicts;

  /// No description provided for @syncPageNoEvents.
  ///
  /// In zh, this message translates to:
  /// **'暂无事件 · 完成「立即同步」后在此记录'**
  String get syncPageNoEvents;

  /// No description provided for @syncPagePeerAddr.
  ///
  /// In zh, this message translates to:
  /// **'对端地址'**
  String get syncPagePeerAddr;

  /// No description provided for @syncPagePendingCount.
  ///
  /// In zh, this message translates to:
  /// **'{n} 项待解决'**
  String syncPagePendingCount(int n);

  /// No description provided for @syncPageRefresh.
  ///
  /// In zh, this message translates to:
  /// **'刷新'**
  String get syncPageRefresh;

  /// No description provided for @syncPageRelay.
  ///
  /// In zh, this message translates to:
  /// **'中继'**
  String get syncPageRelay;

  /// No description provided for @syncPageRelayAddr.
  ///
  /// In zh, this message translates to:
  /// **'中继地址'**
  String get syncPageRelayAddr;

  /// No description provided for @syncPageRoomHint.
  ///
  /// In zh, this message translates to:
  /// **'共享房间 Room ID'**
  String get syncPageRoomHint;

  /// No description provided for @syncPageRoomId.
  ///
  /// In zh, this message translates to:
  /// **'房间号'**
  String get syncPageRoomId;

  /// No description provided for @syncPageScanning.
  ///
  /// In zh, this message translates to:
  /// **'扫描中…'**
  String get syncPageScanning;

  /// No description provided for @syncPageSelfPeers.
  ///
  /// In zh, this message translates to:
  /// **'本机 {name} · {peers} 台已配对'**
  String syncPageSelfPeers(String name, int peers);

  /// No description provided for @syncPageStartSync.
  ///
  /// In zh, this message translates to:
  /// **'开始同步'**
  String get syncPageStartSync;

  /// No description provided for @syncPageStatConflicts.
  ///
  /// In zh, this message translates to:
  /// **'冲突待解决'**
  String get syncPageStatConflicts;

  /// No description provided for @syncPageStatDeleted.
  ///
  /// In zh, this message translates to:
  /// **'已删除项'**
  String get syncPageStatDeleted;

  /// No description provided for @syncPageStatPulledFiles.
  ///
  /// In zh, this message translates to:
  /// **'拉取的文件'**
  String get syncPageStatPulledFiles;

  /// No description provided for @syncPageStatPushedFiles.
  ///
  /// In zh, this message translates to:
  /// **'推送的文件'**
  String get syncPageStatPushedFiles;

  /// No description provided for @syncPageStatus.
  ///
  /// In zh, this message translates to:
  /// **'同步状态'**
  String get syncPageStatus;

  /// No description provided for @syncPageStatusIdle.
  ///
  /// In zh, this message translates to:
  /// **'空闲 · 无进行中的传输'**
  String get syncPageStatusIdle;

  /// No description provided for @syncPageStatusRunning.
  ///
  /// In zh, this message translates to:
  /// **'正在同步 · 块级增量传输中（阻塞式单次会话）'**
  String get syncPageStatusRunning;

  /// No description provided for @syncPageSummaryConflicts.
  ///
  /// In zh, this message translates to:
  /// **'冲突 {n}'**
  String syncPageSummaryConflicts(int n);

  /// No description provided for @syncPageSummaryDeleted.
  ///
  /// In zh, this message translates to:
  /// **'删除 {n}'**
  String syncPageSummaryDeleted(int n);

  /// No description provided for @syncPageSummaryLine.
  ///
  /// In zh, this message translates to:
  /// **'拉取 {pulled} · 推送 {pushed} · 删除 {deleted} · 冲突 {conflicts}'**
  String syncPageSummaryLine(
    int pulled,
    int pushed,
    int deleted,
    int conflicts,
  );

  /// No description provided for @syncPageSummaryMerged.
  ///
  /// In zh, this message translates to:
  /// **'合并 {n}'**
  String syncPageSummaryMerged(int n);

  /// No description provided for @syncPageSummaryPulled.
  ///
  /// In zh, this message translates to:
  /// **'拉取 {n}'**
  String syncPageSummaryPulled(int n);

  /// No description provided for @syncPageSummaryPushed.
  ///
  /// In zh, this message translates to:
  /// **'推送 {n}'**
  String syncPageSummaryPushed(int n);

  /// No description provided for @syncPageSyncNow.
  ///
  /// In zh, this message translates to:
  /// **'立即同步'**
  String get syncPageSyncNow;

  /// No description provided for @syncPageSyncing.
  ///
  /// In zh, this message translates to:
  /// **'同步中…'**
  String get syncPageSyncing;

  /// No description provided for @syncPageTitle.
  ///
  /// In zh, this message translates to:
  /// **'同步'**
  String get syncPageTitle;

  /// No description provided for @vaultPageAlgoLabel.
  ///
  /// In zh, this message translates to:
  /// **'算法'**
  String get vaultPageAlgoLabel;

  /// No description provided for @vaultPageAlgoValue.
  ///
  /// In zh, this message translates to:
  /// **'AES-256-GCM · FastCDC 64KB'**
  String get vaultPageAlgoValue;

  /// No description provided for @vaultPageBackToFolder.
  ///
  /// In zh, this message translates to:
  /// **'返回当前文件夹'**
  String get vaultPageBackToFolder;

  /// No description provided for @vaultPageBurn.
  ///
  /// In zh, this message translates to:
  /// **'阅后即焚'**
  String get vaultPageBurn;

  /// No description provided for @vaultPageCancel.
  ///
  /// In zh, this message translates to:
  /// **'取消'**
  String get vaultPageCancel;

  /// No description provided for @vaultPageCipherThumb.
  ///
  /// In zh, this message translates to:
  /// **'密文缩略图'**
  String get vaultPageCipherThumb;

  /// No description provided for @vaultPageClose.
  ///
  /// In zh, this message translates to:
  /// **'关闭'**
  String get vaultPageClose;

  /// No description provided for @vaultPageConflict.
  ///
  /// In zh, this message translates to:
  /// **'冲突'**
  String get vaultPageConflict;

  /// No description provided for @vaultPageCreate.
  ///
  /// In zh, this message translates to:
  /// **'创建'**
  String get vaultPageCreate;

  /// No description provided for @vaultPageCreatedFolder.
  ///
  /// In zh, this message translates to:
  /// **'已创建「{name}」· 独立派生子密钥 FSK'**
  String vaultPageCreatedFolder(String name);

  /// No description provided for @vaultPageDetailSub.
  ///
  /// In zh, this message translates to:
  /// **'{size} · {kind}'**
  String vaultPageDetailSub(String size, String kind);

  /// No description provided for @vaultPageDetailThumb.
  ///
  /// In zh, this message translates to:
  /// **'加密缩略图 · 解锁后解密预览'**
  String get vaultPageDetailThumb;

  /// No description provided for @vaultPageDone.
  ///
  /// In zh, this message translates to:
  /// **'完成'**
  String get vaultPageDone;

  /// No description provided for @vaultPageEmptyFolderSub.
  ///
  /// In zh, this message translates to:
  /// **'将文件拖入此处，或点击下方按钮导入（自动分块加密）。'**
  String get vaultPageEmptyFolderSub;

  /// No description provided for @vaultPageEmptyFolderTitle.
  ///
  /// In zh, this message translates to:
  /// **'此文件夹为空'**
  String get vaultPageEmptyFolderTitle;

  /// No description provided for @vaultPageEmptySearchSub.
  ///
  /// In zh, this message translates to:
  /// **'换个关键词，或返回文件夹后重新搜索。'**
  String get vaultPageEmptySearchSub;

  /// No description provided for @vaultPageEmptySearchTitle.
  ///
  /// In zh, this message translates to:
  /// **'未找到匹配内容'**
  String get vaultPageEmptySearchTitle;

  /// No description provided for @vaultPageEmptyVaultSub.
  ///
  /// In zh, this message translates to:
  /// **'导入的文件将自动分块加密，密钥仅存于本机。'**
  String get vaultPageEmptyVaultSub;

  /// No description provided for @vaultPageEmptyVaultTitle.
  ///
  /// In zh, this message translates to:
  /// **'此保险箱为空'**
  String get vaultPageEmptyVaultTitle;

  /// No description provided for @vaultPageEncInfo.
  ///
  /// In zh, this message translates to:
  /// **'加密信息'**
  String get vaultPageEncInfo;

  /// No description provided for @vaultPageExport.
  ///
  /// In zh, this message translates to:
  /// **'导出'**
  String get vaultPageExport;

  /// No description provided for @vaultPageExportCount.
  ///
  /// In zh, this message translates to:
  /// **'导出 {n} 个文件'**
  String vaultPageExportCount(int n);

  /// No description provided for @vaultPageExportFailedItem.
  ///
  /// In zh, this message translates to:
  /// **'导出失败 {name}：{error}'**
  String vaultPageExportFailedItem(String name, String error);

  /// No description provided for @vaultPageExportNoneSelected.
  ///
  /// In zh, this message translates to:
  /// **'所选项目中没有可导出的文件'**
  String get vaultPageExportNoneSelected;

  /// No description provided for @vaultPageExportToHere.
  ///
  /// In zh, this message translates to:
  /// **'导出到此处'**
  String get vaultPageExportToHere;

  /// No description provided for @vaultPageExportedMany.
  ///
  /// In zh, this message translates to:
  /// **'已导出 {done}/{total} 个文件到 {dir}'**
  String vaultPageExportedMany(int done, int total, String dir);

  /// No description provided for @vaultPageExportedTo.
  ///
  /// In zh, this message translates to:
  /// **'已导出到 {path} · SHA-256 终验通过'**
  String vaultPageExportedTo(String path);

  /// No description provided for @vaultPageFileLabel.
  ///
  /// In zh, this message translates to:
  /// **'文件'**
  String get vaultPageFileLabel;

  /// No description provided for @vaultPageFolder.
  ///
  /// In zh, this message translates to:
  /// **'文件夹'**
  String get vaultPageFolder;

  /// No description provided for @vaultPageFsKeyLabel.
  ///
  /// In zh, this message translates to:
  /// **'文件密钥'**
  String get vaultPageFsKeyLabel;

  /// No description provided for @vaultPageFsKeyValue.
  ///
  /// In zh, this message translates to:
  /// **'FSKey · 随机生成'**
  String get vaultPageFsKeyValue;

  /// No description provided for @vaultPageImport.
  ///
  /// In zh, this message translates to:
  /// **'导入文件'**
  String get vaultPageImport;

  /// No description provided for @vaultPageImportFailed.
  ///
  /// In zh, this message translates to:
  /// **'导入失败 {name}'**
  String vaultPageImportFailed(String name);

  /// No description provided for @vaultPageImportFileError.
  ///
  /// In zh, this message translates to:
  /// **'{name}：{error}'**
  String vaultPageImportFileError(String name, String error);

  /// No description provided for @vaultPageImportFolderUnsupported.
  ///
  /// In zh, this message translates to:
  /// **'{name}（文件夹暂不支持导入）'**
  String vaultPageImportFolderUnsupported(String name);

  /// No description provided for @vaultPageImportedCount.
  ///
  /// In zh, this message translates to:
  /// **'已导入 {n} 个文件 · CDC 分块加密入箱'**
  String vaultPageImportedCount(int n);

  /// No description provided for @vaultPageKindArchive.
  ///
  /// In zh, this message translates to:
  /// **'压缩包'**
  String get vaultPageKindArchive;

  /// No description provided for @vaultPageKindDoc.
  ///
  /// In zh, this message translates to:
  /// **'文档'**
  String get vaultPageKindDoc;

  /// No description provided for @vaultPageKindFile.
  ///
  /// In zh, this message translates to:
  /// **'文件'**
  String get vaultPageKindFile;

  /// No description provided for @vaultPageKindFolder.
  ///
  /// In zh, this message translates to:
  /// **'加密文件夹'**
  String get vaultPageKindFolder;

  /// No description provided for @vaultPageKindImage.
  ///
  /// In zh, this message translates to:
  /// **'图片'**
  String get vaultPageKindImage;

  /// No description provided for @vaultPageKindVideo.
  ///
  /// In zh, this message translates to:
  /// **'视频'**
  String get vaultPageKindVideo;

  /// No description provided for @vaultPageMkdirHint.
  ///
  /// In zh, this message translates to:
  /// **'例如：合同归档'**
  String get vaultPageMkdirHint;

  /// No description provided for @vaultPageMkdirSub.
  ///
  /// In zh, this message translates to:
  /// **'文件夹独立派生子密钥 FSK'**
  String get vaultPageMkdirSub;

  /// No description provided for @vaultPageMkdirTitle.
  ///
  /// In zh, this message translates to:
  /// **'新建加密文件夹'**
  String get vaultPageMkdirTitle;

  /// No description provided for @vaultPageNameLabel.
  ///
  /// In zh, this message translates to:
  /// **'名称'**
  String get vaultPageNameLabel;

  /// No description provided for @vaultPageNewFolder.
  ///
  /// In zh, this message translates to:
  /// **'新建文件夹'**
  String get vaultPageNewFolder;

  /// No description provided for @vaultPageNoFiles.
  ///
  /// In zh, this message translates to:
  /// **'无文件'**
  String get vaultPageNoFiles;

  /// No description provided for @vaultPageNoFilesEmptySub.
  ///
  /// In zh, this message translates to:
  /// **'此文件夹为空 —— 将文件拖入此处，或点击下方「导入文件」。'**
  String get vaultPageNoFilesEmptySub;

  /// No description provided for @vaultPageNoFilesSub.
  ///
  /// In zh, this message translates to:
  /// **'此层级没有文件，可从左侧进入子文件夹，或点击下方「导入文件」。'**
  String get vaultPageNoFilesSub;

  /// No description provided for @vaultPageNoSubfolders.
  ///
  /// In zh, this message translates to:
  /// **'无子文件夹'**
  String get vaultPageNoSubfolders;

  /// No description provided for @vaultPageNoTags.
  ///
  /// In zh, this message translates to:
  /// **'暂无标签'**
  String get vaultPageNoTags;

  /// No description provided for @vaultPageOpen.
  ///
  /// In zh, this message translates to:
  /// **'打开'**
  String get vaultPageOpen;

  /// No description provided for @vaultPagePreview.
  ///
  /// In zh, this message translates to:
  /// **'预览'**
  String get vaultPagePreview;

  /// No description provided for @vaultPagePreviewNote.
  ///
  /// In zh, this message translates to:
  /// **'引擎未开放明文预览通道（明文不出引擎层）；查看内容请先「导出」。'**
  String get vaultPagePreviewNote;

  /// No description provided for @vaultPagePreviewSub.
  ///
  /// In zh, this message translates to:
  /// **'{size} · 解锁会话内解密'**
  String vaultPagePreviewSub(String size);

  /// No description provided for @vaultPagePreviewTitle.
  ///
  /// In zh, this message translates to:
  /// **'预览 · {name}'**
  String vaultPagePreviewTitle(String name);

  /// No description provided for @vaultPageRename.
  ///
  /// In zh, this message translates to:
  /// **'重命名'**
  String get vaultPageRename;

  /// No description provided for @vaultPageRenameFileSub.
  ///
  /// In zh, this message translates to:
  /// **'修改加密元数据中的文件名'**
  String get vaultPageRenameFileSub;

  /// No description provided for @vaultPageRenameFolderSub.
  ///
  /// In zh, this message translates to:
  /// **'修改加密元数据中的文件夹名'**
  String get vaultPageRenameFolderSub;

  /// No description provided for @vaultPageRenamed.
  ///
  /// In zh, this message translates to:
  /// **'已重命名 · 加密元数据已更新'**
  String get vaultPageRenamed;

  /// No description provided for @vaultPageSave.
  ///
  /// In zh, this message translates to:
  /// **'保存'**
  String get vaultPageSave;

  /// No description provided for @vaultPageSearching.
  ///
  /// In zh, this message translates to:
  /// **'搜索「{query}」'**
  String vaultPageSearching(String query);

  /// No description provided for @vaultPageSelectedCount.
  ///
  /// In zh, this message translates to:
  /// **'已选 {n} 项'**
  String vaultPageSelectedCount(int n);

  /// No description provided for @vaultPageShare.
  ///
  /// In zh, this message translates to:
  /// **'阅后即焚分享'**
  String get vaultPageShare;

  /// No description provided for @vaultPageShareFailed.
  ///
  /// In zh, this message translates to:
  /// **'分享创建失败'**
  String get vaultPageShareFailed;

  /// No description provided for @vaultPageShareIdAndToken.
  ///
  /// In zh, this message translates to:
  /// **'ID：{id}\n令牌：{token}'**
  String vaultPageShareIdAndToken(String id, String token);

  /// No description provided for @vaultPageShareNote.
  ///
  /// In zh, this message translates to:
  /// **'对方凭令牌打开后分享即失效，令牌不可复用。'**
  String get vaultPageShareNote;

  /// No description provided for @vaultPageShareSub.
  ///
  /// In zh, this message translates to:
  /// **'一次性 P2P 会话 · 对端打开后限时自动擦除'**
  String get vaultPageShareSub;

  /// No description provided for @vaultPageShareTokenLabel.
  ///
  /// In zh, this message translates to:
  /// **'一次性分享令牌（仅显示这一次）'**
  String get vaultPageShareTokenLabel;

  /// No description provided for @vaultPageSizeLabel.
  ///
  /// In zh, this message translates to:
  /// **'大小'**
  String get vaultPageSizeLabel;

  /// No description provided for @vaultPageStatusLabel.
  ///
  /// In zh, this message translates to:
  /// **'状态'**
  String get vaultPageStatusLabel;

  /// No description provided for @vaultPageSynced.
  ///
  /// In zh, this message translates to:
  /// **'已同步'**
  String get vaultPageSynced;

  /// No description provided for @vaultPageTags.
  ///
  /// In zh, this message translates to:
  /// **'标签'**
  String get vaultPageTags;

  /// No description provided for @vaultPageTagsHint.
  ///
  /// In zh, this message translates to:
  /// **'逗号分隔，如: 工作,私密'**
  String get vaultPageTagsHint;

  /// No description provided for @vaultPageTagsSub.
  ///
  /// In zh, this message translates to:
  /// **'管理「{name}」的标签'**
  String vaultPageTagsSub(String name);

  /// No description provided for @vaultPageTagsTitle.
  ///
  /// In zh, this message translates to:
  /// **'标签管理'**
  String get vaultPageTagsTitle;

  /// No description provided for @vaultPageTagsUpdated.
  ///
  /// In zh, this message translates to:
  /// **'标签已更新'**
  String get vaultPageTagsUpdated;

  /// No description provided for @vaultPageTargetFile.
  ///
  /// In zh, this message translates to:
  /// **'目标文件'**
  String get vaultPageTargetFile;

  /// No description provided for @vaultPageTargetFolder.
  ///
  /// In zh, this message translates to:
  /// **'目标文件夹'**
  String get vaultPageTargetFolder;

  /// No description provided for @vaultPageTreeAllFiles.
  ///
  /// In zh, this message translates to:
  /// **'全部文件'**
  String get vaultPageTreeAllFiles;

  /// No description provided for @vaultPageTreeCurrentPath.
  ///
  /// In zh, this message translates to:
  /// **'当前路径'**
  String get vaultPageTreeCurrentPath;

  /// No description provided for @vaultPageTreeQuickAccess.
  ///
  /// In zh, this message translates to:
  /// **'快速访问'**
  String get vaultPageTreeQuickAccess;

  /// No description provided for @vaultPageTypeLabel.
  ///
  /// In zh, this message translates to:
  /// **'类型'**
  String get vaultPageTypeLabel;

  /// No description provided for @vaultPageUnnamed.
  ///
  /// In zh, this message translates to:
  /// **'未命名'**
  String get vaultPageUnnamed;

  /// No description provided for @vaultPageUnnamedFile.
  ///
  /// In zh, this message translates to:
  /// **'未命名文件'**
  String get vaultPageUnnamedFile;

  /// No description provided for @vaultPageUnnamedFolder.
  ///
  /// In zh, this message translates to:
  /// **'未命名文件夹'**
  String get vaultPageUnnamedFolder;

  /// No description provided for @vaultPageUnnamedWithId.
  ///
  /// In zh, this message translates to:
  /// **'未命名-{id}'**
  String vaultPageUnnamedWithId(int id);

  /// No description provided for @vaultPageViewGrid.
  ///
  /// In zh, this message translates to:
  /// **'网格'**
  String get vaultPageViewGrid;

  /// No description provided for @vaultPageViewList.
  ///
  /// In zh, this message translates to:
  /// **'列表'**
  String get vaultPageViewList;

  /// No description provided for @vaultPageViewSplit.
  ///
  /// In zh, this message translates to:
  /// **'分栏'**
  String get vaultPageViewSplit;

  /// No description provided for @vaultPageWipe.
  ///
  /// In zh, this message translates to:
  /// **'安全擦除'**
  String get vaultPageWipe;

  /// No description provided for @vaultPageWipeAll.
  ///
  /// In zh, this message translates to:
  /// **'全部擦除'**
  String get vaultPageWipeAll;

  /// No description provided for @vaultPageWipeConfirm.
  ///
  /// In zh, this message translates to:
  /// **'确认擦除'**
  String get vaultPageWipeConfirm;

  /// No description provided for @vaultPageWipeCount.
  ///
  /// In zh, this message translates to:
  /// **'安全擦除 {n} 个文件'**
  String vaultPageWipeCount(int n);

  /// No description provided for @vaultPageWipeFileSub.
  ///
  /// In zh, this message translates to:
  /// **'销毁文件密钥 FSKey 并摘除块清单'**
  String get vaultPageWipeFileSub;

  /// No description provided for @vaultPageWipeFolderFailed.
  ///
  /// In zh, this message translates to:
  /// **'「{name}」{error}（已销毁 {count} 个文件）'**
  String vaultPageWipeFolderFailed(String name, String error, int count);

  /// No description provided for @vaultPageWipeFolderSub.
  ///
  /// In zh, this message translates to:
  /// **'递归销毁文件夹及其内文件的密钥与密文'**
  String get vaultPageWipeFolderSub;

  /// No description provided for @vaultPageWipeManyDone.
  ///
  /// In zh, this message translates to:
  /// **'已批量安全擦除 {n} 个项目'**
  String vaultPageWipeManyDone(int n);

  /// No description provided for @vaultPageWipeManyFailed.
  ///
  /// In zh, this message translates to:
  /// **'批量擦除完成：{failed}/{n} 项失败'**
  String vaultPageWipeManyFailed(int failed, int n);

  /// No description provided for @vaultPageWipeManyFilesSub.
  ///
  /// In zh, this message translates to:
  /// **'销毁 {n} 个文件的密钥并删除密文'**
  String vaultPageWipeManyFilesSub(int n);

  /// No description provided for @vaultPageWipeManyFoldersSub.
  ///
  /// In zh, this message translates to:
  /// **'销毁密钥并删除密文（含 {folders} 个文件夹，递归）'**
  String vaultPageWipeManyFoldersSub(int folders);

  /// No description provided for @vaultPageWipeManyNote.
  ///
  /// In zh, this message translates to:
  /// **'将批量擦除 {n} 个项目，不可恢复，且同步传播到已配对设备。'**
  String vaultPageWipeManyNote(int n);

  /// No description provided for @vaultPageWipeManyTitle.
  ///
  /// In zh, this message translates to:
  /// **'批量安全擦除'**
  String get vaultPageWipeManyTitle;

  /// No description provided for @vaultPageWipeNote.
  ///
  /// In zh, this message translates to:
  /// **'擦除将同步传播到所有已配对设备，操作会记入审计日志。'**
  String get vaultPageWipeNote;

  /// No description provided for @vaultPageWiped.
  ///
  /// In zh, this message translates to:
  /// **'已安全擦除'**
  String get vaultPageWiped;

  /// No description provided for @vsStepsDefaultLabel.
  ///
  /// In zh, this message translates to:
  /// **'步骤 {n}'**
  String vsStepsDefaultLabel(Object n);

  /// No description provided for @windowChromeClose.
  ///
  /// In zh, this message translates to:
  /// **'关闭'**
  String get windowChromeClose;

  /// No description provided for @windowChromeMaximize.
  ///
  /// In zh, this message translates to:
  /// **'最大化'**
  String get windowChromeMaximize;

  /// No description provided for @windowChromeMinimize.
  ///
  /// In zh, this message translates to:
  /// **'最小化'**
  String get windowChromeMinimize;

  /// No description provided for @windowChromeRestore.
  ///
  /// In zh, this message translates to:
  /// **'向下还原'**
  String get windowChromeRestore;
}

class _AppLocalizationsDelegate
    extends LocalizationsDelegate<AppLocalizations> {
  const _AppLocalizationsDelegate();

  @override
  Future<AppLocalizations> load(Locale locale) {
    return SynchronousFuture<AppLocalizations>(lookupAppLocalizations(locale));
  }

  @override
  bool isSupported(Locale locale) =>
      <String>['en', 'zh'].contains(locale.languageCode);

  @override
  bool shouldReload(_AppLocalizationsDelegate old) => false;
}

AppLocalizations lookupAppLocalizations(Locale locale) {
  // Lookup logic when only language code is specified.
  switch (locale.languageCode) {
    case 'en':
      return AppLocalizationsEn();
    case 'zh':
      return AppLocalizationsZh();
  }

  throw FlutterError(
    'AppLocalizations.delegate failed to load unsupported locale "$locale". This is likely '
    'an issue with the localizations generation tool. Please file an issue '
    'on GitHub with a reproducible sample app and the gen-l10n configuration '
    'that was used.',
  );
}
