import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/icons/app_icon.dart';
import '../../core/theme/design_tokens.dart';
import '../../service/engine_service.dart';
import '../../state/session_controller.dart';

/// 保险箱页（P2 最小验收界面：文件夹树 / 导入 / 导出 / 重命名 / 擦除 / 搜索 / 分享）。
/// 完整资源管理器交互（三视图 / 右键菜单 / 多选）随 P4-2 落地。
class VaultPage extends ConsumerStatefulWidget {
  const VaultPage({super.key});

  @override
  ConsumerState<VaultPage> createState() => _VaultPageState();
}

class _VaultPageState extends ConsumerState<VaultPage> {
  int _folder = 0;
  String? _folderName;
  List<Map<String, dynamic>> _folders = [];
  List<Map<String, dynamic>> _files = [];
  bool _loading = true;
  String? _error;
  final _searchCtl = TextEditingController();
  bool _searching = false;

  Object? get _handle {
    final s = ref.read(sessionProvider);
    return s is SessionUnlocked ? s.sessionHandle : null;
  }

  @override
  void initState() {
    super.initState();
    _refresh();
  }

  Future<void> _refresh() async {
    final handle = _handle;
    if (handle == null) return;
    setState(() => _loading = true);
    final listing = await ref.read(vaultEngineProvider).list(handle, _folder);
    if (!mounted) return;
    setState(() {
      _loading = false;
      if (listing == null) {
        _error = '索引读取失败';
      } else {
        _error = null;
        _folders = (listing['folders'] as List).cast<Map<String, dynamic>>();
        _files = (listing['files'] as List).cast<Map<String, dynamic>>();
      }
    });
  }

  void _toast(String msg) {
    if (!mounted) return;
    ScaffoldMessenger.of(context)
        .showSnackBar(SnackBar(content: Text(msg), duration: const Duration(seconds: 2)));
  }

  Future<void> _import() async {
    final file = await openFile();
    if (file == null) return;
    final engine = ref.read(vaultEngineProvider);
    final (err, id) = await engine.importFile(_handle!, file.path, _folder);
    _toast(err ?? '已导入（文件 ID $id）');
    await _refresh();
  }

  Future<void> _export(Map<String, dynamic> f) async {
    final engine = ref.read(vaultEngineProvider);
    final location = await getSaveLocation(suggestedName: f['name'] as String);
    if (location == null) return;
    final err = await engine.exportFile(_handle!, f['id'] as int, location.path);
    _toast(err ?? '已导出到 ${location.path}');
  }

  Future<void> _delete(Map<String, dynamic> f) async {
    final ok = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: const Text('安全擦除'),
        content: Text('将销毁「${f['name']}」的密钥并删除密文，不可恢复。确认？'),
        actions: [
          TextButton(onPressed: () => Navigator.pop(ctx, false), child: const Text('取消')),
          FilledButton(
            style: FilledButton.styleFrom(
                backgroundColor: DesignTokens.dangerDark),
            onPressed: () => Navigator.pop(ctx, true),
            child: const Text('擦除'),
          ),
        ],
      ),
    );
    if (ok != true) return;
    final err = await ref.read(vaultEngineProvider).deleteFile(_handle!, f['id'] as int);
    _toast(err ?? '已安全擦除');
    await _refresh();
  }

  Future<void> _mkdir() async {
    final ctl = TextEditingController();
    final name = await showDialog<String>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: const Text('新建文件夹'),
        content: TextField(controller: ctl, autofocus: true, decoration: const InputDecoration(hintText: '文件夹名称')),
        actions: [
          TextButton(onPressed: () => Navigator.pop(ctx), child: const Text('取消')),
          FilledButton(
            onPressed: () => Navigator.pop(ctx, ctl.text.trim()),
            child: const Text('创建'),
          ),
        ],
      ),
    );
    if (name == null || name.isEmpty) return;
    final (err, _) = await ref.read(vaultEngineProvider).mkdir(_handle!, _folder, name);
    _toast(err ?? '已创建「$name」');
    await _refresh();
  }

  Future<void> _rename(Map<String, dynamic> f) async {
    final ctl = TextEditingController(text: f['name'] as String);
    final name = await showDialog<String>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: const Text('重命名'),
        content: TextField(controller: ctl, autofocus: true),
        actions: [
          TextButton(onPressed: () => Navigator.pop(ctx), child: const Text('取消')),
          FilledButton(onPressed: () => Navigator.pop(ctx, ctl.text.trim()), child: const Text('确定')),
        ],
      ),
    );
    if (name == null || name.isEmpty || name == f['name']) return;
    final err = await ref.read(vaultEngineProvider).renameFile(_handle!, f['id'] as int, name);
    _toast(err ?? '已重命名');
    await _refresh();
  }

  Future<void> _share(Map<String, dynamic> f) async {
    final result = await ref
        .read(vaultEngineProvider)
        .shareCreate(_handle!, f['id'] as int, 3600, 1);
    if (result == null) {
      _toast('分享创建失败');
      return;
    }
    if (!mounted) return;
    showDialog<void>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: const Text('阅后即焚分享（1 次打开机会）'),
        content: SelectableText(
          '分享 ID：${result['shareId']}\n打开令牌：${result['token']}\n\n'
          '令牌仅显示这一次；对方凭令牌打开后分享即失效。',
        ),
        actions: [
          FilledButton(onPressed: () => Navigator.pop(ctx), child: const Text('好的')),
        ],
      ),
    );
  }

  Future<void> _runSearch() async {
    final q = _searchCtl.text.trim();
    if (q.isEmpty) {
      setState(() => _searching = false);
      await _refresh();
      return;
    }
    final result = await ref.read(vaultEngineProvider).search(_handle!, q);
    if (!mounted) return;
    final files = result == null
        ? <Map<String, dynamic>>[]
        : (result['files'] as List).cast<Map<String, dynamic>>();
    setState(() {
      _searching = true;
      _files = files;
      _folders = [];
    });
  }

  String _sizeLabel(dynamic size) {
    final n = (size as num).toDouble();
    if (n < 1024) return '$n B';
    if (n < 1024 * 1024) return '${(n / 1024).toStringAsFixed(1)} KB';
    return '${(n / 1024 / 1024).toStringAsFixed(1)} MB';
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Scaffold(
      body: Padding(
        padding: const EdgeInsets.all(20),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Wrap(
              spacing: 8,
              runSpacing: 8,
              crossAxisAlignment: WrapCrossAlignment.center,
              children: [
                Text('保险箱', style: theme.textTheme.headlineMedium),
                if (_folder != 0)
                  Text(_folderName ?? '', style: theme.textTheme.bodySmall),
                SizedBox(
                  width: 200,
                  child: TextField(
                    controller: _searchCtl,
                    onSubmitted: (_) => _runSearch(),
                    decoration: const InputDecoration(
                      hintText: '搜索文件名 / 标签 / 全文',
                      prefixIcon: Icon(Icons.search),
                    ),
                  ),
                ),
                IconButton(
                  tooltip: '新建文件夹',
                  onPressed: _mkdir,
                  icon: const AppIcon('folder'),
                ),
                FilledButton.icon(
                  onPressed: _import,
                  icon: const AppIcon('plus', size: 16),
                  label: const Text('导入'),
                ),
              ],
            ),
            if (_searching)
              TextButton(
                onPressed: () {
                  _searchCtl.clear();
                  _runSearch();
                },
                child: const Text('← 返回当前文件夹'),
              ),
            const SizedBox(height: 12),
            if (_error != null)
              Text(_error!, style: TextStyle(color: theme.colorScheme.error))
            else if (_loading)
              const Center(child: CircularProgressIndicator())
            else
              Expanded(
                child: _files.isEmpty && _folders.isEmpty
                    ? Center(
                        child: Text('此文件夹为空', style: theme.textTheme.bodySmall),
                      )
                    : ListView.builder(
                        itemCount: _folders.length + _files.length,
                        itemBuilder: (context, i) {
                          if (i < _folders.length) {
                            final fo = _folders[i];
                            return ListTile(
                              leading: const AppIcon('folder'),
                              title: Text(fo['name'] as String),
                              onTap: () async {
                                // 面板骨架期：根目录下的文件夹进入即列出
                                final prev = _folderName;
                                setState(() {
                                  _folder = fo['id'] as int;
                                  _folderName = fo['name'] as String;
                                });
                                await _refresh();
                                if (!mounted) return;
                                if (_error != null) setState(() => _folderName = prev);
                              },
                            );
                          }
                          final f = _files[i - _folders.length];
                          final tags = (f['tags'] as List?)?.cast<String>() ?? const [];
                          return ListTile(
                            leading: const AppIcon('file'),
                            title: Text(f['name'] as String),
                            subtitle: Text(
                              '${_sizeLabel(f['size'])}'
                              '${tags.isEmpty ? '' : '  ·  ${tags.join(', ')}'}',
                              style: DesignTokens.caption.copyWith(
                                color: theme.textTheme.bodySmall?.color,
                                fontFamily: DesignTokens.monoFamily,
                              ),
                            ),
                            trailing: Row(
                              mainAxisSize: MainAxisSize.min,
                              children: [
                                IconButton(
                                  tooltip: '导出',
                                  icon: const AppIcon('down', size: 18),
                                  onPressed: () => _export(f),
                                ),
                                IconButton(
                                  tooltip: '重命名',
                                  icon: const AppIcon('edit', size: 18),
                                  onPressed: () => _rename(f),
                                ),
                                IconButton(
                                  tooltip: '阅后即焚分享',
                                  icon: const AppIcon('share', size: 18),
                                  onPressed: () => _share(f),
                                ),
                                IconButton(
                                  tooltip: '安全擦除',
                                  icon: AppIcon('flame', size: 18,
                                      color: theme.colorScheme.error),
                                  onPressed: () => _delete(f),
                                ),
                              ],
                            ),
                          );
                        },
                      ),
              ),
          ],
        ),
      ),
    );
  }
}
