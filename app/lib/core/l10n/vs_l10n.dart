import '../../l10n/app_localizations.dart';
import '../security/password_policy.dart';

/// 非组件层访问当前语言的桥（P4-7）。
///
/// 状态层（Riverpod Notifier）与服务层不在组件树内，拿不到 `BuildContext`，
/// 但它们产生的文案同样是用户可见的（锁屏错误提示、同步结果通知等）。
/// 这里由 [MaterialApp.builder] 在每次本地化解析后注入当前 [AppLocalizations]，
/// 调用方用 `VsL10n.orNull?.xxx ?? '中文兜底'` 的形式取值：
///
/// - 兜底文案保证「注入之前」也能读出可理解的内容（首帧前、单元测试里）；
/// - 注入之后即随界面语言切换（切换语言会重建 MaterialApp 子树）。
abstract final class VsL10n {
  static AppLocalizations? _current;

  /// 由 MaterialApp.builder 调用。
  static void attach(AppLocalizations l) => _current = l;

  /// 当前语言的本地化实例；未初始化时为 null。
  static AppLocalizations? get orNull => _current;
}

/// 密码强度档位 → 当前语言。
extension VsStrengthL10n on PasswordStrength {
  String labelOf(AppLocalizations l) => switch (this) {
        PasswordStrength.empty => '',
        PasswordStrength.weak => l.stateStrengthWeak,
        PasswordStrength.fair => l.stateStrengthFair,
        PasswordStrength.good => l.stateStrengthGood,
        PasswordStrength.strong => l.stateStrengthStrong,
      };
}

/// 门槛问题代码 → 当前语言。
extension VsPasswordIssueL10n on PasswordIssue {
  String messageOf(AppLocalizations l) => switch (this) {
        PasswordIssue.tooShort => l.statePwTooShort(PasswordPolicy.minLength),
        PasswordIssue.tooWeak => l.statePwTooWeak,
      };
}
