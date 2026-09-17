import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../ffi/vault_core_bridge.dart';

/// 引擎桥单例：优先加载原生库，失败时回退 Stub（骨架阶段允许，P1 起必须硬失败）。
final vaultCoreBridgeProvider = Provider<VaultCoreBridge>((ref) {
  return VaultCoreBridgeFfi.tryOpen() ?? VaultCoreBridgeStub();
});
