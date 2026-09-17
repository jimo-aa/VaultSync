import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../core/icons/app_icon.dart';
import '../../state/session_controller.dart';

/// 锁屏：主密码解锁（F-02）+ 生物识别（F-04）+ 防护冷却倒计时（P1-5）。
class LockScreen extends ConsumerStatefulWidget {
  const LockScreen({super.key});

  @override
  ConsumerState<LockScreen> createState() => _LockScreenState();
}

class _LockScreenState extends ConsumerState<LockScreen> {
  final _controller = TextEditingController();
  bool _obscure = true;
  bool _busy = false;
  Timer? _ticker;

  @override
  void initState() {
    super.initState();
    // 冷却倒计时刷新
    _ticker = Timer.periodic(const Duration(milliseconds: 500), (_) {
      if (mounted) setState(() {});
    });
  }

  @override
  void dispose() {
    _ticker?.cancel();
    _controller.dispose();
    super.dispose();
  }

  Future<void> _unlock() async {
    if (_busy) return;
    setState(() => _busy = true);
    await ref.read(sessionProvider.notifier).unlock(_controller.text);
    if (mounted) setState(() => _busy = false);
  }

  Future<void> _unlockBio() async {
    if (_busy) return;
    setState(() => _busy = true);
    await ref.read(sessionProvider.notifier).unlockBio();
    if (mounted) setState(() => _busy = false);
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final session = ref.watch(sessionProvider);
    final locked = session is SessionLocked ? session : const SessionLocked();

    // 未锁定 → 由路由守卫跳转；本页只在锁定态渲染。
    if (session is SessionUnlocked) {
      WidgetsBinding.instance.addPostFrameCallback((_) => context.go('/vault'));
      return const Scaffold(body: Center(child: CircularProgressIndicator()));
    }

    final cooling = locked.cooling;
    final remaining = locked.cooldownUntil?.difference(DateTime.now());
    final remainingText = remaining == null
        ? ''
        : '${remaining.inMinutes.toString().padLeft(2, '0')}:'
            '${(remaining.inSeconds % 60).toString().padLeft(2, '0')}';

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
              if (cooling)
                Row(
                  mainAxisAlignment: MainAxisAlignment.center,
                  children: [
                    Icon(Icons.timer_outlined, size: 18, color: theme.colorScheme.error),
                    const SizedBox(width: 6),
                    Text(
                      '防护冷却中 $remainingText 后可重试',
                      style: TextStyle(color: theme.colorScheme.error),
                    ),
                  ],
                )
              else if (locked.hint != null)
                Text(locked.hint!, style: TextStyle(color: theme.colorScheme.error))
              else if (locked.failedAttempts > 0)
                Text(
                  '密码错误，已失败 ${locked.failedAttempts} 次（连续 3 次触发冷却）',
                  style: TextStyle(color: theme.colorScheme.error),
                ),
              const SizedBox(height: 8),
              TextField(
                controller: _controller,
                obscureText: _obscure,
                autofocus: true,
                enabled: !cooling && !_busy,
                onSubmitted: (_) => _unlock(),
                decoration: InputDecoration(
                  hintText: '输入主密码（首次输入将创建保险箱）',
                  prefixIcon: const Icon(Icons.lock_outline),
                  suffixIcon: IconButton(
                    icon: Icon(_obscure
                        ? Icons.visibility_off_outlined
                        : Icons.visibility_outlined),
                    onPressed: () => setState(() => _obscure = !_obscure),
                  ),
                ),
              ),
              const SizedBox(height: 16),
              FilledButton(
                onPressed: cooling || _busy ? null : _unlock,
                style: FilledButton.styleFrom(minimumSize: const Size.fromHeight(48)),
                child: _busy
                    ? const SizedBox(
                        width: 20,
                        height: 20,
                        child: CircularProgressIndicator(strokeWidth: 2),
                      )
                    : const Text('解锁'),
              ),
              const SizedBox(height: 8),
              TextButton.icon(
                onPressed: cooling || _busy ? null : _unlockBio,
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
