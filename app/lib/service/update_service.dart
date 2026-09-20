import 'dart:convert';
import 'dart:io';

import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../ffi/vault_core_bridge.dart';
import 'engine_service.dart';

/// 更新通道客户端（P5-7 的客户端侧）。
///
/// 设计约束（why）：
/// - **验签在引擎里做**：`vault_core_verify_update_manifest` 用的是与 P2P 身份同族的
///   Ed25519 实现（`vault-crypto::sig`），Dart 侧不引入任何密码学依赖；
/// - **fail-closed**：清单地址或内置公钥为空时一律 `disabled`，绝不"跳过校验照常更新"；
/// - 清单签名覆盖**清单原始字节**（不做 JSON 规范化，避免规范化歧义被利用）；
/// - 安装包下载后用清单里的 sha256 复核（同样走引擎的 `vault_core_file_sha256`）。
///
/// 服务端约定（发布方需遵守，见 packaging/README.md §三）：
///   GET `<manifestUrl>`       → 清单 JSON 原始字节（application/json）
///   GET `<manifestUrl>.sig`   → 上述字节的 Ed25519 签名（hex 文本，可含换行）
class UpdateManifest {
  const UpdateManifest({
    required this.version,
    required this.platform,
    required this.url,
    required this.sha256,
    this.notes = '',
  });

  final String version;
  final String platform;
  final String url;
  final String sha256;
  final String notes;

  static UpdateManifest? tryParse(String raw) {
    try {
      final m = jsonDecode(raw) as Map<String, dynamic>;
      final version = m['version'];
      final platform = m['platform'];
      final url = m['url'];
      final sha256 = m['sha256'];
      if (version is! String ||
          platform is! String ||
          url is! String ||
          sha256 is! String) {
        return null;
      }
      return UpdateManifest(
        version: version,
        platform: platform,
        url: url,
        sha256: sha256.toLowerCase(),
        notes: (m['notes'] as String?) ?? '',
      );
    } catch (_) {
      return null;
    }
  }
}

/// 检查结果。`status` 取值：
/// `disabled`（未配置）/ `ok`（已是最新）/ `available`（有新版本）/ `error`（失败，附 message）。
class UpdateCheck {
  const UpdateCheck._(this.status, {this.manifest, this.message});

  const UpdateCheck.disabled()
      : this._('disabled', message: '未配置更新通道（需清单地址 + 发布公钥）');
  const UpdateCheck.upToDate(String v) : this._('ok', message: '已是最新版本 v$v');
  const UpdateCheck.available(UpdateManifest m)
      : this._('available', manifest: m);
  const UpdateCheck.error(String m) : this._('error', message: m);

  final String status;
  final UpdateManifest? manifest;
  final String? message;

  bool get isAvailable => status == 'available';
}

class UpdateService {
  UpdateService(this._bridge);

  final VaultCoreBridge _bridge;

  /// 平台标记：与清单里的 `platform` 字段比对，避免把 macOS 包装到 Windows。
  static String get platformTag {
    if (Platform.isWindows) return 'windows-x64';
    if (Platform.isMacOS) return 'macos-universal';
    return 'linux-x64';
  }

  Future<UpdateCheck> check({
    required String manifestUrl,
    required String publicKeyHex,
    required String currentVersion,
  }) async {
    final url = manifestUrl.trim();
    final pk = publicKeyHex.trim();
    // fail-closed：缺任一项就不检查、更不更新
    if (url.isEmpty || pk.isEmpty) return const UpdateCheck.disabled();
    if (!url.startsWith('https://')) {
      return const UpdateCheck.error('更新清单必须使用 https');
    }

    final HttpClient client = HttpClient();
    try {
      final manifestBytes = await _get(client, url);
      final sigText = await _get(client, '$url.sig');
      final sigHex = utf8.decode(sigText).trim();

      // 验签：签名覆盖清单原始字节
      final json = _bridge.verifyUpdateManifest(
        utf8.decode(manifestBytes),
        sigHex,
        pk,
      );
      if (json == null || json['ok'] != true) {
        return const UpdateCheck.error('清单验签失败：来源不可信，已拒绝');
      }

      final manifest = UpdateManifest.tryParse(utf8.decode(manifestBytes));
      if (manifest == null) return const UpdateCheck.error('清单格式非法');
      if (manifest.platform != platformTag) {
        return UpdateCheck.error(
            '清单平台为 ${manifest.platform}，与本机 $platformTag 不符');
      }
      if (!_isNewer(manifest.version, currentVersion)) {
        return UpdateCheck.upToDate(currentVersion);
      }
      return UpdateCheck.available(manifest);
    } catch (e) {
      return UpdateCheck.error('检查更新失败：$e');
    } finally {
      client.close(force: true);
    }
  }

  /// 下载安装包 → sha256 复核 → 启动安装程序。返回 null 表示成功。
  Future<String?> downloadAndLaunch(UpdateManifest m) async {
    final HttpClient client = HttpClient();
    try {
      final dir = Directory(
          '${Directory.systemTemp.path}${Platform.pathSeparator}vaultsync-update');
      await dir.create(recursive: true);
      final name = m.url.split('/').last;
      final file = File('${dir.path}${Platform.pathSeparator}$name');

      final req = await client.getUrl(Uri.parse(m.url));
      final resp = await req.close();
      if (resp.statusCode != 200) {
        return '下载失败：HTTP ${resp.statusCode}';
      }
      final sink = file.openWrite();
      await resp.pipe(sink); // pipe 内部会 close sink

      final got = _bridge.fileSha256(file.path);
      if (got == null) return '无法计算安装包哈希';
      if (got.toLowerCase() != m.sha256) {
        // 哈希不符：删除并拒绝（可能是传输损坏或被替换）
        await file.delete().catchError((_) => file);
        return '安装包校验失败（sha256 不符），已拒绝安装';
      }

      if (Platform.isWindows) {
        await Process.start(file.path, const [],
            mode: ProcessStartMode.detached);
      } else {
        await Process.start('open', [file.path],
            mode: ProcessStartMode.detached);
      }
      return null;
    } catch (e) {
      return '更新失败：$e';
    } finally {
      client.close(force: true);
    }
  }

  static Future<List<int>> _get(HttpClient client, String url) async {
    final req = await client.getUrl(Uri.parse(url));
    final resp = await req.close();
    if (resp.statusCode != 200) {
      throw StateError('HTTP ${resp.statusCode} @ $url');
    }
    final bytes = <int>[];
    await for (final chunk in resp) {
      bytes.addAll(chunk);
    }
    return bytes;
  }

  /// 语义化版本比较（只比数字段；预发布后缀按"更低"处理）。
  static bool _isNewer(String remote, String local) {
    List<int> parse(String v) {
      final core = v.split('-').first.split('+').first;
      return core.split('.').map((s) => int.tryParse(s.trim()) ?? 0).toList();
    }

    final r = parse(remote);
    final l = parse(local);
    for (var i = 0; i < 3; i++) {
      final a = i < r.length ? r[i] : 0;
      final b = i < l.length ? l[i] : 0;
      if (a != b) return a > b;
    }
    return false;
  }
}

final updateServiceProvider = Provider<UpdateService>(
  (ref) => UpdateService(ref.watch(vaultCoreBridgeProvider)),
);
