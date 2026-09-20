import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../core/l10n/vs_l10n.dart';
import '../service/engine_service.dart';
import 'models/p2p.dart';
import 'session_controller.dart';

/// 设备页状态。引擎侧 `p2p_status` 仅给出快照（无逐设备在线态），
/// peers 即已配对设备；online 态在真实后端落地前据 alive 近似展示。
class DeviceState {
  const DeviceState({this.status, this.loading = false, this.error});

  final SyncStatus? status;
  final bool loading;
  final String? error;

  DeviceState copyWith({SyncStatus? status, bool? loading, String? error}) =>
      DeviceState(
        status: status ?? this.status,
        loading: loading ?? this.loading,
        error: error ?? this.error,
      );
}

class DeviceController extends Notifier<DeviceState> {
  @override
  DeviceState build() => const DeviceState();

  Object? get _handle {
    final s = ref.read(sessionProvider);
    return s is SessionUnlocked ? s.sessionHandle : null;
  }

  /// 拉取 P2P 状态快照。
  Future<void> refresh() async {
    final handle = _handle;
    if (handle == null) {
      state = DeviceState(
          error: VsL10n.orNull?.stateDeviceNeedUnlock ?? '请先解锁');
      return;
    }
    state = const DeviceState(loading: true);
    final json = await ref.read(vaultEngineProvider).status(handle);
    if (json == null) {
      state = DeviceState(
          error: VsL10n.orNull?.stateDeviceLoadFailed ?? '设备状态读取失败');
      return;
    }
    state = DeviceState(status: SyncStatus.fromJson(json));
  }
}

final deviceProvider = NotifierProvider<DeviceController, DeviceState>(DeviceController.new);