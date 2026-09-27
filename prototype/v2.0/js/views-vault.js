/* ============================================================================
 * VaultSync V2.0 原型 — 保险箱资源管理器
 * 依据：docs/v2.0/05-02 §一/§三/§四/§五/§六/§七、03 §4.4、04 §2.1–§2.3
 * 交互约定：单击选中 · Ctrl/Shift 多选 · 双击打开 · 右键菜单 · 二级界面一律 modal
 * ==========================================================================*/
(function (global) {
  'use strict';

  var VS = global.VS;
  var h = VS.util.h, UI = VS.ui, F = VS.fmt, D = VS.data, S = VS.store, U = VS.util;

  VS.pages = VS.pages || {};
  VS.actions = VS.actions || {};

  /* ======================================================================
   * 工具
   * ==================================================================== */
  function denyWrite() { return VS.derive.denyWrite(); }

  function writeDisabledReason() {
    var code = denyWrite();
    if (!code) return null;
    return '不可用（错误码 ' + code + ' · ' + VS.ERR[code].title + '）：' + VS.ERR[code].hint;
  }

  function fileIcon(entry) {
    if (entry.kind === 'folder') return 'folder';
    switch (entry.type) {
      case 'image': return 'file-image';
      case 'archive': return 'file-archive';
      case 'video': return 'image';
      case 'sheet': return 'file-text';
      default: return 'file-text';
    }
  }

  function conflictBadges(entry) {
    var out = [];
    if (entry.conflict) out.push(UI.badge({ text: '冲突副本', tone: 'quarantine', icon: 'alert' }));
    if (entry.rev > 4) out.push(UI.badge({ text: 'rev ' + entry.rev, tone: 'stale' }));
    if (entry.truncated) out.push(UI.badge({ text: '仅索引前 64 MiB', tone: 'stale' }));
    if (!entry.fulltextSupported) out.push(UI.badge({ text: '不参与全文检索', tone: 'stale' }));
    if (S.get('readOnly')) out.push(UI.badge({ text: '只读', tone: 'stale', icon: 'lock' }));
    return out;
  }

  function sortedEntries(list) {
    var sort = S.get('sort');
    var arr = list.slice();
    arr.sort(function (a, b) {
      /* 文件夹永远在前 */
      if ((a.kind === 'folder') !== (b.kind === 'folder')) return a.kind === 'folder' ? -1 : 1;
      var k = sort.key;
      var va = k === 'size' ? a.size : k === 'modifiedMs' ? a.modifiedMs : k === 'rev' ? a.rev : a.name;
      var vb = k === 'size' ? b.size : k === 'modifiedMs' ? b.modifiedMs : k === 'rev' ? b.rev : b.name;
      var r = (typeof va === 'string') ? va.localeCompare(vb, 'zh-Hans-CN') : (va - vb);
      return sort.dir === 'asc' ? r : -r;
    });
    return arr;
  }

  function currentList() {
    var cwd = S.get('cwd');
    var list = D.children(cwd);
    var tags = S.get('filterTags');
    if (tags.length) list = list.filter(function (e) { return (e.tags || []).some(function (t) { return tags.indexOf(t) >= 0; }); });
    return sortedEntries(list);
  }

  /* ======================================================================
   * 主页面
   * ==================================================================== */
  VS.pages['vault'] = function (ctx) {
    var page = h('div', { class: 'page', style: { height: '100%' } });

    /* ---------------- 头部 ---------------- */
    page.appendChild(h('div', { class: 'page-head' }, [
      h('div', { class: 'page-titles' }, [
        h('h1', { class: 't-h1', text: '保险箱' }),
        h('div', { class: 't-caption', text: D.totals().files + ' 个文件 · ' + D.totals().folders + ' 个文件夹 · 已加密 ' + F.bytes(D.totals().size) + '（只统计密文条目字节，含容器开销）' })
      ]),
      h('div', { class: 'page-actions' }, [
        UI.seg({ value: S.get('view'), items: [
          { value: 'grid', label: '网格', icon: 'grid' },
          { value: 'list', label: '列表', icon: 'list' },
          { value: 'columns', label: '分栏', icon: 'columns' }
        ], onChange: function (v) { S.set({ view: v }, true); rerender(); } }),
        UI.btn({ icon: 'import', label: '导入', onClick: openImport }),
        UI.btn({ icon: 'folder', label: '新建文件夹', onClick: openNewFolder }),
        UI.btn({ icon: 'export', label: '导出', disabled: !S.get('selection').length,
          title: S.get('selection').length ? '导出到外部路径（只读降级下仍允许）' : '请先选择条目',
          onClick: openExport }),
        UI.iconBtn({ icon: 'more', label: '更多操作', onClick: function (e) { moreMenu(e.currentTarget); } })
      ])
    ]));

    /* ---------------- 工具条 ---------------- */
    var cwdPath = D.pathOf(S.get('cwd'));
    var crumbWrap = h('div', { class: 'vault-crumbs' });
    var crumbs = [{ id: 0, name: '根' }].concat(cwdPath.map(function (p) { return { id: p.id, name: p.name }; }));
    crumbs.forEach(function (c, i) {
      crumbWrap.appendChild(h('span', {
        class: 'crumb', 'aria-current': i === crumbs.length - 1 ? 'page' : null,
        onclick: function () { S.set({ cwd: c.id, selection: [] }, true); rerender(); }
      }, [h('span', { text: c.name })]));
      if (i < crumbs.length - 1) crumbWrap.appendChild(h('span', { class: 'crumb-sep', text: '›' }));
    });

    var sortSelect = UI.select({
      value: S.get('sort').key,
      options: [
        { value: 'name', label: '名称' }, { value: 'modifiedMs', label: '修改时间' },
        { value: 'size', label: '大小' }, { value: 'rev', label: '版本 rev' }
      ],
      onChange: function (v) { S.mutate(function (st) { st.sort.key = v; }, true); rerender(); }
    });
    var dirBtn = UI.iconBtn({
      icon: S.get('sort').dir === 'asc' ? 'arrow-up' : 'arrow-down',
      label: S.get('sort').dir === 'asc' ? '升序' : '降序',
      onClick: function () { S.mutate(function (st) { st.sort.dir = st.sort.dir === 'asc' ? 'desc' : 'asc'; }, true); rerender(); }
    });
    var tagFilter = UI.select({
      value: '',
      placeholder: '按标签筛选',
      options: [{ value: '', label: '全部标签' }].concat(D.allTags().map(function (t) { return { value: t.tag, label: t.tag + '（' + t.count + '）' }; })),
      onChange: function (v) { S.set({ filterTags: v ? [v] : [] }, true); rerender(); }
    });

    page.appendChild(h('div', { class: 'page-toolbar' }, [
      crumbWrap,
      h('span', { class: 'grow' }),
      UI.searchBox({ placeholder: '在保险箱中检索（名称 / 标签 / 全文）',
        onKeyDown: function (e) { if (e.key === 'Enter' && e.target.value.trim()) openSearch(e.target.value.trim()); } }),
      tagFilter,
      sortSelect, dirBtn,
      UI.iconBtn({ icon: 'refresh', label: '刷新列表', onClick: function () { UI.toast({ tone: 'info', title: '已刷新', msg: 'vault_core_vault_list 分页快照' }); rerender(); } })
    ]));

    /* ---------------- 主体：树 + 视图 ---------------- */
    var body = h('div', { class: 'vault-body' });
    body.appendChild(renderTree());

    var mainArea = h('div', { class: 'vault-main' });
    var list = currentList();
    if (!list.length) {
      mainArea.appendChild(UI.empty({
        icon: 'folder', title: '这个文件夹是空的',
        desc: '拖拽文件到窗口即可导入，或使用工具栏的「导入」按钮。',
        actions: [UI.btn({ label: '导入文件', variant: 'primary', icon: 'import', onClick: openImport })]
      }));
    } else if (S.get('view') === 'grid') {
      mainArea.appendChild(renderGrid(list));
    } else if (S.get('view') === 'list') {
      mainArea.appendChild(renderList(list));
    } else {
      mainArea.appendChild(renderColumns());
    }
    body.appendChild(mainArea);

    /* 超宽档常驻详情面板 */
    var detailId = S.get('detailId') || (S.get('selection').length === 1 ? S.get('selection')[0] : null);
    if (S.get('bp') === 'wide' && detailId) body.appendChild(renderDetailPanel(detailId));
    page.appendChild(body);

    /* ---------------- 底部任务条 ---------------- */
    page.appendChild(renderTaskStrip());

    return page;

    /* ---------- 内部渲染函数 ---------- */
    function rerender() {
      var host = document.querySelector('.vs-page');
      if (!host) return;
      host.innerHTML = '';
      host.appendChild(VS.pages['vault'](ctx));
    }

    function renderTree() {
      var tree = h('div', { class: 'vault-tree' });
      tree.appendChild(h('div', { class: 'pane-title', text: '文件夹' }));
      function node(folder, depth) {
        var count = D.childFiles(folder.id).length + D.childFolders(folder.id).length;
        tree.appendChild(h('div', {
          class: 'tree-node', style: { paddingLeft: (8 + depth * 14) + 'px' },
          'aria-current': S.get('cwd') === folder.id ? 'true' : null,
          onclick: function () { S.set({ cwd: folder.id, selection: [] }, true); rerender(); },
          oncontextmenu: function (e) { folderMenu(e, folder); }
        }, [
          h('span', { class: 'tree-ico', html: VS.icon('folder', 15) }),
          h('span', { class: 'tree-label', text: folder.name }),
          count ? h('span', { class: 'tree-count', text: String(count) }) : null
        ]));
        D.childFolders(folder.id).forEach(function (c) { node(c, depth + 1); });
      }
      tree.appendChild(h('div', {
        class: 'tree-node', 'aria-current': S.get('cwd') === 0 ? 'true' : null,
        onclick: function () { S.set({ cwd: 0, selection: [] }, true); rerender(); }
      }, [h('span', { class: 'tree-ico', html: VS.icon('drive', 15) }), h('span', { class: 'tree-label', text: '根目录' })]));
      D.vault.folders.filter(function (f) { return f.parent === 0; }).forEach(function (f) { node(f, 0); });
      tree.appendChild(h('div', { class: 'mt-3' }, [
        UI.btn({ label: '新建文件夹', size: 'sm', icon: 'plus', block: true, disabled: !!denyWrite(), title: writeDisabledReason() || '', onClick: openNewFolder })
      ]));
      return tree;
    }

    function renderGrid(list) {
      var grid = h('div', { class: 'file-grid' });
      list.forEach(function (e) {
        var sel = S.get('selection').indexOf(e.id) >= 0;
        var card = h('div', {
          class: 'file-card', tabindex: '0', dataset: { selected: String(sel) },
          'aria-selected': String(sel),
          'aria-label': e.name + ' · ' + (e.kind === 'folder' ? '文件夹' : F.bytes(e.size)) + ' · ' + F.dateShort(e.modifiedMs),
          onclick: function (ev) { select(e, ev); },
          ondblclick: function () { open(e); },
          oncontextmenu: function (ev) { entryMenu(ev, e); },
          onkeydown: function (ev) { if (ev.key === 'Enter') open(e); }
        }, [
          h('span', { class: 'fc-check' }),
          e.kind === 'file' ? h('span', { class: 'fc-lock', title: '端到端加密（AES-256-GCM）', html: VS.icon('lock', 13) }) : null,
          h('div', { class: 'fc-thumb', dataset: { kind: e.kind === 'folder' ? 'folder' : e.type } }, [
            h('span', { html: VS.icon(fileIcon(e), e.kind === 'folder' ? 30 : 26) })
          ]),
          h('div', { class: 'fc-name t-truncate', title: e.name, text: e.name }),
          h('div', { class: 'fc-meta' }, [
            h('span', { text: e.kind === 'folder' ? D.children(e.id).length + ' 项' : F.bytes(e.size) }),
            h('span', { text: '·' }),
            h('span', { text: F.relative(e.modifiedMs) })
          ]),
          h('div', { class: 'fc-badges' }, conflictBadges(e))
        ]);
        grid.appendChild(card);
      });
      return grid;
    }

    function renderList(list) {
      var wrap = h('div', { class: 'file-list' });
      wrap.appendChild(h('div', { class: 'file-row', style: { color: 'var(--c-text-2)', fontSize: '12px', cursor: 'default' } }, [
        h('span'), h('span', { text: '名称' }), h('span', { text: '大小' }),
        h('span', { text: '修改时间' }), h('span', { text: '状态徽标' })
      ]));
      list.forEach(function (e) {
        var sel = S.get('selection').indexOf(e.id) >= 0;
        wrap.appendChild(h('div', {
          class: 'file-row', tabindex: '0', dataset: { selected: String(sel) },
          onclick: function (ev) { select(e, ev); },
          ondblclick: function () { open(e); },
          oncontextmenu: function (ev) { entryMenu(ev, e); }
        }, [
          h('span', { class: 'fr-ico', html: VS.icon(fileIcon(e), 18) }),
          h('span', { class: 'fr-name' }, [
            h('span', { class: 't-truncate', text: e.name }),
            e.kind === 'file' ? h('span', { style: { color: 'var(--c-encrypt)' }, html: VS.icon('lock', 12) }) : null
          ]),
          h('span', { class: 't-num t-caption', text: e.kind === 'folder' ? '—' : F.bytes(e.size) }),
          h('span', { class: 't-caption', text: F.dateShort(e.modifiedMs) }),
          h('span', { class: 'fr-badges' }, conflictBadges(e))
        ]));
      });
      return wrap;
    }

    function renderColumns() {
      var wrap = h('div', { class: 'columns-view' });
      var cwd = S.get('cwd');
      /* 第 1 栏：当前层级的父链 */
      var pane1 = h('div', { class: 'columns-pane' }, [h('div', { class: 'pane-title', text: '文件夹树' })]);
      D.vault.folders.filter(function (f) { return f.parent === 0; }).forEach(function (f) {
        pane1.appendChild(h('div', {
          class: 'tree-node', 'aria-current': String(cwd) === String(f.id) ? 'true' : null,
          onclick: function () { S.set({ cwd: f.id, selection: [] }, true); rerender(); }
        }, [h('span', { class: 'tree-ico', html: VS.icon('folder', 15) }), h('span', { class: 'tree-label', text: f.name })]));
      });
      wrap.appendChild(pane1);

      /* 第 2 栏：当前层级条目 */
      var pane2 = h('div', { class: 'columns-pane' }, [h('div', { class: 'pane-title', text: '条目' })]);
      currentList().forEach(function (e) {
        pane2.appendChild(h('div', {
          class: 'tree-node', 'aria-current': S.get('selection').indexOf(e.id) >= 0 ? 'true' : null,
          onclick: function () { S.set({ selection: [e.id], detailId: e.id }, true); rerender(); },
          ondblclick: function () { open(e); },
          oncontextmenu: function (ev) { entryMenu(ev, e); }
        }, [
          h('span', { class: 'tree-ico', html: VS.icon(fileIcon(e), 15) }),
          h('span', { class: 'tree-label', text: e.name }),
          e.kind === 'folder' ? h('span', { class: 'tree-count', text: '›' }) : null
        ]));
      });
      wrap.appendChild(pane2);

      /* 第 3 栏：选中项详情（分栏内嵌） */
      var selId = S.get('selection')[0] || S.get('detailId');
      var pane3 = h('div', { class: 'columns-pane' }, [h('div', { class: 'pane-title', text: '详情' })]);
      if (selId) pane3.appendChild(renderDetailInline(selId));
      else pane3.appendChild(UI.empty({ icon: 'info', title: '未选择条目', desc: '单击左侧条目查看详情' }));
      wrap.appendChild(pane3);
      return wrap;
    }

    function renderDetailPanel(id) {
      var panel = h('div', { class: 'vault-detail detail-panel' });
      panel.appendChild(renderDetailInline(id));
      return panel;
    }

    function renderDetailInline(id) {
      var e = D.entry(id);
      if (!e) return UI.empty({ icon: 'help', title: '条目不存在' });
      var frag = document.createDocumentFragment();
      frag.appendChild(h('div', { class: 'detail-hero' }, [
        h('div', { class: 'dh-thumb', style: { height: '68px' } }, [h('span', { html: VS.icon(fileIcon(e), 30) })]),
        h('div', { class: 't-strong t-truncate', style: { maxWidth: '100%' }, title: e.name, text: e.name }),
        h('div', { class: 't-caption', text: e.kind === 'folder' ? '文件夹' : F.bytes(e.size) + ' · ' + F.dateShort(e.modifiedMs) }),
        h('div', { class: 'row gap-1' }, conflictBadges(e)),
        h('div', { class: 'row gap-2' }, [
          UI.btn({ label: '打开', size: 'sm', variant: 'primary', onClick: function () { open(e); } }),
          UI.btn({ label: '详情', size: 'sm', onClick: function () { openDetail(e); } })
        ])
      ]));
      frag.appendChild(h('div', { class: 'detail-section' }, [
        h('div', { class: 'detail-section-title' }, [UI.icon('lock', 12), h('span', { text: '加密' })]),
        UI.kv([
          { k: '算法套件', v: e.kind === 'folder' ? '—' : (e.cipherSuite === 2 ? '2 · X25519+ML-KEM-768 混合' : '1 · 经典 ECDH') },
          { k: '格式版本', v: 'VSVB v' + (e.formatVer || 3) },
          { k: '文件密钥', v: e.kind === 'folder' ? '—' : 'FSKey（不显示密钥值）', mono: true },
          { k: 'nonce 前缀', v: e.noncePrefix || '—', mono: true },
          { k: '分片数', v: e.metaShardCount || '—' }
        ])
      ]));
      frag.appendChild(h('div', { class: 'detail-section' }, [
        h('div', { class: 'detail-section-title' }, [UI.icon('tag', 12), h('span', { text: '标签' })]),
        h('div', { class: 'row wrap gap-1' }, (e.tags || []).length ? e.tags.map(function (t) { return UI.chip(t); }) : [h('span', { class: 't-caption', text: '无标签' })]),
        h('div', { class: 'mt-2' }, [UI.btn({ label: '编辑标签', size: 'sm', disabled: !!denyWrite(), title: writeDisabledReason() || '', onClick: function () { tagModal(e); } })])
      ]));
      return frag;
    }
  };

  /* ======================================================================
   * 选择与打开
   * ==================================================================== */
  function select(entry, ev) {
    var sel = S.get('selection').slice();
    if (ev && (ev.ctrlKey || ev.metaKey)) {
      var i = sel.indexOf(entry.id);
      if (i >= 0) sel.splice(i, 1); else sel.push(entry.id);
    } else if (ev && ev.shiftKey) {
      var list = currentList().map(function (e) { return e.id; });
      var last = sel.length ? sel[sel.length - 1] : entry.id;
      var a = list.indexOf(last), b = list.indexOf(entry.id);
      if (a >= 0 && b >= 0) {
        sel = list.slice(Math.min(a, b), Math.max(a, b) + 1);
      } else sel = [entry.id];
    } else {
      sel = [entry.id];
    }
    S.mutate(function (st) { st.selection = sel; st.detailId = sel.length === 1 ? sel[0] : null; });
    VS.app.renderRoute();
  }

  function open(entry) {
    if (entry.kind === 'folder') {
      S.mutate(function (st) { st.cwd = entry.id; st.selection = []; st.detailId = null; });
      VS.app.renderRoute();
      return;
    }
    openDetail(entry);
  }

  /* ======================================================================
   * 右键菜单（R-01 … R-14，逐项启用 / 禁用条件）
   * ==================================================================== */
  function entryMenu(ev, entry) {
    ev.preventDefault();
    var deny = denyWrite();
    var sel = S.get('selection').indexOf(entry.id) >= 0 ? S.get('selection') : [entry.id];
    var multi = sel.length > 1;
    var isFolder = entry.kind === 'folder';

    var items = [
      { label: '打开', icon: 'external', onClick: function () { open(entry); } },
      { label: '在详情中查看', icon: 'info', onClick: function () { openDetail(entry); } },
      { sep: true },
      { group: '导出（只读降级下仍允许）' },
      { label: '导出…', icon: 'export', sub: '导出到外部路径，不改保险箱', onClick: function () { openExport(sel); } },
      { label: '导出并覆盖已存在文件', icon: 'save', sub: 'overwrite = true', onClick: function () { openExport(sel, true); } },
      { sep: true },
      { group: '写库操作（维护态 / 只读降级下禁用）' },
      { label: '重命名…', icon: 'edit', shortcut: 'F2', disabled: !!deny, reason: writeDisabledReason(),
        onClick: function () { renameModal(entry); } },
      { label: '编辑标签…', icon: 'tag', disabled: !!deny, reason: writeDisabledReason(),
        onClick: function () { tagModal(entry); } },
      { label: '移动到…', icon: 'migrate', disabled: !!deny, reason: writeDisabledReason(),
        onClick: function () { moveModal(entry); } },
      { label: isFolder ? '删除文件夹（递归）…' : (multi ? '删除所选 ' + sel.length + ' 项…' : '删除…'), icon: 'trash', danger: true,
        disabled: !!deny, reason: writeDisabledReason(), onClick: function () { confirmWipe(sel, 'delete'); } },
      { label: '安全擦除…', icon: 'erase', danger: true, disabled: !!deny, reason: writeDisabledReason(),
        onClick: function () { confirmWipe(sel, 'erase'); } },
      { sep: true },
      { group: '分享' },
      { label: '创建阅后即焚分享…', icon: 'share', disabled: !!deny, reason: writeDisabledReason(),
        onClick: function () { createShare(entry); } },
      { label: '分享记录', icon: 'history', onClick: function () { S.set({ route: 'sync' }, true); VS.app.renderRoute(); } },
      { sep: true },
      { group: '只读操作' },
      { label: '校验此文件', icon: 'check-circle', onClick: function () { verifyModal([entry.id]); } },
      { label: '生成缩略图', icon: 'image', disabled: !S.cap('CAP_THUMBNAIL'),
        reason: 'CAP_THUMBNAIL 未置位 → 缩略图预览区不渲染', onClick: function () {
          UI.toast({ tone: 'info', title: '缩略图已生成', msg: '≤256px 长边 · 不落盘 · 不进剪贴板 · 落锁即清' });
        } },
      { label: '隐写嵌入…', icon: 'stego', disabled: !S.get('stegoEnabled'),
        reason: S.get('stegoEnabled') ? '' : '隐写未启用（错误码 7）→ 入口不渲染',
        onClick: function () { S.set({ route: 'stego' }, true); VS.app.renderRoute(); } },
      { sep: true },
      { label: '显示详情', icon: 'info', onClick: function () { openDetail(entry); } }
    ];
    UI.menu(ev.clientX, ev.clientY, items);
  }

  function folderMenu(ev, folder) { ev.preventDefault(); entryMenu(ev, folder); }

  function moreMenu(anchor) {
    var r = anchor.getBoundingClientRect();
    UI.menu(r.left - 120, r.bottom + 6, [
      { label: '全选', icon: 'check', shortcut: 'Ctrl+A', onClick: function () {
          S.mutate(function (st) { st.selection = currentList().map(function (e) { return e.id; }); });
          VS.app.renderRoute();
        } },
      { label: '反选', icon: 'refresh', onClick: function () {
          var cur = S.get('selection');
          S.mutate(function (st) { st.selection = currentList().map(function (e) { return e.id; }).filter(function (id) { return cur.indexOf(id) < 0; }); });
          VS.app.renderRoute();
        } },
      { sep: true },
      { label: '全库完整性校验…', icon: 'layers', onClick: function () { verifyModal(null); } },
      { label: '查看已删除与待清理', icon: 'trash', onClick: deletedModal },
      { label: '存储占用统计', icon: 'gauge', onClick: storageModal },
      { sep: true },
      { label: '按标签筛选…', icon: 'filter', onClick: function (ev) { tagFilterMenu(ev); } },
      { label: '导出当前列表清单', icon: 'export', onClick: function () {
          UI.toast({ tone: 'info', title: '已导出清单', msg: '仅含元数据，不含文件名原文以外的敏感字段' });
        } }
    ]);
  }

  function tagFilterMenu() {
    var m = UI.modal({
      title: '按标签筛选', size: 'sm',
      body: [h('div', { class: 'row wrap gap-2' }, D.allTags().map(function (t) {
        var on = S.get('filterTags').indexOf(t.tag) >= 0;
        return h('span', {
          class: 'chip' + (on ? ' chip-accent' : ''), style: { cursor: 'pointer' },
          onclick: function () {
            var cur = S.get('filterTags').slice();
            var i = cur.indexOf(t.tag);
            if (i >= 0) cur.splice(i, 1); else cur.push(t.tag);
            S.set({ filterTags: cur }, true); m.close(); VS.app.renderRoute();
          }
        }, [h('span', { text: t.tag + '（' + t.count + '）' })]);
      }))],
      footer: [UI.btn({ label: '清除筛选', onClick: function () { S.set({ filterTags: [] }, true); m.close(); VS.app.renderRoute(); } }),
               UI.btn({ label: '关闭', variant: 'primary', onClick: function () { m.close(); } })]
    });
  }

  /* ======================================================================
   * 文件详情（12 个分区）
   * ==================================================================== */
  function openDetail(entry) {
    if (entry.kind === 'folder') { folderDetail(entry); return; }
    var chunks = [];
    for (var i = 0; i < Math.min(entry.chunkCount, 240); i++) {
      chunks.push(i < entry.chunkCount * 0.8 ? 'done' : (i === Math.floor(entry.chunkCount * 0.8) ? 'running' : 'pending'));
    }
    var tabs = [
      { id: 'basic', label: '基本信息' },
      { id: 'crypto', label: '加密与分片' },
      { id: 'index', label: '检索与缩略图' },
      { id: 'erase', label: '擦除与副本' },
      { id: 'audit', label: '审计关联' }
    ];
    var activeTab = 'basic';
    var bodyHost = h('div', { class: 'col gap-4' });
    var tabHost = h('div');
    function renderTabs() {
      tabHost.innerHTML = '';
      tabHost.appendChild(UI.tabs({
        value: activeTab,
        items: tabs.map(function (t) { return { value: t.id, label: t.label }; }),
        onChange: function (v) { activeTab = v; renderTabs(); renderTab(); }
      }));
    }
    renderTabs();

    function renderTab() {
      bodyHost.innerHTML = '';
      if (activeTab === 'basic') {
        bodyHost.appendChild(UI.card({ title: 'A · 文件基本信息', body: UI.kv([
          { k: 'id', v: 'f_' + entry.id, mono: true, copy: true },
          { k: '名称', v: entry.name },
          { k: '类型', v: entry.kind + ' · ' + entry.mime },
          { k: '父文件夹', v: (D.entry(entry.parent) || {}).name || '根目录' },
          { k: '大小', v: F.bytes(entry.size) },
          { k: '创建时间', v: F.dateLong(entry.createdMs) },
          { k: '修改时间', v: F.dateLong(entry.modifiedMs) },
          { k: '版本 rev', v: String(entry.rev) }
        ]) }));
        bodyHost.appendChild(UI.card({ title: '标签', body: h('div', { class: 'row wrap gap-2' }, (entry.tags || []).map(function (t) { return UI.chip(t); })) }));
      } else if (activeTab === 'crypto') {
        bodyHost.appendChild(UI.card({ title: 'B · 加密元数据', body: UI.kv([
          { k: 'cipher_suite', v: entry.cipherSuite === 2 ? '2 · X25519 + ML-KEM-768 混合' : '1 · 经典 ECDH + HKDF' },
          { k: 'kdf_ver', v: '2（Argon2id 64 MiB · t=3 · p=2）' },
          { k: 'chunk_cfg', v: '1 · CDC 64–256 KiB（小文件 16 KiB）' },
          { k: 'flags', v: 'bit0 擦除元数据区 = 1 · bit1 元数据分片 = 1 · bit2 已终验 = 1 · bit3–15 = 0' },
          { k: 'format_ver', v: '3（>3 向前拒绝，绝不尽力解析）' },
          { k: 'hdr_crc32', v: entry.hdrCrc32, mono: true }
        ]) }));
        bodyHost.appendChild(UI.card({ title: 'C · CDC 分块详情', sub: '块上限 256 KiB · 下限 16 KiB；非末片先落 .part、终验后原子改名', body: [
          UI.kv([
            { k: 'meta_shard_count', v: String(entry.metaShardCount) },
            { k: 'meta_shard_span', v: '4096 块 / 片' },
            { k: 'data_region_offset', v: '0x' + D.hash(8), mono: true },
            { k: 'chunk_count', v: F.num(entry.chunkCount) },
            { k: 'nonce_prefix', v: entry.noncePrefix + '（4B BE 前缀 ‖ 8B BE 计数器）', mono: true },
            { k: 'file_sha256', v: entry.fileSha256, mono: true, copy: true }
          ]),
          h('div', { class: 'mt-3' }, [h('div', { class: 'detail-section-title', text: '块状态矩阵（前 ' + chunks.length + ' 块）' })]),
          UI.chunkMatrix(chunks),
          h('div', { class: 'legend mt-3' }, [
            legendItem('var(--c-success)', 'done'), legendItem('var(--c-accent-alt)', 'running'),
            legendItem('var(--c-danger)', 'failed'), legendItem('var(--c-surface-alt)', 'pending')
          ])
        ] }));
        bodyHost.appendChild(UI.card({ title: 'D · 密钥层次（只显示存在性，不显示密钥值）', body: UI.kv([
          { k: 'FSKey', v: entry.fskeyAlgo, mono: true },
          { k: 'FSK', v: 'HKDF(MK, "fsk/' + entry.parent + '")' + (entry.rotateWith ? ' · 轮换后为索引内 fskey_override' : ''), mono: true },
          { k: '块 nonce', v: 'per-file 随机前缀 ‖ 块计数器（同一 FSKey 下禁止随机 nonce）' },
          { k: '从属密钥', v: '索引 key_id=1 · 隐写载荷 key_id=5（归安全中心展示）' },
          { k: 'rotated_with', v: entry.rotateWith, mono: true }
        ]) }));
      } else if (activeTab === 'index') {
        bodyHost.appendChild(UI.card({ title: 'H · 检索与提取状态', body: [
          UI.kv([
            { k: 'extract_ver', v: '3' },
            { k: 'truncated', v: entry.truncated ? 'true — 仅索引了前 64 MiB' : 'false' },
            { k: '类型支持', v: entry.fulltextSupported ? '参与全文检索' : '该类型不参与全文检索' },
            { k: '单文件上限', v: '提取文本 64 MiB · token 10 000' }
          ]),
          entry.truncated ? h('div', { class: 'mt-3' }, [UI.alertbar({ tone: 'warn', title: '仅索引了前 64 MiB', text: '超出部分可被文件名与标签命中，但不会被全文命中。' })]) : null
        ] }));
        bodyHost.appendChild(UI.card({ title: 'I · 缩略图信息', sub: '不落盘 · 不进剪贴板 · 锁定或转只读立即清空', body: S.cap('CAP_THUMBNAIL')
          ? (entry.thumb
            ? UI.kv([{ k: 'mime', v: 'image/png' }, { k: '尺寸', v: '256 × 160（≤256 px 长边）' }, { k: 'bytes', v: F.bytes(40960) }, { k: 'sha256', v: D.hash(64), mono: true, copy: true }])
            : h('div', { class: 't-caption', text: '该类型不支持缩略图预览（仅 png / jpg / webp / gif 首帧），预览区不渲染。' }))
          : UI.empty({ icon: 'ban', title: 'CAP_THUMBNAIL 未置位', desc: '按能力三档处理：整块不渲染，不显示占位「即将推出」。' })
        }));
      } else if (activeTab === 'erase') {
        var meta = UI.ERASE_CLASS[entry.eraseClass] || UI.ERASE_CLASS[4];
        bodyHost.appendChild(UI.card({ title: 'J · 擦除状态', body: [
          h('div', { class: 'row gap-3' }, [UI.eraseBadge(entry.eraseClass), UI.badge({ text: '介质：' + mediaKindLabel(entry.mediaKind), tone: 'stale' })]),
          h('div', { class: 'mt-3' }, [UI.kv([
            { k: '抹除强度原值', v: 'erase_class = ' + entry.eraseClass + '（' + meta.label + '）' },
            { k: '介质类型', v: 'media_kind = ' + entry.mediaKind + '（' + mediaKindLabel(entry.mediaKind) + '）' },
            { k: '说明', v: meta.note }
          ])]),
          h('div', { class: 'mt-3' }, [UI.alertbar({
            tone: entry.eraseClass === 3 ? 'info' : 'warn',
            title: entry.eraseClass === 3 ? '「安全擦除」字样成立' : '介质覆写未执行',
            text: entry.eraseClass === 3
              ? '仅当 erase_class = platform_secure 且 method_bits 含平台安全删除位时才允许出现「安全擦除」字样。'
              : '文案必须包含「介质覆写未执行」，禁止显示为「安全擦除」。'
          })])
        ] }));
        bodyHost.appendChild(UI.card({ title: 'G · 副本与误删保护', body: [
          UI.alertbar({ tone: 'info', title: '误删保护窗口 24 小时', text: '窗口内删除先保留加密副本、FSKey 仍可用 → 「加密擦除」可撤销。窗口结束后副本与密钥一并销毁。' }),
          h('div', { class: 'mt-3' }, [UI.btn({ label: '查看已删除副本', size: 'sm', onClick: deletedModal })])
        ] }));
      } else {
        bodyHost.appendChild(UI.card({ title: 'K · 审计关联', sub: '脱敏口径：不含目标路径、不含名称原文、不含查询词', body: [
          UI.table({
            columns: [
              { key: 'seq', label: 'seq', render: function (r) { return h('span', { class: 't-mono', text: '#' + r.seq }); } },
              { key: 'opLabel', label: '操作' },
              { key: 'tsMs', label: '时间', render: function (r) { return F.dateLong(r.tsMs); } },
              { key: 'headHash', label: '链头', render: function (r) { return h('span', { class: 't-mono', text: U.shortHash(r.headHash, 8, 6) }); } }
            ],
            rows: D.auditLog.filter(function (a) { return a.cat === 'vault'; }).slice(0, 8),
            maxHeight: '260px'
          }),
          h('div', { class: 'mt-3' }, [UI.muted('缩略图生成、存储占用统计、列表/详情读取、诊断包导出为「无审计路径」的有意设计，UI 不得暗示已记录审计。')])
        ] }));
      }
    }
    renderTab();

    var m = UI.modal({
      title: entry.name, sub: F.bytes(entry.size) + ' · rev ' + entry.rev + ' · ' + F.dateLong(entry.modifiedMs),
      size: 'xl', flush: false,
      body: [tabHost, h('div', { class: 'mt-4' }, [bodyHost])],
      footer: [
        UI.btn({ label: '校验', icon: 'check-circle', onClick: function () { verifyModal([entry.id]); } }),
        UI.btn({ label: '导出', icon: 'export', onClick: function () { openExport([entry.id]); } }),
        S.get('stegoEnabled') ? UI.btn({ label: '隐写嵌入', icon: 'stego', onClick: function () { S.set({ route: 'stego' }, true); m.close(); VS.app.renderRoute(); } }) : null,
        h('span', { class: 'foot-left' }),
        UI.btn({ label: '重命名', disabled: !!denyWrite(), title: writeDisabledReason() || '', onClick: function () { renameModal(entry); } }),
        UI.btn({ label: '删除', variant: 'danger', disabled: !!denyWrite(), title: writeDisabledReason() || '', onClick: function () { confirmWipe([entry.id], 'delete'); } }),
        UI.btn({ label: '关闭', variant: 'primary', onClick: function () { m.close(); } })
      ]
    });
  }

  function legendItem(color, label) {
    return h('span', { class: 'lg-item' }, [h('span', { class: 'lg-swatch', style: { background: color } }), h('span', { text: label })]);
  }
  function mediaKindLabel(k) {
    return ({ 1: 'SSD（固态盘）', 2: 'HDD（机械盘）', 3: '网络盘', 4: '不可判定 Unknown' })[k] || '不可判定 Unknown';
  }

  function folderDetail(folder) {
    var kids = D.children(folder.id);
    var m = UI.modal({
      title: '文件夹属性', sub: folder.name, size: 'md',
      body: [
        UI.kv([
          { k: '名称', v: folder.name },
          { k: '条目数', v: F.num(kids.length) + '（递归 ' + F.num(kids.length * 3) + '）' },
          { k: '创建时间', v: F.dateLong(folder.createdMs) },
          { k: '修改时间', v: F.dateLong(folder.modifiedMs) },
          { k: 'FSK', v: 'HKDF(MK, "fsk/' + folder.id + '") — 文件夹级密钥，文件夹内文件共享 FSK', mono: true }
        ]),
        h('div', { class: 'mt-4' }, [UI.alertbar({ tone: 'info', title: '递归删除语义', text: '删除文件夹会递归删除其中全部条目；24 小时保护窗口内保留加密副本，窗口结束一并销毁。' })])
      ],
      footer: [
        UI.btn({ label: '重命名', disabled: !!denyWrite(), title: writeDisabledReason() || '', onClick: function () { m.close(); renameModal(folder); } }),
        UI.btn({ label: '递归删除…', variant: 'danger', disabled: !!denyWrite(), title: writeDisabledReason() || '', onClick: function () { m.close(); confirmWipe([folder.id], 'delete'); } }),
        UI.btn({ label: '关闭', variant: 'primary', onClick: function () { m.close(); } })
      ]
    });
  }

  /* ======================================================================
   * 导入向导（M-01）
   * ==================================================================== */
  function openImport() {
    var deny = denyWrite();
    if (deny) { UI.toast({ tone: deny === 9 ? 'warn' : 'info', title: '无法导入', msg: '错误码 ' + deny + ' · ' + VS.ERR[deny].title }); return; }
    var target = UI.select({
      value: String(S.get('cwd')),
      options: [{ value: '0', label: '根目录' }].concat(D.vault.folders.map(function (f) { return { value: String(f.id), label: f.name }; }))
    });
    var suite = UI.select({
      value: '1',
      options: [
        { value: '1', label: '1 · 经典 ECDH + HKDF（默认）', sub: '全平台可用' },
        { value: '2', label: '2 · X25519 + ML-KEM-768 混合', sub: S.cap('CAP_PQ_HYBRID') ? '需要两端 CAP_PQ_HYBRID 均置位并协商通过' : '本端 CAP_PQ_HYBRID 未置位 → 不可选，且不假装成功', disabled: !S.cap('CAP_PQ_HYBRID') }
      ]
    });
    if (!S.cap('CAP_PQ_HYBRID')) suite.querySelector('.select-btn').setAttribute('aria-disabled', 'true');
    var extract = UI.checkbox({ label: '提取文本进入全文索引', sub: 'extract = true（默认）；仅对支持的类型生效', checked: true });
    var drop = h('div', {
      class: 'card panel-alt', style: { padding: 'var(--sp-6)', textAlign: 'center', borderStyle: 'dashed' }
    }, [
      h('span', { style: { color: 'var(--c-text-2)' }, html: VS.icon('import', 36) }),
      h('div', { class: 't-strong mt-2', text: '把文件拖到这里，或点击选择' }),
      h('div', { class: 't-caption', text: '单文件上限 1 TiB；超过则拒绝并给出可读诊断，绝不截断' })
    ]);

    var m = UI.modal({
      title: '导入到保险箱', sub: '写入路径：明文只存在于引擎层；已完成的块先落在 .part', size: 'lg',
      body: [
        drop,
        h('div', { class: 'mt-4' }, [UI.kv([
          { k: '模拟选择', v: '3 个文件 · 共 ' + F.bytes(48234496) },
          { k: '其中 1 个', v: '备份归档-2025.tar · ' + F.bytes(268435456) + ' · 将触发 CDC 分块（256 KiB/块）' }
        ])]),
        h('div', { class: 'grid-2 mt-4' }, [
          UI.field({ label: '目标文件夹', control: target }),
          S.cap('CAP_PQ_HYBRID')
            ? UI.field({ label: '加密算法套件', control: suite, hint: '协商失败一律取低档 1 并写审计 + 显示降级提示' })
            : h('div', { class: 'field' }, [
                UI.field({ label: '加密算法套件', control: suite, hint: 'CAP_PQ_HYBRID 未置位' }),
                UI.alertbar({ tone: 'warn', title: '能力不支持（码 13）', text: '混合 KEM 本端不可用：选项置灰且不降级为假装成功；实际写入将使用经典档 1。' })
              ])
        ]),
        h('div', { class: 'mt-3' }, [extract]),
        h('div', { class: 'mt-4' }, [UI.alertbar({
          tone: 'info', title: '取消语义（码 12）',
          text: '取消时已完成块保留在 .part（不删除），索引不变；可在同一会话内重试复用已完成部分。'
        })])
      ],
      footer: [
        UI.btn({ label: '取消', onClick: function () { m.close(); UI.toast({ tone: 'info', title: '已取消', msg: '错误码 12 · 已完成部分保留在 .part' }); } }),
        UI.btn({ label: '开始导入', variant: 'primary', icon: 'import', onClick: function () {
          m.close();
          UI.toast({ tone: 'info', title: '导入已入队', msg: 'TASK_QUEUED(6) → TASK_PROGRESS(7) → TASK_DONE(8)' });
          S.mutate(function (st) {
            D.queue.unshift({ taskId: 'task-' + Date.now(), name: '备份归档-2025.tar', dir: 'up', kind: 'import',
              state: 'running', doneBytes: 0, totalBytes: 268435456, doneChunks: 0, totalChunks: 1024,
              rateBps: 2097152, etaMs: 128000, resumable: true, peer: '本机', path: 'direct', startedMs: Date.now(), priority: 'interactive' });
          }, true);
        } })
      ]
    });
  }

  /* ======================================================================
   * 导出（M-02）
   * ==================================================================== */
  function openExport(ids, forceOverwrite) {
    ids = ids || S.get('selection');
    if (!ids || !ids.length) { UI.toast({ tone: 'warn', title: '请先选择要导出的条目' }); return; }
    var overwrite = UI.checkbox({ label: '覆盖已存在的目标文件', sub: 'overwrite = false（默认）；未勾选且目标存在 → 错误码 7', checked: !!forceOverwrite });
    var path = UI.input({ mono: true, value: 'D:\\导出\\VaultSync\\' });
    var m = UI.modal({
      title: '导出到外部路径', sub: ids.length + ' 个条目 · 导出是唯一把明文写出引擎的路径，必须由你显式发起', size: 'md',
      body: [
        UI.field({ label: '目标路径', control: path, hint: '路径不会写入审计（可能含用户名等个人信息）' }),
        h('div', { class: 'mt-3' }, [overwrite]),
        h('div', { class: 'mt-4' }, [UI.card({ title: '将导出', flush: true, body: UI.table({
          columns: [{ key: 'name', label: '名称' }, { key: 'size', label: '大小', render: function (r) { return F.bytes(r.size); } }],
          rows: ids.map(function (id) { var e = D.entry(id); return { id: id, name: e ? e.name : id, size: e ? e.size : 0 }; }),
          maxHeight: '200px'
        }) })]),
        h('div', { class: 'mt-3' }, [UI.alertbar({
          tone: 'info', title: '只读降级下仍允许导出',
          text: '导出到外部路径不改动保险箱，因此在只读（码 9）与维护态（码 10）下保持可用。'
        })]),
        h('div', { class: 'mt-2' }, [UI.muted('导出重组后 file_sha256 不一致 → 删除 .tmp、返回 3 并记录 ERROR_DIAG，明确不输出可疑明文。取消（12）→ 删除 .tmp，外部路径不留半成品。')])
      ],
      footer: [
        UI.btn({ label: '取消', onClick: function () { m.close(); UI.toast({ tone: 'info', title: '已取消', msg: '错误码 12 · 外部路径不留半成品' }); } }),
        UI.btn({ label: '开始导出', variant: 'primary', icon: 'export', onClick: function () {
          m.close();
          UI.toast({ tone: 'success', title: '导出完成', msg: ids.length + ' 个文件已写出到 ' + path.value + '（原文明文仅在引擎层短暂存在）' });
        } })
      ]
    });
  }

  /* ======================================================================
   * 重命名 / 标签 / 移动
   * ==================================================================== */
  function renameModal(entry) {
    var input = UI.input({ value: entry.name });
    var m = UI.modal({
      title: '重命名', sub: '审计不含名称原文（脱敏口径）', size: 'sm',
      body: [UI.field({ label: '新名称', control: input, hint: 'UTF-8；不改变密文内容与 FSKey' })],
      footer: [UI.btn({ label: '取消', onClick: function () { m.close(); } }),
               UI.btn({ label: '重命名', variant: 'primary', onClick: function () {
                 if (!input.value.trim()) return;
                 entry.name = input.value.trim(); m.close(); VS.app.renderRoute();
                 UI.toast({ tone: 'success', title: '已重命名', msg: 'vault.rename 已写审计（不含名称原文）' });
               } })]
    });
  }

  function tagModal(entry) {
    var wrap = h('div', { class: 'row wrap gap-2' });
    function render() {
      wrap.innerHTML = '';
      if (!entry.tags.length) wrap.appendChild(h('span', { class: 't-caption', text: '暂无标签' }));
      entry.tags.forEach(function (t) {
        wrap.appendChild(UI.chip({ text: t, onRemove: function () {
          entry.tags = entry.tags.filter(function (x) { return x !== t; }); render();
        } }));
      });
    }
    render();
    var input = UI.input({ placeholder: '输入标签后回车，或从下方建议中选择' });
    var suggest = h('div', { class: 'row wrap gap-2 mt-2' }, D.allTags().slice(0, 8).map(function (t) {
      return UI.chip({ text: t.tag, onRemove: function () {
        if (entry.tags.indexOf(t.tag) < 0) entry.tags.push(t.tag); render();
      } });
    }));
    input.addEventListener('keydown', function (e) {
      if (e.key === 'Enter' && input.value.trim()) {
        if (entry.tags.indexOf(input.value.trim()) < 0) entry.tags.push(input.value.trim());
        input.value = ''; render();
      }
    });
    var m = UI.modal({
      title: '编辑标签', sub: '打标签属于写库操作：只读降级（9）与维护态（10）下禁用', size: 'sm',
      body: [
        UI.card({ title: '当前标签', body: wrap }),
        h('div', { class: 'mt-3' }, [UI.field({ label: '新增标签', control: input })]),
        h('div', { class: 'mt-3' }, [h('div', { class: 'detail-section-title', text: '已有标签建议' }), suggest])
      ],
      footer: [UI.btn({ label: '取消', onClick: function () { m.close(); } }),
               UI.btn({ label: '保存', variant: 'primary', onClick: function () {
                 m.close(); UI.toast({ tone: 'success', title: '标签已保存', msg: 'vault.tag 已写审计（不含标签原文）' }); VS.app.renderRoute();
               } })]
    });
  }

  function moveModal(entry) {
    var sel = UI.select({
      value: '0',
      block: true,
      options: [{ value: '0', label: '根目录' }].concat(D.vault.folders.map(function (f) { return { value: String(f.id), label: f.name }; }))
    });
    var m = UI.modal({
      title: '移动到…', size: 'sm',
      body: [UI.field({ label: '目标文件夹', control: sel, hint: '移动会改变父节点并递增 rev；不改动密文' })],
      footer: [UI.btn({ label: '取消', onClick: function () { m.close(); } }),
               UI.btn({ label: '移动', variant: 'primary', onClick: function () {
                 entry.parent = Number(sel.getValue()); m.close(); VS.app.renderRoute();
                 UI.toast({ tone: 'success', title: '已移动' });
               } })]
    });
  }

  /* ======================================================================
   * 新建文件夹
   * ==================================================================== */
  function openNewFolder() {
    if (denyWrite()) { UI.toast({ tone: 'warn', title: '无法新建文件夹', msg: '错误码 ' + denyWrite() + ' · ' + VS.ERR[denyWrite()].title }); return; }
    var input = UI.input({ placeholder: '文件夹名称' });
    var m = UI.modal({
      title: '新建文件夹', size: 'sm',
      body: [UI.field({ label: '名称', control: input, hint: '将创建 fsk/{folder_id} 派生的文件夹级密钥 FSK' })],
      footer: [UI.btn({ label: '取消', onClick: function () { m.close(); } }),
               UI.btn({ label: '创建', variant: 'primary', onClick: function () {
                 var name = input.value.trim() || '新建文件夹';
                 var id = ++D.vault.nextId;
                 D.vault.folders.push({ id: id, kind: 'folder', name: name, parent: S.get('cwd'), size: 0,
                   createdMs: Date.now(), modifiedMs: Date.now(), tags: [], depth: 1 });
                 m.close(); VS.app.renderRoute();
                 UI.toast({ tone: 'success', title: '已创建', msg: name });
               } })]
    });
  }
  VS.actions['newFolder'] = openNewFolder;
  VS.actions['import'] = openImport;

  /* ======================================================================
   * 删除 / 擦除确认（M-03 · ConfirmWipe 新增确认段）
   * ==================================================================== */
  function confirmWipe(ids, mode) {
    var entries = ids.map(function (id) { return D.entry(id); }).filter(Boolean);
    var totalSize = entries.reduce(function (a, e) { return a + (e.size || 0); }, 0);
    var probeEntry = entries[0] || {};
    var eraseClass = probeEntry.eraseClass || 4;
    var mediaKind = probeEntry.mediaKind || 4;
    var meta = UI.ERASE_CLASS[eraseClass] || UI.ERASE_CLASS[4];
    var isErase = mode === 'erase';
    var protectUntil = Date.now() + 24 * 3600 * 1000;

    var multiRound = UI.checkbox({ label: '启用多轮覆写（HDD 场景）', sub: '默认关闭：时间成本高，需显式开启', checked: false });
    var body = [
      UI.card({ title: '① 影响范围', body: UI.kv([
        { k: '条目数', v: F.num(entries.length) + ' 项' + (entries.some(function (e) { return e.kind === 'folder'; }) ? '（含递归子项）' : '') },
        { k: '合计字节', v: F.bytes(totalSize) },
        { k: '条目列表', v: entries.slice(0, 4).map(function (e) { return e.name; }).join('、') + (entries.length > 4 ? ' 等 ' + entries.length + ' 项' : '') }
      ]) }),
      UI.card({ title: '② 本次将采用的擦除强度', sub: '由 vault_core_erase_class_probe(path) 实时返回，显示引擎原值', body: [
        h('div', { class: 'row gap-3' }, [UI.eraseBadge(eraseClass), UI.badge({ text: 'media_kind = ' + mediaKind + ' · ' + mediaKindLabel(mediaKind), tone: 'stale' })]),
        h('div', { class: 'mt-3' }, [UI.kv([
          { k: 'erase_class', v: eraseClass + ' · ' + meta.label },
          { k: 'method', v: 'bit0 覆写 / bit1 TRIM / bit2 平台安全删除 / bit3 加密擦除' },
          { k: 'method_bits', v: '0b' + (eraseClass === 3 ? '0110' : eraseClass === 2 ? '0001' : eraseClass === 1 ? '1000' : '0000'), mono: true },
          { k: '说明', v: meta.note }
        ])]),
        eraseClass !== 3 ? h('div', { class: 'mt-3' }, [UI.alertbar({
          tone: 'warn', title: '介质覆写未执行',
          text: '「安全擦除」字样仅在 erase_class = platform_secure 且 method_bits 含平台安全删除位时成立。本次不满足该条件。'
        })]) : null,
        h('div', { class: 'mt-3' }, [
          h('div', { class: 'detail-section-title', text: 'degradations[]（逐条显示，不隐藏）' }),
          h('ul', { class: 'col gap-1' }, [
            mediaKind === 1 ? h('li', { class: 't-caption', text: '· 本机为固态盘：TRIM 为提示性指令，不保证物理擦除' }) : null,
            mediaKind === 3 ? h('li', { class: 't-caption', text: '· 网络盘不提供块级删除语义' }) : null,
            mediaKind === 4 ? h('li', { class: 't-caption', text: '· 平台无 TRIM API，已降为 crypto_only' }) : null,
            h('li', { class: 't-caption', text: '· 探测失败不算失败：降为 unknown + crypto_only，不阻塞删除' })
          ].filter(Boolean))
        ])
      ] }),
      UI.card({ title: '③ 误删保护窗口', body: [
        UI.alertbar({ tone: 'info', title: '24 小时内可撤销',
          text: '保护窗口内删除会先保留加密副本、FSKey 仍可用 →「加密擦除」可撤销。窗口结束（' + F.dateLong(protectUntil) + '）后副本与密钥一并销毁。' }),
        h('div', { class: 'mt-3' }, [UI.switchCtl({ label: '同时删除已配对设备上的副本', sub: '会广播删除传播（墓碑）到全部对端', checked: true })])
      ] }),
      UI.card({ title: '④ 附加选项', body: [
        mediaKind === 2 ? multiRound : h('div', { class: 't-caption', text: '多轮覆写仅对 HDD 有意义，当前介质不适用。' }),
        h('div', { class: 'mt-3' }, [UI.checkbox({ label: '物理删除失败时标记为「待清理」并在启动时重试', checked: true })])
      ] })
    ];

    var m = UI.modal({
      title: isErase ? '安全擦除' : '删除', sub: entries.length + ' 项 · 该操作会写入链式审计日志',
      size: 'lg', tone: 'danger',
      body: body,
      footer: [
        UI.btn({ label: '取消', onClick: function () { m.close(); } }),
        UI.btn({ label: isErase ? '执行擦除' : '删除', variant: 'danger', icon: isErase ? 'erase' : 'trash', onClick: function () {
          m.close();
          var failed = probeEntry.id >= 106; /* 演示：部分条目物理删除失败 */
          if (failed) {
            UI.toast({ tone: 'warn', title: '介质擦除待完成', msg: '容器已标记「待清理」，启动清理扫描将重试。注意：不是「已擦除」。', duration: 7000 });
          } else {
            UI.toast({ tone: 'success', title: '已删除', msg: '已加密擦除；介质覆写未执行。24 小时内可撤销。' });
          }
          confirmPhraseSecondStep(ids, failed);
        } })
      ]
    });
  }

  function confirmPhraseSecondStep(ids, failed) {
    /* 二次确认：文档要求删除前确认框必须显示强度（已在第一步给出），此处给出撤销入口 */
    var m = UI.modal({
      title: failed ? '介质擦除待完成' : '删除已提交', size: 'sm', tone: failed ? 'danger' : 'maintenance',
      body: [
        UI.alertbar({
          tone: failed ? 'danger' : 'info',
          title: failed ? '物理删除失败，已标记「待清理」' : '24 小时内可撤销',
          text: failed
            ? 'UI 显示「介质擦除待完成」，不得显示「已擦除」。容器将在启动清理扫描时重试。'
            : '保护窗口内加密副本仍可解封恢复。'
        }),
        h('div', { class: 'mt-3' }, [UI.muted('审计写入失败不阻断业务：事后取证与可用性的取舍（见 LOG.md）。')])
      ],
      footer: [
        UI.btn({ label: '查看已删除与待清理', onClick: function () { m.close(); deletedModal(); } }),
        UI.btn({ label: failed ? '我知道了' : '撤销删除', variant: 'primary', onClick: function () {
          m.close(); UI.toast({ tone: 'success', title: '已撤销删除', msg: '加密副本已恢复，FSKey 仍可用' });
        } })
      ]
    });
  }

  function deletedModal() {
    var m = UI.modal({
      title: '已删除与待清理', sub: '误删保护窗口 24 小时；窗口外进入待清理', size: 'lg',
      body: [UI.table({
        columns: [
          { key: 'name', label: '名称' },
          { key: 'size', label: '大小', render: function (r) { return F.bytes(r.size); } },
          { key: 'deletedMs', label: '删除时间', render: function (r) { return F.dateLong(r.deletedMs); } },
          { key: 'eraseClass', label: '擦除强度', render: function (r) { return UI.eraseBadge(r.eraseClass); } },
          { key: 'state', label: '状态', render: function (r) {
            return r.pendingCleanup
              ? UI.badge({ text: '介质擦除待完成', tone: 'danger', icon: 'alert' })
              : UI.badge({ text: '可恢复 · 剩 ' + F.duration(r.protectUntil - Date.now()), tone: 'success' });
          } },
          { key: 'act', label: '', align: 'right', render: function (r) {
            return h('div', { class: 'row gap-1', style: { justifyContent: 'flex-end' } }, [
              r.pendingCleanup
                ? UI.btn({ label: '立即重试清理', size: 'sm', variant: 'danger', disabled: !!denyWrite(), title: writeDisabledReason() || '' })
                : UI.btn({ label: '恢复', size: 'sm', disabled: !!denyWrite(), title: writeDisabledReason() || '', onClick: function () {
                    UI.toast({ tone: 'success', title: '已恢复', msg: r.name });
                  } })
            ]);
          } }
        ],
        rows: D.deletedEntries
      })]
    });
  }

  function storageModal() {
    var byFolder = {};
    D.vault.entries.forEach(function (e) {
      var f = D.entry(e.parent);
      var key = f ? f.name : '根目录';
      byFolder[key] = (byFolder[key] || 0) + e.size;
    });
    var rows = Object.keys(byFolder).map(function (k) { return { name: k, size: byFolder[k] }; })
      .sort(function (a, b) { return b.size - a.size; });
    var max = rows.length ? rows[0].size : 1;
    var m = UI.modal({
      title: '存储占用', sub: '真值档：递归 vault_core_vault_list 的 size 求和（服务层聚合，走工作 Isolate）', size: 'md',
      body: [
        UI.kv([{ k: '已加密总量', v: F.bytes(D.totals().size) + '（含容器开销）' }, { k: '文件数', v: F.num(D.totals().files) }]),
        h('div', { class: 'mt-4 col gap-2' }, rows.slice(0, 8).map(function (r) {
          return h('div', { class: 'row gap-3' }, [
            h('span', { class: 't-truncate', style: { width: '120px' }, text: r.name }),
            h('span', { class: 'grow' }, [UI.bar(r.size / max, 'accent', 'sm')]),
            h('span', { class: 't-caption t-num', style: { width: '88px', textAlign: 'right' }, text: F.bytes(r.size) })
          ]);
        })),
        h('div', { class: 'mt-4' }, [UI.alertbar({
          tone: 'info', title: '整行不展示：磁盘占用百分比 / 卷剩余空间',
          text: '契约清单中没有卷容量查询接口 → 该行不渲染（不写「待接入」）。这是原则 7「不谎报能力」的落地。'
        })])
      ]
    });
  }

  /* ======================================================================
   * 完整性校验（M-27）
   * ==================================================================== */
  function verifyModal(ids) {
    var finished = false;
    var bar = UI.bar(0, 'accent');
    var status = h('div', { class: 't-caption', text: '准备中…' });
    var cancelBtn;
    var m = UI.modal({
      title: ids ? '校验所选文件' : '全库完整性校验', sub: 'vault_verify_all · 可取消（错误码 12 视为正常结果）', size: 'md',
      body: [
        h('div', { class: 'row gap-3' }, [UI.ring({ pct: 0, size: 56, stroke: 6 }), h('div', { class: 'grow' }, [bar, status])]),
        h('div', { class: 'mt-4' }, [UI.alertbar({
          tone: 'info', title: '失败即定位到具体块',
          text: 'GCM 校验失败会定位到具体分片 / 块；该文件被隔离并提示重新导入，不会静默丢弃。'
        })])
      ],
      footer: function () {
        cancelBtn = UI.btn({ label: '取消扫描', onClick: function () {
          finished = true; m.close();
          UI.toast({ tone: 'info', title: '已取消', msg: '错误码 12 · 资源在 5s 内释放（tasks::cancel_releases_within_5s）' });
        } });
        return [UI.btn({ label: '后台运行', onClick: function () { m.close(); UI.toast({ tone: 'info', title: '已转入后台', msg: '进度见底部任务条' }); } }), cancelBtn];
      }
    });
    var pct = 0;
    var timer = setInterval(function () {
      if (finished) { clearInterval(timer); return; }
      pct += 0.07;
      if (pct >= 1) { pct = 1; clearInterval(timer); finished = true; }
      bar.firstChild.style.width = Math.round(pct * 100) + '%';
      var ring = m.el.querySelector('.ring');
      if (ring) ring.replaceWith(UI.ring({ pct: pct, size: 56, stroke: 6 }));
      status.textContent = '已检查 ' + F.num(Math.round(pct * 4812)) + ' 项 · 失败 ' + (pct > 0.6 ? 1 : 0) + ' 项';
      if (pct === 1) {
        status.textContent = '完成 · 4 812 项已检查 · 1 项失败（已隔离：f_' + D.hash(8) + '）';
        setTimeout(function () {
          m.close();
          UI.toast({ tone: 'warn', title: '校验完成，1 项失败', msg: '失败文件已隔离并提示重新导入，不静默丢弃', duration: 7000 });
        }, 700);
      }
    }, 240);
  }

  /* ======================================================================
   * 检索（P1-A / P1-A-1）
   * ==================================================================== */
  function openSearch(initialQuery) {
    var query = initialQuery || '';
    var classes = { name: true, tag: true, content: true };
    var limit = 50;
    var fulltextAvailable = S.cap('CAP_FULLTEXT');

    var input = UI.input({ value: query, placeholder: '输入关键词后回车', onKeyDown: function (e) { if (e.key === 'Enter') run(); } });
    var resultsHost = h('div', { class: 'col gap-3' });
    var summary = h('div', { class: 't-caption', text: '尚未检索' });

    var clsRow = h('div', { class: 'row gap-3' }, ['name', 'tag', 'content'].map(function (k) {
      var label = { name: '文件名', tag: '标签', content: '全文内容' }[k];
      var cb = UI.checkbox({ label: label, checked: classes[k], onChange: function (e) { classes[k] = e.target.checked; run(); } });
      if (k === 'content' && !fulltextAvailable) { cb.input.disabled = true; cb.querySelector('.check-text').appendChild(h('span', { class: 'check-sub', text: 'CAP_FULLTEXT 未置位 → 本引擎仅支持文件名与标签检索' })); }
      return cb;
    }));

    function run() {
      resultsHost.innerHTML = '';
      var q = (input.value || query).toLowerCase().trim();
      if (!q) { resultsHost.appendChild(UI.empty({ icon: 'search', title: '输入关键词开始检索' })); summary.textContent = '尚未检索'; return; }
      var hits = [];
      D.vault.entries.forEach(function (e) {
        if (classes.name && e.name.toLowerCase().indexOf(q) >= 0) hits.push({ e: e, cls: 'name' });
        else if (classes.tag && (e.tags || []).some(function (t) { return t.toLowerCase().indexOf(q) >= 0; })) hits.push({ e: e, cls: 'tag' });
        else if (classes.content && fulltextAvailable && e.fulltextSupported && !e.truncated) hits.push({ e: e, cls: 'content', snippet: true });
      });
      summary.textContent = '命中 ' + F.num(hits.length) + ' 项 · 上限 ' + F.num(limit) + ' · rankVer 3';
      if (!hits.length) {
        resultsHost.appendChild(UI.empty({ icon: 'search', title: '没有命中', desc: '不支持的类型不产生命中，也不会伪造命中。' }));
        return;
      }
      ['name', 'tag', 'content'].forEach(function (cls) {
        var group = hits.filter(function (x) { return x.cls === cls; });
        if (!group.length) return;
        resultsHost.appendChild(h('div', { class: 'detail-section-title', text: { name: '文件名命中', tag: '标签命中', content: '全文命中' }[cls] + '（' + group.length + '）' }));
        group.slice(0, limit).forEach(function (hit) {
          var e = hit.e;
          var node = h('div', { class: 'list-row', onclick: function () { openDetail(e); } }, [
            h('span', { class: 'fr-ico', html: VS.icon(fileIcon(e), 18) }),
            h('div', { class: 'lr-main' }, [
              h('div', { class: 'lr-title' }, [
                h('span', { class: 't-truncate', text: e.name }),
                h('span', { class: 't-caption', text: '· ' + F.bytes(e.size) + ' · ' + F.relative(e.modifiedMs) })
              ]),
              hit.snippet ? h('div', { class: 'hit-snippet mt-1' }, [
                h('span', { text: '…季度营收 ' }),
                h('mark', { text: '«▲»' }),
                h('span', { class: 'redact', text: ' ••• ' }),
                h('span', { text: '环比…' }),
                h('div', { class: 't-caption mt-1', text: '第 3 段 · 字符 412' })
              ]) : (e.truncated ? h('div', { class: 't-caption mt-1', text: '仅索引了前 64 MiB，此处不产生全文命中' }) : null),
              !e.fulltextSupported ? h('div', { class: 't-caption mt-1', text: '该类型不参与全文检索' }) : null
            ]),
            h('span', { class: 't-caption t-mono', text: 'rev ' + e.rev })
          ]);
          resultsHost.appendChild(node);
        });
      });
      resultsHost.appendChild(h('div', { class: 'mt-2' }, [UI.alertbar({
        tone: 'info', title: '此处有命中但不展示',
        text: 'scan:true 的段落整段替换为 •••；片段与位置提示来自引擎脱敏结果，外壳不接触明文。'
      })]));
    }

    var m = UI.modal({
      title: '检索', sub: 'vault_search_v2 · 单文件提取上限 64 MiB / 10 000 token', size: 'lg',
      body: [
        h('div', { class: 'row gap-2' }, [h('div', { class: 'grow' }, [input]), UI.btn({ label: '检索', variant: 'primary', icon: 'search', onClick: run })]),
        h('div', { class: 'mt-3 row-between' }, [clsRow, UI.select({ value: '50', options: [{ value: '20', label: '20 条' }, { value: '50', label: '50 条' }, { value: '100', label: '100 条' }] })]),
        fulltextAvailable ? null : h('div', { class: 'mt-3' }, [UI.alertbar({ tone: 'warn', title: '能力不支持（码 13）', text: '本引擎仅支持文件名与标签检索 —— 不谎报全文检索。' })]),
        h('div', { class: 'mt-3' }, [summary]),
        h('div', { class: 'mt-2' }, [resultsHost])
      ],
      footer: [UI.btn({ label: '关闭', variant: 'primary', onClick: function () { m.close(); } })]
    });
    if (query) run();
  }
  VS.actions['openSearch'] = openSearch;

  /* ======================================================================
   * 阅后即焚分享（M-1x）
   * ==================================================================== */
  function createShare(entry) {
    var expiry = UI.select({
      value: '60',
      options: [
        { value: '5', label: '5 分钟' }, { value: '15', label: '15 分钟' },
        { value: '60', label: '1 小时' }, { value: '1440', label: '24 小时' }
      ]
    });
    var sessions = UI.select({ value: '1', options: [{ value: '1', label: '1 次（阅后即焚）' }, { value: '5', label: '5 次' }, { value: '0', label: '不限（有时效）' }] });
    var device = UI.select({ value: D.devices[1].id, options: D.devices.filter(function (d) { return !d.self; }).map(function (d) { return { value: d.id, label: d.name + (d.online ? ' · 在线' : ' · 离线') }; }) });
    var m = UI.modal({
      title: '创建分享票据', sub: '阅后即焚：接收方读取后票据立即失效，密文不落中继', size: 'md',
      body: [
        UI.card({ title: '分享对象', body: [UI.field({ label: '接收设备', control: device })] }),
        h('div', { class: 'mt-3 grid-2' }, [
          UI.field({ label: '有效时长', control: expiry, hint: '过期未读取 → 票据作废，明文不出引擎' }),
          UI.field({ label: '可读取次数', control: sessions, hint: '1 次 = 阅后即焚' })
        ]),
        h('div', { class: 'mt-4' }, [UI.card({ title: '票据', body: [
          UI.copyField('vsburn://' + D.hash(24), '复制票据链接'),
          h('div', { class: 'mt-2' }, [UI.muted('票据本身不含密钥；接收方需通过 Noise 信道用双方指纹核验后解封。')])
        ] })]),
        h('div', { class: 'mt-3' }, [UI.alertbar({
          tone: 'warn', title: '对方未接受，分享已撤销',
          text: '若接收方在有效期内未接受，票据自动撤销且不重试。已读取的文件不再自动擦除。'
        })])
      ],
      footer: [
        UI.btn({ label: '取消', onClick: function () { m.close(); } }),
        UI.btn({ label: '创建', variant: 'primary', icon: 'share', onClick: function () {
          m.close();
          D.shares.unshift({ id: 'shr-' + D.hash(4), fileId: entry.id, file: entry.name, kind: 'burn',
            createdMs: Date.now(), expiresMs: Date.now() + 3600000, status: 'waiting', sessions: 0, maxSessions: 1,
            device: (D.devices.find(function (d) { return d.id === device.getValue(); }) || {}).name || '—', burnedMs: null });
          UI.toast({ tone: 'success', title: '票据已创建', msg: 'p2p.burn 已写审计' });
        } })
      ]
    });
  }

  /* ======================================================================
   * 底部任务条
   * ==================================================================== */
  function renderTaskStrip() {
    var running = D.queue.filter(function (t) { return t.state === 'running' || t.state === 'queued'; });
    if (!running.length) return h('span', { class: 'hidden' });
    var first = running[0];
    return h('div', { class: 'card card-pad row gap-3', style: { position: 'sticky', bottom: '0' } }, [
      h('span', { style: { color: 'var(--c-accent-alt)' }, html: VS.icon('sync', 16) }),
      h('div', { class: 'grow' }, [
        h('div', { class: 'row-between' }, [
          h('span', { class: 't-truncate', text: first.name }),
          h('span', { class: 't-caption t-num', text: F.pct(first.doneBytes / first.totalBytes) + ' · ' + (F.rate(first.rateBps) || '速率未知') })
        ]),
        UI.bar(first.doneBytes / first.totalBytes, 'accent', 'sm')
      ]),
      running.length > 1 ? UI.badge({ text: '共 ' + running.length + ' 个任务', tone: 'stale' }) : null,
      UI.btn({ label: '查看队列', size: 'sm', onClick: function () { VS.go('queue'); } })
    ]);
  }

})(window);
