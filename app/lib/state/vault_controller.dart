import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../presentation/models/vault_entry.dart';
import '../service/engine_service.dart';
import 'session_controller.dart';

/// 保险箱页统一状态：面包屑 / 列表 / 多选 / 视图 / 搜索。
/// 页面组件只读此状态并调用 controller 方法，不自行持有业务状态。
class VaultUiState {
  const VaultUiState({
    this.crumbs = const [(0, null)],
    this.listing = const VaultListing(folders: [], files: []),
    this.loading = true,
    this.error,
    this.selected = const {},
    this.anchor,
    this.view = VaultViewMode.grid,
    this.searchQuery,
  });

  /// 面包屑栈：每项 (文件夹 id, 名称)，首项为根（id 0）。
  final List<(int, String?)> crumbs;
  final VaultListing listing;
  final bool loading;
  final String? error;
  final Set<int> selected;
  final int? anchor; // Shift 连选锚点
  final VaultViewMode view;
  final String? searchQuery;

  int get currentFolder => crumbs.last.$1;
  bool get atRoot => crumbs.length == 1;

  VaultUiState copyWith({
    List<(int, String?)>? crumbs,
    VaultListing? listing,
    bool? loading,
    String? error,
    Set<int>? selected,
    int? anchor,
    VaultViewMode? view,
    String? searchQuery,
    bool clearError = false,
    bool clearSearch = false,
  }) =>
      VaultUiState(
        crumbs: crumbs ?? this.crumbs,
        listing: listing ?? this.listing,
        loading: loading ?? this.loading,
        error: clearError ? null : (error ?? this.error),
        selected: selected ?? this.selected,
        anchor: anchor ?? this.anchor,
        view: view ?? this.view,
        searchQuery: clearSearch ? null : (searchQuery ?? this.searchQuery),
      );
}

class VaultUiController extends Notifier<VaultUiState> {
  @override
  VaultUiState build() {
    // 会话切换（解锁 / 锁定 / 伪装）时重置并按新句柄刷新。
    ref.listen(sessionProvider, (prev, next) {
      if (prev is SessionUnlocked || next is SessionUnlocked) {
        state = VaultUiState(loading: next is SessionUnlocked);
        if (next is SessionUnlocked) refresh();
      }
    });
    return const VaultUiState();
  }

  VaultEngine get _engine => ref.read(vaultEngineProvider);

  Object? get _handle {
    final s = ref.read(sessionProvider);
    return s is SessionUnlocked ? s.sessionHandle : null;
  }

  Future<void> refresh() async {
    final handle = _handle;
    if (handle == null) return;
    state = state.copyWith(loading: true, clearError: true);
    final json = await _engine.list(handle, state.currentFolder);
    if (json == null) {
      state = state.copyWith(loading: false, error: '索引读取失败');
      return;
    }
    final listing = VaultListing.fromJson(json);
    // 清掉已不存在的选中项。
    final selected = state.selected
        .where((id) => listing.entries.any((e) => e.id == id))
        .toSet();
    state = state.copyWith(
        loading: false, listing: listing, selected: selected);
  }

  /// 进入子文件夹；环守卫：目标已在面包屑栈中（含当前层）则忽略。
  Future<void> openFolder(int id, String name) async {
    if (state.crumbs.any((c) => c.$1 == id)) return;
    state = state.copyWith(
      crumbs: [...state.crumbs, (id, name)],
      selected: const {},
      clearSearch: true,
      anchor: null,
    );
    await refresh();
  }

  /// 面包屑 / 快速访问跳转（顶栏面包屑同样走这里，页面经 listen 自动刷新）。
  Future<void> goToCrumb(int index) async {
    if (index < 0 || index >= state.crumbs.length - 1) return;
    state = state.copyWith(
      crumbs: state.crumbs.sublist(0, index + 1),
      selected: const {},
      clearSearch: true,
      anchor: null,
    );
    await refresh();
  }

  void select(VaultEntry e,
      {bool toggle = false, bool range = false, bool ctrl = false, bool shift = false}) {
    final order = state.listing.entries;
    if (shift) {
      final anchor = state.anchor;
      if (anchor == null || !order.any((x) => x.id == anchor)) {
        _selectOnly(e.id);
        return;
      }
      final a = order.indexWhere((x) => x.id == anchor);
      final b = order.indexWhere((x) => x.id == e.id);
      final lo = a <= b ? a : b;
      final hi = a <= b ? b : a;
      state = state.copyWith(
        selected: order.sublist(lo, hi + 1).map((x) => x.id).toSet(),
        anchor: e.id,
      );
      return;
    }
    if (ctrl) {
      final selected = {...state.selected};
      selected.contains(e.id) ? selected.remove(e.id) : selected.add(e.id);
      state = state.copyWith(selected: selected, anchor: e.id);
      return;
    }
    _selectOnly(e.id);
  }

  void _selectOnly(int id) =>
      state = state.copyWith(selected: {id}, anchor: id);

  void clearSelection() {
    if (state.selected.isEmpty) return;
    state = state.copyWith(selected: const {}, anchor: null);
  }

  void setView(VaultViewMode mode) => state = state.copyWith(view: mode);

  Future<void> runSearch(String q) async {
    final handle = _handle;
    if (handle == null || q.isEmpty) return;
    state = state.copyWith(searchQuery: q, loading: true, clearError: true);
    final result = await _engine.search(handle, q);
    final files = result == null
        ? const <Map<String, dynamic>>[]
        : (result['files'] as List).cast<Map<String, dynamic>>();
    if (!ref.mounted) return; // controller 已销毁（锁定）
    state = state.copyWith(
      loading: false,
      listing: VaultListing(
        folders: const [],
        files: files
            .map((m) => VaultEntry.fromJson(Map<String, dynamic>.from(m)))
            .toList(),
      ),
    );
  }

  /// 清空搜索并回到当前文件夹。
  Future<void> exitSearch() async {
    state = state.copyWith(clearSearch: true, selected: const {}, anchor: null);
    await refresh();
  }
}

final vaultUiProvider = NotifierProvider<VaultUiController, VaultUiState>(
    VaultUiController.new);
