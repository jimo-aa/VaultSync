import 'dart:async';

import 'package:flutter_riverpod/flutter_riverpod.dart';

/// 通知分级（对应原型色彩语义）。
enum NotifGrade { info, ok, warning, danger }

extension NotifGradeX on NotifGrade {
  bool get isDanger => this == NotifGrade.danger;
}

/// 单条应用通知。
class AppNotification {
  const AppNotification({
    required this.grade,
    required this.message,
    required this.ts,
  });

  final NotifGrade grade;
  final String message;
  final DateTime ts;
}

/// 通知中心状态：已分级通知列表 + 未读数。
class NotificationState {
  const NotificationState({this.items = const [], this.unread = 0});

  final List<AppNotification> items;
  final int unread;

  NotificationState copyWith({List<AppNotification>? items, int? unread}) =>
      NotificationState(items: items ?? this.items, unread: unread ?? this.unread);
}

/// 通知中心：全局单例状态 + 事件流。
/// 引擎异常、配对失败、同步结果等跨页面事件统一经这里通知；铃铛读 unread。
class NotificationController extends Notifier<NotificationState> {
  final _stream = StreamController<AppNotification>.broadcast();

  /// 事件流：供监听方（如同步页）响应最新一条通知。
  Stream<AppNotification> get stream => _stream.stream;

  @override
  NotificationState build() {
    ref.onDispose(_stream.close);
    return const NotificationState();
  }

  void add(NotifGrade grade, String message) {
    final n = AppNotification(grade: grade, message: message, ts: DateTime.now());
    state = state.copyWith(items: [n, ...state.items], unread: state.unread + 1);
    _stream.add(n);
  }

  void info(String msg) => add(NotifGrade.info, msg);
  void ok(String msg) => add(NotifGrade.ok, msg);
  void warning(String msg) => add(NotifGrade.warning, msg);
  void danger(String msg) => add(NotifGrade.danger, msg);

  void markAllRead() {
    if (state.unread == 0) return;
    state = NotificationState(items: state.items, unread: 0);
  }
}

final notificationProvider =
    NotifierProvider<NotificationController, NotificationState>(
        NotificationController.new);