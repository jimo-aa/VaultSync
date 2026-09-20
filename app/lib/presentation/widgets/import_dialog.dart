import 'dart:io';

import 'package:desktop_drop/desktop_drop.dart';
import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';

import '../../core/theme/design_tokens.dart';
import '../../l10n/app_localizations.dart';
import 'vs_ui.dart';

/// 导入文件弹窗（对应原型 `#ov-import`）：拖放 + 多选，不限文件类型。
///
/// 返回用户确认导入的本地文件路径列表；取消返回 null。
/// 类型不限：`openFiles()` 不传 acceptedTypeGroups（Windows 上即「所有文件」）。
Future<List<String>?> showImportDialog(BuildContext context) {
  final l = AppLocalizations.of(context);
  final picked = ValueNotifier<List<({String path, int size})>>(
      <({String path, int size})>[]);
  return showVsModal<List<String>>(
    context: context,
    title: l.importDialogTitle,
    sub: l.importDialogSub,
    icon: 'up',
    micTone: VsTone.info,
    width: 520,
    body: (ctx) => _ImportBody(picked: picked),
    actions: (ctx) => [
      VsButton(
        label: l.importDialogCancel,
        tone: VsBtnTone.ghost,
        onPressed: () => Navigator.pop(ctx),
      ),
      ValueListenableBuilder<List<({String path, int size})>>(
        valueListenable: picked,
        builder: (_, list, _) => VsButton(
          label: list.isEmpty
              ? l.importDialogImport
              : l.importDialogImportCount(list.length),
          icon: 'up',
          tone: VsBtnTone.primary,
          onPressed: list.isEmpty
              ? null
              : () => Navigator.pop(ctx, [for (final f in list) f.path]),
        ),
      ),
    ],
  ).whenComplete(picked.dispose);
}

class _ImportBody extends StatefulWidget {
  const _ImportBody({required this.picked});

  final ValueNotifier<List<({String path, int size})>> picked;

  @override
  State<_ImportBody> createState() => _ImportBodyState();
}

class _ImportBodyState extends State<_ImportBody> {
  bool _dragging = false;

  /// 追加去重（按路径）；目录与已存在项忽略。
  void _add(Iterable<String> paths) {
    final next = [...widget.picked.value];
    final seen = {for (final f in next) f.path};
    for (final p in paths) {
      if (seen.contains(p)) continue;
      if (!File(p).existsSync() || FileSystemEntity.isDirectorySync(p)) {
        continue;
      }
      seen.add(p);
      next.add((path: p, size: File(p).lengthSync()));
    }
    widget.picked.value = next;
  }

  void _remove(String path) {
    widget.picked.value =
        widget.picked.value.where((f) => f.path != path).toList();
  }

  Future<void> _pick() async {
    // 不限类型：不传 acceptedTypeGroups（Windows 上即「所有文件」）。
    final files = await openFiles();
    if (!mounted) return;
    _add(files.map((f) => f.path));
  }

  @override
  Widget build(BuildContext context) {
    final l = AppLocalizations.of(context);
    final v = context.vs;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        DropTarget(
          onDragEntered: (_) => setState(() => _dragging = true),
          onDragExited: (_) => setState(() => _dragging = false),
          onDragDone: (d) {
            setState(() => _dragging = false);
            _add(d.files.map((f) => f.path));
          },
          child: MouseRegion(
            cursor: SystemMouseCursors.click,
            child: GestureDetector(
              onTap: _pick,
              child: Container(
                width: double.infinity,
                decoration: BoxDecoration(
                  color: _dragging ? v.blueSoft : Colors.transparent,
                  borderRadius: BorderRadius.circular(DesignTokens.rMd),
                ),
                child: CustomPaint(
                  painter: _DashedBorderPainter(
                    color: _dragging ? v.blue : v.border,
                    radius: DesignTokens.rMd,
                  ),
                  child: Padding(
                    padding: const EdgeInsets.symmetric(
                        horizontal: 16, vertical: 26),
                    child: Column(
                      children: [
                        AppIcon('up',
                            size: 28, color: _dragging ? v.blue : v.text3),
                        const SizedBox(height: 8),
                        Text.rich(
                          TextSpan(
                            children: [
                              TextSpan(
                                text: l.importDialogDropHere,
                                style: TextStyle(
                                  fontSize: 13.5,
                                  fontWeight: FontWeight.w600,
                                  color: _dragging ? v.blue : v.text2,
                                ),
                              ),
                              TextSpan(
                                text: l.importDialogOrClick,
                                style: TextStyle(
                                  fontSize: 13.5,
                                  color: _dragging ? v.blue : v.text3,
                                ),
                              ),
                            ],
                          ),
                          textAlign: TextAlign.center,
                        ),
                        const SizedBox(height: 4),
                        Text(l.importDialogHint,
                            style: TextStyle(fontSize: 12, color: v.text3)),
                      ],
                    ),
                  ),
                ),
              ),
            ),
          ),
        ),
        const SizedBox(height: DesignTokens.sp3),
        ValueListenableBuilder<List<({String path, int size})>>(
          valueListenable: widget.picked,
          builder: (_, list, _) => list.isEmpty
              ? Padding(
                  padding: const EdgeInsets.symmetric(vertical: 10),
                  child: Text(l.importDialogNoFiles,
                      style: TextStyle(fontSize: 12.5, color: v.text3)),
                )
              : ConstrainedBox(
                  constraints: const BoxConstraints(maxHeight: 176),
                  child: SingleChildScrollView(
                    child: Column(
                      children: [
                        for (final f in list)
                          _ImportRow(
                            path: f.path,
                            size: f.size,
                            onRemove: () => _remove(f.path),
                          ),
                      ],
                    ),
                  ),
                ),
        ),
        const SizedBox(height: DesignTokens.sp3),
        VsNote(l.importDialogNote, tone: VsTone.enc),
      ],
    );
  }
}

class _ImportRow extends StatelessWidget {
  const _ImportRow(
      {required this.path, required this.size, required this.onRemove});

  final String path;
  final int size;
  final VoidCallback onRemove;

  static String _fmtSize(int n) {
    if (n < 1024) return '$n B';
    if (n < 1024 * 1024) return '${(n / 1024).toStringAsFixed(1)} KB';
    if (n < 1024 * 1024 * 1024) {
      return '${(n / 1024 / 1024).toStringAsFixed(1)} MB';
    }
    return '${(n / 1024 / 1024 / 1024).toStringAsFixed(1)} GB';
  }

  @override
  Widget build(BuildContext context) {
    final l = AppLocalizations.of(context);
    final v = context.vs;
    final name = path.split(RegExp(r'[\\/]')).last;
    return Container(
      margin: const EdgeInsets.only(bottom: 8),
      padding: const EdgeInsets.fromLTRB(11, 8, 6, 8),
      decoration: BoxDecoration(
        color: v.bg,
        borderRadius: BorderRadius.circular(DesignTokens.rSm),
        border: Border.all(color: v.borderSoft),
      ),
      child: Row(
        children: [
          Container(
            width: 26,
            height: 26,
            decoration: BoxDecoration(
              color: v.encSoft,
              borderRadius: BorderRadius.circular(7),
            ),
            child: Center(child: AppIcon('file', size: 13, color: v.enc)),
          ),
          const SizedBox(width: 10),
          Expanded(
            child: Text(name,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: TextStyle(fontSize: 12.5, color: v.text)),
          ),
          const SizedBox(width: 8),
          Text(_fmtSize(size),
              style: TextStyle(fontSize: 11.5, color: v.text3)),
          const SizedBox(width: 4),
          VsIconButton(
              icon: 'close',
              size: 14,
              tooltip: l.importDialogRemove,
              onPressed: onRemove),
        ],
      ),
    );
  }
}

/// 虚线圆角描边（原型 `.dropzone` 的 `1.5px dashed`）。
class _DashedBorderPainter extends CustomPainter {
  const _DashedBorderPainter({required this.color, required this.radius});

  final Color color;
  final double radius;

  @override
  void paint(Canvas canvas, Size size) {
    final paint = Paint()
      ..color = color
      ..strokeWidth = 1.5
      ..style = PaintingStyle.stroke;
    final rrect = RRect.fromRectAndRadius(
      Rect.fromLTWH(0.75, 0.75, size.width - 1.5, size.height - 1.5),
      Radius.circular(radius),
    );
    const dash = 6.0;
    const gap = 4.0;
    final path = Path()..addRRect(rrect);
    for (final metric in path.computeMetrics()) {
      // 沿圆角矩形路径按 dash/gap 交替绘制
      for (double d = 0; d < metric.length; d += dash + gap) {
        final end = (d + dash).clamp(0.0, metric.length);
        canvas.drawPath(metric.extractPath(d, end), paint);
      }
    }
  }

  @override
  bool shouldRepaint(_DashedBorderPainter old) => old.color != color;
}
