import 'dart:convert';

import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/theme/design_tokens.dart';
import '../../core/theme/vs_motion.dart';
import '../../l10n/app_localizations.dart';
import '../../service/engine_service.dart';
import '../../state/notification_controller.dart';
import '../../state/session_controller.dart';
import '../models/vault_entry.dart';
import '../widgets/vs_ui.dart';

/// PNG 过滤（隐写只接受无损 PNG；JPG 等有损压缩会破坏 LSB）。
const _kPngGroup = XTypeGroup(label: 'PNG', extensions: ['png']);

/// 载荷长度前缀（native/vault-stego `LEN_PREFIX`）：4 字节大端长度。
const _kLenPrefix = 4;

/// AEAD 开销：AES-256-GCM 的 12 字节 nonce + 16 字节 tag。
const _kAeadOverhead = 28;

/// 载荷上限遍历的文件夹数（与 sync_controller 冲突扫描同口径）。
const _kMaxFolders = 256;

/// 下拉可列出的文件上限（避免超大保险箱把弹窗拖垮）。
const _kMaxFiles = 512;

/// P5-3 隐写术页：会话级开关 + 嵌入 / 提取两条链路 + 容量参考。
/// 视觉 1:1 对应原型 page-stego（.stego-tile 双卡 + 容量参考卡）。
///
/// 引擎语义（docs/05-05）：载荷 = AEAD(HKDF(MK,"stego-payload"), 长度前缀‖文件名‖明文)，
/// 再按位写入 PNG 的 RGB LSB —— 图片里是**密文**，提取必须持有同一保险箱的 MK。
class StegoPage extends ConsumerStatefulWidget {
  const StegoPage({super.key});

  @override
  ConsumerState<StegoPage> createState() => _StegoPageState();
}

class _StegoPageState extends ConsumerState<StegoPage> {
  bool _enabled = false;
  bool _loaded = false; // 首次状态读取完成前不给「启用」按钮可点
  bool _busy = false;

  @override
  void initState() {
    super.initState();
    Future.microtask(_loadStatus);
  }

  /// 当前会话句柄（仅解锁态可用）。
  Object? get _handle {
    final s = ref.read(sessionProvider);
    return s is SessionUnlocked ? s.sessionHandle : null;
  }

  void _notify(NotifGrade grade, String msg) =>
      ref.read(notificationProvider.notifier).add(grade, msg);

  Future<void> _loadStatus() async {
    final handle = _handle;
    if (handle == null) {
      if (mounted) setState(() => _loaded = true);
      return;
    }
    final on = await ref.read(vaultEngineProvider).stegoEnabled(handle);
    if (!mounted) return;
    setState(() {
      _enabled = on;
      _loaded = true;
    });
  }

  /// 开关为会话级（引擎已记审计）；失败保留原状态并如实提示。
  Future<void> _toggle(bool on) async {
    final l = AppLocalizations.of(context);
    final handle = _handle;
    if (handle == null) {
      _notify(NotifGrade.warning, l.stegoPageNeedUnlock);
      return;
    }
    setState(() => _busy = true);
    final err = await ref.read(vaultEngineProvider).stegoSetEnabled(handle, on);
    if (!mounted) return;
    setState(() {
      _busy = false;
      if (err == null) _enabled = on;
    });
    if (err != null) {
      _notify(NotifGrade.danger, l.stegoPageToggleFailed(err));
      return;
    }
    _notify(NotifGrade.ok, on ? l.stegoPageEnableDone : l.stegoPageDisableDone);
  }

  /// 打开嵌入弹窗。
  ///
  /// 弹窗页脚依赖弹窗内部状态（三项齐备 + 容量足够才可提交），
  /// 因此走「showDialog + VsModalScaffold」——与 security_page 的销毁弹窗同款写法；
  /// showVsModal 的 actions 只能由调用方在构建时给定，装不下这种联动。
  Future<void> _openEmbed() async {
    final l = AppLocalizations.of(context);
    final handle = _handle;
    if (handle == null) {
      _notify(NotifGrade.warning, l.stegoPageNeedUnlock);
      return;
    }
    await showDialog<void>(
      context: context,
      barrierColor: context.vs.backdrop,
      builder: (_) => _EmbedDialog(handle: handle),
    );
  }

  /// 打开提取弹窗（同嵌入弹窗的呈现方式）。
  Future<void> _openExtract() async {
    final l = AppLocalizations.of(context);
    final handle = _handle;
    if (handle == null) {
      _notify(NotifGrade.warning, l.stegoPageNeedUnlock);
      return;
    }
    await showDialog<void>(
      context: context,
      barrierColor: context.vs.backdrop,
      builder: (_) => _ExtractDialog(handle: handle),
    );
  }

  @override
  Widget build(BuildContext context) {
    final v = context.vs;
    final l = AppLocalizations.of(context);

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        VsPageHeader(
          title: l.stegoPageTitle,
          sub: l.stegoPageSub,
          actions: [
            VsBadge(
              _enabled ? l.stegoPageEnabled : l.stegoPageDisabled,
              tone: _enabled ? VsTone.enc : VsTone.mute,
            ),
          ],
        ),
        // 开关区：未启用给「默认关闭 + 审计」的警示与主按钮；启用后同位置给停用与双重保护说明
        if (!_enabled) ...[
          VsNote(l.stegoPageDisabledNote, tone: VsTone.warn),
          const SizedBox(height: DesignTokens.sp3),
          Align(
            alignment: Alignment.centerLeft,
            child: VsButton(
              label: l.stegoPageEnable,
              icon: 'stego',
              tone: VsBtnTone.primary,
              onPressed: (_loaded && !_busy) ? () => _toggle(true) : null,
            ),
          ),
        ] else ...[
          Align(
            alignment: Alignment.centerLeft,
            child: VsButton(
              label: l.stegoPageDisable,
              icon: 'x',
              tone: VsBtnTone.ghost,
              onPressed: _busy ? null : () => _toggle(false),
            ),
          ),
          const SizedBox(height: DesignTokens.sp3),
          VsNote(l.stegoPageEnabledNote, tone: VsTone.enc),
        ],
        const SizedBox(height: DesignTokens.sp4),
        // 两张入口卡（.stego-grid：1fr 1fr，两卡等高）
        // 页面处在滚动容器内（纵向无界），stretch 必须先由 IntrinsicHeight 定高。
        IntrinsicHeight(
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Expanded(
                child: _StegoTile(
                  icon: 'img',
                  title: l.stegoPageTileEmbed,
                  sub: l.stegoPageTileEmbedSub,
                  disabledHint: l.stegoPageTileLocked,
                  onTap: _enabled ? _openEmbed : null,
                ),
              ),
              const SizedBox(width: DesignTokens.sp3),
              Expanded(
                child: _StegoTile(
                  icon: 'search',
                  title: l.stegoPageTileExtract,
                  sub: l.stegoPageTileExtractSub,
                  disabledHint: l.stegoPageTileLocked,
                  onTap: _enabled ? _openExtract : null,
                ),
              ),
            ],
          ),
        ),
        const SizedBox(height: DesignTokens.sp4),
        // 容量参考卡：公式 + 三档实测数值（Dart 现算，与引擎 capacity_bytes 同式）
        VsCard(
          header: l.stegoPageCapacityCard,
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(l.stegoPageCapacityFormula,
                  style: TextStyle(
                      fontSize: 12.5,
                      fontFamily: DesignTokens.monoFamily,
                      color: v.text2)),
              const SizedBox(height: DesignTokens.sp3),
              VsKv.text(
                [
                  for (final (w, h) in _kCapacitySamples)
                    (
                      '$w×$h',
                      l.stegoPageSizeBytes(
                          _capacityOf(w, h), _fmtBytes(l, _capacityOf(w, h)))
                    ),
                ],
                labelW: 96,
                mono: true,
              ),
              const SizedBox(height: DesignTokens.sp2),
              Text(l.stegoPageCapacityLimit,
                  style: TextStyle(fontSize: 12, color: v.text3)),
              const SizedBox(height: DesignTokens.sp3),
              VsNote(l.stegoPageSafetyNote, tone: VsTone.info, icon: 'lock'),
            ],
          ),
        ),
      ],
    );
  }
}

/// 容量参考三档（原型 cap-wall 的档位；数值全部现算）。
const _kCapacitySamples = <(int, int)>[(1280, 720), (1920, 1080), (3840, 2160)];

/// 容量公式（docs/05-05 §3.2）：宽 × 高 × 3 ÷ 8 − 4；极小图片容量为 0。
int _capacityOf(int width, int height) {
  final cap = width * height * 3 ~/ 8 - _kLenPrefix;
  return cap < 0 ? 0 : cap;
}

/// 载荷字节数 = 4 字节长度前缀 + 文件名（UTF-8）+ 明文 + AEAD 开销。
int _payloadBytes(String name, int plainSize) =>
    _kLenPrefix + utf8.encode(name).length + plainSize + _kAeadOverhead;

/// 字节数 → 可读文本（1024 进制；单位文案走 ARB）。
String _fmtBytes(AppLocalizations l, int n) {
  if (n < 1024) return l.stegoPageSizeB(n);
  final kb = n / 1024;
  if (kb < 1024) return l.stegoPageSizeKb(kb.toStringAsFixed(1));
  return l.stegoPageSizeMb((kb / 1024).toStringAsFixed(2));
}

/// 单张入口卡（.stego-tile）：hover 上浮 2px + enc 描边；未启用时禁用并降透明度。
class _StegoTile extends StatefulWidget {
  const _StegoTile({
    required this.icon,
    required this.title,
    required this.sub,
    this.onTap,
    this.disabledHint,
  });

  final String icon;
  final String title;
  final String sub;
  final VoidCallback? onTap;
  final String? disabledHint;

  @override
  State<_StegoTile> createState() => _StegoTileState();
}

class _StegoTileState extends State<_StegoTile> {
  bool _hover = false;

  @override
  Widget build(BuildContext context) {
    final v = context.vs;
    final enabled = widget.onTap != null;
    final hot = _hover && enabled;
    final tile = MouseRegion(
      cursor: enabled ? SystemMouseCursors.click : MouseCursor.defer,
      onEnter: (_) => setState(() => _hover = true),
      onExit: (_) => setState(() => _hover = false),
      child: GestureDetector(
        onTap: widget.onTap,
        child: AnimatedContainer(
          duration: VsMotion.ms(context, 150),
          transform: Matrix4.translationValues(0, hot ? -2 : 0, 0),
          padding: const EdgeInsets.all(DesignTokens.sp4),
          decoration: BoxDecoration(
            color: v.surface,
            borderRadius: BorderRadius.circular(DesignTokens.rMd),
            border: Border.all(color: hot ? v.enc : v.borderSoft),
            boxShadow: hot ? v.shadow2 : null,
          ),
          child: Opacity(
            opacity: enabled ? 1 : .45,
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Container(
                  width: 42,
                  height: 42,
                  decoration: BoxDecoration(
                    color: v.encSoft,
                    borderRadius: BorderRadius.circular(12),
                  ),
                  child: Center(
                      child: AppIcon(widget.icon, size: 20, color: v.enc)),
                ),
                const SizedBox(height: 12),
                Text(widget.title,
                    style: TextStyle(
                        fontSize: 14.5,
                        fontWeight: FontWeight.w600,
                        color: v.text)),
                const SizedBox(height: 4),
                Text(widget.sub,
                    style: TextStyle(fontSize: 12.5, color: v.text2)),
              ],
            ),
          ),
        ),
      ),
    );
    final semantic = Semantics(
      button: true,
      enabled: enabled,
      label: widget.title,
      hint: enabled ? null : widget.disabledHint,
      child: tile,
    );
    final hint = widget.disabledHint;
    return (hint == null || enabled)
        ? semantic
        : Tooltip(message: hint, child: semantic);
  }
}

/// 保险箱文件项（含所在文件夹路径，供下拉展示）。
class _VaultFile {
  const _VaultFile({
    required this.id,
    required this.name,
    required this.size,
    required this.path,
  });

  final int id;
  final String name;
  final int size;
  final String path;
}

/// 从根目录 BFS 收集全部文件（同 sync_controller.refreshConflicts 的遍历口径）。
Future<List<_VaultFile>> _collectFiles(
    VaultEngine engine, Object handle) async {
  final out = <_VaultFile>[];
  final queue = <(int, String)>[(0, '')];
  final seen = <int>{0};
  while (queue.isNotEmpty && seen.length <= _kMaxFolders) {
    final (folder, path) = queue.removeAt(0);
    final json = await engine.list(handle, folder);
    if (json == null) continue;
    final listing = VaultListing.fromJson(json);
    for (final f in listing.files) {
      out.add(_VaultFile(id: f.id, name: f.name, size: f.size, path: path));
    }
    if (out.length >= _kMaxFiles) break;
    for (final sub in listing.folders) {
      if (seen.add(sub.id)) {
        queue.add((sub.id, path.isEmpty ? sub.name : '$path / ${sub.name}'));
      }
    }
  }
  return out;
}

/// 嵌入弹窗：选保险箱文件 → 选 PNG（读容量）→ 选输出位置 → 嵌入。
/// 未启用、格式不支持、容量不足三种情况都在提交前拦下（不让引擎必然失败）。
class _EmbedDialog extends ConsumerStatefulWidget {
  const _EmbedDialog({required this.handle});

  final Object handle;

  @override
  ConsumerState<_EmbedDialog> createState() => _EmbedDialogState();
}

class _EmbedDialogState extends ConsumerState<_EmbedDialog> {
  List<_VaultFile> _files = const [];
  bool _loadingFiles = true;
  int? _fileId;
  String? _imagePath;
  String? _outPath;
  int _capacity = -1; // <0 表示引擎错误码（-3 IO / -6 非受支持 PNG）
  bool _busy = false;
  String? _error;

  @override
  void initState() {
    super.initState();
    Future.microtask(_loadFiles);
  }

  Future<void> _loadFiles() async {
    final files =
        await _collectFiles(ref.read(vaultEngineProvider), widget.handle);
    if (!mounted) return;
    setState(() {
      _files = files;
      _loadingFiles = false;
    });
  }

  _VaultFile? get _selected {
    final id = _fileId;
    if (id == null) return null;
    for (final f in _files) {
      if (f.id == id) return f;
    }
    return null;
  }

  int get _need {
    final f = _selected;
    return f == null ? 0 : _payloadBytes(f.name, f.size);
  }

  /// 三项齐备且容量足够才允许提交。
  bool get _canSubmit {
    final f = _selected;
    return !_busy &&
        f != null &&
        _imagePath != null &&
        _outPath != null &&
        _capacity >= 0 &&
        _need <= _capacity;
  }

  Future<void> _pickImage() async {
    final file = await openFile(acceptedTypeGroups: const [_kPngGroup]);
    if (file == null) return;
    setState(() {
      _imagePath = file.path;
      _capacity = -1;
      _error = null;
    });
    final cap = await ref
        .read(vaultEngineProvider)
        .stegoCapacity(widget.handle, file.path);
    if (!mounted) return;
    setState(() => _capacity = cap);
  }

  Future<void> _pickOutput() async {
    final image = _imagePath;
    final base =
        image == null ? 'image.png' : image.split(RegExp(r'[\\/]')).last;
    final suggested = base.toLowerCase().endsWith('.png')
        ? '${base.substring(0, base.length - 4)}.stego.png'
        : '$base.stego.png';
    final loc = await getSaveLocation(suggestedName: suggested);
    if (loc == null) return;
    setState(() {
      _outPath = loc.path;
      _error = null;
    });
  }

  Future<void> _embed() async {
    final f = _selected;
    final image = _imagePath;
    final out = _outPath;
    if (f == null || image == null || out == null) return;
    final l = AppLocalizations.of(context);
    setState(() {
      _busy = true;
      _error = null;
    });
    final err = await ref
        .read(vaultEngineProvider)
        .stegoEmbed(widget.handle, f.id, image, out);
    if (!mounted) return;
    if (err != null) {
      // 引擎返回的已是可读文案（未启用 / 格式 / 容量），照实展示
      setState(() {
        _busy = false;
        _error = err;
      });
      return;
    }
    ref.read(notificationProvider.notifier).ok(l.stegoPageEmbedDone(out));
    Navigator.of(context).pop();
  }

  @override
  Widget build(BuildContext context) {
    final v = context.vs;
    final l = AppLocalizations.of(context);
    final selected = _selected;
    final need = _need;

    return VsModalScaffold(
      title: l.stegoPageEmbedTitle,
      sub: l.stegoPageEmbedSub,
      icon: 'img',
      micTone: VsTone.enc,
      width: 520,
      actions: (ctx) => [
        VsButton(
            label: l.stegoPageCancel,
            tone: VsBtnTone.ghost,
            onPressed: _busy ? null : () => Navigator.of(ctx).pop()),
        VsButton(
            label: _busy ? l.stegoPageEmbedding : l.stegoPageEmbedAction,
            tone: VsBtnTone.primary,
            onPressed: _canSubmit ? _embed : null),
      ],
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          _fieldLabel(l.stegoPagePickFile),
          if (_loadingFiles)
            Text(l.stegoPageFileLoading,
                style: TextStyle(fontSize: 12.5, color: v.text3))
          else if (_files.isEmpty)
            Text(l.stegoPageFileEmpty,
                style: TextStyle(fontSize: 12.5, color: v.text3))
          else
            DropdownButtonFormField<int>(
              initialValue: _fileId,
              isExpanded: true,
              menuMaxHeight: 280,
              style: TextStyle(fontSize: 13, color: v.text),
              dropdownColor: v.surface,
              decoration: const InputDecoration(),
              hint: Text(l.stegoPagePickFileHint,
                  style: TextStyle(fontSize: 13, color: v.text3)),
              items: [
                for (final f in _files)
                  DropdownMenuItem(
                    value: f.id,
                    child: Text(
                      l.stegoPageFileOption(
                        f.name,
                        _fmtBytes(l, f.size),
                        f.path.isEmpty ? l.stegoPageFileRoot : f.path,
                      ),
                      overflow: TextOverflow.ellipsis,
                    ),
                  ),
              ],
              onChanged: (val) => setState(() {
                _fileId = val;
                _error = null;
              }),
            ),
          if (selected != null) ...[
            const SizedBox(height: 6),
            Text(l.stegoPagePlainSize(_fmtBytes(l, selected.size)),
                style: TextStyle(
                    fontSize: 11.5,
                    fontFamily: DesignTokens.monoFamily,
                    color: v.text3)),
          ],
          _fieldLabel(l.stegoPagePickImage),
          _pickRow(
            v,
            l,
            path: _imagePath,
            label: l.stegoPagePickImage,
            onPick: _pickImage,
          ),
          if (_imagePath != null && _capacity < 0) ...[
            const SizedBox(height: 6),
            Text(
              _capacity == -3
                  ? l.stegoPageCapacityIo
                  : l.stegoPageCapacityFormat,
              style: TextStyle(fontSize: 12, color: v.danger),
            ),
          ],
          if (selected != null && _capacity >= 0) ...[
            const SizedBox(height: 6),
            Text(
              l.stegoPageCapacityLine(
                  _fmtBytes(l, _capacity), _fmtBytes(l, need)),
              style: TextStyle(
                  fontSize: 11.5,
                  fontFamily: DesignTokens.monoFamily,
                  color: v.text2),
            ),
            if (need > _capacity) ...[
              const SizedBox(height: 4),
              Text(
                l.stegoPageCapacityShort(_fmtBytes(l, need - _capacity)),
                style: TextStyle(fontSize: 12, color: v.danger),
              ),
            ],
          ],
          _fieldLabel(l.stegoPagePickOutput),
          _pickRow(
            v,
            l,
            path: _outPath,
            label: l.stegoPagePickOutput,
            onPick: _pickOutput,
          ),
          const SizedBox(height: 14),
          VsNote(l.stegoPageEmbedNote, tone: VsTone.info, icon: 'lock'),
          if (_error != null) ...[
            const SizedBox(height: 12),
            Text(_error!, style: TextStyle(fontSize: 12.5, color: v.danger)),
          ],
        ],
      ),
    );
  }

  /// 路径行：已选路径（mono，过长省略）+ 选择按钮。
  Widget _pickRow(
    VsScheme v,
    AppLocalizations l, {
    required String? path,
    required String label,
    required VoidCallback onPick,
  }) {
    return Row(
      children: [
        Expanded(
          child: Text(
            path ?? l.stegoPageNotChosen,
            style: TextStyle(
              fontSize: 11.5,
              fontFamily: DesignTokens.monoFamily,
              color: path == null ? v.text3 : v.text,
            ),
            overflow: TextOverflow.ellipsis,
          ),
        ),
        const SizedBox(width: 10),
        VsButton(label: label, small: true, onPressed: onPick),
      ],
    );
  }

  Widget _fieldLabel(String text) => Padding(
        padding: const EdgeInsets.only(top: 12, bottom: 6),
        child: Text(text,
            style:
                const TextStyle(fontSize: 12.5, fontWeight: FontWeight.w500)),
      );
}

/// 提取弹窗：选携带数据的 PNG → 选还原位置 → 提取。
/// 引擎失败只返回 null（不区分原因），文案按「无载荷 / 格式 / 换库」三种可能如实说明。
class _ExtractDialog extends ConsumerStatefulWidget {
  const _ExtractDialog({required this.handle});

  final Object handle;

  @override
  ConsumerState<_ExtractDialog> createState() => _ExtractDialogState();
}

class _ExtractDialogState extends ConsumerState<_ExtractDialog> {
  String? _imagePath;
  String? _destPath;
  bool _busy = false;
  String? _error;

  bool get _canSubmit => !_busy && _imagePath != null && _destPath != null;

  Future<void> _pickImage() async {
    final file = await openFile(acceptedTypeGroups: const [_kPngGroup]);
    if (file == null) return;
    setState(() {
      _imagePath = file.path;
      _error = null;
    });
  }

  Future<void> _pickDest() async {
    final loc = await getSaveLocation(suggestedName: 'restored.bin');
    if (loc == null) return;
    setState(() {
      _destPath = loc.path;
      _error = null;
    });
  }

  Future<void> _extract() async {
    final image = _imagePath;
    final dest = _destPath;
    if (image == null || dest == null) return;
    final l = AppLocalizations.of(context);
    setState(() {
      _busy = true;
      _error = null;
    });
    final result = await ref
        .read(vaultEngineProvider)
        .stegoExtract(widget.handle, image, dest);
    if (!mounted) return;
    if (result == null) {
      setState(() {
        _busy = false;
        _error = l.stegoPageExtractFailed;
      });
      ref.read(notificationProvider.notifier).danger(l.stegoPageExtractFailed);
      return;
    }
    final name = (result['name'] as String?) ?? '';
    final size = (result['size'] as num?)?.toInt() ?? 0;
    ref
        .read(notificationProvider.notifier)
        .ok(l.stegoPageExtractDone(name, _fmtBytes(l, size)));
    Navigator.of(context).pop();
  }

  @override
  Widget build(BuildContext context) {
    final v = context.vs;
    final l = AppLocalizations.of(context);

    return VsModalScaffold(
      title: l.stegoPageExtractTitle,
      sub: l.stegoPageExtractSub,
      icon: 'search',
      micTone: VsTone.enc,
      width: 520,
      actions: (ctx) => [
        VsButton(
            label: l.stegoPageCancel,
            tone: VsBtnTone.ghost,
            onPressed: _busy ? null : () => Navigator.of(ctx).pop()),
        VsButton(
            label: _busy ? l.stegoPageExtracting : l.stegoPageExtractAction,
            tone: VsBtnTone.primary,
            onPressed: _canSubmit ? _extract : null),
      ],
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          _fieldLabel(l.stegoPagePickStegoImage),
          _pickRow(
            v,
            l,
            path: _imagePath,
            label: l.stegoPagePickStegoImage,
            onPick: _pickImage,
          ),
          _fieldLabel(l.stegoPagePickRestore),
          _pickRow(
            v,
            l,
            path: _destPath,
            label: l.stegoPagePickRestore,
            onPick: _pickDest,
          ),
          const SizedBox(height: 14),
          VsNote(l.stegoPageExtractNote, tone: VsTone.enc, icon: 'lock'),
          if (_error != null) ...[
            const SizedBox(height: 12),
            Text(_error!, style: TextStyle(fontSize: 12.5, color: v.danger)),
          ],
        ],
      ),
    );
  }

  Widget _pickRow(
    VsScheme v,
    AppLocalizations l, {
    required String? path,
    required String label,
    required VoidCallback onPick,
  }) {
    return Row(
      children: [
        Expanded(
          child: Text(
            path ?? l.stegoPageNotChosen,
            style: TextStyle(
              fontSize: 11.5,
              fontFamily: DesignTokens.monoFamily,
              color: path == null ? v.text3 : v.text,
            ),
            overflow: TextOverflow.ellipsis,
          ),
        ),
        const SizedBox(width: 10),
        VsButton(label: label, small: true, onPressed: onPick),
      ],
    );
  }

  Widget _fieldLabel(String text) => Padding(
        padding: const EdgeInsets.only(top: 12, bottom: 6),
        child: Text(text,
            style:
                const TextStyle(fontSize: 12.5, fontWeight: FontWeight.w500)),
      );
}
