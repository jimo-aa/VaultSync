import 'dart:io';
import 'dart:math' as math;

import 'package:desktop_drop/desktop_drop.dart';
import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/theme/design_tokens.dart';
import '../../l10n/app_localizations.dart';
import '../../service/engine_service.dart';
import '../../state/notification_controller.dart';
import '../../state/session_controller.dart';
import '../../state/vault_controller.dart';
import '../models/vault_entry.dart';
import '../shell/home_shell.dart';
import '../widgets/import_dialog.dart';
import '../widgets/vs_ui.dart';

/// 保险箱页（纯展示）：全部业务状态在 [vaultUiProvider]。
/// 选中走卡片本地 Listener（即时）；根节点点击空白清除选中（用 guard 抑制冒泡）。
class VaultPage extends ConsumerStatefulWidget {
  const VaultPage({super.key});

  @override
  ConsumerState<VaultPage> createState() => _VaultPageState();
}

class _VaultPageState extends ConsumerState<VaultPage> {
  /// 卡片 onPointerUp 已处理选中时置位，抑制随后冒泡到根部的「清除选中」。
  bool _tapGuard = false;

  /// 导入弹窗开启中：页面级放置区让位给弹窗内放置区（见 `_import`）。
  bool _importOpen = false;

  @override
  void initState() {
    super.initState();
    Future.microtask(() => ref.read(vaultUiProvider.notifier).refresh());
  }

  Object? get _handle {
    final s = ref.read(sessionProvider);
    return s is SessionUnlocked ? s.sessionHandle : null;
  }

  void _notify(NotifGrade grade, String msg) =>
      ref.read(notificationProvider.notifier).add(grade, msg);

  VaultUiController get _ctl => ref.read(vaultUiProvider.notifier);

  // ---- 操作（引擎调用，结果经 controller 刷新） ----

  /// 逐文件导入：目录跳过（引擎只接受文件），失败按文件提示，成功数汇总成一条通知。
  Future<void> _importFiles(List<String> paths) async {
    final handle = _handle;
    if (handle == null || paths.isEmpty) return;
    final l = AppLocalizations.of(context);
    final engine = ref.read(vaultEngineProvider);
    final folder = ref.read(vaultUiProvider).currentFolder;
    var done = 0;
    final failed = <String>[];
    for (final p in paths) {
      final name = _baseName(p);
      if (FileSystemEntity.isDirectorySync(p)) {
        failed.add(l.vaultPageImportFolderUnsupported(name));
        continue;
      }
      final (err, _) = await engine.importFile(handle, p, folder);
      if (err != null) {
        failed.add(l.vaultPageImportFileError(name, err));
      } else {
        done++;
      }
    }
    if (done > 0) _notify(NotifGrade.ok, l.vaultPageImportedCount(done));
    for (final f in failed) {
      _notify(NotifGrade.danger, l.vaultPageImportFailed(f));
    }
    await _ctl.refresh();
  }

  /// 工具栏 / 空态入口：打开导入弹窗（拖放 + 多选，不限类型）。
  /// 弹窗打开期间禁用页面级放置区——desktop_drop 会把拖放事件广播给所有命中的
  /// DropTarget（含被弹窗盖住的那个），不禁用会导致同一批文件被导入两次。
  Future<void> _import() async {
    setState(() => _importOpen = true);
    final paths = await showImportDialog(context);
    if (mounted) setState(() => _importOpen = false);
    if (paths == null || paths.isEmpty) return;
    await _importFiles(paths);
  }

  Future<void> _export(VaultEntry f) async {
    final l = AppLocalizations.of(context);
    final location = await getSaveLocation(suggestedName: f.name);
    if (location == null) return;
    final err = await ref
        .read(vaultEngineProvider)
        .exportFile(_handle!, f.id, location.path);
    _notify(err == null ? NotifGrade.ok : NotifGrade.danger,
        err ?? l.vaultPageExportedTo(location.path));
  }

  /// 批量导出（右键菜单「导出 N 个文件」）：选目标目录后逐个解密输出。
  Future<void> _exportMany(List<VaultEntry> entries) async {
    final l = AppLocalizations.of(context);
    final files = entries.where((e) => !e.isFolder).toList();
    if (files.isEmpty) {
      _notify(NotifGrade.warning, l.vaultPageExportNoneSelected);
      return;
    }
    final dir =
        await getDirectoryPath(confirmButtonText: l.vaultPageExportToHere);
    if (dir == null) return;
    final engine = ref.read(vaultEngineProvider);
    var done = 0;
    for (final f in files) {
      final err = await engine.exportFile(
          _handle!,
          f.id,
          '$dir${Platform.pathSeparator}'
          '${f.name.isEmpty ? l.vaultPageUnnamedWithId(f.id) : f.name}');
      if (err == null) {
        done++;
      } else {
        _notify(NotifGrade.danger, l.vaultPageExportFailedItem(f.name, err));
      }
    }
    _notify(done > 0 ? NotifGrade.ok : NotifGrade.danger,
        l.vaultPageExportedMany(done, files.length, dir));
  }

  static String _baseName(String path) => path.split(RegExp(r'[\\/]')).last;

  Future<void> _delete(VaultEntry f) async {
    final l = AppLocalizations.of(context);
    final ok = await showVsModal<bool>(
      context: context,
      title: l.vaultPageWipe,
      sub: f.isFolder ? l.vaultPageWipeFolderSub : l.vaultPageWipeFileSub,
      icon: 'trash',
      micTone: VsTone.danger,
      body: (ctx) => Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          VsKv.text([
            (
              f.isFolder ? l.vaultPageTargetFolder : l.vaultPageTargetFile,
              f.name.isEmpty ? l.vaultPageUnnamed : f.name
            ),
            if (!f.isFolder) (l.vaultPageSizeLabel, fmtSize(f.size)),
          ]),
          VsNote(l.vaultPageWipeNote, tone: VsTone.danger),
        ],
      ),
      actions: (ctx) => [
        VsButton(
            label: l.vaultPageCancel,
            tone: VsBtnTone.ghost,
            onPressed: () => Navigator.pop(ctx, false)),
        VsButton(
            label: l.vaultPageWipeConfirm,
            tone: VsBtnTone.danger,
            onPressed: () => Navigator.pop(ctx, true)),
      ],
    );
    if (ok != true) return;
    final err = await _wipe(f);
    _notify(err == null ? NotifGrade.ok : NotifGrade.danger,
        err ?? l.vaultPageWiped);
    await _ctl.refresh();
  }

  /// 擦除单个条目：文件夹走递归删除（返回删除的文件数），文件走单文件擦除。
  Future<String?> _wipe(VaultEntry e) async {
    final l = AppLocalizations.of(context);
    final engine = ref.read(vaultEngineProvider);
    if (!e.isFolder) return engine.deleteFile(_handle!, e.id);
    final (err, n) = await engine.deleteFolder(_handle!, e.id);
    return err == null ? null : l.vaultPageWipeFolderFailed(e.name, err, n);
  }

  Future<void> _deleteMany(Iterable<VaultEntry> entries) async {
    final l = AppLocalizations.of(context);
    final items = entries.toList();
    final n = items.length;
    final folders = items.where((e) => e.isFolder).length;
    final ok = await showVsModal<bool>(
      context: context,
      title: l.vaultPageWipeManyTitle,
      sub: folders > 0
          ? l.vaultPageWipeManyFoldersSub(folders)
          : l.vaultPageWipeManyFilesSub(n),
      icon: 'trash',
      micTone: VsTone.danger,
      body: (ctx) => VsNote(l.vaultPageWipeManyNote(n), tone: VsTone.danger),
      actions: (ctx) => [
        VsButton(
            label: l.vaultPageCancel,
            tone: VsBtnTone.ghost,
            onPressed: () => Navigator.pop(ctx, false)),
        VsButton(
            label: l.vaultPageWipeAll,
            tone: VsBtnTone.danger,
            onPressed: () => Navigator.pop(ctx, true)),
      ],
    );
    if (ok != true) return;
    var failed = 0;
    for (final e in items) {
      if (await _wipe(e) != null) failed++;
    }
    _notify(
        failed == 0 ? NotifGrade.ok : NotifGrade.danger,
        failed == 0
            ? l.vaultPageWipeManyDone(n)
            : l.vaultPageWipeManyFailed(failed, n));
    _ctl.clearSelection();
    await _ctl.refresh();
  }

  Future<void> _mkdir() async {
    final l = AppLocalizations.of(context);
    final ctl = TextEditingController();
    final name = await showVsModal<String>(
      context: context,
      title: l.vaultPageMkdirTitle,
      sub: l.vaultPageMkdirSub,
      icon: 'folder',
      micTone: VsTone.gold,
      width: 400,
      body: (ctx) => Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(l.vaultPageNameLabel,
              style:
                  const TextStyle(fontSize: 12.5, fontWeight: FontWeight.w500)),
          const SizedBox(height: 6),
          TextField(
            controller: ctl,
            autofocus: true,
            decoration: InputDecoration(hintText: l.vaultPageMkdirHint),
          ),
        ],
      ),
      actions: (ctx) => [
        VsButton(
            label: l.vaultPageCancel,
            tone: VsBtnTone.ghost,
            onPressed: () => Navigator.pop(ctx)),
        VsButton(
            label: l.vaultPageCreate,
            tone: VsBtnTone.primary,
            onPressed: () => Navigator.pop(ctx, ctl.text.trim())),
      ],
    );
    if (name == null || name.isEmpty) return;
    final (err, _) = await ref
        .read(vaultEngineProvider)
        .mkdir(_handle!, ref.read(vaultUiProvider).currentFolder, name);
    _notify(NotifGrade.ok, err ?? l.vaultPageCreatedFolder(name));
    await _ctl.refresh();
  }

  Future<void> _rename(VaultEntry f) async {
    final l = AppLocalizations.of(context);
    final ctl = TextEditingController(text: f.name);
    final name = await showVsModal<String>(
      context: context,
      title: l.vaultPageRename,
      sub: f.isFolder ? l.vaultPageRenameFolderSub : l.vaultPageRenameFileSub,
      icon: 'edit',
      width: 400,
      body: (ctx) => TextField(controller: ctl, autofocus: true),
      actions: (ctx) => [
        VsButton(
            label: l.vaultPageCancel,
            tone: VsBtnTone.ghost,
            onPressed: () => Navigator.pop(ctx)),
        VsButton(
            label: l.vaultPageSave,
            tone: VsBtnTone.primary,
            onPressed: () => Navigator.pop(ctx, ctl.text.trim())),
      ],
    );
    if (name == null || name.isEmpty || name == f.name) return;
    final engine = ref.read(vaultEngineProvider);
    final err = f.isFolder
        ? await engine.renameFolder(_handle!, f.id, name)
        : await engine.renameFile(_handle!, f.id, name);
    _notify(err == null ? NotifGrade.ok : NotifGrade.danger,
        err ?? l.vaultPageRenamed);
    await _ctl.refresh();
  }

  Future<void> _setTags(VaultEntry f) async {
    final l = AppLocalizations.of(context);
    final ctl = TextEditingController(text: f.tags.join(','));
    final tagsStr = await showVsModal<String>(
      context: context,
      title: l.vaultPageTagsTitle,
      sub: l.vaultPageTagsSub(f.name.isEmpty ? l.vaultPageUnnamed : f.name),
      icon: 'tag',
      micTone: VsTone.enc,
      width: 400,
      body: (ctx) => Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Wrap(
            spacing: 6,
            runSpacing: 6,
            children: [for (final t in f.tags) VsTagChip('#$t', trailing: '×')],
          ),
          const SizedBox(height: 14),
          TextField(
            controller: ctl,
            decoration: InputDecoration(hintText: l.vaultPageTagsHint),
          ),
        ],
      ),
      actions: (ctx) => [
        VsButton(
            label: l.vaultPageCancel,
            tone: VsBtnTone.ghost,
            onPressed: () => Navigator.pop(ctx)),
        VsButton(
            label: l.vaultPageSave,
            tone: VsBtnTone.primary,
            onPressed: () => Navigator.pop(ctx, ctl.text.trim())),
      ],
    );
    if (tagsStr == null) return;
    final tags = tagsStr
        .split(',')
        .map((s) => s.trim())
        .where((s) => s.isNotEmpty)
        .toList();
    final err =
        await ref.read(vaultEngineProvider).setTags(_handle!, f.id, tags);
    _notify(err == null ? NotifGrade.ok : NotifGrade.danger,
        err ?? l.vaultPageTagsUpdated);
    await _ctl.refresh();
  }

  /// 预览（原型 `#ov-preview`）：大画面占位 + 说明。
  /// 引擎不提供明文预览通道（明文不出引擎层），故此处只展示密文缩略图与提示。
  Future<void> _preview(VaultEntry f) async {
    if (!mounted) return;
    final l = AppLocalizations.of(context);
    final v = context.vs;
    await showVsModal<void>(
      context: context,
      title:
          l.vaultPagePreviewTitle(f.name.isEmpty ? l.vaultPageUnnamed : f.name),
      sub: l.vaultPagePreviewSub(fmtSize(f.size)),
      icon: 'eye',
      micTone: _micTone(f),
      width: 560,
      body: (ctx) => Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Container(
            height: 320,
            width: double.infinity,
            decoration: BoxDecoration(
              borderRadius: BorderRadius.circular(DesignTokens.rMd),
              gradient: _thumbGradient(v, f),
              color: _thumbGradient(v, f) == null ? v.surfaceAlt : null,
            ),
            child: Column(
              mainAxisAlignment: MainAxisAlignment.center,
              children: [
                AppIcon(_iconFor(f), size: 52, color: v.enc),
                const SizedBox(height: 10),
                Text(l.vaultPageCipherThumb,
                    style: TextStyle(
                        fontSize: 12.5, color: v.enc.withValues(alpha: .85))),
              ],
            ),
          ),
          const SizedBox(height: DesignTokens.sp4),
          VsNote(l.vaultPagePreviewNote, tone: VsTone.info),
        ],
      ),
      actions: (ctx) => [
        VsButton(
            label: l.vaultPageClose,
            tone: VsBtnTone.ghost,
            onPressed: () => Navigator.pop(ctx)),
        VsButton(
            label: l.vaultPageExport,
            icon: 'down',
            tone: VsBtnTone.primary,
            onPressed: () {
              Navigator.pop(ctx);
              _export(f);
            }),
      ],
    );
  }

  Future<void> _share(VaultEntry f) async {
    final l = AppLocalizations.of(context);
    final result = await ref
        .read(vaultEngineProvider)
        .shareCreate(_handle!, f.id, 3600, 1);
    if (result == null) {
      _notify(NotifGrade.danger, l.vaultPageShareFailed);
      return;
    }
    if (!mounted) return;
    await showVsModal<void>(
      context: context,
      title: l.vaultPageShare,
      sub: l.vaultPageShareSub,
      icon: 'flame',
      micTone: VsTone.danger,
      body: (ctx) => Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          VsKv.text([
            (
              l.vaultPageFileLabel,
              f.name.isEmpty ? l.vaultPageUnnamed : f.name
            ),
            (l.vaultPageSizeLabel, fmtSize(f.size)),
          ]),
          Text(l.vaultPageShareTokenLabel,
              style:
                  const TextStyle(fontSize: 12.5, fontWeight: FontWeight.w500)),
          const SizedBox(height: 6),
          Container(
            width: double.infinity,
            padding: const EdgeInsets.all(12),
            decoration: BoxDecoration(
              color: context.vs.bg,
              borderRadius: BorderRadius.circular(8),
              border: Border.all(color: context.vs.border),
            ),
            child: SelectableText(
              l.vaultPageShareIdAndToken(
                  result['shareId'].toString(), result['token'].toString()),
              style: const TextStyle(
                  fontSize: 12, fontFamily: DesignTokens.monoFamily),
            ),
          ),
          const SizedBox(height: 12),
          VsNote(l.vaultPageShareNote, tone: VsTone.info),
        ],
      ),
      actions: (ctx) => [
        VsButton(
            label: l.vaultPageDone,
            tone: VsBtnTone.primary,
            onPressed: () => Navigator.pop(ctx)),
      ],
    );
  }

  Future<void> _openDetail(VaultEntry f) async {
    if (f.isFolder) {
      await _ctl.openFolder(f.id, f.name);
      return;
    }
    if (!mounted) return;
    final l = AppLocalizations.of(context);
    final v = context.vs;
    await showVsModal<void>(
      context: context,
      title: f.name.isEmpty ? l.vaultPageUnnamedFile : f.name,
      sub: l.vaultPageDetailSub(fmtSize(f.size), _kindName(f)),
      icon: _iconFor(f),
      micTone: _micTone(f),
      width: 560,
      body: (ctx) => Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Container(
            height: 130,
            width: double.infinity,
            decoration: BoxDecoration(
              borderRadius: BorderRadius.circular(DesignTokens.rMd),
              gradient: _thumbGradient(v, f),
            ),
            child: Column(
              mainAxisAlignment: MainAxisAlignment.center,
              children: [
                AppIcon(_iconFor(f), size: 38, color: v.enc),
                const SizedBox(height: 8),
                Text(l.vaultPageDetailThumb,
                    style: TextStyle(
                        fontSize: 12, color: v.enc.withValues(alpha: .8))),
              ],
            ),
          ),
          const SizedBox(height: 16),
          VsKv.text([
            (l.vaultPageTypeLabel, _kindName(f)),
            (l.vaultPageSizeLabel, fmtSize(f.size)),
            (
              l.vaultPageTags,
              f.tags.isEmpty
                  ? l.vaultPageNoTags
                  : f.tags.map((t) => '#$t').join(' ')
            ),
          ]),
          Text(l.vaultPageEncInfo,
              style: const TextStyle(
                  fontSize: 11.5,
                  fontWeight: FontWeight.w600,
                  color: Color(0xFF5E6980))),
          const SizedBox(height: 8),
          VsKv.text([
            (l.vaultPageAlgoLabel, l.vaultPageAlgoValue),
            (l.vaultPageFsKeyLabel, l.vaultPageFsKeyValue),
          ], mono: true),
        ],
      ),
      actions: (ctx) => [
        VsButton(
            label: l.vaultPageExport,
            icon: 'down',
            tone: VsBtnTone.primary,
            small: true,
            onPressed: () {
              Navigator.pop(ctx);
              _export(f);
            }),
        VsButton(
            label: l.vaultPageBurn,
            icon: 'flame',
            tone: VsBtnTone.dangerGhost,
            small: true,
            onPressed: () {
              Navigator.pop(ctx);
              _share(f);
            }),
        VsButton(
            label: l.vaultPageRename,
            icon: 'edit',
            tone: VsBtnTone.ghost,
            small: true,
            onPressed: () {
              Navigator.pop(ctx);
              _rename(f);
            }),
        VsButton(
            label: l.vaultPageTags,
            icon: 'tag',
            tone: VsBtnTone.ghost,
            small: true,
            onPressed: () {
              Navigator.pop(ctx);
              _setTags(f);
            }),
        VsButton(
            label: l.vaultPageWipe,
            icon: 'trash',
            tone: VsBtnTone.dangerGhost,
            small: true,
            onPressed: () {
              Navigator.pop(ctx);
              _delete(f);
            }),
      ],
    );
  }

  // ---- 右键菜单 ----
  // 1:1 对应原型 openCtxMenu：单选文件 / 单选文件夹 / 多选 三套菜单，
  // 表头为条目名或「已选 N 项」，出现位置为右键点击处（超出视口自动回收）。

  Future<void> _onSecondaryTap(VaultEntry e, Offset position) async {
    final l = AppLocalizations.of(context);
    if (!ref.read(vaultUiProvider).selected.contains(e.id)) _select(e);
    final state = ref.read(vaultUiProvider);
    final picked = state.listing.entries
        .where((x) => state.selected.contains(x.id))
        .toList();
    final n = state.selected.length;

    if (n > 1) {
      await showVsCtxMenu(context, position, l.vaultPageSelectedCount(n), [
        VsCtxEntry(
            l.vaultPageExportCount(n), 'down', () => _exportMany(picked)),
        const VsCtxEntry.sep(),
        VsCtxEntry(l.vaultPageWipeCount(n), 'trash', () => _deleteMany(picked),
            danger: true),
      ]);
      return;
    }

    await showVsCtxMenu(context, position, e.name, [
      VsCtxEntry(l.vaultPageOpen, e.isFolder ? 'folder' : 'file',
          () => _openDetail(e)),
      if (!e.isFolder) ...[
        VsCtxEntry(l.vaultPagePreview, 'eye', () => _preview(e)),
        const VsCtxEntry.sep(),
        VsCtxEntry(l.vaultPageExport, 'down', () => _export(e)),
        VsCtxEntry(l.vaultPageShare, 'flame', () => _share(e)),
        const VsCtxEntry.sep(),
      ],
      VsCtxEntry(l.vaultPageRename, 'edit', () => _rename(e)),
      VsCtxEntry(l.vaultPageTags, 'tag', () => _setTags(e)),
      const VsCtxEntry.sep(),
      VsCtxEntry(l.vaultPageWipe, 'trash', () => _delete(e), danger: true),
    ]);
  }

  // ---- 选择（即时；根节点清除用 guard 抑制冒泡） ----

  void _select(VaultEntry e) {
    _tapGuard = true;
    final kb = HardwareKeyboard.instance;
    final ctrl =
        kb.logicalKeysPressed.contains(LogicalKeyboardKey.controlLeft) ||
            kb.logicalKeysPressed.contains(LogicalKeyboardKey.controlRight);
    final shift =
        kb.logicalKeysPressed.contains(LogicalKeyboardKey.shiftLeft) ||
            kb.logicalKeysPressed.contains(LogicalKeyboardKey.shiftRight);
    _ctl.select(e, ctrl: ctrl, shift: shift);
  }

  void _clearSelection() {
    if (_tapGuard) {
      _tapGuard = false; // 这次点击命中了条目，不清除
      return;
    }
    _ctl.clearSelection();
  }

  // ---- 工具 ----

  static String fmtSize(int n) {
    if (n < 1024) return '$n B';
    if (n < 1024 * 1024) return '${(n / 1024).toStringAsFixed(1)} KB';
    if (n < 1024 * 1024 * 1024) {
      return '${(n / 1024 / 1024).toStringAsFixed(1)} MB';
    }
    return '${(n / 1024 / 1024 / 1024).toStringAsFixed(1)} GB';
  }

  String _kindName(VaultEntry e) {
    final l = AppLocalizations.of(context);
    if (e.isFolder) return l.vaultPageKindFolder;
    return switch (_iconFor(e)) {
      'img' => l.vaultPageKindImage,
      'zip' => l.vaultPageKindArchive,
      'pdf' || 'txt' => l.vaultPageKindDoc,
      'video' => l.vaultPageKindVideo,
      _ => l.vaultPageKindFile,
    };
  }

  VsTone _micTone(VaultEntry e) => switch (_iconFor(e)) {
        'img' => VsTone.enc,
        'zip' => VsTone.gold,
        _ => VsTone.info,
      };

  String _iconFor(VaultEntry e) {
    if (e.isFolder) return 'folder';
    final ext =
        e.name.contains('.') ? e.name.split('.').last.toLowerCase() : '';
    if (['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg'].contains(ext)) {
      return 'img';
    }
    if (['zip', '7z', 'rar', 'tar', 'gz'].contains(ext)) return 'zip';
    if (['pdf'].contains(ext)) return 'pdf';
    if (['doc', 'docx', 'md', 'txt'].contains(ext)) return 'txt';
    if (['mp4', 'mkv', 'mov', 'avi'].contains(ext)) return 'video';
    return 'file';
  }

  Gradient? _thumbGradient(VsScheme v, VaultEntry e) => switch (_iconFor(e)) {
        'img' => const LinearGradient(
            begin: Alignment.topLeft,
            end: Alignment.bottomRight,
            colors: [Color(0xFF2A3145), Color(0xFF3A3355), Color(0xFF443B62)]),
        'zip' => const LinearGradient(
            begin: Alignment.topLeft,
            end: Alignment.bottomRight,
            colors: [Color(0xFF2A2A38), Color(0xFF3A3448)]),
        'folder' => null,
        _ => const LinearGradient(
            begin: Alignment.topLeft,
            end: Alignment.bottomRight,
            colors: [Color(0xFF20283A), Color(0xFF2A3450)]),
      };

  Color _kindColor(String ic) => switch (ic) {
        'img' => context.vs.enc,
        'zip' => context.vs.gold,
        'folder' => context.vs.text3,
        _ => context.vs.blue,
      };

  @override
  Widget build(BuildContext context) {
    final l = AppLocalizations.of(context);
    final v = context.vs;
    final state = ref.watch(vaultUiProvider);

    // 消费全局搜索（顶栏 Ctrl K 输入）
    final globalQ = ref.watch(globalSearchProvider);
    if (globalQ.isNotEmpty && globalQ != state.searchQuery) {
      WidgetsBinding.instance
          .addPostFrameCallback((_) => _ctl.runSearch(globalQ));
    }

    // 内容不足一屏时补足高度：根部手势必须覆盖条目下方的空白区，
    // 否则点击卡片下方无法取消选中（只有卡片同行右侧能点）。
    final minH = math.max(
        0.0,
        MediaQuery.heightOf(context) -
            DesignTokens.topbarH -
            DesignTokens.sp5 * 2 -
            1);

    return Listener(
      // 每次按下先复位守卫：右键选中后守卫会残留到下一次点击，若不复位，
      // 紧随其后的「点空白取消选中」会被上一次的守卫吞掉（实测复现）。
      behavior: HitTestBehavior.translucent,
      onPointerDown: (_) => _tapGuard = false,
      child: GestureDetector(
        // 点击空白处清除选中；命中条目的点击由 _tapGuard 抑制（见 _clearSelection）
        onTap: _clearSelection,
        behavior: HitTestBehavior.translucent,
        child: ConstrainedBox(
          constraints: BoxConstraints(minHeight: minH),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              // 工具栏（.toolbar）
              Row(
                children: [
                  VsSeg<VaultViewMode>(
                    segments: [
                      (VaultViewMode.grid, l.vaultPageViewGrid, 'grid'),
                      (VaultViewMode.list, l.vaultPageViewList, 'list'),
                      (VaultViewMode.columns, l.vaultPageViewSplit, 'split'),
                    ],
                    selected: state.view,
                    onChanged: _ctl.setView,
                  ),
                  if (state.selected.isNotEmpty) ...[
                    const SizedBox(width: 10),
                    VsBadge(l.vaultPageSelectedCount(state.selected.length),
                        tone: VsTone.info, plain: true),
                  ],
                  const Spacer(),
                  VsButton(
                      label: l.vaultPageNewFolder,
                      icon: 'plus',
                      tone: VsBtnTone.ghost,
                      onPressed: _mkdir),
                  const SizedBox(width: 8),
                  VsButton(
                      label: l.vaultPageImport,
                      icon: 'up',
                      tone: VsBtnTone.primary,
                      onPressed: _import),
                ],
              ),
              if (state.searchQuery != null)
                Padding(
                  padding: const EdgeInsets.only(top: 10),
                  child: Row(children: [
                    Text(l.vaultPageSearching(state.searchQuery!),
                        style: TextStyle(fontSize: 12.5, color: v.text2)),
                    MouseRegion(
                      cursor: SystemMouseCursors.click,
                      child: GestureDetector(
                        onTap: () {
                          ref.read(globalSearchProvider.notifier).set('');
                          _ctl.exitSearch();
                        },
                        child: Padding(
                          padding: const EdgeInsets.only(left: 8),
                          child: Text(l.vaultPageBackToFolder,
                              style: TextStyle(fontSize: 12.5, color: v.blue)),
                        ),
                      ),
                    ),
                  ]),
                ),
              const SizedBox(height: 12),
              _buildContent(v, state),
            ],
          ),
        ),
      ),
    );
  }

  Widget _buildContent(VsScheme v, VaultUiState state) {
    final l = AppLocalizations.of(context);
    if (state.error != null) {
      return SizedBox(
          height: 300,
          child: Center(
              child: Text(state.error!, style: TextStyle(color: v.danger))));
    }
    if (state.loading) {
      return SizedBox(
        height: 300,
        child: Center(
          child: SizedBox(
            width: 26,
            height: 26,
            child: CircularProgressIndicator(strokeWidth: 2.4, color: v.gold),
          ),
        ),
      );
    }
    if (state.listing.isEmpty) {
      // 根目录 / 子文件夹 / 搜索三种空态文案区分
      final searching = state.searchQuery != null;
      final inFolder = !state.atRoot;
      return SizedBox(
        height: 380,
        child: VsEmpty(
          icon: searching ? 'search' : (inFolder ? 'folder' : 'vault'),
          title: searching
              ? l.vaultPageEmptySearchTitle
              : inFolder
                  ? l.vaultPageEmptyFolderTitle
                  : l.vaultPageEmptyVaultTitle,
          sub: searching
              ? l.vaultPageEmptySearchSub
              : inFolder
                  ? l.vaultPageEmptyFolderSub
                  : l.vaultPageEmptyVaultSub,
          action: VsButton(
              label: l.vaultPageImport,
              icon: 'up',
              tone: VsBtnTone.primary,
              onPressed: _import),
        ),
      );
    }
    return DropTarget(
      enable: !_importOpen,
      onDragDone: (d) => _importFiles(d.files.map((f) => f.path).toList()),
      child: switch (state.view) {
        VaultViewMode.grid => _grid(v, state),
        VaultViewMode.list => _list(v, state),
        VaultViewMode.columns => _columns(v, state),
      },
    );
  }

  Widget _grid(VsScheme v, VaultUiState state) {
    final width = MediaQuery.widthOf(context) - DesignTokens.sidebarW - 48;
    final cols = (width / 172).floor().clamp(3, 8);
    return GridView.builder(
      shrinkWrap: true,
      physics: const NeverScrollableScrollPhysics(),
      gridDelegate: SliverGridDelegateWithFixedCrossAxisCount(
        crossAxisCount: cols,
        mainAxisSpacing: DesignTokens.sp3,
        crossAxisSpacing: DesignTokens.sp3,
        childAspectRatio: 172 / 176,
      ),
      itemCount: state.listing.entries.length,
      itemBuilder: (context, i) {
        final e = state.listing.entries[i];
        return _FileCard(
          key: ValueKey(e.id),
          entry: e,
          selected: state.selected.contains(e.id),
          icon: e.isFolder ? 'folder' : _iconFor(e),
          iconColor: _kindColor(e.isFolder ? 'folder' : _iconFor(e)),
          thumbGradient: _thumbGradient(v, e),
          onTap: () => _select(e),
          onDoubleTap: () => _openDetail(e),
          onSecondary: (pos) => _onSecondaryTap(e, pos),
        );
      },
    );
  }

  Widget _list(VsScheme v, VaultUiState state) {
    final l = AppLocalizations.of(context);
    return Container(
      decoration: BoxDecoration(
        color: v.surface,
        borderRadius: BorderRadius.circular(DesignTokens.rMd),
        border: Border.all(color: v.borderSoft),
      ),
      clipBehavior: Clip.antiAlias,
      child: Column(
        children: [
          Container(
            padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 9),
            decoration: BoxDecoration(
                border: Border(bottom: BorderSide(color: v.borderSoft))),
            child: Row(
              children: [
                _th(v, l.vaultPageNameLabel, flex: 6),
                _th(v, l.vaultPageTypeLabel, flex: 2),
                _th(v, l.vaultPageSizeLabel, flex: 2),
                _th(v, l.vaultPageStatusLabel, flex: 2),
                const SizedBox(width: 40),
              ],
            ),
          ),
          for (final e in state.listing.entries)
            _FileRow(
              key: ValueKey(e.id),
              entry: e,
              selected: state.selected.contains(e.id),
              icon: e.isFolder ? 'folder' : _iconFor(e),
              kindName: _kindName(e),
              sizeText: e.isFolder ? '—' : fmtSize(e.size),
              onTap: () => _select(e),
              onDoubleTap: () => _openDetail(e),
              onSecondary: (pos) => _onSecondaryTap(e, pos),
            ),
        ],
      ),
    );
  }

  Widget _th(VsScheme v, String text, {int flex = 1}) => Expanded(
        flex: flex,
        child: Text(text,
            style: TextStyle(
                fontSize: 11.5,
                fontWeight: FontWeight.w600,
                letterSpacing: 0.05,
                color: v.text3)),
      );

  /// 分栏视图：左侧树（快速访问 + 当前路径 + 子文件夹 + 标签），右侧文件表。
  Widget _columns(VsScheme v, VaultUiState state) {
    final l = AppLocalizations.of(context);
    final folders = state.listing.folders;
    final files = state.listing.files;
    final tags = {for (final e in state.listing.entries) ...e.tags};

    return Row(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Container(
          width: 230,
          padding: const EdgeInsets.all(10),
          decoration: BoxDecoration(
            color: v.surface,
            borderRadius: BorderRadius.circular(DesignTokens.rMd),
            border: Border.all(color: v.borderSoft),
          ),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              _treeSec(v, l.vaultPageTreeQuickAccess),
              _treeItem(v, l.vaultPageTreeAllFiles, 'vault',
                  on: state.atRoot, onTap: () => _ctl.goToCrumb(0)),
              _treeSec(v, l.vaultPageTreeCurrentPath),
              for (var i = 1; i < state.crumbs.length; i++)
                _treeItem(
                  v,
                  state.crumbs[i].$2?.isEmpty == true
                      ? l.vaultPageUnnamedFolder
                      : (state.crumbs[i].$2 ?? l.vaultPageUnnamedFolder),
                  'folder',
                  on: i == state.crumbs.length - 1,
                  onTap: () => _ctl.goToCrumb(i),
                ),
              _treeSec(v, l.vaultPageFolder),
              if (folders.isEmpty)
                Padding(
                  padding:
                      const EdgeInsets.symmetric(horizontal: 9, vertical: 6),
                  child: Text(l.vaultPageNoSubfolders,
                      style: TextStyle(fontSize: 13, color: v.text3)),
                ),
              for (final fo in folders)
                _treeItem(
                    v,
                    fo.name.isEmpty ? l.vaultPageUnnamedFolder : fo.name,
                    'folder',
                    onTap: () => _ctl.openFolder(fo.id, fo.name)),
              if (tags.isNotEmpty) ...[
                _treeSec(v, l.vaultPageTags),
                Padding(
                  padding:
                      const EdgeInsets.symmetric(horizontal: 6, vertical: 4),
                  child: Wrap(
                    spacing: 6,
                    runSpacing: 6,
                    children: [
                      for (final t in tags.take(6)) VsTagChip('#$t'),
                    ],
                  ),
                ),
              ],
            ],
          ),
        ),
        const SizedBox(width: DesignTokens.sp4),
        Expanded(
          child: files.isEmpty
              ? SizedBox(
                  height: 240,
                  child: VsEmpty(
                    icon: 'file',
                    title: l.vaultPageNoFiles,
                    sub: folders.isEmpty
                        ? l.vaultPageNoFilesEmptySub
                        : l.vaultPageNoFilesSub,
                    action: VsButton(
                        label: l.vaultPageImport,
                        icon: 'up',
                        tone: VsBtnTone.primary,
                        onPressed: _import),
                  ),
                )
              : _list(v, state),
        ),
      ],
    );
  }

  Widget _treeSec(VsScheme v, String text) => Padding(
        padding: const EdgeInsets.fromLTRB(9, 12, 9, 5),
        child: Text(text,
            style: TextStyle(
                fontSize: 11,
                fontWeight: FontWeight.w600,
                letterSpacing: 0.07,
                color: v.text3)),
      );

  Widget _treeItem(VsScheme v, String name, String icon,
      {bool on = false, VoidCallback? onTap}) {
    return Padding(
      padding: const EdgeInsets.only(bottom: 1),
      child: MouseRegion(
        cursor: SystemMouseCursors.click,
        child: GestureDetector(
          onTap: onTap,
          child: Container(
            padding: const EdgeInsets.symmetric(horizontal: 9, vertical: 6),
            decoration: BoxDecoration(
              color: on ? v.goldSoft : Colors.transparent,
              borderRadius: BorderRadius.circular(7),
            ),
            child: Row(
              children: [
                AppIcon(icon, size: 15, color: on ? v.gold : v.text2),
                const SizedBox(width: 8),
                Expanded(
                  child: Text(name,
                      style: TextStyle(
                          fontSize: 13,
                          fontWeight: on ? FontWeight.w600 : FontWeight.w400,
                          color: on ? v.gold : v.text2)),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

/// 同步状态徽章：冲突文件名含 .conflict- 标记。
Widget _syncBadge(VsScheme v, VaultEntry e, AppLocalizations l) {
  if (e.name.contains('.conflict-')) {
    return VsBadge(l.vaultPageConflict, tone: VsTone.danger);
  }
  return VsBadge(l.vaultPageSynced, tone: VsTone.ok);
}

// ============================================================
// 网格卡片（.fcard）：hover 上浮 2px + 阴影；选中蓝描边；即时本地动效
// ============================================================

class _FileCard extends StatefulWidget {
  const _FileCard({
    required this.entry,
    required this.selected,
    required this.icon,
    required this.iconColor,
    required this.thumbGradient,
    required this.onTap,
    required this.onDoubleTap,
    required this.onSecondary,
    super.key,
  });

  final VaultEntry entry;
  final bool selected;
  final String icon;
  final Color iconColor;
  final Gradient? thumbGradient;
  final VoidCallback onTap;
  final VoidCallback onDoubleTap;
  final void Function(Offset) onSecondary;

  @override
  State<_FileCard> createState() => _FileCardState();
}

class _FileCardState extends State<_FileCard> {
  bool _hover = false;

  @override
  Widget build(BuildContext context) {
    final l = AppLocalizations.of(context);
    final v = context.vs;
    final e = widget.entry;
    final selected = widget.selected;
    final lift = _hover && !selected;

    return Semantics(
      // 无障碍（P4-7）：屏幕阅读器播报名称 / 类型 / 大小 + 选中态。
      button: true,
      selected: selected,
      label: e.isFolder
          ? '${e.name.isEmpty ? l.vaultPageUnnamed : e.name} · ${l.vaultPageFolder}'
          : '${e.name.isEmpty ? l.vaultPageUnnamed : e.name} · '
              '${_FileRow.fmtSize(context, e.size)}',
      child: MouseRegion(
        cursor: SystemMouseCursors.click,
        onEnter: (_) => setState(() => _hover = true),
        onExit: (_) => setState(() => _hover = false),
        child: GestureDetector(
          // 单击选中走 Listener（即时生效）；这里只留双击与右键，
          // 避免 onTap+onDoubleTap 共存时的 ~300ms 双击判定延时。
          onDoubleTap: widget.onDoubleTap,
          onSecondaryTapDown: (d) => widget.onSecondary(d.globalPosition),
          child: Listener(
            onPointerUp: (_) => widget.onTap(),
            behavior: HitTestBehavior.opaque,
            child: AnimatedContainer(
              duration: const Duration(milliseconds: 120),
              curve: Curves.easeOut,
              transform: Matrix4.translationValues(0, lift ? -2 : 0, 0),
              padding: const EdgeInsets.all(10),
              decoration: BoxDecoration(
                color: selected ? v.blueSoft : v.surface,
                borderRadius: BorderRadius.circular(DesignTokens.rMd),
                border: Border.all(
                    color: selected
                        ? v.blue
                        : _hover
                            ? v.border
                            : v.borderSoft),
                boxShadow: selected
                    ? [
                        BoxShadow(
                            color: v.blueSoft, blurRadius: 0, spreadRadius: 2)
                      ]
                    : lift
                        ? v.shadow2
                        : const [],
              ),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Expanded(
                    child: Container(
                      width: double.infinity,
                      decoration: BoxDecoration(
                        gradient: widget.thumbGradient,
                        color:
                            widget.thumbGradient == null ? v.surfaceAlt : null,
                        borderRadius: BorderRadius.circular(DesignTokens.rSm),
                      ),
                      child: Stack(
                        children: [
                          Center(
                            child: AppIcon(widget.icon,
                                size: e.isFolder ? 40 : 30,
                                color: widget.iconColor),
                          ),
                          if (!e.isFolder)
                            Positioned(
                              top: 7,
                              right: 7,
                              child: Container(
                                width: 21,
                                height: 21,
                                decoration: BoxDecoration(
                                  color: v.encSoft,
                                  borderRadius: BorderRadius.circular(6),
                                ),
                                child: Center(
                                    child: AppIcon('lock',
                                        size: 11, color: v.enc)),
                              ),
                            ),
                        ],
                      ),
                    ),
                  ),
                  const SizedBox(height: 9),
                  Text(e.name.isEmpty ? l.vaultPageUnnamed : e.name,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: TextStyle(
                          fontSize: 13,
                          fontWeight: FontWeight.w500,
                          color: v.text)),
                  const SizedBox(height: 3),
                  Row(
                    children: [
                      Expanded(
                        child: Text(
                            e.isFolder
                                ? l.vaultPageFolder
                                : _FileRow.fmtSize(context, e.size),
                            style: TextStyle(fontSize: 11.5, color: v.text3)),
                      ),
                      if (!e.isFolder) _syncBadge(v, e, l),
                    ],
                  ),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}

// ============================================================
// 列表行（.filetable tbody tr）：hover / 选中即时高亮
// ============================================================

class _FileRow extends StatefulWidget {
  const _FileRow({
    required this.entry,
    required this.selected,
    required this.icon,
    required this.kindName,
    required this.sizeText,
    required this.onTap,
    required this.onDoubleTap,
    required this.onSecondary,
    super.key,
  });

  final VaultEntry entry;
  final bool selected;
  final String icon;
  final String kindName;
  final String sizeText;
  final VoidCallback onTap;
  final VoidCallback onDoubleTap;
  final void Function(Offset) onSecondary;

  /// 供 _FileCard 复用的尺寸格式化。
  static String fmtSize(BuildContext context, int n) {
    if (n < 1024) return '$n B';
    if (n < 1024 * 1024) return '${(n / 1024).toStringAsFixed(1)} KB';
    if (n < 1024 * 1024 * 1024) {
      return '${(n / 1024 / 1024).toStringAsFixed(1)} MB';
    }
    return '${(n / 1024 / 1024 / 1024).toStringAsFixed(1)} GB';
  }

  @override
  State<_FileRow> createState() => _FileRowState();
}

class _FileRowState extends State<_FileRow> {
  bool _hover = false;

  @override
  Widget build(BuildContext context) {
    final l = AppLocalizations.of(context);
    final v = context.vs;
    final e = widget.entry;
    final selected = widget.selected;
    final name = e.name.isEmpty ? l.vaultPageUnnamed : e.name;

    return Semantics(
      // 无障碍（P4-7）：行内容一次性播报，避免逐列朗读碎片。
      button: true,
      selected: selected,
      label:
          '$name · ${widget.kindName} · ${e.isFolder ? '' : widget.sizeText}',
      child: MouseRegion(
        cursor: SystemMouseCursors.click,
        onEnter: (_) => setState(() => _hover = true),
        onExit: (_) => setState(() => _hover = false),
        child: GestureDetector(
          onDoubleTap: widget.onDoubleTap,
          onSecondaryTapDown: (d) => widget.onSecondary(d.globalPosition),
          child: Listener(
            onPointerUp: (_) => widget.onTap(),
            behavior: HitTestBehavior.opaque,
            child: AnimatedContainer(
              duration: const Duration(milliseconds: 100),
              padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 9),
              decoration: BoxDecoration(
                color: selected
                    ? v.blueSoft
                    : _hover
                        ? v.surfaceAlt
                        : Colors.transparent,
              ),
              child: Row(
                children: [
                  Expanded(
                    flex: 6,
                    child: Row(
                      children: [
                        Container(
                          width: 30,
                          height: 30,
                          decoration: BoxDecoration(
                            color: switch (widget.icon) {
                              'img' => v.encSoft,
                              'zip' || 'folder' => v.goldSoft,
                              'pdf' || 'txt' => v.blueSoft,
                              _ => v.surfaceAlt,
                            },
                            borderRadius: BorderRadius.circular(8),
                          ),
                          child: Center(
                            child: AppIcon(widget.icon,
                                size: 15,
                                color: switch (widget.icon) {
                                  'img' => v.enc,
                                  'zip' || 'folder' => v.gold,
                                  'pdf' || 'txt' => v.blue,
                                  _ => v.text3,
                                }),
                          ),
                        ),
                        const SizedBox(width: 10),
                        Expanded(
                          child: Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              Text(e.name.isEmpty ? l.vaultPageUnnamed : e.name,
                                  maxLines: 1,
                                  overflow: TextOverflow.ellipsis,
                                  style: TextStyle(
                                      fontSize: 13,
                                      fontWeight: FontWeight.w500,
                                      color: v.text)),
                              Text(
                                e.tags.isEmpty
                                    ? widget.kindName
                                    : e.tags.map((t) => '#$t').join(' '),
                                style: TextStyle(fontSize: 12, color: v.text3),
                                maxLines: 1,
                                overflow: TextOverflow.ellipsis,
                              ),
                            ],
                          ),
                        ),
                      ],
                    ),
                  ),
                  Expanded(
                      flex: 2,
                      child: Text(widget.kindName,
                          style: TextStyle(fontSize: 13, color: v.text2))),
                  Expanded(
                      flex: 2,
                      child: Text(widget.sizeText,
                          style: TextStyle(fontSize: 13, color: v.text2))),
                  Expanded(
                      flex: 2,
                      child: e.isFolder
                          ? const VsBadge('—', plain: true)
                          : _syncBadge(v, e, l)),
                  SizedBox(
                    width: 40,
                    child: VsIconButton(
                      icon: 'dots',
                      size: 16,
                      onPressed: () {
                        // 对应原型 data-more：菜单锚定在该行右下角
                        final box = context.findRenderObject() as RenderBox;
                        final topLeft = box.localToGlobal(Offset.zero);
                        widget.onSecondary(Offset(
                            topLeft.dx + box.size.width - 210,
                            topLeft.dy + box.size.height + 4));
                      },
                    ),
                  ),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}
