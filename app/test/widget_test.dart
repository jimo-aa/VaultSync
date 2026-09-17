import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:app/presentation/router/app_router.dart';
import 'package:app/state/session_controller.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  test('SessionController：默认锁定，错误密码计数，正确口令解锁', () {
    final container = ProviderContainer();
    addTearDown(container.dispose);
    final controller = container.read(sessionProvider.notifier);

    expect(container.read(sessionProvider), isA<SessionLocked>());
    expect(controller.unlock('wrong'), isFalse);
    expect(
      container.read(sessionProvider),
      isA<SessionLocked>().having((s) => s.failedAttempts, 'failedAttempts', 1),
    );

    expect(controller.unlock('1234'), isTrue);
    expect(container.read(sessionProvider), isA<SessionUnlocked>());

    controller.lock();
    expect(container.read(sessionProvider), isA<SessionLocked>());
  });

  testWidgets('未解锁时重定向到锁屏，输入正确主密码后进入保险箱', (tester) async {
    await tester.pumpWidget(
      ProviderScope(
        child: Consumer(
          builder: (context, ref, _) => MaterialApp.router(
            routerConfig: ref.watch(appRouterProvider),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('输入主密码'), findsOneWidget);

    await tester.enterText(find.byType(TextField), '1234');
    await tester.testTextInput.receiveAction(TextInputAction.done);
    await tester.pumpAndSettle();

    expect(find.textContaining('骨架占位页'), findsOneWidget);
  });
}
