import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../core/icons/app_icon.dart';
import '../../state/session_controller.dart';

/// 锁屏：主密码解锁（转盘动效、生物识别在 P1 阶段对接）。
class LockScreen extends ConsumerStatefulWidget {
  const LockScreen({super.key});

  @override
  ConsumerState<LockScreen> createState() => _LockScreenState();
}

class _LockScreenState extends ConsumerState<LockScreen> {
  final _controller = TextEditingController();
  bool _obscure = true;
  String? _error;

  void _unlock() {
    final ok = ref.read(sessionProvider.notifier).unlock(_controller.text);
    if (ok) {
      context.go('/vault');
    } else {
      setState(() => _error = '密码错误，请重试');
    }
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final failed = ref.watch(
      sessionProvider.select((s) => s is SessionLocked ? s.failedAttempts : 0),
    );

    return Scaffold(
      body: Center(
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 360),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              AppIcon('logo', size: 72, color: theme.colorScheme.primary),
              const SizedBox(height: 12),
              Text('VaultSync', style: theme.textTheme.displayLarge),
              const SizedBox(height: 40),
              if (failed >= 3)
                Text('连续失败 $failed 次，已触发防护延时', style: TextStyle(color: theme.colorScheme.error))
              else if (_error != null)
                Text(_error!, style: TextStyle(color: theme.colorScheme.error)),
              const SizedBox(height: 8),
              TextField(
                controller: _controller,
                obscureText: _obscure,
                autofocus: true,
                onSubmitted: (_) => _unlock(),
                decoration: InputDecoration(
                  hintText: '输入主密码',
                  prefixIcon: const Icon(Icons.lock_outline),
                  suffixIcon: IconButton(
                    icon: Icon(_obscure ? Icons.visibility_off_outlined : Icons.visibility_outlined),
                    onPressed: () => setState(() => _obscure = !_obscure),
                  ),
                ),
              ),
              const SizedBox(height: 16),
              FilledButton(
                onPressed: _unlock,
                style: FilledButton.styleFrom(minimumSize: const Size.fromHeight(48)),
                child: const Text('解锁'),
              ),
              const SizedBox(height: 8),
              TextButton.icon(
                // TODO(P1-3): 对接平台安全区生物识别路径。
                onPressed: () {},
                icon: const AppIcon('finger'),
                label: const Text('生物识别'),
              ),
            ],
          ),
        ),
      ),
    );
  }
}
