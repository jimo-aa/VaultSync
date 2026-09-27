// 打洞探针 CLI（P8-4；配套 docs/v2.0/14-打洞NAT矩阵验收手册.md §七）。
//
// 用途：在**任意一端**（云主机 netns 内 / 本机 / 容器）调用引擎的打洞探针，打印结构化结果，
// 供 NAT 矩阵逐格抄录。**不要求与对端已配对**：探针走真实路径（VSR2 接入 → 从 OK 行取
// 观测端点 → Noise 握手 → Hello 候选交换 → 映射观测 → 打洞），但不读写保险箱内容。
//
// 因为引擎挂在「已解锁会话」上，本工具会在 --dir 下创建一个**一次性临时保险箱**并解锁
// （仅承载身份与数据目录；探针不碰里面的文件）。
//
// 用法（矩阵里两端各跑一次，room/code 两端必须一致）：
//   dart run tool/nat_probe.dart \
//     --relay 203.0.113.9:47471 --room nat-cell-1 --token <relay-token> \
//     --code natlab-shared-code --role initiator --dir /tmp/np-a
//   # 另一端把 --role 换成 responder、--dir 换 /tmp/np-b；两端并行执行（房间需两端都在）
//
// 退出码：0 = 打通（ok=true）｜1 = 未打通（ok=false，JSON 仍打印）｜2 = 调用失败（null）
// 输出：一行 `PUNCH {json}`（便于脚本抓取）+ 人类可读摘要；诊断在引擎事件日志（stderr 侧）。
// ignore_for_file: avoid_print

import 'dart:convert';
import 'dart:ffi';
import 'dart:io';

import 'package:ffi/ffi.dart';

typedef CreateC = Int32 Function(Pointer<Utf8>, Pointer<Utf8>, Int32);
typedef UnlockNative = Int32 Function(Pointer<Utf8>, Pointer<Utf8>, Int32,
    Pointer<Pointer<Void>>, Pointer<Uint64>);
typedef UnlockDart = int Function(
    Pointer<Utf8>, Pointer<Utf8>, int, Pointer<Pointer<Void>>, Pointer<Uint64>);
typedef LockC = Void Function(Pointer<Void>);
typedef PunchProbeC = Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>,
    Pointer<Utf8>, Pointer<Utf8>, Pointer<Utf8>, Uint32, Uint64);
typedef PunchProbeDart = Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>,
    Pointer<Utf8>, Pointer<Utf8>, Pointer<Utf8>, int, int);
typedef P2pStatusC = Pointer<Utf8> Function(Pointer<Void>);
typedef FreeStringC = Void Function(Pointer<Utf8>);

/// 临时保险箱口令（本工具自建自用、用完即弃；不承载任何真实数据）。
const _tmpPassword = 'nat-probe-tmp-1234';

/// 定位共享库（Windows = `.dll`；Linux/macOS = `.so`/`.dylib`）。
String _libPath() {
  final override = Platform.environment['VAULT_CORE_LIB'];
  if (override != null && override.isNotEmpty) return override;
  final sep = Platform.pathSeparator;
  final rel = '..${sep}native${sep}target${sep}release';
  if (Platform.isWindows) return '$rel${sep}vault_core.dll';
  if (Platform.isMacOS) return '$rel${sep}libvault_core.dylib';
  return '$rel${sep}libvault_core.so';
}

String? _arg(List<String> a, String name) {
  final i = a.indexOf(name);
  if (i < 0 || i + 1 >= a.length) return null;
  return a[i + 1];
}

Future<int> main(List<String> argv) async {
  final relay = _arg(argv, '--relay');
  final room = _arg(argv, '--room');
  final token = _arg(argv, '--token');
  final code = _arg(argv, '--code') ?? '';
  final role = (_arg(argv, '--role') ?? 'initiator').toLowerCase();
  final dir = _arg(argv, '--dir') ?? Directory.systemTemp.createTempSync('nat_probe_').path;
  final timeout = int.tryParse(_arg(argv, '--timeout') ?? '') ?? 8000;

  if (relay == null || room == null || token == null) {
    stderr.writeln('用法：dart run tool/nat_probe.dart --relay <host:port> --room <room> '
        '--token <relay-token> [--code <shared-code>] [--role initiator|responder] '
        '[--dir <dir>] [--timeout <ms>]');
    return 2;
  }
  if (role != 'initiator' && role != 'responder') {
    stderr.writeln('--role 只能是 initiator / responder（Noise 握手必须一端发起一端响应）');
    return 2;
  }
  if (relay.startsWith('127.') || relay.startsWith('localhost')) {
    // 不是错误，但矩阵里必须指向**公网**中继 —— 如实提醒，避免把回环数据当穿透证据
    stderr.writeln('提醒：中继是回环地址；回环结果只能作自检，不能作穿透证据（手册 §三.3）');
  }

  final libPath = _libPath();
  if (!File(libPath).existsSync()) {
    stderr.writeln('找不到共享库：$libPath\n（先 `cargo build --release -p vault-core`，'
        '或用 VAULT_CORE_LIB 指定路径）');
    return 2;
  }
  final lib = DynamicLibrary.open(libPath);
  final create = lib.lookupFunction<CreateC, int Function(Pointer<Utf8>, Pointer<Utf8>, int)>(
      'vault_core_create_vault');
  final unlock = lib.lookupFunction<UnlockNative, UnlockDart>('vault_core_unlock');
  final lock = lib.lookupFunction<LockC, void Function(Pointer<Void>)>('vault_core_lock');
  final probe = lib.lookupFunction<PunchProbeC, PunchProbeDart>(
      'vault_core_p2p_punch_probe');
  final freeStr = lib.lookupFunction<FreeStringC, void Function(Pointer<Utf8>)>(
      'vault_core_free_string');

  final vaultPath = '$dir${Platform.pathSeparator}nat-probe.vsvb';
  // 目录不存在则建（手动在不同 netns 里起两端时最容易踩）
  Directory(dir).createSync(recursive: true);
  final vp = vaultPath.toNativeUtf8();
  final pw = _tmpPassword.toNativeUtf8();
  try {
    if (!File(vaultPath).existsSync()) {
      final rc = create(vp, pw, 0);
      if (rc != 0) {
        stderr.writeln('创建临时保险箱失败：$rc（目录 $dir）');
        return 2;
      }
    }
    final handleOut = calloc<Pointer<Void>>();
    final waitOut = calloc<Uint64>();
    int rc;
    try {
      rc = unlock(vp, pw, 0, handleOut, waitOut);
    } finally {
      // 句柄仍需保留到探针结束，故只释放这两个中间分配
    }
    if (rc != 0 || handleOut.value.address == 0) {
      stderr.writeln('解锁临时保险箱失败：$rc（等待 ${waitOut.value} ms）');
      calloc.free(handleOut);
      calloc.free(waitOut);
      return 2;
    }
    final handle = handleOut.value;
    final pa = relay.toNativeUtf8();
    final pr = room.toNativeUtf8();
    final pt = token.toNativeUtf8();
    final pc = code.toNativeUtf8();
    final t0 = DateTime.now();
    final out = probe(handle, pa, pr, pt, pc, role == 'initiator' ? 0 : 1, timeout);
    final wall = DateTime.now().difference(t0).inMilliseconds;
    final json = out.address == 0 ? null : out.toDartString();
    if (out.address != 0) freeStr(out);
    calloc.free(pa);
    calloc.free(pr);
    calloc.free(pt);
    calloc.free(pc);
    lock(handle);
    calloc.free(handleOut);
    calloc.free(waitOut);

    if (json == null) {
      stderr.writeln('探针调用失败（返回 null）——最近引擎事件（诊断在此）：');
      final status = lib.lookupFunction<P2pStatusC, Pointer<Utf8> Function(Pointer<Void>)>(
          'vault_core_p2p_status');
      final st = status(handle);
      if (st.address != 0) {
        final sj = jsonDecode(st.toDartString()) as Map<String, dynamic>;
        freeStr(st);
        final ev = (sj['events'] as List?)?.reversed.take(6).toList() ?? const [];
        for (final e in ev) {
          stderr.writeln('  · $e');
        }
        if (ev.isEmpty) stderr.writeln('  （无事件：可能卡在接入/握手前的等待房间）');
      }
      stderr.writeln('排查：① 房间要两端都在（先起的一端会在中继等待房间排队，TTL 见中继配置）'
          '；② token 两端与中继配置一致（≥16 字节）；③ 中继需启用 [observe]（否则 reason 会是 no_observe_endpoint）');
      lock(handle);
      calloc.free(handleOut);
      calloc.free(waitOut);
      return 2;
    }
    print('PUNCH $json');
    final m = jsonDecode(json) as Map<String, dynamic>;
    print('role=${m['role']} ok=${m['ok']} reason=${m['reason']} kind=${m['kind']} '
        'symmetricNat=${m['symmetricNat']} candidates=${m['localCandidates']}/${m['remoteCandidates']} '
        'mappedPort=${m['mappedPort']} peerObservedPort=${m['peerObservedPort']} '
        'elapsedMs=${m['elapsedMs']} 墙钟=${wall}ms peer=${m['peerDeviceId']}');
    // 端口一致 → 锥形映射；不同 → 对称 NAT（与手册 §6.3 的判据一致）
    if (m['mappedPort'] != null && m['peerObservedPort'] != null) {
      final same = m['mappedPort'] == m['peerObservedPort'];
      print('NAT 判定：${same ? '非对称（锥形，端口一致）' : '对称（两观测目标端口不同）'}');
    }
    return m['ok'] == true ? 0 : 1;
  } finally {
    calloc.free(vp);
    calloc.free(pw);
  }
}
