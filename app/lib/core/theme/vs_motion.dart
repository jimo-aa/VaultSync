import 'package:flutter/material.dart';

/// prefers-reduced-motion（P4-7）。
///
/// 平台侧「减少动画 / 关闭动画效果」开启时（Windows 无障碍设置、macOS 辅助功能、
/// 系统属性的动画开关），Flutter 会把它映射到 [MediaQueryData.disableAnimations]。
/// 此时把动效压到 1ms（而不是 0，避免个别控制器/隐式动画在零时长下的边界断言），
/// 保留状态变化但去掉位移与渐变过程。
abstract final class VsMotion {
  /// 是否应减少动效。
  static bool reduced(BuildContext context) =>
      MediaQuery.maybeDisableAnimationsOf(context) ?? false;

  /// 按当前无障碍设置缩放一段动效时长。
  static Duration scale(BuildContext context, Duration d) =>
      reduced(context) ? const Duration(milliseconds: 1) : d;

  /// 常用简写：毫秒。
  static Duration ms(BuildContext context, int millis) =>
      scale(context, Duration(milliseconds: millis));
}
