import 'package:flutter/material.dart';
import 'package:flutter_svg/flutter_svg.dart';

/// 原型（prototype/index.html）SVG 图标库的资产封装。
/// 全部图标位于 assets/icons/，单色线性风格，随主题 currentColor 着色。
class AppIcon extends StatelessWidget {
  const AppIcon(this.name, {super.key, this.size = 20, this.color});

  /// 图标名，与 assets/icons/`<name>`.svg 对应（如 'vault'，i- 前缀已去除）。
  final String name;
  final double size;
  final Color? color;

  @override
  Widget build(BuildContext context) {
    final iconTheme = IconTheme.of(context);
    return SvgPicture.asset(
      'assets/icons/$name.svg',
      width: size,
      height: size,
      colorFilter: ColorFilter.mode(
        color ?? iconTheme.color ?? Colors.white,
        BlendMode.srcIn,
      ),
    );
  }
}
