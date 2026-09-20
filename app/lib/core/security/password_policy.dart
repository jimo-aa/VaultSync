/// 主密码强度策略（P4-1）：首次创建与改密共用同一把尺子。
///
/// 只做本地启发式评估与门槛校验，不派生任何密钥——密码明文随后交给
/// Rust 引擎（Argon2id 派生 KEK），Dart 侧不做也不应做任何密钥运算。
library;

/// 强度档位（对应原型 .strength 的 s1–s4）。
enum PasswordStrength {
  empty(0, ''),
  weak(1, '弱'),
  fair(2, '一般'),
  good(3, '较强'),
  strong(4, '强');

  const PasswordStrength(this.level, this.label);

  final int level;
  final String label;
}

/// 门槛未通过的原因代码。
enum PasswordIssue { tooShort, tooWeak }

abstract final class PasswordPolicy {
  /// 创建保险箱时的最短长度（P1 的 ≥4 位临时下限在 P4-1 收紧为 8）。
  static const minLength = 8;

  /// 强度评级：对应原型 app.js 的评级逻辑（长度 / 大小写 / 数字+符号 各一档，
  /// 16 位以上且已满三档升为「强」）。
  static PasswordStrength evaluate(String v) {
    if (v.isEmpty) return PasswordStrength.empty;
    var s = 0;
    if (v.length >= 8) s++;
    if (v.length >= 12 &&
        v.contains(RegExp(r'[A-Z]')) &&
        v.contains(RegExp(r'[a-z]'))) {
      s++;
    }
    if (v.contains(RegExp(r'\d')) && v.contains(RegExp(r'[^A-Za-z0-9]'))) {
      s++;
    }
    if (v.length >= 16 && s == 3) s = 4;
    return PasswordStrength.values[s.clamp(1, 4)];
  }

  /// 硬性门槛（首次创建、改密、伪装密码共用）；通过返回 null。
  ///
  /// 返回的是**问题代码**而不是文案：状态层与组件层不在同一处取词，
  /// 文案由 UI 侧经 [PasswordIssue] 映射到当前语言（见 core/l10n/vs_l10n.dart）。
  static PasswordIssue? validate(String v) {
    if (v.length < minLength) return PasswordIssue.tooShort;
    if (evaluate(v).level < PasswordStrength.fair.level) {
      return PasswordIssue.tooWeak;
    }
    return null;
  }
}
