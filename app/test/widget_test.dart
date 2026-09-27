import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:shared_preferences/shared_preferences.dart';

import 'package:app/core/security/password_policy.dart';
import 'package:app/l10n/app_localizations.dart';
import 'package:app/presentation/router/app_router.dart';
import 'package:app/service/engine_service.dart';
import 'package:app/state/app_prefs.dart';
import 'package:app/state/session_controller.dart';
import 'package:app/state/settings_controller.dart';

/// 内存版引擎桩：覆盖解锁 / 伪装 / 首启创建 / 冷却语义。
class FakeEngine implements VaultEngine {
  FakeEngine({this.exists = true});

  bool exists;
  bool disguiseExistsFlag = true;
  int fails = 0;

  static const disguisePassword = '88888888';

  @override
  bool get isNative => false;

  String primaryPassword = '';

  @override
  Future<void> ensureCreated(String password) async {
    primaryPassword = password;
    exists = true;
  }

  @override
  Future<void> ensureDisguiseCreated(String password) async {
    disguiseExistsFlag = true;
  }

  @override
  Future<UnlockOutcome> unlock(String password) async {
    final primaryOk = password == '1234' ||
        (primaryPassword.isNotEmpty && password == primaryPassword);
    if (primaryOk) {
      return const UnlockSuccess(
          disguise: false, sessionHandle: 'handle-primary');
    }
    if (disguiseExistsFlag && password == disguisePassword) {
      return const UnlockSuccess(
          disguise: true, sessionHandle: 'handle-disguise');
    }
    fails += 1;
    if (fails >= 3) return const UnlockCooldown(30000);
    return UnlockWrongPassword(cooldownMs: 0);
  }

  @override
  Future<UnlockOutcome> unlockBio() async =>
      const UnlockFailure('尚未绑定生物识别（设置中绑定后可用）');

  @override
  Future<bool> vaultExists() async => exists;

  @override
  Future<bool> disguiseExists() async => disguiseExistsFlag;

  @override
  Future<bool> bioBound() async => false;

  @override
  Future<int> cooldownRemainingMs() async => 0;

  @override
  Future<String?> changePassword(
          Object sessionHandle, String oldPassword, String newPassword) =>
      Future.value(null);

  @override
  void lock(Object sessionHandle) {}

  // ==== P4：生物识别绑定 ====
  @override
  Future<String?> bindBio(Object sessionHandle, {bool bind = true}) =>
      Future.value(null);

  // ==== P3 p2p（桩：本测试未覆盖，一律空实现）====
  @override
  Future<Map<String, dynamic>?> pairBegin(Object sessionHandle) =>
      Future.value(null);

  @override
  Future<Map<String, dynamic>?> pairJoin(
          Object sessionHandle, String addr, String code) =>
      Future.value(null);

  @override
  Future<Map<String, dynamic>?> sync(Object sessionHandle, String addr) =>
      Future.value(null);

  @override
  Future<Map<String, dynamic>?> syncRelay(
          Object sessionHandle, String relay, String room, String token) =>
      Future.value(null);

  @override
  Future<int> serveRelay(
          Object sessionHandle, String relay, String room, String token) =>
      Future.value(0);

  @override
  Future<Map<String, dynamic>?> relaySettingsGet(Object sessionHandle) =>
      Future.value(null);

  @override
  Future<int> relaySettingsSet(Object sessionHandle, String json) =>
      Future.value(0);

  @override
  Future<Map<String, dynamic>?> syncRelayCandidates(
          Object sessionHandle, String room) =>
      Future.value(null);

  @override
  Future<int> serveRelayCandidates(Object sessionHandle, String room) =>
      Future.value(0);

  @override
  Future<Map<String, dynamic>?> status(Object sessionHandle) =>
      Future.value(null);

  @override
  Future<Map<String, dynamic>?> syncPolicyGet(Object sessionHandle) =>
      Future.value(null);

  @override
  Future<int> syncPolicySet(
          Object sessionHandle, Map<String, dynamic> policy) =>
      Future.value(0);

  @override
  Future<int> setSyncContext(Object sessionHandle,
          {required bool inWindow, required String netType}) =>
      Future.value(0);

  @override
  Future<Map<String, dynamic>?> stegoPlan(Object sessionHandle, int fileId,
          List<String> imagePaths, Map<String, dynamic>? opts) =>
      Future.value(null);

  @override
  Future<int> applyEdit(Object sessionHandle, int fileId, String src,
          {int keepRevs = 1}) =>
      Future.value(0);

  @override
  Future<Map<String, dynamic>?> stegoEmbedMulti(Object sessionHandle, int fileId,
          List<String> imagePaths, Map<String, dynamic>? opts) =>
      Future.value(null);

  @override
  Future<Map<String, dynamic>?> stegoExtractMulti(
          Object sessionHandle, List<String> imagePaths, String destPath) =>
      Future.value(null);

  @override
  Future<String?> unpair(Object sessionHandle, String deviceId) =>
      Future.value(null);

  @override
  Future<Map<String, dynamic>?> destroyArm(
          Object sessionHandle, String addr, String target, int delaySecs) =>
      Future.value(null);

  @override
  Future<int> destroyCancel(Object sessionHandle) => Future.value(0);

  @override
  Future<int> conflictResolve(Object sessionHandle, int keepId, int dropId,
          {bool secure = false}) =>
      Future.value(0);

  @override
  Future<String> primaryPath() => Future.value('C:/fake/primary.vsvb');

  @override
  Future<String> disguisePath() => Future.value('C:/fake/disguise.vsvb');

  // ==== P2 保险箱操作（桩：引擎操作在本测试未覆盖，一律空实现）====

  @override
  Future<(String?, int)> mkdir(Object sessionHandle, int parent, String name) =>
      Future.value((null, 0));

  @override
  Future<Map<String, dynamic>?> list(Object sessionHandle, int folder) =>
      Future.value({
        'folders': <Map<String, dynamic>>[],
        'files': [
          {
            'id': 1,
            'name': 'demo.txt',
            'size': 4,
            'tags': <String>[],
            'modifiedMs': 0
          },
        ],
      });

  @override
  Future<Map<String, dynamic>?> search(Object sessionHandle, String query) =>
      Future.value(null);

  @override
  Future<(String?, int)> importFile(
          Object sessionHandle, String src, int folder) =>
      Future.value((null, 0));

  @override
  Future<String?> exportFile(Object sessionHandle, int fileId, String dest) =>
      Future.value(null);

  @override
  Future<String?> renameFile(Object sessionHandle, int fileId, String name) =>
      Future.value(null);

  @override
  Future<String?> deleteFile(Object sessionHandle, int fileId,
          {bool secure = true}) =>
      Future.value(null);

  @override
  Future<String?> renameFolder(
          Object sessionHandle, int folderId, String name) =>
      Future.value(null);

  @override
  Future<(String?, int)> deleteFolder(Object sessionHandle, int folderId,
          {bool secure = true}) =>
      Future.value((null, 0));

  @override
  Future<String?> setTags(
          Object sessionHandle, int targetId, List<String> tags) =>
      Future.value(null);

  @override
  Future<Map<String, dynamic>?> shareCreate(
          Object sessionHandle, int fileId, int ttlSecs, int maxOpens) =>
      Future.value(null);

  @override
  Future<Map<String, dynamic>?> verifyAll(Object sessionHandle) =>
      Future.value(null);

  @override
  Future<Map<String, dynamic>?> destroyQueueAll(
          Object sessionHandle, int delaySecs) =>
      Future.value(null);

  @override
  Future<Map<String, dynamic>?> pendingOrders(Object sessionHandle) =>
      Future.value(null);

  @override
  Future<int> cancelOrders(Object sessionHandle) => Future.value(0);

  @override
  Future<String?> rememberPeerAddr(
          Object sessionHandle, String deviceId, String addr) =>
      Future.value(null);

  @override
  Future<String?> rotateFileKey(Object sessionHandle, int fileId) =>
      Future.value(null);

  @override
  Future<String?> rotateFolderKeys(Object sessionHandle, int folderId) =>
      Future.value(null);

  @override
  Future<String?> rotateMk(Object sessionHandle, String password) =>
      Future.value(null);

  @override
  Future<Map<String, dynamic>?> auditList(Object sessionHandle) =>
      Future.value(null);

  @override
  Future<Map<String, dynamic>?> auditVerify(Object sessionHandle) =>
      Future.value(null);

  @override
  Future<String?> auditExport(Object sessionHandle, String dest) =>
      Future.value(null);

  @override
  Future<String?> destroyLocal(Object sessionHandle, {bool secure = true}) =>
      Future.value(null);

  @override
  Future<bool> stegoEnabled(Object sessionHandle) => Future.value(false);

  @override
  Future<String?> stegoSetEnabled(Object sessionHandle, bool on) =>
      Future.value(null);

  @override
  Future<int> stegoCapacity(Object sessionHandle, String imagePath) =>
      Future.value(0);

  @override
  Future<String?> stegoEmbed(
          Object sessionHandle, int fileId, String imagePath, String outPath) =>
      Future.value(null);

  @override
  Future<Map<String, dynamic>?> stegoExtract(
          Object sessionHandle, String imagePath, String destPath) =>
      Future.value(null);

  @override
  Future<String?> shareOpen(
          Object sessionHandle, int shareId, String token, String dest) =>
      Future.value(null);
}

void main() {
  test('SessionController：首启创建 → 解锁 → 伪装 → 冷却 → 锁定', () async {
    SharedPreferences.setMockInitialValues({});
    final prefs = await SharedPreferences.getInstance();
    final engine = FakeEngine(exists: false);
    final container = ProviderContainer(overrides: [
      vaultEngineProvider.overrideWithValue(engine),
      sharedPreferencesProvider.overrideWithValue(prefs),
    ]);
    addTearDown(container.dispose);
    final controller = container.read(sessionProvider.notifier);

    // 首启：弱密码被强度策略拦下（P4-1），且不计入失败次数
    await controller.unlock('1234');
    final weak = container.read(sessionProvider) as SessionLocked;
    expect(weak.hint, contains('至少 8 位'));
    expect(weak.failedAttempts, 0);
    expect(engine.exists, isFalse);

    // 达到门槛后创建并立即解封
    await controller.unlock('VaultSync#2026');
    expect(container.read(sessionProvider), isA<SessionUnlocked>());
    expect(engine.exists, isTrue);
    controller.lock();
    expect(container.read(sessionProvider), isA<SessionLocked>());

    // 错误密码 ×2 → WrongPassword；第 3 次 → 冷却
    await controller.unlock('wrong1');
    expect(container.read(sessionProvider), isA<SessionLocked>());
    await controller.unlock('wrong2');
    await controller.unlock('wrong3');
    final locked = container.read(sessionProvider) as SessionLocked;
    expect(locked.cooling, isTrue);
    expect(locked.cooldownUntil, isNotNull);

    // 冷却中再试被本地拦截
    final before = locked.cooldownUntil;
    await controller.unlock('1234');
    expect((container.read(sessionProvider) as SessionLocked).cooldownUntil,
        same(before));

    // 伪装入口（P4-5）：开关关闭时即使命中伪装密码也不放行（对外与密码错误同形）
    final engine2 = FakeEngine(exists: true);
    final container2 = ProviderContainer(overrides: [
      vaultEngineProvider.overrideWithValue(engine2),
      sharedPreferencesProvider.overrideWithValue(prefs),
    ]);
    addTearDown(container2.dispose);
    await container2
        .read(sessionProvider.notifier)
        .unlock(FakeEngine.disguisePassword);
    expect(container2.read(sessionProvider), isA<SessionLocked>());

    // 打开开关后放行，并进入伪空间会话
    await container2.read(settingsProvider.notifier).setDecoy(true);
    await container2
        .read(sessionProvider.notifier)
        .unlock(FakeEngine.disguisePassword);
    final d = container2.read(sessionProvider) as SessionUnlocked;
    expect(d.disguise, isTrue);
  });

  testWidgets('无障碍：锁屏关键控件暴露语义标签（P5-8 走查）', (tester) async {
    SharedPreferences.setMockInitialValues({});
    final prefs = await SharedPreferences.getInstance();
    // 必须在测试体内 dispose：框架在 teardown 之前校验句柄已释放
    final semantics = tester.ensureSemantics();
    await tester.pumpWidget(
      ProviderScope(
        overrides: [
          vaultEngineProvider.overrideWithValue(FakeEngine()),
          sharedPreferencesProvider.overrideWithValue(prefs),
        ],
        child: Consumer(
          builder: (context, ref, _) => MaterialApp.router(
            routerConfig: ref.watch(appRouterProvider),
            locale: const Locale('zh'),
            localizationsDelegates: const [
              AppLocalizations.delegate,
              GlobalMaterialLocalizations.delegate,
              GlobalWidgetsLocalizations.delegate,
              GlobalCupertinoLocalizations.delegate,
            ],
            supportedLocales: AppLocalizations.supportedLocales,
          ),
        ),
      ),
    );
    await tester.pump(const Duration(milliseconds: 700)); // 转盘为无限动画

    // 按钮级：VsButton / VsIconButton 均带语义标签（P4-7 起内建）
    // 用 RegExp：显式 label 会与子级文本合并，精确匹配不可靠
    expect(find.bySemanticsLabel(RegExp('解锁')), findsWidgets);
    expect(find.bySemanticsLabel(RegExp('生物识别')), findsWidgets);
    // 密码可见性开关：此前是裸 GestureDetector，P5-8 补语义
    expect(find.bySemanticsLabel(RegExp('显示 / 隐藏主密码')), findsOneWidget);
    semantics.dispose();
  });

  test('PasswordPolicy：门槛与强度评级', () {
    // 长度不足
    expect(PasswordPolicy.validate('1234'), PasswordIssue.tooShort);
    // 8 位但只有单一字符类（无数字/符号档）
    expect(PasswordPolicy.validate('abcdefgh'), PasswordIssue.tooWeak);
    // 8 位 + 数字 + 符号 → 达标
    expect(PasswordPolicy.validate('Abcdefgh1!'), isNull);
    expect(PasswordPolicy.evaluate('Abcdefgh1!'), PasswordStrength.fair);
    // ≥12 位且大小写齐全 + 数字符号 → 较强；≥16 位再升一档
    expect(PasswordPolicy.evaluate('Abcdefgh1!xy'), PasswordStrength.good);
    expect(
        PasswordPolicy.evaluate('Abcdefgh1!xyzw23'), PasswordStrength.strong);
    expect(PasswordPolicy.evaluate(''), PasswordStrength.empty);
  });

  testWidgets('未解锁时重定向到锁屏，输入正确主密码后进入保险箱', (tester) async {
    SharedPreferences.setMockInitialValues({});
    final prefs = await SharedPreferences.getInstance();
    await tester.pumpWidget(
      ProviderScope(
        overrides: [
          vaultEngineProvider.overrideWithValue(FakeEngine()),
          sharedPreferencesProvider.overrideWithValue(prefs),
        ],
        child: Consumer(
          builder: (context, ref, _) => MaterialApp.router(
            routerConfig: ref.watch(appRouterProvider),
            // 与产品默认一致：模板语言 zh（不跟随测试宿主机的 en）。
            locale: const Locale('zh'),
            localizationsDelegates: const [
              AppLocalizations.delegate,
              GlobalMaterialLocalizations.delegate,
              GlobalWidgetsLocalizations.delegate,
              GlobalCupertinoLocalizations.delegate,
            ],
            supportedLocales: AppLocalizations.supportedLocales,
          ),
        ),
      ),
    );
    // 锁屏转盘为无限旋转动画（对应原型 dialspin），不能 pumpAndSettle。
    await tester.pump(const Duration(milliseconds: 700));

    expect(find.textContaining('输入主密码'), findsOneWidget);

    await tester.enterText(find.byType(TextField), '1234');
    await tester.testTextInput.receiveAction(TextInputAction.done);
    await tester.pumpAndSettle();

    // P2 起 /vault 为真实保险箱页（列出 FakeEngine 的文件）
    expect(find.text('demo.txt'), findsOneWidget);
  });
}
