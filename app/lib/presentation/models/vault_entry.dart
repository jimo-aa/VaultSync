/// 保险箱条目展示模型（P4-2：三视图 / 多选 / 右键）。
library;

/// 视图模式。
enum VaultViewMode { grid, list, columns }

/// 单条：文件夹或文件（来自 `vault_list` 真实 JSON）。
class VaultEntry {
  const VaultEntry({
    required this.id,
    required this.name,
    required this.isFolder,
    this.size = 0,
    this.tags = const [],
    this.modifiedMs,
  });

  final int id;
  final String name;
  final bool isFolder;
  final int size;
  final List<String> tags;
  final int? modifiedMs;

  bool get isSyncConflict =>
      !isFolder && name.contains('.conflict-') && name.endsWith('）') == false;

  factory VaultEntry.fromJson(Map<String, dynamic> json, {bool isFolder = false}) =>
      VaultEntry(
        id: (json['id'] as num).toInt(),
        name: (json['name'] as String?) ?? '',
        isFolder: isFolder,
        size: (json['size'] as num?)?.toInt() ?? 0,
        tags: ((json['tags'] as List?) ?? const []).whereType<String>().toList(),
        modifiedMs: (json['modifiedMs'] as num?)?.toInt(),
      );

  factory VaultEntry.folder({required int id, required String name}) => VaultEntry(
        id: id,
        name: name,
        isFolder: true,
      );
}

/// 完整列表（文件夹 + 文件，分别排序后合并展示）。
class VaultListing {
  const VaultListing({required this.folders, required this.files});

  final List<VaultEntry> folders;
  final List<VaultEntry> files;

  /// 文件夹在前、文件在后（各自按名称排序）。
  List<VaultEntry> get entries => [...folders, ...files];

  bool get isEmpty => folders.isEmpty && files.isEmpty;

  factory VaultListing.fromJson(Map<String, dynamic> json) {
    final folders = ((json['folders'] as List?) ?? const [])
        .whereType<Map>()
        .map((m) => VaultEntry.fromJson(Map<String, dynamic>.from(m), isFolder: true))
        .toList();
    final files = ((json['files'] as List?) ?? const [])
        .whereType<Map>()
        .map((m) => VaultEntry.fromJson(Map<String, dynamic>.from(m)))
        .toList();
    folders.sort((a, b) => a.name.toLowerCase().compareTo(b.name.toLowerCase()));
    files.sort((a, b) => a.name.toLowerCase().compareTo(b.name.toLowerCase()));
    return VaultListing(folders: folders, files: files);
  }
}