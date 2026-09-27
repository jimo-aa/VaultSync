/* ============================================================================
 * VaultSync V2.0 移动端原型 — 保险箱域（<720 档）
 * 依据：docs/v2.0/03 §2.2 断点表 / §4.3 手机窄屏主壳 · 04 §1.2 减配声明 / §六 差异矩阵
 *       05-02 §三/§四/§五/§六/§七 保险箱载荷与语义
 *
 * 移动端形态硬约束（与桌面端刻意不同）：
 *   1. 单栏列表为主；视图切换器只有「列表 / 网格」——手机上不提供「分栏」（不是置灰）
 *   2. 二级/三级界面一律 MUI.sheet / actionSheet / confirmSheet；禁止 UI.modal
 *   3. 详情为全屏路由（tab:false + back:true）
 *   4. 列表项操作：长按 → MUI.actionSheet；左滑为等价降级
 *   5. 搜索是全屏路由，不是顶栏输入框
 *   6. 写库类操作在只读降级（9）/ 维护态（10）下 disabled + reason；导出仍可用
 *   7. 能力三档：能力位未置位 → 整块不渲染（返回 null），其次禁用并写明原因，绝不假装成功
 * ==========================================================================*/
(function (global) {
  'use strict';

  var VS = global.VS;
  var h = VS.util.h, UI = VS.ui, MUI = VS.mui, F = VS.fmt, D = VS.data, S = VS.store, U = VS.util;

  VS.pages = VS.pages || {};
  VS.actions = VS.actions || {};

  /* ======================================================================
   * 0. 通用工具
   * ==================================================================== */

  /** 写操作判据：唯一来源 VS.derive.denyWrite()（9 只读 / 10 维护 / null 允许） */
  function writeDeny() { return VS.derive.denyWrite(); }

  /**
   * 动作面板项 / 行 reason 文案：一律带错误码。
   * 注意：MUI.actionSheet 不会把 reason 写进 title / aria-label，只会渲染成「不可用」
   * 角标并在点击时 toast 全文 —— 因此 reason 必须是自解释的完整句子，不能依赖悬浮提示。
   */
  function writeReason(code) {
    if (!code) return '';
    return '不可用（错误码 ' + code + ' · ' + VS.ERR[code].title + '）：' + VS.ERR[code].hint;
  }

  /** 写操作项的 disabled / reason 组合（面板项与行共用） */
  function writeGate() {
    var code = writeDeny();
    return { code: code, disabled: !!code, reason: writeReason(code) };
  }

  /**
   * 面板项文案：MUI.actionSheet 只渲染 label / sub，不会把 reason 写进 title，
   * 也不会做 aria-label —— 因此把错误码直接并入可见 label，保证禁用原因对用户可见、
   * 对读屏可见（契约要求：所有可交互元素必须有可读标签）。
   */
  function itemLabel(label, gate) {
    if (!gate.disabled) return label;
    return gate.code ? label + '（码 ' + gate.code + '）' : label;
  }

  function entryIcon(e) {
    if (!e) return 'file';
    if (e.kind === 'folder') return 'folder';
    switch (e.type) {
      case 'image': return 'file-image';
      case 'archive': return 'file-archive';
      case 'video': return 'image';
      case 'sheet': return 'file-text';
      default: return 'file-text';
    }
  }

  function mediaKindLabel(k) {
    return ({ 1: 'SSD（固态盘）', 2: 'HDD（机械盘）', 3: '网络盘', 4: '不可判定 Unknown' })[k] || '不可判定 Unknown';
  }

  function eraseMeta(cls) { return UI.ERASE_CLASS[cls] || UI.ERASE_CLASS[4]; }

  /** method_bits 原型映射（真实值由 vault_core_erase_class_probe 返回） */
  function methodBitsFor(cls) {
    return ({ 1: '0b1000', 2: '0b0001', 3: '0b0110', 4: '0b0000' })[cls] || '0b0000';
  }

  /**
   * 允许出现「安全擦除」字样的充要条件（05-02 §三.4）：
   * erase_class = platform_secure(3) 且 method_bits 含「平台安全删除」位（bit2）。
   * 这里按位串显式判定，不做正则花活。
   */
  var PLATFORM_SECURE_DELETE_BIT = 2;
  function secureEraseWordAllowed(cls) {
    if (cls !== 3) return false;
    var bits = methodBitsFor(cls);          /* 形如 '0b0110' */
    var payload = bits.slice(2);
    return payload.charAt(payload.length - 1 - PLATFORM_SECURE_DELETE_BIT) === '1';
  }

  /**
   * 操作类型标签。措辞硬约束：更强的「安全擦除」表述仅在充要条件成立时使用，
   * 否则一律用物理事实描述（删除 + 加密擦除），绝不夸大也不含糊。
   */
  function opTypeLabel(isErase, wordOk) {
    if (!isErase) return '删除（delete）';
    return wordOk ? '安全擦除（erase）' : '删除 + 加密擦除（erase，介质覆写未执行）';
  }

  function degradationsFor(cls, mediaKind) {
    var out = [];
    if (cls === 1) out.push('平台无块级安全删除指令 → 仅销毁 FSKey，密文容器留在介质上；介质覆写未执行。');
    if (cls === 2) out.push('已执行单轮零覆写（method_bits 含覆写位），不构成 NIST 多轮抹除。');
    if (cls === 4) out.push('介质类型探测失败 → 降为 unknown + crypto_only，不阻塞删除；介质覆写未执行。');
    if (mediaKind === 1) out.push('本机为固态盘：TRIM 为提示性指令，不保证物理擦除（垃圾回收可能保留旧数据块）。');
    if (mediaKind === 3) out.push('网络盘不提供块级删除语义，服务端副本不受本地擦除控制。');
    if (mediaKind === 4) out.push('平台无 TRIM API 或探测超时 → 已降为 crypto_only。');
    out.push('探测失败不算失败：降为 unknown + crypto_only，不阻塞删除流程。');
    return out;
  }

  function chunkStates(e, max) {
    max = max || 120;
    var states = [], n = Math.min(e.chunkCount || 1, max);
    var runningAt = Math.floor(n * 0.78);
    for (var i = 0; i < n; i++) {
      if (i === Math.floor(n * 0.52)) states.push('failed');
      else if (i < runningAt) states.push('done');
      else if (i === runningAt) states.push('running');
      else states.push('pending');
    }
    return states;
  }

  /** 图例（内联色块；views.css 未在移动端装载，不引用 .legend/.lg-swatch） */
  function legendRow(pairs) {
    return h('div', { class: 'row wrap gap-3 mt-2' }, pairs.map(function (p) {
      return h('span', { class: 'row gap-2' }, [
        h('span', { style: { width: '10px', height: '10px', borderRadius: '3px', background: p.color, flex: 'none' } }),
        h('span', { class: 't-caption', text: p.label })
      ]);
    }));
  }

  /** 徽标组：手机上一行放得下才放，超出的在详情页给全量 */
  function entryBadges(e, compact) {
    var out = [];
    if (!e || e.kind !== 'file') return out;
    if (e.conflict) out.push(UI.badge({ text: '冲突副本', tone: 'quarantine', icon: 'alert' }));
    if (compact) return out.slice(0, 2);
    if (e.rev > 4) out.push(UI.badge({ text: 'rev ' + e.rev, tone: 'stale' }));
    if (e.truncated) out.push(UI.badge({ text: '仅索引前 64 MiB', tone: 'stale' }));
    if (!e.fulltextSupported) out.push(UI.badge({ text: '不参与全文检索', tone: 'stale' }));
    if (e.kind === 'folder') out.push(UI.badge({ text: '文件夹', tone: 'accent' }));
    return out;
  }

  /* ---------------------------------------------------------------------- *
   * 原型 Mock 全文语料（SCOPE: mock）
   *
   * 设计约束：外壳（Dart / JS）永不接触明文，真实片段只可能来自引擎的脱敏结果。
   * 原型没有引擎，因此这里为每个可检索条目构造一份**确定性**的伪语料，只用于
   * 演示「命中 → 片段 + 位置提示」的交互形态：
   *   · 语料由条目元数据（类型 / 标签 / rev / 名称词干）确定性派生，保证每次一致；
   *   · 命中与否由检索词真实决定 —— 查不到就是查不到，绝不伪造命中；
   *   · 片段一律用「脱敏 + 有限上下文」形态呈现（••• 表示 scan:true 整段替换）。
   * 报告口径：这是原型演示数据，不代表引擎的提取或排序能力。
   * ---------------------------------------------------------------------- */
  var MOCK_PARAGRAPHS = [
    '本季度营收较上一季度环比增长，主要来自企业客户续约。',
    '项目范围、里程碑与验收口径见附表，风险项已登记。',
    '评审结论：架构分层清晰，密钥层次符合零信任约定。',
    '客户名录与联系方式属于敏感字段，导出需显式二次确认。',
    '现场采集素材已归档，缩略图按需生成且不落盘。',
    '轮换说明：MK 由 CSPRNG 生成且永不变更，KEK 仅用于解封。',
    '同步策略采用 CDC 分块，局部修改不触发全文件重加密。',
    '审计链采用链式哈希，写入失败不阻断业务但发 ERROR_DIAG。'
  ];
  var TEXT_INDEX_CACHE = {};
  /** 确定性伪语料：同一 id 每次得到同一批段落 */
  function mockTextIndex(e) {
    var key = String(e.id);
    if (TEXT_INDEX_CACHE[key]) return TEXT_INDEX_CACHE[key];
    var stem = String(e.name || '').replace(/\.[A-Za-z0-9]+$/, '');
    var paras = ['标题：' + stem + '（rev ' + (e.rev || 1) + '）']
      .concat(e.tags || [])
      .concat(MOCK_PARAGRAPHS);
    TEXT_INDEX_CACHE[key] = paras;
    return paras;
  }
  function mockContentHits(e, q) {
    if (!q) return null;
    var paras = mockTextIndex(e);
    for (var i = 0; i < paras.length; i++) {
      var at = paras[i].toLowerCase().indexOf(q);
      if (at >= 0) return { para: i + 1, at: at, text: paras[i] };
    }
    return null;
  }

  /* ======================================================================
   * 1. 底部面板通用件
   * ==================================================================== */

  /** 面板内小节标题（mobile.css 的 .msection-title；不用 views.css 的 .detail-section-title） */
  function secTitle(icon, text) {
    return h('div', { class: 'msection-title' }, [
      h('span', { html: VS.icon(icon, 13) }),
      h('span', { text: text })
    ]);
  }

  /** 面板内的等宽说明块（无 .detail-section-title 依赖） */
  function noteBlock(text) {
    return h('div', { class: 't-caption', style: { lineHeight: '1.7', marginTop: '6px' }, text: text });
  }

  /** 整宽主按钮（mobile.css 的 .mbtn-block，≥48px 触控目标） */
  function blockBtn(label, variant, onClick, o) {
    o = o || {};
    return h('button', {
      class: 'mbtn-block', type: 'button',
      dataset: { variant: variant || 'ghost' },
      title: o.title || label, 'aria-label': o.title || label,
      disabled: !!o.disabled,
      onclick: o.disabled ? null : onClick
    }, [
      o.icon ? h('span', { html: VS.icon(o.icon, 18) }) : null,
      h('span', { text: label })
    ]);
  }

  /** 数据来源标注（原则 7：真值就必须写清来源） */
  function srcNote(text) {
    return h('div', { class: 't-caption t-mono', style: { opacity: '.8', marginTop: '4px' }, text: '来源：' + text });
  }

  /* ======================================================================
   * 2. 服务动作（面板 + 页内共用）
   * ==================================================================== */

  /** 条目「⋯ / 长按」动作面板 —— 与桌面端右键菜单信息架构对齐，形态改为底部动作面板 */
  function entryActions(ctx, e) {
    var gate = writeGate();
    var isFolder = e.kind === 'folder';
    var items = [
      { label: '打开', icon: 'external', sub: isFolder ? '进入该文件夹' : '全屏打开文件详情', onClick: function () { openEntry(ctx, e); } },
      { label: '详情', icon: 'info', sub: '基本信息 / 加密 / 检索 / 擦除 / 审计', onClick: function () { openDetail(ctx, e); } },
      { group: '导出（只读降级与维护态下仍可用）' },
      { label: '导出…', icon: 'export', sub: '导出到外部路径；不改动保险箱，故不受写冻结影响', onClick: function () { exportSheet([e], ctx); } },
      { group: '写库操作（只读降级 9 / 维护态 10 下禁用）' },
      { label: itemLabel('重命名…', gate), icon: 'edit', disabled: gate.disabled, reason: gate.reason, onClick: function () { renameSheet(e, ctx); } },
      { label: itemLabel('编辑标签…', gate), icon: 'tag', disabled: gate.disabled, reason: gate.reason, onClick: function () { tagSheet(e, ctx); } },
      { label: itemLabel('移动到…', gate), icon: 'migrate', disabled: gate.disabled, reason: gate.reason, onClick: function () { moveSheet(e, ctx); } },
      {
        label: itemLabel('创建阅后即焚分享…', { disabled: gate.disabled || !S.cap('CAP_BURN_SHARE'), code: gate.disabled ? gate.code : (S.cap('CAP_BURN_SHARE') ? null : 13) }),
        icon: 'share',
        disabled: gate.disabled || !S.cap('CAP_BURN_SHARE'),
        reason: gate.disabled ? gate.reason : 'CAP_BURN_SHARE 未置位（错误码 13 · 能力不支持）：本端不提供阅后即焚分享入口，不降级为假装成功',
        onClick: function () { shareSheet(e); }
      },
      { group: '只读操作' },
      { label: '校验此文件', icon: 'check-circle', onClick: function () { verifySheet([e.id]); } },
      { label: itemLabel('删除…', gate), icon: 'trash', danger: true, disabled: gate.disabled, reason: gate.reason, onClick: function () { wipeSheet([e], 'delete', ctx); } },
      { label: itemLabel('安全擦除…', gate), icon: 'erase', danger: true, disabled: gate.disabled, reason: gate.reason, onClick: function () { wipeSheet([e], 'erase', ctx); } },
      { label: '显示详情', icon: 'scan', onClick: function () { openDetail(ctx, e); } }
    ];
    MUI.actionSheet({ title: e.name, sub: isFolder ? '文件夹 · 递归语义在删除确认中给出' : F.bytes(e.size) + ' · rev ' + e.rev, items: items });
  }

  function openEntry(ctx, e) {
    if (e.kind === 'folder') ctx.go('folder', { id: e.id });
    else ctx.go('file', { id: e.id });
  }

  function openDetail(ctx, e) {
    if (e.kind === 'folder') folderSheet(e, ctx);
    else ctx.go('file', { id: e.id });
  }

  /* ---------------- 2.1 导入向导（M-01 移动端形态） ------------------- */
  function importSheet(cwdId) {
    var deny = writeDeny();
    if (deny) {
      UI.toast({ tone: deny === 9 ? 'warn' : 'info', title: '无法导入', msg: '错误码 ' + deny + ' · ' + VS.ERR[deny].title + '：' + VS.ERR[deny].hint });
      return;
    }
    var target = MUI.select({
      value: String(cwdId),
      block: true,
      options: [{ value: '0', label: '根目录' }].concat(D.vault.folders.map(function (f) { return { value: String(f.id), label: f.name }; }))
    });
    var pqOk = S.cap('CAP_PQ_HYBRID');
    var suite = MUI.select({
      value: '1',
      block: true,
      options: [
        { value: '1', label: '1 · 经典 ECDH + HKDF', sub: '全平台可用，默认档' },
        {
          value: '2', label: '2 · X25519 + ML-KEM-768 混合 KEM',
          sub: pqOk ? '需要两端 CAP_PQ_HYBRID 均置位并协商通过' : '本端 CAP_PQ_HYBRID 未置位 → 不可选，且不降级为假装成功',
          disabled: !pqOk
        }
      ]
    });
    var extract = MUI.checkbox({
      label: '提取文本进入全文索引（extract = true）',
      sub: '默认勾选；仅对支持的类型生效，超 64 MiB 只索引前 64 MiB',
      checked: true
    });

    MUI.sheet({
      title: '导入到保险箱', sub: '明文只存在于引擎层；已完成块先落 .part', size: 'tall',
      body: [
        h('div', {
          class: 'mcard', style: { borderStyle: 'dashed', textAlign: 'center', padding: 'var(--sp-5) var(--sp-4)' }
        }, [
          h('span', { style: { color: 'var(--c-text-2)' }, html: VS.icon('import', 34) }),
          h('div', { class: 't-strong mt-2', text: '选择要导入的文件' }),
          h('div', { class: 't-caption mt-1', text: '移动端不提供拖拽区：改用系统文件选择器' }),
          h('div', { class: 'mt-3' }, [
            blockBtn('选择文件', 'primary', function () {
              UI.toast({ tone: 'info', title: '已选择 3 个文件', msg: '共 ' + F.bytes(48234496) + '（原型模拟）' });
            }, { icon: 'import' })
          ])
        ]),
        h('div', { class: 'mt-3' }, [MUI.mkv([
          { k: '本次选择', v: '3 个文件 · ' + F.bytes(48234496) },
          { k: '最大一个', v: '备份归档-2025.tar · ' + F.bytes(268435456) },
          { k: '单文件上限', v: '1 TiB；超过则拒绝并给出可读诊断，绝不静默截断' }
        ])]),
        h('div', { class: 'mt-3' }, [UI.field({ label: '目标文件夹', control: target, hint: '写入路径由引擎决定，外壳只传 folder_id' })]),
        h('div', { class: 'mt-3' }, [
          UI.field({ label: '加密算法套件', control: suite }),
          pqOk ? h('div', { class: 'mt-2' }, [MUI.muted('协商失败一律取低档 1，并写审计 + 显示降级提示。')])
            : h('div', { class: 'mt-2' }, [UI.alertbar({
                tone: 'warn', title: '能力不支持（码 13）',
                text: '混合 KEM 本端不可用：选项置灰，实际写入固定为经典档 1 —— 不降级为假装成功。'
              })])
        ]),
        h('div', { class: 'mt-3' }, [extract]),
        h('div', { class: 'mt-3' }, [MUI.limitedNote('取消语义（码 12）：取消时已完成块保留在 .part，索引不变；同一会话内重试可复用已完成部分。')])
      ],
      footer: function (close) {
        return [
          blockBtn('开始导入', 'primary', function () {
            close();
            UI.toast({ tone: 'info', title: '导入已入队', msg: 'TASK_QUEUED(6) → TASK_PROGRESS(7) → TASK_DONE(8)' });
          }, { icon: 'import' }),
          blockBtn('取消', 'ghost', function () {
            close();
            UI.toast({ tone: 'info', title: '已取消 · 错误码 12', msg: '视为正常结果；已完成的 .part 保留' });
          })
        ];
      }
    });
  }

  /* ---------------- 2.2 导出（M-02） -------------------------------- */
  function exportSheet(entries, ctx) {
    if (!entries || !entries.length) { UI.toast({ tone: 'warn', title: '没有可导出的条目' }); return; }
    var path = MUI.input({ mono: true, value: 'D:\\导出\\VaultSync\\' });
    var overwrite = MUI.checkbox({ label: '覆盖已存在的目标文件', sub: 'overwrite = false（默认）；未勾选且目标存在 → 错误码 7', checked: false });
    var total = entries.reduce(function (a, e) { return a + (e.size || 0); }, 0);
    MUI.sheet({
      title: '导出到外部路径',
      sub: entries.length + ' 项 · 合计 ' + F.bytes(total) + ' · 导出是唯一把明文写出引擎的路径，必须由你显式发起',
      size: 'tall',
      body: [
        UI.field({ label: '目标路径', control: path, hint: '路径不写入审计（可能含用户名等个人信息）' }),
        h('div', { class: 'mt-3' }, [overwrite]),
        h('div', { class: 'mt-3' }, [MUI.mkv(entries.slice(0, 6).map(function (e) {
          return { k: e.name, v: e.kind === 'folder' ? '文件夹（递归 ' + D.children(e.id).length + ' 项）' : F.bytes(e.size) };
        }))]),
        entries.length > 6 ? h('div', { class: 't-caption mt-2', text: '另有 ' + F.num(entries.length - 6) + ' 项未列出' }) : null,
        h('div', { class: 'mt-3' }, [UI.alertbar({
          tone: 'info', title: '只读降级下仍允许导出',
          text: '导出到外部路径不改动保险箱，因此在只读（码 9）与维护态（码 10）下保持可用，且不参与写冻结判定。'
        })]),
        h('div', { class: 'mt-2' }, [noteBlock('导出重组后 file_sha256 不一致 → 删除 .tmp、返回码 3 并记录 ERROR_DIAG，明确不输出可疑明文；取消（码 12）→ 删除 .tmp，外部路径不留半成品。')])
      ],
      footer: function (close) {
        return [
          blockBtn('开始导出', 'primary', function () {
            close();
            UI.toast({ tone: 'success', title: '导出完成', msg: entries.length + ' 项已写出到 ' + (path.value || '目标路径') });
          }, { icon: 'export' }),
          blockBtn('取消', 'ghost', function () {
            close();
            UI.toast({ tone: 'info', title: '已取消 · 错误码 12', msg: '已删除 .tmp，外部路径不留半成品' });
          })
        ];
      }
    });
  }

  /* ---------------- 2.3 重命名 ------------------------------------- */
  function renameSheet(e, ctx) {
    var gate = writeGate();
    if (gate.disabled) { UI.toast({ tone: 'warn', title: '重命名不可用', msg: gate.reason }); return; }
    var input = MUI.input({ value: e.name });
    MUI.sheet({
      title: '重命名', sub: '不改动密文内容与 FSKey', size: '',
      body: [
        UI.field({ label: '新名称', control: input, hint: 'UTF-8；审计不含名称原文（脱敏口径）' }),
        h('div', { class: 'mt-3' }, [noteBlock('写入 vault.rename 审计事件；审计 detail 只含 file_id 与字节数，不含名称原文。')])
      ],
      footer: function (close) {
        return [
          blockBtn('保存', 'primary', function () {
            var v = (input.value || '').trim();
            if (!v) { UI.toast({ tone: 'warn', title: '名称不能为空' }); return; }
            e.name = v;
            close();
            ctx.refresh();
            UI.toast({ tone: 'success', title: '已重命名', msg: 'vault.rename 已写审计（不含名称原文）' });
          }),
          blockBtn('取消', 'ghost', function () { close(); })
        ];
      }
    });
  }

  /* ---------------- 2.4 编辑标签 ----------------------------------- */
  function tagSheet(e, ctx) {
    var gate = writeGate();
    if (gate.disabled) { UI.toast({ tone: 'warn', title: '编辑标签不可用', msg: gate.reason }); return; }
    if (!e.tags) e.tags = [];
    var chips = h('div', { class: 'row wrap gap-2' });
    function renderChips() {
      chips.innerHTML = '';
      if (!e.tags.length) chips.appendChild(h('span', { class: 't-caption', text: '暂无标签' }));
      e.tags.forEach(function (t) {
        chips.appendChild(MUI.chip({
          text: t,
          onRemove: function () { e.tags = e.tags.filter(function (x) { return x !== t; }); renderChips(); }
        }));
      });
    }
    renderChips();
    var input = MUI.input({ placeholder: '输入标签后回车新增' });
    input.addEventListener('keydown', function (ev) {
      if (ev.key !== 'Enter') return;
      var v = (input.value || '').trim();
      if (!v) return;
      if (e.tags.indexOf(v) < 0) e.tags.push(v);
      input.value = '';
      renderChips();
    });
    var suggest = h('div', { class: 'row wrap gap-2' }, D.allTags().slice(0, 8).map(function (t) {
      return MUI.chip({
        text: t.tag, title: '已有标签 · ' + F.num(t.count) + ' 项使用',
        onRemove: function () { if (e.tags.indexOf(t.tag) < 0) e.tags.push(t.tag); renderChips(); }
      });
    }));
    MUI.sheet({
      title: '编辑标签', sub: '打标签属写库操作：只读降级（9）与维护态（10）下禁用',
      body: [
        secTitle('tag', '当前标签'),
        h('div', { class: 'mt-1' }, [chips]),
        h('div', { class: 'mt-3' }, [UI.field({ label: '新增标签', control: input })]),
        h('div', { class: 'mt-3' }, [secTitle('sparkle', '建议标签'), h('div', { class: 'mt-2' }, [suggest])]),
        h('div', { class: 'mt-3' }, [noteBlock('标签是写库操作（vault.tag）：只读降级与维护态下入口禁用；审计不含标签原文。')])
      ],
      footer: function (close) {
        return [
          blockBtn('保存', 'primary', function () {
            close(); ctx.refresh();
            UI.toast({ tone: 'success', title: '标签已保存', msg: 'vault.tag 已写审计（不含标签原文）' });
          }),
          blockBtn('取消', 'ghost', function () { close(); })
        ];
      }
    });
  }

  /* ---------------- 2.5 移动到 ------------------------------------- */
  function moveSheet(e, ctx) {
    var gate = writeGate();
    if (gate.disabled) { UI.toast({ tone: 'warn', title: '移动不可用', msg: gate.reason }); return; }
    var sel = MUI.select({
      value: String(e.parent || 0), block: true,
      options: [{ value: '0', label: '根目录' }].concat(D.vault.folders
        .filter(function (f) { return f.id !== e.id; })
        .map(function (f) { return { value: String(f.id), label: f.name }; }))
    });
    MUI.sheet({
      title: '移动到…', sub: e.name,
      body: [
        UI.field({ label: '目标文件夹', control: sel, hint: '改变父节点并递增 rev；不改动密文与 FSKey' })
      ],
      footer: function (close) {
        return [
          blockBtn('移动', 'primary', function () {
            e.parent = Number(sel.getValue());
            close(); ctx.refresh();
            UI.toast({ tone: 'success', title: '已移动', msg: 'vault.move 已写审计' });
          }),
          blockBtn('取消', 'ghost', function () { close(); })
        ];
      }
    });
  }

  /* ---------------- 2.6 新建文件夹 --------------------------------- */
  function newFolderSheet(cwdId, ctx) {
    var gate = writeGate();
    if (gate.disabled) { UI.toast({ tone: 'warn', title: '无法新建文件夹', msg: gate.reason }); return; }
    var input = MUI.input({ placeholder: '文件夹名称' });
    MUI.sheet({
      title: '新建文件夹', sub: '将创建 fsk/{folder_id} 派生的文件夹级密钥 FSK',
      body: [
        UI.field({ label: '名称', control: input, hint: '文件夹内文件共享该 FSK；不改动 MK' }),
        h('div', { class: 'mt-3' }, [noteBlock('FSK = HKDF(MK, "fsk/{folder_id}")，不落盘明文；轮换 MK 后由索引中的 fskey_override 承接。')])
      ],
      footer: function (close) {
        return [
          blockBtn('创建', 'primary', function () {
            var name = (input.value || '').trim() || '新建文件夹';
            var id = ++D.vault.nextId;
            D.vault.folders.push({
              id: id, kind: 'folder', name: name, parent: cwdId || 0, size: 0,
              createdMs: Date.now(), modifiedMs: Date.now(), tags: [], depth: 1
            });
            close(); ctx.refresh();
            UI.toast({ tone: 'success', title: '已创建', msg: name + ' · FSK 已派生（不显示密钥值）' });
          }),
          blockBtn('取消', 'ghost', function () { close(); })
        ];
      }
    });
  }

  /* ---------------- 2.7 文件夹属性 --------------------------------- */
  function folderSheet(folder, ctx) {
    var kids = D.children(folder.id);
    var gate = writeGate();
    MUI.sheet({
      title: '文件夹属性', sub: folder.name,
      body: [
        MUI.mkv([
          { k: '条目数', v: F.num(kids.length) + '（递归 ' + F.num(kids.length * 3) + '）' },
          { k: '创建时间', v: F.dateLong(folder.createdMs) },
          { k: '修改时间', v: F.dateLong(folder.modifiedMs) },
          { k: 'FSK', v: 'HKDF(MK, "fsk/' + folder.id + '") — 只显示存在性', mono: true }
        ]),
        h('div', { class: 'mt-3' }, [UI.alertbar({
          tone: 'info', title: '递归删除语义',
          text: '删除文件夹会递归删除其中全部条目；24 小时保护窗口内保留加密副本，窗口结束一并销毁。'
        })])
      ],
      footer: function (close) {
        return [
          blockBtn('打开', 'primary', function () { close(); ctx.go('folder', { id: folder.id }); }, { icon: 'folder' }),
          blockBtn('重命名…', 'ghost', function () { close(); renameSheet(folder, ctx); }, { disabled: gate.disabled, title: gate.reason || '重命名' }),
          blockBtn('递归删除…', 'danger', function () { close(); wipeSheet([folder], 'delete', ctx); }, { disabled: gate.disabled, title: gate.reason || '递归删除' }),
          blockBtn('关闭', 'ghost', function () { close(); })
        ];
      }
    });
  }

  /* ---------------- 2.8 删除 / 安全擦除确认（四段硬约束） ------------ */
  function wipeSheet(entries, mode, ctx) {
    entries = (entries || []).filter(Boolean);
    if (!entries.length) return;
    var gate = writeGate();
    if (gate.disabled) { UI.toast({ tone: 'warn', title: '操作不可用', msg: gate.reason }); return; }

    var totalSize = entries.reduce(function (a, e) { return a + (e.size || 0); }, 0);
    var recursive = entries.some(function (e) { return e.kind === 'folder'; });
    var probe = entries[0] || {};
    var eraseClass = probe.eraseClass || 4;
    var mediaKind = probe.mediaKind || 4;
    var meta = eraseMeta(eraseClass);
    var isErase = mode === 'erase';
    var protectUntil = Date.now() + 24 * 3600 * 1000;
    var wordOk = secureEraseWordAllowed(eraseClass);

    var multiRound = MUI.checkbox({
      label: '启用多轮覆写', sub: '仅 HDD（media_kind = 2）有意义；默认关闭，时间成本高', checked: false
    });
    var pendingMark = MUI.checkbox({
      label: '物理删除失败时标记为「待清理」并在启动时重试', sub: '不会显示「已擦除」', checked: true
    });
    var propagate = MUI.switchCtl({ label: '同时删除已配对设备上的副本', sub: '广播删除传播（墓碑）到全部对端', checked: true });

    var body = [
      /* ① 影响范围 */
      MUI.mcard({
        title: '① 影响范围',
        body: MUI.mkv([
          { k: '条目数', v: F.num(entries.length) + ' 项' + (recursive ? '（含递归子项）' : '') },
          { k: '合计字节', v: F.bytes(totalSize) },
          {
            k: '名称预览',
            v: entries.slice(0, 3).map(function (e) { return e.name; }).join('、') +
              (entries.length > 3 ? ' 等 ' + F.num(entries.length) + ' 项' : '')
          },
          { k: '操作类型', v: opTypeLabel(isErase, wordOk) }
        ])
      }),
      /* ② 本次将采用的擦除强度 */
      h('div', { class: 'mt-3' }, [MUI.mcard({
        title: '② 本次将采用的擦除强度',
        sub: '由 vault_core_erase_class_probe(path) 实时返回，显示引擎原值',
        body: [
          h('div', { class: 'row gap-3 wrap' }, [
            MUI.eraseBadge(eraseClass),
            UI.badge({ text: 'media_kind = ' + mediaKind, tone: 'stale' }),
            UI.badge({ text: 'method_bits = ' + methodBitsFor(eraseClass), tone: 'stale' })
          ]),
          h('div', { class: 'mt-3' }, [MUI.mkv([
            { k: 'erase_class', v: eraseClass + ' · ' + meta.label },
            { k: 'method_bits', v: methodBitsFor(eraseClass), mono: true },
            { k: 'media_kind', v: mediaKind + ' · ' + mediaKindLabel(mediaKind) },
            { k: '引擎原值说明', v: meta.note }
          ])]),
          h('div', { class: 'mt-3' }, [secTitle('alert', 'degradations[]（逐条列出，不隐藏）')]),
          h('div', { class: 'col gap-1 mt-2' }, degradationsFor(eraseClass, mediaKind).map(function (d) {
            return h('div', { class: 't-caption', text: '· ' + d });
          })),
          h('div', { class: 'mt-3' }, [UI.alertbar(wordOk ? {
            tone: 'info', title: '平台安全删除位已置位',
            text: 'erase_class = platform_secure 且 method_bits 含平台安全删除位（bit2 = 1），本条目满足措辞成立的充要条件。'
          } : {
            tone: 'warn', title: '介质覆写未执行',
            text: '本次 erase_class 为 ' + meta.key + '：按文案硬约束必须写明「介质覆写未执行」；'
              + '在满足充要条件之前，界面不使用更弱的表述替代，也不使用更强的表述夸大。'
          })])
        ]
      })]),
      /* ③ 误删保护窗口 */
      h('div', { class: 'mt-3' }, [MUI.mcard({
        title: '③ 误删保护窗口 24 小时',
        body: [
          UI.alertbar({
            tone: 'info', title: '窗口内可撤销',
            text: '保护窗口内先保留加密副本、FSKey 仍可用 →「加密擦除」可撤销。窗口结束（' + F.dateLong(protectUntil) + '）后副本与密钥一并销毁。'
          }),
          h('div', { class: 'mt-3' }, [propagate])
        ]
      })]),
      /* ④ 附加选项 */
      h('div', { class: 'mt-3' }, [MUI.mcard({
        title: '④ 附加选项',
        body: [
          mediaKind === 2 ? multiRound
            : h('div', { class: 't-caption', text: '多轮覆写仅对 HDD（media_kind = 2）有意义，当前介质 ' + mediaKindLabel(mediaKind) + ' 不适用。' }),
          h('div', { class: 'mt-3' }, [pendingMark])
        ]
      })]),
      h('div', { class: 'mt-3' }, [MUI.limitedNote('审计失败不阻断业务：本条操作会写链式审计（vault.delete / security.erase）；写入失败仅发 ERROR_DIAG。')])
    ];

    MUI.confirmSheet({
      title: opTypeLabel(isErase, wordOk),
      sub: F.num(entries.length) + ' 项 · 该操作会写入链式审计日志',
      tone: 'danger',
      confirmLabel: isErase ? '执行擦除' : '删除',
      cancelLabel: '取消',
      body: body,
      onConfirm: function () {
        /* 原型演示：id ≥ 106 的条目模拟物理删除失败 */
        var failed = (probe.id || 0) >= 106;
        if (failed) {
          UI.toast({
            tone: 'warn', title: '介质擦除待完成',
            msg: '容器已标记「待清理」，启动清理扫描将重试。注意：这不是「已擦除」。', duration: 7000
          });
        } else {
          UI.toast({
            tone: 'success',
            title: opTypeLabel(isErase, wordOk),
            msg: wordOk ? '平台安全删除指令已执行' : '介质覆写未执行；已加密擦除，24 小时内可撤销。'
          });
        }
        afterWipeSheet(entries, ctx, failed);
      }
    });
  }

  /** 删除/擦除后的撤销与待清理入口（底部面板） */
  function afterWipeSheet(entries, ctx, failed) {
    var names = entries.slice(0, 3).map(function (e) { return e.name; }).join('、');
    MUI.sheet({
      title: failed ? '介质擦除待完成' : '删除已提交',
      sub: names + (entries.length > 3 ? ' 等 ' + F.num(entries.length) + ' 项' : ''),
      body: [
        UI.alertbar(
          failed
            ? {
                tone: 'danger', title: '物理删除失败，已标记「待清理」',
                text: '界面必须显示「介质擦除待完成」，不得显示「已擦除」。容器将在启动清理扫描时重试。'
              }
            : {
                tone: 'info', title: '24 小时内可撤销',
                text: '保护窗口内加密副本仍可解封恢复；窗口结束后副本与密钥一并销毁。'
              }
        ),
        h('div', { class: 'mt-3' }, [MUI.mkv([
          { k: '条目数', v: F.num(entries.length) + ' 项' },
          { k: '撤销截止', v: F.dateLong(Date.now() + 24 * 3600 * 1000) },
          { k: '清理重试', v: '启动扫描 / 手动「立即重试清理」' }
        ])]),
        h('div', { class: 'mt-3' }, [noteBlock('审计写入失败不阻断业务：事后取证与可用性的取舍（见 LOG.md）。')])
      ],
      footer: function (close) {
        return [
          failed
            ? blockBtn('查看待清理', 'primary', null, { disabled: true, title: '已是待清理状态' })
            : blockBtn('撤销删除', 'primary', function () {
                close();
                UI.toast({ tone: 'success', title: '已撤销删除', msg: '加密副本已恢复，FSKey 仍可用' });
              }),
          blockBtn('查看已删除与待清理', 'ghost', function () { close(); ctx.go('deleted'); }, { icon: 'trash' }),
          blockBtn('关闭', 'ghost', function () { close(); })
        ];
      }
    });
  }

  /* ---------------- 2.9 完整性校验进度（dismissible:false） ---------- */
  function verifySheet(ids) {
    var finished = false, pct = 0;
    var total = ids ? ids.length : D.totals().files;
    var bar = MUI.bar(0, 'accent');
    var ringHost = h('div');
    ringHost.appendChild(MUI.ring({ pct: 0, size: 64, stroke: 6 }));
    var status = h('div', { class: 't-caption', text: '准备中…' });
    var fail = 0;

    var sheet = MUI.sheet({
      title: ids ? '校验所选文件' : '全库完整性校验',
      sub: 'vault_core_verify_all · 可取消（码 12 视为正常结果）',
      dismissible: false,
      body: [
        h('div', { class: 'row gap-3' }, [ringHost, h('div', { class: 'grow' }, [bar, h('div', { class: 'mt-2' }, [status])])]),
        h('div', { class: 'mt-3' }, [MUI.mkv([
          { k: '已检查', v: F.num(0) + ' / ' + F.num(total) },
          { k: '失败计数', v: F.num(0) },
          { k: '当前对象', v: '—', mono: true }
        ])]),
        h('div', { class: 'mt-3' }, [UI.alertbar({
          tone: 'info', title: '失败即定位到具体块',
          text: 'GCM 校验失败会定位到具体分片 / 块；该文件被隔离并提示重新导入，不会静默丢弃。'
        })])
      ],
      footer: function () {
        return [
          blockBtn('取消扫描', 'ghost', function () {
            finished = true;
            clearInterval(timer);
            sheet.close();
            UI.toast({ tone: 'info', title: '已取消 · 错误码 12', msg: '视为正常结果；资源在 5s 内释放（tasks::cancel_releases_within_5s）' });
          }, { icon: 'cancel' })
        ];
      }
    });

    var timer = setInterval(function () {
      if (finished) { clearInterval(timer); return; }
      pct += 0.08;
      if (pct >= 1) { pct = 1; clearInterval(timer); finished = true; fail = 1; }
      bar.firstChild.style.width = Math.round(pct * 100) + '%';
      var old = sheet.el.querySelector('.ring');
      if (old) old.replaceWith(MUI.ring({ pct: pct, size: 64, stroke: 6 }));
      var done = Math.round(pct * total);
      status.textContent = '已检查 ' + F.num(done) + ' 项 · 失败 ' + F.num(fail) + ' 项';
      var kv = sheet.el.querySelector('.mkv');
      if (kv) {
        kv.innerHTML = '';
        kv.appendChild(MUI.mkv([
          { k: '已检查', v: F.num(done) + ' / ' + F.num(total) },
          { k: '失败计数', v: F.num(fail) },
          { k: '当前对象', v: pct < 1 ? 'f_' + D.hash(8) : '—', mono: true }
        ]).firstChild);
      }
      if (pct === 1) {
        status.textContent = '完成 · ' + F.num(total) + ' 项已检查 · ' + F.num(fail) + ' 项失败（已隔离：f_' + D.hash(8) + '）';
        setTimeout(function () {
          sheet.close();
          UI.toast({ tone: 'warn', title: '校验完成，1 项失败', msg: '失败文件已隔离并提示重新导入，不静默丢弃', duration: 7000 });
        }, 800);
      }
    }, 260);
  }

  /* ---------------- 2.10 存储占用 ---------------------------------- */
  function storageSheet() {
    var byFolder = {};
    D.vault.entries.forEach(function (e) {
      var parent = D.entry(e.parent);
      var key = parent ? parent.name : '根目录';
      byFolder[key] = (byFolder[key] || 0) + e.size;
    });
    var rows = Object.keys(byFolder).map(function (k) { return { name: k, size: byFolder[k] }; })
      .sort(function (a, b) { return b.size - a.size; });
    var max = rows.length ? rows[0].size : 1;
    var total = D.totals().size;

    MUI.sheet({
      title: '存储占用',
      sub: '真值档：递归 vault_core_vault_list 的 size 求和（服务层聚合，走工作 Isolate）',
      size: 'tall',
      body: [
        h('div', { class: 'mstats' }, [
          MUI.mstat({ value: F.bytes(total), label: '已加密总量（真值）', src: 'vault_core_vault_list' }),
          MUI.mstat({ value: F.num(D.totals().files), label: '文件条目', src: 'vault_core_vault_list' })
        ]),
        h('div', { class: 'mt-3' }, [secTitle('layers', '按文件夹分布（前 8 项）')]),
        h('div', { class: 'col gap-3 mt-2' }, rows.slice(0, 8).map(function (r) {
          return h('div', { class: 'row gap-3' }, [
            h('span', { class: 't-truncate', style: { width: '96px', flex: 'none' }, title: r.name, text: r.name }),
            h('span', { class: 'grow' }, [MUI.bar(r.size / max, 'accent', 'sm')]),
            h('span', { class: 't-caption t-num', style: { width: '82px', textAlign: 'right', flex: 'none' }, text: F.bytes(r.size) })
          ]);
        })),
        h('div', { class: 'mt-3' }, [UI.alertbar({
          tone: 'info', title: '整行不渲染：磁盘占用百分比 / 卷剩余空间',
          text: '契约清单无该接口 → 该行不渲染（不写「待接入」，也不编造估算值）。这是原则 7「不谎报能力」的落地。'
        })]),
        srcNote('vault_core_vault_list')
      ],
      footer: function (close) { return [blockBtn('关闭', 'ghost', function () { close(); })]; }
    });
  }

  /* ---------------- 2.11 阅后即焚分享 ------------------------------ */
  function shareSheet(entry) {
    var deny = writeDeny();
    if (deny) {
      UI.toast({ tone: 'warn', title: '无法创建分享', msg: writeReason(deny) });
      return;
    }
    var device = MUI.select({
      value: D.devices.filter(function (d) { return !d.self; })[0].id,
      block: true,
      options: D.devices.filter(function (d) { return !d.self; }).map(function (d) {
        return { value: d.id, label: d.name + (d.online ? ' · 在线' : ' · 离线（待上线送达）') };
      })
    });
    var expiry = MUI.select({
      value: '60', block: true,
      options: [
        { value: '5', label: '5 分钟' }, { value: '15', label: '15 分钟' },
        { value: '60', label: '60 分钟' }, { value: '1440', label: '1440 分钟（24 小时）' }
      ]
    });
    var sessions = MUI.select({
      value: '1', block: true,
      options: [
        { value: '1', label: '1 次（阅后即焚）' },
        { value: '5', label: '5 次' },
        { value: '0', label: '不限（仅受时效约束）' }
      ]
    });
    MUI.sheet({
      title: '创建阅后即焚分享',
      sub: '接收方读取后票据立即失效；密文不落中继，中继只转发',
      size: 'tall',
      body: [
        UI.field({ label: '接收设备', control: device }),
        h('div', { class: 'mt-3' }, [UI.field({ label: '有效时长', control: expiry, hint: '过期未读取 → 票据作废，明文不出引擎' })]),
        h('div', { class: 'mt-3' }, [UI.field({ label: '可读取次数', control: sessions, hint: '1 次 = 阅后即焚' })]),
        h('div', { class: 'mt-3' }, [secTitle('receipt', '分享票据')]),
        h('div', { class: 'mt-2' }, [MUI.copyField('vsburn://' + D.hash(24), '复制票据链接')]),
        h('div', { class: 'mt-3' }, [MUI.mkv([
          { k: '关联文件', v: entry.name },
          { k: '票据密钥', v: '不含在票据内；接收方经 Noise 信道核验双方指纹后解封', mono: false },
          { k: '中继可见性', v: '仅密文与路由元数据' }
        ])]),
        h('div', { class: 'mt-3' }, [UI.alertbar({
          tone: 'warn', title: '对方未接受，分享已撤销',
          text: '有效期内接收方未接受则票据自动撤销且不重试。已读取的文件不再自动擦除。'
        })]),
        h('div', { class: 'mt-2' }, [MUI.limitedNote('读取完成后的载体销毁：已加密擦除（+ TRIM）。若介质为 SSD，TRIM 为提示性指令，不保证物理擦除。')])
      ],
      footer: function (close) {
        return [
          blockBtn('创建并复制票据', 'primary', function () {
            close();
            UI.toast({ tone: 'success', title: '票据已创建', msg: 'p2p.burn 已写审计；对方未接受则自动撤销' });
          }, { icon: 'share' }),
          blockBtn('取消', 'ghost', function () { close(); })
        ];
      }
    });
  }

  /* ---------------- 2.12 排序 / 标签筛选 / 移动端演示 --------------- */
  function sortSheet(ctx) {
    var cur = S.get('sort') || { key: 'name', dir: 'asc' };
    var defs = [
      { key: 'name', label: '名称', icon: 'sort' },
      { key: 'modifiedMs', label: '修改时间', icon: 'clock' },
      { key: 'size', label: '大小', icon: 'layers' }
    ];
    var items = defs.map(function (d) {
      return {
        label: d.label + (cur.key === d.key ? '（当前）' : ''),
        icon: d.icon,
        sub: cur.key === d.key ? (cur.dir === 'asc' ? '升序 · 再点一次切换为降序' : '降序 · 再点一次切换为升序') : '点击应用并切换排序键',
        onClick: function () {
          S.mutate(function (st) {
            if (st.sort.key === d.key) st.sort.dir = st.sort.dir === 'asc' ? 'desc' : 'asc';
            else { st.sort.key = d.key; st.sort.dir = 'asc'; }
          }, true);
          ctx.refresh();
        }
      };
    });
    items.push({ label: '恢复默认（名称升序）', icon: 'rotate-ccw', onClick: function () {
      S.mutate(function (st) { st.sort = { key: 'name', dir: 'asc' }; }, true);
      ctx.refresh();
    } });
    MUI.actionSheet({ title: '排序', sub: '文件夹永远排在文件之前', items: items });
  }

  function tagFilterSheet(ctx) {
    var cur = S.get('filterTags') || [];
    var body = h('div', { class: 'row wrap gap-2' }, D.allTags().map(function (t) {
      var on = cur.indexOf(t.tag) >= 0;
      return MUI.chip({
        text: t.tag + '（' + F.num(t.count) + '）',
        tone: on ? 'accent' : null,
        title: on ? '点击取消该标签筛选' : '点击加入筛选',
        onRemove: function () {
          var next = (S.get('filterTags') || []).slice();
          var i = next.indexOf(t.tag);
          if (i >= 0) next.splice(i, 1); else next.push(t.tag);
          S.set({ filterTags: next }, true);
          ctx.refresh();
        }
      });
    }));
    MUI.sheet({
      title: '按标签筛选',
      sub: '已选 ' + F.num(cur.length) + ' 个标签 · 命中条目会即时过滤',
      body: [h('div', { class: 'mt-1' }, [MUI.muted('多标签为「或」关系；标签是写库对象的只读视图，筛选本身不改数据。')]), body],
      footer: function (close) {
        return [
          blockBtn('清除筛选', 'ghost', function () { S.set({ filterTags: [] }, true); close(); ctx.refresh(); }),
          blockBtn('完成', 'primary', function () { close(); })
        ];
      }
    });
  }

  /* ======================================================================
   * 3. 列表渲染（vault 根页 / folder 钻取页共用）
   * ==================================================================== */

  function entryRow(ctx, e, withSwipe) {
    var gate = writeGate();
    var isFolder = e.kind === 'folder';
    var sub = isFolder
      ? D.children(e.id).length + ' 项 · ' + F.relative(e.modifiedMs)
      : F.bytes(e.size) + ' · ' + F.relative(e.modifiedMs);

    /** 每次都要新建节点：DOM 节点不可复用，badges 也是每行独立的 */
    function freshBadges() {
      return h('span', { class: 'row gap-1' }, entryBadges(e, true));
    }

    if (!withSwipe) {
      return MUI.mrow({
        icon: entryIcon(e),
        title: e.name,
        sub: sub,
        badges: freshBadges(),
        onClick: function () { openEntry(ctx, e); },
        onLongPress: function () { entryActions(ctx, e); }
      });
    }

    /*
     * 左滑 = 等价降级（MUI.mrow 内部已接好 Pointer 拖拽 + 520ms 长按 + 右键），
     * 这里只负责给出行内三项快捷操作；长按仍走 entryActions 动作面板。
     */
    return MUI.mrow({
      icon: entryIcon(e),
      title: e.name,
      sub: sub,
      badges: freshBadges(),
      onLongPress: function () { entryActions(ctx, e); },
      onClick: function () { openEntry(ctx, e); },
      swipe: [
        { label: '导出', icon: 'export', tone: 'accent', onClick: function () { exportSheet([e], ctx); } },
        {
          label: '重命名', icon: 'edit',
          onClick: function () {
            if (gate.disabled) UI.toast({ tone: 'warn', title: '重命名不可用', msg: gate.reason });
            else renameSheet(e, ctx);
          }
        },
        {
          label: '删除', icon: 'trash', tone: 'danger',
          onClick: function () {
            if (gate.disabled) UI.toast({ tone: 'warn', title: '删除不可用', msg: gate.reason });
            else wipeSheet([e], 'delete', ctx);
          }
        }
      ]
    });
  }

  function bodyRows(ctx, list, withSwipe) {
    if (!list.length) {
      var gate = writeGate();
      var actions = [];
      if (!gate.disabled) {
        actions.push(h('button', {
          class: 'mbtn-block', type: 'button', dataset: { variant: 'primary' },
          title: '导入文件', onclick: function () { importSheet(ctx.cwdId); }
        }, [h('span', { html: VS.icon('import', 18) }), h('span', { text: '导入文件' })]));
      }
      return [
        MUI.empty({
          icon: 'folder', title: '这里还没有条目',
          desc: gate.disabled
            ? '当前处于写冻结（' + writeReason(writeDeny()) + '），导入入口不可用。'
            : '使用右下角「＋」导入文件或新建文件夹；手机端不提供拖拽区。',
          actions: actions.length ? actions : null
        }),
        h('div', { class: 't-caption', style: { textAlign: 'center', padding: '0 var(--sp-4)' },
          text: ctx.filtered ? '筛选条件下没有命中：清除标签筛选后重试。' : '列表来自 vault_core_vault_list 分页快照。' })
      ];
    }
    return [MUI.mlist(list.map(function (e) { return entryRow(ctx, e, withSwipe); }))];
  }

  function gridCards(ctx, list) {
    var grid = h('div', {
      class: 'file-grid',
      /*
       * 说明：.file-grid / .file-card / .hit-snippet 定义在 v2.0/assets/views.css，
       * 而移动端页面只加载 tokens/base/components/mobile.css（见 mobile/index.html）。
       * 契约要求复用这四个类名，因此这里内联等价样式作为兜底，保证两端观感一致。
       */
      style: {
        display: 'grid',
        gridTemplateColumns: 'repeat(auto-fill, minmax(148px, 1fr))',
        gap: 'var(--sp-3)'
      }
    });
    list.forEach(function (e) {
      var isFolder = e.kind === 'folder';
      var card = h('div', {
        class: 'file-card', tabindex: '0',
        'aria-label': e.name + ' · ' + (isFolder ? '文件夹 · ' + D.children(e.id).length + ' 项' : F.bytes(e.size) + ' · ' + F.relative(e.modifiedMs)),
        title: e.name,
        style: {
          position: 'relative', display: 'flex', flexDirection: 'column', gap: 'var(--sp-2)',
          padding: 'var(--sp-3)', borderRadius: 'var(--r-md)',
          background: 'var(--c-surface)', border: '1px solid var(--c-border)',
          cursor: 'pointer', minWidth: '0'
        },
        onclick: function () { openEntry(ctx, e); },
        onkeydown: function (ev) { if (ev.key === 'Enter') openEntry(ctx, e); },
        oncontextmenu: function (ev) { ev.preventDefault(); entryActions(ctx, e); }
      }, [
        h('span', { style: { color: isFolder ? 'var(--c-accent)' : 'var(--c-text-2)' }, html: VS.icon(entryIcon(e), 24) }),
        h('div', { class: 't-truncate', style: { fontSize: 'var(--fs-body)', fontWeight: '500' }, title: e.name, text: e.name }),
        h('div', { class: 't-caption', text: isFolder ? D.children(e.id).length + ' 项' : F.bytes(e.size) }),
        h('div', { class: 'row wrap gap-1' }, entryBadges(e, true))
      ]);
      grid.appendChild(card);
    });
    return grid;
  }

  function renderListBody(ctx) {
    var S2 = S;
    var list = D.children(ctx.cwdId);
    var tags = S2.get('filterTags') || [];
    if (tags.length) {
      list = list.filter(function (e) { return (e.tags || []).some(function (t) { return tags.indexOf(t) >= 0; }); });
    }
    ctx.filtered = tags.length > 0;
    list = sortList(list);
    if (!list.length) return bodyRows(ctx, list, false);
    var view = ctx.view === 'grid' ? 'grid' : 'list';
    return view === 'grid' ? [gridCards(ctx, list)] : bodyRows(ctx, list, true);
  }

  function sortList(list) {
    var sort = S.get('sort') || { key: 'name', dir: 'asc' };
    var arr = list.slice();
    arr.sort(function (a, b) {
      if ((a.kind === 'folder') !== (b.kind === 'folder')) return a.kind === 'folder' ? -1 : 1;
      var k = sort.key;
      var va = k === 'size' ? a.size : k === 'modifiedMs' ? a.modifiedMs : a.name;
      var vb = k === 'size' ? b.size : k === 'modifiedMs' ? b.modifiedMs : b.name;
      var r = typeof va === 'string' ? va.localeCompare(vb, 'zh-Hans-CN') : (va - vb);
      return sort.dir === 'asc' ? r : -r;
    });
    return arr;
  }

  /** 顶部路径面包屑（可点回退） + 排序入口 + 标签筛选入口 */
  function crumbBar(ctx) {
    var crumbs = [{ id: 0, name: '根' }].concat(D.pathOf(ctx.cwdId).map(function (p) { return { id: p.id, name: p.name }; }));
    var wrap = h('div', { class: 'breadcrumb', style: { overflowX: 'auto' } });
    crumbs.forEach(function (c, i) {
      wrap.appendChild(h('span', {
        class: 'crumb', title: '回到 ' + c.name,
        'aria-current': i === crumbs.length - 1 ? 'page' : null,
        onclick: function () {
          if (i === crumbs.length - 1) return;
          if (c.id === 0) { if (!ctx.isRoot) ctx.nav.switchTab('vault'); return; }
          ctx.go('folder', { id: c.id });
        }
      }, [h('span', { text: c.name })]));
      if (i < crumbs.length - 1) wrap.appendChild(h('span', { class: 'crumb-sep', text: '›' }));
    });
    return h('div', { class: 'row gap-2' }, [
      h('div', { class: 'grow' }, [wrap]),
      h('button', {
        class: 'btn btn-icon btn-ghost', type: 'button', title: '排序', 'aria-label': '排序',
        onclick: function () { sortSheet(ctx); }
      }, [h('span', { html: VS.icon('sort', 18) })]),
      h('button', {
        class: 'btn btn-icon btn-ghost', type: 'button',
        title: '按标签筛选', 'aria-label': '按标签筛选',
        onclick: function () { tagFilterSheet(ctx); }
      }, [h('span', { html: VS.icon('filter', 18) })])
    ]);
  }

  function viewSeg(ctx) {
    /* 手机上不提供「分栏」：切换器里根本不出现该项（不是置灰） */
    return MUI.mseg({
      value: ctx.view,
      items: [
        { value: 'list', label: '列表', icon: 'list' },
        { value: 'grid', label: '网格', icon: 'grid' }
      ],
      onChange: function (v) {
        S.set({ view: v }, true);
        ctx.refresh();
      }
    });
  }

  function makeCtx(ctx, tab, cwdId, isRoot) {
    var c = {
      tab: tab,
      cwdId: cwdId,
      isRoot: !!isRoot,
      go: ctx.go,
      nav: ctx.nav,
      view: S.get('view') === 'grid' ? 'grid' : 'list',
      filtered: false,
      refresh: function () { VS.render && VS.render(); }
    };
    return c;
  }

  /* ======================================================================
   * 4. A. vault（Tab 根页）
   * ==================================================================== */
  VS.pages['vault'] = function (ctx) {
    /* 桌面端可能把 view 留成 columns；手机端归一（并把状态写回，避免残留不可达取值） */
    if (S.get('view') === 'columns') S.set({ view: 'list' }, true);
    var cwdId = 0;
    var c = makeCtx(ctx, 'vault', cwdId, true);

    var totals = D.totals();
    var body = [
      crumbBar(c),
      viewSeg(c),
      MUI.overviewBar([
        { icon: 'file', label: '文件', value: F.num(totals.files) },
        { icon: 'folder', label: '文件夹', value: F.num(totals.folders) },
        { icon: 'lock', label: '已加密', value: F.bytes(totals.size) }
      ])
    ];
    var rows = renderListBody(c);
    rows.forEach(function (n) { body.push(n); });
    body.push(h('div', { class: 't-caption', style: { padding: '0 var(--sp-1)' },
      text: '长按条目 = 动作面板；左滑 = 等价降级（导出 / 重命名 / 删除）。' }));

    return MUI.page({
      title: '保险箱',
      sub: D.pathOf(cwdId).length ? '根目录' : '根目录 · ' + F.num(totals.files) + ' 个文件',
      back: false,
      actions: [
        { icon: 'search', label: '搜索', onClick: function () { ctx.go('search'); } }
      ],
      overflow: overflowItems(c),
      body: MUI.screen(body),
      tab: true,
      fab: S.get('lock') === 'Unlocked'
        ? MUI.fab({ icon: 'plus', label: '新建', onClick: function () { plusActions(c); } })
        : null
    });
  };

  function plusActions(c) {
    /* gate 由 VS.derive.denyWrite() 唯一推导，不在此处再判断一次状态 */
    var gate = writeGate();
    MUI.actionSheet({
      title: '新建', sub: '写库操作在只读降级（9）/ 维护态（10）下不可用',
      items: [
        { label: itemLabel('导入文件…', gate), icon: 'import', disabled: gate.disabled, reason: gate.reason, onClick: function () { importSheet(c.cwdId); } },
        { label: itemLabel('新建文件夹…', gate), icon: 'folder', disabled: gate.disabled, reason: gate.reason, onClick: function () { newFolderSheet(c.cwdId, c); } },
        { label: '检索…', icon: 'search', sub: '打开全屏搜索页（文件名 / 标签 / 全文）', onClick: function () { c.go('search'); } }
      ]
    });
  }

  function overflowItems(c) {
    return [
      { label: '全库完整性校验…', icon: 'layers', sub: 'vault_core_verify_all · 可取消', onClick: function () { verifySheet(null); } },
      { label: '查看已删除与待清理', icon: 'trash', sub: '24 小时误删保护窗口', onClick: function () { c.go('deleted'); } },
      { label: '存储占用统计', icon: 'gauge', sub: '真值档：vault_core_vault_list', onClick: function () { storageSheet(); } },
      { label: '按标签筛选…', icon: 'filter', sub: '已选 ' + F.num((S.get('filterTags') || []).length) + ' 个标签', onClick: function () { tagFilterSheet(c); } },
      {
        label: '导出当前列表清单', icon: 'export',
        sub: '仅含元数据清单；不导出明文内容',
        onClick: function () {
          UI.toast({ tone: 'info', title: '已导出清单', msg: '仅含名称 / 大小 / 时间 / 标签，不含密文与密钥' });
        }
      },
      {
        label: '演示控制台', icon: 'sparkle', sub: '切换只读 / 维护态与能力位，验收状态呈现',
        onClick: function () { if (VS.actions['mobile.demo']) VS.actions['mobile.demo'](); }
      }
    ];
  }

  /* ======================================================================
   * 5. B. folder（钻取 · 全屏 + 返回）
   * ==================================================================== */
  VS.pages['folder'] = function (ctx) {
    var id = ctx.params && ctx.params.id;
    var folder = D.entry(id);
    if (!folder || folder.kind !== 'folder') {
      return MUI.page({
        title: '文件夹不存在',
        back: true,
        body: MUI.screen([MUI.empty({ icon: 'help', title: '找不到该文件夹', desc: '它可能已被删除或移动。' })]),
        tab: true
      });
    }
    var c = makeCtx(ctx, 'folder', folder.id, false);
    var body = [crumbBar(c), viewSeg(c)];
    var rows = renderListBody(c);
    rows.forEach(function (n) { body.push(n); });
    body.push(h('div', { class: 't-caption', style: { padding: '0 var(--sp-1)' },
      text: '长按条目 = 动作面板；左滑 = 等价降级。' }));

    return MUI.page({
      title: folder.name,
      sub: F.num(D.children(folder.id).length) + ' 项 · ' + F.relative(folder.modifiedMs),
      back: true,
      actions: [{ icon: 'search', label: '搜索', onClick: function () { ctx.go('search'); } }],
      overflow: overflowItems(c).concat([
        { group: '文件夹' },
        { label: '文件夹属性…', icon: 'info', onClick: function () { folderSheet(folder, c); } }
      ]),
      body: MUI.screen(body),
      tab: true,
      fab: MUI.fab({ icon: 'plus', label: '新建', onClick: function () { plusActions(c); } })
    });
  };

  /* ======================================================================
   * 6. C. file（文件详情全屏页 · 5 段）
   * ==================================================================== */
  VS.pages['file'] = function (ctx) {
    var id = ctx.params && ctx.params.id;
    var e = D.entry(id);
    if (!e || e.kind !== 'file') {
      return MUI.page({
        title: '文件不存在',
        back: true,
        body: MUI.screen([MUI.empty({ icon: 'help', title: '找不到该文件', desc: '它可能已被删除、移动或隔离。' })]),
        tab: true
      });
    }
    var gate = writeGate();
    var active = 'basic';
    var host = h('div', { class: 'col gap-3' });
    var segHost = h('div');
    var meta = eraseMeta(e.eraseClass);

    function renderSeg() {
      segHost.innerHTML = '';
      segHost.appendChild(MUI.mtabs({
        value: active,
        items: [
          { value: 'basic', label: '基本信息' },
          { value: 'crypto', label: '加密与分片' },
          { value: 'index', label: '检索与缩略图' },
          { value: 'erase', label: '擦除与副本' },
          { value: 'audit', label: '审计关联' }
        ],
        onChange: function (v) { active = v; renderSeg(); renderSection(); }
      }));
    }

    function renderSection() {
      host.innerHTML = '';
      if (active === 'basic') renderBasic();
      else if (active === 'crypto') renderCrypto();
      else if (active === 'index') renderIndex();
      else if (active === 'erase') renderErase();
      else renderAudit();
    }

    /* A · 基本信息 */
    function renderBasic() {
      host.appendChild(MUI.mcard({
        title: 'A · 文件基本信息',
        body: MUI.mkv([
          { k: 'id', v: 'f_' + e.id, mono: true },
          { k: '名称', v: e.name },
          { k: '类型', v: 'file · ' + e.mime },
          { k: '父文件夹', v: (D.entry(e.parent) || {}).name || '根目录' },
          { k: '大小', v: F.bytes(e.size) },
          { k: '创建时间', v: F.dateLong(e.createdMs) },
          { k: '修改时间', v: F.dateLong(e.modifiedMs) },
          { k: '版本 rev', v: String(e.rev) }
        ])
      }));
      host.appendChild(MUI.mcard({
        title: '标签',
        body: [
          h('div', { class: 'row wrap gap-2' }, (e.tags || []).length
            ? e.tags.map(function (t) { return MUI.chip(t); })
            : [h('span', { class: 't-caption', text: '无标签' })]),
          h('div', { class: 'mt-2' }, [MUI.muted('标签为写库对象：只读降级与维护态下编辑入口禁用。')])
        ]
      }));
      var badges = entryBadges(e, false);
      if (badges.length) {
        host.appendChild(MUI.mcard({ title: '状态徽标', body: h('div', { class: 'row wrap gap-2' }, badges) }));
      }
    }

    /* B/C/D · 加密元数据 / CDC 分块 / 密钥层次 */
    function renderCrypto() {
      host.appendChild(MUI.mcard({
        title: 'B · 加密元数据',
        body: MUI.mkv([
          { k: 'cipher_suite', v: e.cipherSuite === 2 ? '2 · X25519 + ML-KEM-768 混合' : '1 · 经典 ECDH + HKDF' },
          { k: 'kdf_ver', v: String(e.kdfVer) + '（Argon2id 64 MiB · t=3 · p=2）' },
          { k: 'chunk_cfg', v: '1 · CDC 64–256 KiB（小文件 16 KiB）' },
          { k: 'flags', v: 'bit0 擦除元数据区 · bit1 元数据分片 · bit2 已终验；bit3–15 为 0' },
          { k: 'format_ver', v: String(e.formatVer) + '（> 3 向前拒绝，绝不尽力解析）' },
          { k: 'hdr_crc32', v: e.hdrCrc32, mono: true }
        ])
      }));
      var chunks = chunkStates(e, 120);
      host.appendChild(MUI.mcard({
        title: 'C · CDC 分块详情',
        sub: '块上限 256 KiB · 下限 16 KiB；非末片先落 .part、终验后原子改名',
        body: [
          MUI.mkv([
            { k: 'meta_shard_count', v: String(e.metaShardCount) },
            { k: 'meta_shard_span', v: F.num(e.metaShardSpan) + ' 块 / 片' },
            { k: 'chunk_count', v: F.num(e.chunkCount) },
            { k: 'nonce_prefix', v: e.noncePrefix + '（4B BE 前缀 ‖ 8B BE 计数器）', mono: true },
            { k: 'file_sha256', v: e.fileSha256, mono: true }
          ]),
          h('div', { class: 'mt-3' }, [secTitle('layers', '块状态矩阵（前 ' + F.num(chunks.length) + ' 块）')]),
          h('div', { class: 'mt-2' }, [MUI.chunkMatrix(chunks)]),
          legendRow([
            { color: 'var(--c-success)', label: 'done' },
            { color: 'var(--c-accent-alt)', label: 'running' },
            { color: 'var(--c-danger)', label: 'failed' },
            { color: 'var(--c-surface-alt)', label: 'pending' }
          ])
        ]
      }));
      host.appendChild(MUI.mcard({
        title: 'D · 密钥层次',
        sub: '只显示存在性，不显示密钥值（零信任：密钥永不出引擎）',
        body: MUI.mkv([
          { k: 'FSKey', v: e.fskeyAlgo, mono: true },
          { k: 'FSK', v: 'HKDF(MK, "fsk/' + e.parent + '") · 轮换后由索引 fskey_override 承接', mono: true },
          { k: '块 nonce', v: 'per-file 随机前缀 ‖ 块计数器（同一 FSKey 下禁止随机 nonce）' },
          { k: '从属密钥', v: '索引 key_id=1 · 隐写载荷 key_id=5（归安全中心展示）' },
          { k: 'rotated_with', v: e.rotateWith, mono: true },
          { k: '密钥值', v: '不可见 — 引擎不导出任何密钥材料', mono: false }
        ])
      }));
    }

    /* H/I · 检索与缩略图 */
    function renderIndex() {
      host.appendChild(MUI.mcard({
        title: 'H · 检索与提取状态',
        body: [
          MUI.mkv([
            { k: 'extract_ver', v: String(e.extractVer) },
            { k: 'truncated', v: e.truncated ? 'true — 仅索引了前 64 MiB' : 'false' },
            { k: '类型支持', v: e.fulltextSupported ? '参与全文检索' : '该类型不参与全文检索' },
            { k: '提取上限', v: '64 MiB / 10 000 token（超出部分不产生全文命中）' }
          ]),
          e.truncated ? h('div', { class: 'mt-3' }, [UI.alertbar({
            tone: 'warn', title: '仅索引了前 64 MiB',
            text: '超出部分仍可被文件名与标签命中，但不会被全文命中 —— 不伪造命中。'
          })]) : null,
          !e.fulltextSupported ? h('div', { class: 'mt-3' }, [UI.alertbar({
            tone: 'info', title: '该类型不参与全文检索',
            text: '按类型如实声明；搜索页对应结果不产生全文命中。'
          })]) : null
        ]
      }));
      /* 能力三档：CAP_THUMBNAIL 未置位 → 整块不渲染 */
      if (S.cap('CAP_THUMBNAIL')) {
        host.appendChild(MUI.mcard({
          title: 'I · 缩略图',
          sub: '不落盘 · 不进剪贴板 · 锁定或转只读立即清空',
          body: e.thumb
            ? MUI.mkv([
                { k: 'mime', v: 'image/png' },
                { k: '尺寸', v: '256 × 160（≤256 px 长边）' },
                { k: 'bytes', v: F.bytes(40960) },
                { k: 'sha256', v: D.hash(64), mono: true }
              ])
            : h('div', { class: 't-caption', text: '该类型不支持缩略图预览（仅 png / jpg / webp / gif 首帧）→ 预览区不渲染。' })
        }));
      }
    }

    /* J/G · 擦除状态与副本 */
    function renderErase() {
      var wordOk = secureEraseWordAllowed(e.eraseClass);
      host.appendChild(MUI.mcard({
        title: 'J · 擦除状态',
        body: [
          h('div', { class: 'row gap-3 wrap' }, [
            MUI.eraseBadge(e.eraseClass),
            UI.badge({ text: 'media_kind = ' + e.mediaKind, tone: 'stale' }),
            UI.badge({ text: 'method_bits = ' + methodBitsFor(e.eraseClass), tone: 'stale' })
          ]),
          h('div', { class: 'mt-3' }, [MUI.mkv([
            { k: 'erase_class 原值', v: String(e.eraseClass) + ' · ' + meta.label },
            { k: 'method_bits', v: methodBitsFor(e.eraseClass), mono: true },
            { k: 'media_kind', v: e.mediaKind + ' · ' + mediaKindLabel(e.mediaKind) },
            { k: '引擎说明', v: meta.note }
          ])]),
          h('div', { class: 'mt-3' }, [secTitle('alert', 'degradations[]')]),
          h('div', { class: 'col gap-1 mt-2' }, degradationsFor(e.eraseClass, e.mediaKind).map(function (d) {
            return h('div', { class: 't-caption', text: '· ' + d });
          })),
          h('div', { class: 'mt-3' }, [UI.alertbar(wordOk ? {
            tone: 'info', title: '平台安全删除位已置位',
            text: 'erase_class = platform_secure 且 method_bits 含平台安全删除位（bit2 = 1），措辞成立的充要条件已满足。'
          } : {
            tone: 'warn', title: '介质覆写未执行',
            text: '本次 erase_class 为 ' + meta.key + '：按文案硬约束必须写明「介质覆写未执行」，界面不得用更强的表述描述本次结果。'
          })])
        ]
      }));
      host.appendChild(MUI.mcard({
        title: 'G · 副本与误删保护',
        body: [
          UI.alertbar({
            tone: 'info', title: '误删保护窗口 24 小时',
            text: '窗口内删除先保留加密副本、FSKey 仍可用 →「加密擦除」可撤销；窗口结束后副本与密钥一并销毁。'
          }),
          h('div', { class: 'mt-3' }, [MUI.mkv([
            { k: '保护状态', v: '受保护（未删除）' },
            { k: '撤销截止', v: '—（删除后起算 24 小时）' },
            { k: '清理重试', v: '启动扫描 / 手动「立即重试清理」' }
          ])])
        ]
      }));
    }

    /* K · 审计关联 */
    function renderAudit() {
      var logs = D.auditLog.filter(function (a) { return a.cat === 'vault'; }).slice(0, 8);
      var rows = logs.map(function (a) {
        return {
          icon: 'link',
          title: '#' + F.num(a.seq) + ' · ' + a.opLabel,
          sub: F.dateLong(a.tsMs) + ' · 链头 ' + U.shortHash(a.headHash, 8, 6),
          trail: (a.detail && a.detail.result === 0)
            ? UI.badge({ text: 'OK', tone: 'success' })
            : UI.badge({ text: '码 ' + ((a.detail && a.detail.result) || 12), tone: 'stale' }),
          chevron: false
        };
      });
      host.appendChild(MUI.mcard({
        title: 'K · 审计关联',
        sub: '脱敏口径：不含目标路径、不含名称原文、不含查询词',
        body: [
          rows.length
            ? MUI.mlist(rows.map(function (r) { return MUI.mrow(r); }), { flush: true })
            : h('div', { class: 't-caption', text: '尚无 vault 类别审计条目。' }),
          h('div', { class: 'mt-3' }, [MUI.muted('缩略图生成、存储占用统计、列表/详情读取、诊断包导出为「无审计路径」的有意设计，界面不得暗示已记录审计。')]),
          srcNote('vault_core_audit_list + vault_core_audit_verify')
        ]
      }));
    }

    renderSeg();
    renderSection();

    /* 底部固定操作条（body 末尾，.msheet-foot 风格） */
    var footer = h('div', {
      class: 'col gap-2',
      style: {
        marginTop: 'var(--sp-2)', paddingTop: 'var(--sp-3)',
        borderTop: '1px solid var(--c-border)'
      }
    }, [
      blockBtn('导出…', 'primary', function () { exportSheet([e], ctx); }, { icon: 'export' }),
      blockBtn('校验此文件', 'ghost', function () { verifySheet([e.id]); }, { icon: 'check-circle' }),
      blockBtn('重命名…', 'ghost', function () { renameSheet(e, ctx); }, { icon: 'edit', disabled: gate.disabled, title: gate.reason || '重命名' }),
      blockBtn('删除…', 'danger', function () { wipeSheet([e], 'delete', ctx); }, { icon: 'trash', disabled: gate.disabled, title: gate.reason || '删除' }),
      S.get('stegoEnabled')
        ? blockBtn('隐写嵌入…', 'ghost', function () {
            UI.toast({ tone: 'info', title: '隐写未接入移动端面板', msg: '本端仅提供只读隐写提取；嵌入请回桌面端。' });
          }, { icon: 'stego' })
        : null,
      gate.disabled ? h('div', { class: 't-caption', text: gate.reason }) : null
    ]);

    return MUI.page({
      title: e.name,
      sub: F.bytes(e.size) + ' · rev ' + e.rev + ' · ' + F.relative(e.modifiedMs),
      back: true,
      actions: [{ icon: 'more', label: '更多操作', onClick: function () { entryActions(ctx, e); } }],
      overflow: null,
      body: MUI.screen([segHost, h('div', { class: 'mt-3' }, [host]), footer]),
      tab: false
    });
  };

  /* ======================================================================
   * 7. D. search（全屏搜索页）
   * ==================================================================== */
  VS.pages['search'] = function (ctx) {
    var fulltextOk = S.cap('CAP_FULLTEXT');
    var scope = { name: true, tag: true, content: fulltextOk };
    var input = MUI.input({
      value: S.get('searchQuery') || '',
      placeholder: '输入关键词',
      onKeyDown: function (ev) { if (ev.key === 'Enter') run(); }
    });
    var results = h('div', { class: 'col gap-3' });
    var summary = h('div', { class: 't-caption', text: '尚未检索' });
    var limit = 50;

    var scopeRow = h('div', { class: 'row wrap gap-3' }, ['name', 'tag', 'content'].map(function (k) {
      var label = { name: '文件名', tag: '标签', content: '全文内容' }[k];
      var disabled = k === 'content' && !fulltextOk;
      var cb = MUI.checkbox({
        label: label, checked: scope[k], disabled: disabled,
        sub: disabled ? '本引擎仅支持文件名与标签检索（CAP_FULLTEXT 未置位，码 13）' : null,
        onChange: function (ev) {
          scope[k] = ev.target.checked;
          if (k === 'content' && !fulltextOk) return;
          run();
        }
      });
      if (disabled) cb.setAttribute('title', '能力不支持（码 13）：本引擎仅支持文件名与标签检索');
      return cb;
    }));

    /* 片段 + 位置提示均来自命中结果本身（脱敏形态：上下文有限、敏感段以 ••• 替代） */
    function snippetNode(hit) {
      var raw = hit.text;
      var at = hit.at;
      var qlen = hit.qlen;
      var head = raw.slice(Math.max(0, at - 12), at);
      var word = raw.slice(at, at + qlen);
      var tail = raw.slice(at + qlen, at + qlen + 16);
      return h('div', {
        class: 'hit-snippet mt-1',
        style: {
          fontFamily: 'var(--font-mono)', fontSize: 'var(--fs-mono)', lineHeight: '1.7',
          padding: 'var(--sp-2) var(--sp-3)', background: 'var(--c-surface-alt)',
          borderRadius: 'var(--r-sm)', borderLeft: '2px solid var(--c-accent)',
          wordBreak: 'break-word'
        }
      }, [
        h('span', { text: '…' + head }),
        h('mark', { style: { background: 'var(--c-accent-soft)', color: 'var(--c-accent)', padding: '0 2px', borderRadius: '3px' }, text: word }),
        h('span', { class: 'redact', style: { color: 'var(--c-text-2)', letterSpacing: '.1em' }, text: ' ••• ' }),
        h('span', { text: tail + '…' }),
        h('div', { class: 't-caption mt-1', text: '第 ' + F.num(hit.para) + ' 段 · 字符 ' + F.num(at) })
      ]);
    }

    function pickEntries(q) {
      var hits = [];
      D.vault.entries.forEach(function (e) {
        if (e.kind !== 'file') return;
        if (scope.name && e.name.toLowerCase().indexOf(q) >= 0) { hits.push({ e: e, cls: 'name' }); return; }
        if (scope.tag && (e.tags || []).some(function (t) { return t.toLowerCase().indexOf(q) >= 0; })) { hits.push({ e: e, cls: 'tag' }); return; }
        if (!scope.content || !fulltextOk) return;
        if (!e.fulltextSupported) return;          /* 该类型不参与全文检索，不伪造命中 */
        if (e.truncated) return;                   /* 仅索引前 64 MiB，候选只来自前 64 MiB */
        var m = mockContentHits(e, q);
        if (m) hits.push({ e: e, cls: 'content', para: m.para, at: m.at, text: m.text, qlen: q.length });
      });
      return hits;
    }

    /** 三段必现文案之一：该类型不参与全文检索 */
    function unsupportedNote() {
      return h('div', { class: 't-caption', style: { marginTop: '2px' }, text: '该类型不参与全文检索' });
    }
    /** 三段必现文案之二：仅索引了前 64 MiB */
    function truncatedNote() {
      return h('div', { class: 't-caption', style: { marginTop: '2px' }, text: '仅索引了前 64 MiB —— 超出部分不会被全文命中' });
    }
    function noteFor(e) {
      if (e.truncated) return truncatedNote();          /* 仅索引了前 64 MiB —— 必须出现 */
      if (!e.fulltextSupported) return unsupportedNote(); /* 该类型不参与全文检索 —— 必须出现 */
      return null;
    }

    function resultRow(hit) {
      var e = hit.e;
      var note = noteFor(e);
      return MUI.mrow({
        icon: entryIcon(e),
        title: e.name,
        sub: F.bytes(e.size) + ' · ' + F.relative(e.modifiedMs) + ' · ' + e.mime,
        trail: UI.badge({ text: { name: '文件名', tag: '标签', content: '全文' }[hit.cls], tone: hit.cls === 'content' ? 'accent' : 'stale' }),
        onClick: function () { ctx.go('file', { id: e.id }); },
        onLongPress: function () {
          MUI.actionSheet({
            title: e.name,
            items: [
              { label: '打开详情', icon: 'info', onClick: function () { ctx.go('file', { id: e.id }); } },
              { label: '导出…', icon: 'export', sub: '只读降级下仍可用', onClick: function () { exportSheet([e], ctx); } },
              { label: '校验此文件', icon: 'check-circle', onClick: function () { verifySheet([e.id]); } }
            ]
          });
        },
        badges: note
      });
    }

    function run() {
      results.innerHTML = '';
      var q = (input.value || '').toLowerCase().trim();
      S.set({ searchQuery: input.value || '' }, true);
      if (!q) {
        results.appendChild(MUI.empty({ icon: 'search', title: '输入关键词开始检索', desc: '检索范围可只勾文件名，减少噪声。' }));
        summary.textContent = '尚未检索';
        return;
      }
      var hits = pickEntries(q);
      summary.textContent = '命中 ' + F.num(hits.length) + ' 项 · 上限 ' + F.num(limit) + ' · rankVer 3';
      if (!hits.length) {
        results.appendChild(MUI.empty({
          icon: 'search', title: '没有命中',
          desc: '不支持的类型不产生命中，也不会伪造命中。'
        }));
        return;
      }
      [['name', '文件名命中'], ['tag', '标签命中'], ['content', '全文命中']].forEach(function (pair) {
        var group = hits.filter(function (x) { return x.cls === pair[0]; });
        if (!group.length) return;
        results.appendChild(MUI.mgroupTitle(pair[1] + '（' + F.num(group.length) + '）'));
        var rows = [];
        group.slice(0, limit).forEach(function (hit) {
          rows.push(resultRow(hit));
          /* 全文命中：片段与位置提示紧跟在行下方（不换行到列表外） */
          if (hit.cls === 'content') rows.push(h('div', { style: { padding: '0 var(--sp-4) 10px' } }, [snippetNode(hit)]));
        });
        results.appendChild(MUI.mlist(rows, { flush: true }));
      });
      results.appendChild(UI.alertbar({
        tone: 'info', title: '此处有命中但不展示',
        text: 'scan:true 的段落整段替换为 •••；片段与位置提示来自引擎脱敏结果，外壳不接触明文。'
      }));
    }

    var body = [
      h('div', { class: 'row gap-2' }, [
        h('div', { class: 'grow' }, [input]),
        h('button', {
          class: 'mbtn-block', type: 'button', dataset: { variant: 'primary' },
          style: { width: '76px', flex: 'none' }, title: '检索', 'aria-label': '检索',
          onclick: run
        }, [h('span', { html: VS.icon('search', 18) })])
      ]),
      h('div', { class: 'mt-3' }, [secTitle('filter', '检索范围')]),
      h('div', { class: 'mt-2' }, [scopeRow]),
      fulltextOk ? null : h('div', { class: 'mt-3' }, [UI.alertbar({
        tone: 'warn', title: '能力不支持（码 13）',
        text: '本引擎仅支持文件名与标签检索 —— 不谎报全文检索，也不降级为假装成功。'
      })]),
      h('div', { class: 'mt-3' }, [summary]),
      h('div', { class: 'mt-2' }, [results])
    ];

    return MUI.page({
      title: '搜索',
      sub: 'vault_search_v2 · 单文件提取上限 64 MiB / 10 000 token',
      back: true,
      actions: [],
      overflow: null,
      body: MUI.screen(body),
      tab: false
    });
  };

  /* ======================================================================
   * 8. E. deleted（已删除与待清理）
   * ==================================================================== */
  VS.pages['deleted'] = function (ctx) {
    var gate = writeGate();
    var list = D.deletedEntries || [];

    function deletedRow(d) {
      var pending = !!d.pendingCleanup;
      return MUI.mrow({
        icon: pending ? 'erase' : 'recover',
        title: d.name,
        sub: F.bytes(d.size) + ' · 删除于 ' + F.relative(d.deletedMs),
        trail: null,
        badges: h('span', { class: 'row gap-1' }, [MUI.eraseBadge(d.eraseClass)]),
        onClick: function () { deletedActions(d); },
        onLongPress: function () { deletedActions(d); },
        swipe: [
          {
            label: '恢复', icon: 'recover',
            onClick: function () {
              if (gate.disabled) { UI.toast({ tone: 'warn', title: '恢复不可用', msg: gate.reason }); return; }
              UI.toast({ tone: 'success', title: '已恢复', msg: d.name + ' · 加密副本已解封' });
            }
          },
          {
            label: '重试清理', icon: 'erase', tone: 'danger',
            onClick: function () {
              if (gate.disabled) { UI.toast({ tone: 'warn', title: '清理不可用', msg: gate.reason }); return; }
              UI.toast({ tone: 'warn', title: '介质擦除待完成', msg: '已重新入队清理扫描；不得显示为「已擦除」。' });
            }
          }
        ]
      });
    }

    function deletedActions(d) {
      var pending = !!d.pendingCleanup;
      MUI.actionSheet({
        title: d.name,
        sub: F.bytes(d.size) + ' · 删除于 ' + F.dateLong(d.deletedMs) + ' · ' + eraseMeta(d.eraseClass).label,
        items: [
          {
            label: itemLabel('恢复', gate), icon: 'recover', disabled: gate.disabled, reason: gate.reason,
            sub: '窗口内加密副本与 FSKey 仍可用',
            onClick: function () { UI.toast({ tone: 'success', title: '已恢复', msg: d.name }); }
          },
          {
            label: itemLabel('立即重试清理', gate), icon: 'erase', danger: true, disabled: gate.disabled, reason: gate.reason,
            sub: pending ? '当前状态：介质擦除待完成' : '跳过 24 小时保护窗口，直接销毁副本与密钥',
            onClick: function () {
              UI.toast({ tone: 'warn', title: '介质擦除待完成', msg: '物理删除失败时显示「待清理」，不得显示「已擦除」。' });
            }
          },
          { label: '查看擦除强度说明', icon: 'info', sub: eraseMeta(d.eraseClass).note, onClick: function () {
            UI.toast({ tone: 'info', title: eraseMeta(d.eraseClass).label, msg: eraseMeta(d.eraseClass).note });
          } }
        ]
      });
    }

    var listRows = [];
    list.forEach(function (d) {
      var pending = !!d.pendingCleanup;
      listRows.push(deletedRow(d));
      listRows.push(h('div', {
        class: 'row wrap gap-1',
        style: { padding: '0 var(--sp-4) 10px', marginTop: '-2px' }
      }, [
        pending
          ? UI.badge({ text: '介质擦除待完成', tone: 'danger', icon: 'alert' })
          : UI.badge({ text: '可恢复 · 剩 ' + F.duration(d.protectUntil - Date.now()), tone: 'success' }),
        UI.badge({ text: 'media_kind = ' + d.mediaKind + ' · ' + mediaKindLabel(d.mediaKind), tone: 'stale' }),
        d.fsKeyDestroyedMs ? UI.badge({ text: 'FSKey 已销毁', tone: 'stale' }) : null
      ]));
    });

    var body = [
      MUI.limitedNote('24 小时误删保护窗口：窗口内保留加密副本与 FSKey，「加密擦除」可撤销；窗口结束后副本与密钥一并销毁。物理删除失败的条目显示「介质擦除待完成」，不得显示「已擦除」。'),
      listRows.length
        ? MUI.mlist(listRows)
        : MUI.empty({ icon: 'trash', title: '没有待清理条目', desc: '删除的条目会在保护窗口结束后出现在这里。' }),
      h('div', { class: 't-caption', text: '长按条目 = 动作面板；左滑 = 等价降级（恢复 / 立即重试清理）。' }),
      srcNote('vault_core_erase_status + vault_core_list_deleted')
    ];

    return MUI.page({
      title: '已删除与待清理',
      sub: F.num(list.length) + ' 项 · 误删保护窗口 24 小时',
      back: true,
      actions: [],
      overflow: [
        { label: itemLabel('立即重试全部清理', gate), icon: 'erase', disabled: gate.disabled, reason: gate.reason,
          onClick: function () { UI.toast({ tone: 'warn', title: '介质擦除待完成', msg: '已入队全部待清理条目。' }); } },
        { label: '导出清理报告', icon: 'export', onClick: function () {
          UI.toast({ tone: 'info', title: '已导出', msg: '清理报告仅含 id / 字节数 / 擦除强度原值' });
        } }
      ],
      body: MUI.screen(body),
      tab: false
    });
  };

  /* ======================================================================
   * 9. 动作注册（供其它移动端视图 / 演示控制台调用）
   * ==================================================================== */
  VS.actions['mobile.import'] = function () { importSheet(S.get('cwd') || 0); };
  VS.actions['mobile.newFolder'] = function () {
    var ctx = { cwdId: S.get('cwd') || 0, refresh: function () { VS.render && VS.render(); } };
    newFolderSheet(ctx.cwdId, ctx);
  };
  VS.actions['mobile.search'] = function () { VS.nav && VS.nav.go('search'); };
  VS.actions['mobile.deleted'] = function () { VS.nav && VS.nav.go('deleted'); };
  VS.actions['mobile.verifyAll'] = function () { verifySheet(null); };
  VS.actions['mobile.storage'] = function () { storageSheet(); };
  VS.actions['mobile.exportList'] = function () {
    UI.toast({ tone: 'info', title: '已导出清单', msg: '仅含元数据，不含文件名以外的敏感字段' });
  };

})(window);
