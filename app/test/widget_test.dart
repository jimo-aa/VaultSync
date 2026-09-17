import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:app/presentation/router/app_router.dart';
import 'package:app/service/engine_service.dart';
import 'package:app/state/session_controller.dart';

/// 内存版引擎桩：覆盖解锁 / 伪装 / 首启创建 / 冷却语义。
class FakeEngine implements VaultEngine {
  FakeEngine({this.exists = true});

  bool exists;
  bool disguiseExistsFlag = true;
  int fails = 0;

  static const disguisePassword = '88888888';

  @override
  bool get isNative => false;

  @override
  Future<void> ensureCreated(String password) async => exists = true;

  @override
  Future<UnlockOutcome> unlock(String password) async {
    if (password == '1234') {
      return const UnlockSuccess(disguise: false, sessionHandle: 'handle-primary');
    }
    if (disguiseExistsFlag && password == disguisePassword) {
      return const UnlockSuccess(disguise: true, sessionHandle: 'handle-disguise');
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
  Future<String?> changePassword(Object sessionHandle, String oldPassword, String newPassword) =>
      Future.value(null);

  @override
  void lock(Object sessionHandle) {}

  @override
  Future<String> primaryPath() => Future.value('C:/fake/primary.vsvb');

  @override
  Future<String> disguisePath() => Future.value('C:/fake/disguise.vsvb');
}

void main() {
  test('SessionController：首启创建 → 解锁 → 伪装 → 冷却 → 锁定', () async {
    final engine = FakeEngine(exists: false);
    final container = ProviderContainer(overrides: [
      vaultEngineProvider.overrideWithValue(engine),
    ]);
    addTearDown(container.dispose);
    final controller = container.read(sessionProvider.notifier);

    // 首次运行：输入即创建
    await controller.unlock('1234');
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
    expect((container.read(sessionProvider) as SessionLocked).cooldownUntil, same(before));

    // 伪装密码：冷却重置？——引擎冷却按域共享，这里验证伪装路径成功语义
    engine.fails = 0;
    final engine2 = FakeEngine(exists: true);
    final container2 = ProviderContainer(overrides: [
      vaultEngineProvider.overrideWithValue(engine2),
    ]);
    addTearDown(container2.dispose);
    await container2.read(sessionProvider.notifier).unlock(FakeEngine.disguisePassword);
    final d = container2.read(sessionProvider) as SessionUnlocked;
    expect(d.disguise, isTrue);
  });

  testWidgets('未解锁时重定向到锁屏，输入正确主密码后进入保险箱', (tester) async {
    await tester.pumpWidget(
      ProviderScope(
        overrides: [vaultEngineProvider.overrideWithValue(FakeEngine())],
        child: Consumer(
          builder: (context, ref, _) => MaterialApp.router(
            routerConfig: ref.watch(appRouterProvider),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.textContaining('主密码'), findsOneWidget);

    await tester.enterText(find.byType(TextField), '1234');
    await tester.testTextInput.receiveAction(TextInputAction.done);
    await tester.pumpAndSettle();

    expect(find.textContaining('骨架占位页'), findsOneWidget);
  });
}
