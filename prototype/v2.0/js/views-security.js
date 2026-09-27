/* ============================================================================
 * VaultSync V2.0 原型 — 视图：安全中心 / 隐写术 / 全屏强告警（/alarm）
 * 纯静态、零依赖、经典脚本（不使用 ES Module / fetch / 外部库）
 *
 * 注册契约：
 *   VS.pages['security'] = function (ctx) -> Node      ctx = { params: {}, go: fn }
 *   VS.pages['stego']    = function (ctx) -> Node
 *   VS.pages['alarm']    = function (ctx) -> Node      （内部用 UI.fullscreen 阻塞式全屏）
 *   VS.actions['…']      = function (ctx) -> void
 *
 * 语料依据：
 *   docs/v2.0/04 §三.5（安全中心六项仪表）
 *   docs/v2.0/05-02 §三.4–.5（擦除强度 / 可用性矩阵）、§七.1（审计脱敏与导出）
 *   docs/v2.0/05-05（隐写术：边界、容量、反检测、plan / embed_multi / extract_multi）
 *   docs/v2.0/09 P7-5 / P7-10 / P7-11 / P8-10 / P9-5
 *
 * 纪律：
 *   - 接口无数据一律灰态，不显示 0、不显示绿
 *   - 「安全擦除」字样仅在 erase_class = platform_secure 且 method_bits 含平台安全删除位时出现
 *   - 引擎原值直出，不把 unknown 美化成「安全擦除」
 *   - 容量真值只来自 vault_core_stego_plan，UI 不自行计算容量
 *   - 只读降级 / 维护态：写操作禁用并给 reason；维护态下「擦除 / 销毁」是唯一被允许的写操作
 *   - 审计连续写入失败 → 灰态「审计不可用」，不阻断业务，也不声称「审计通过」
 *
 * 本文件只依赖 core.js / ui.js / data.js，不修改任何其它文件；
 * UI 未提供的原语一律在本文件内部实现为局部函数。
 * ==========================================================================*/
(function (global) {
  'use strict';

  var VS = global.VS;
  var h = VS.util.h, U = VS.util, UI = VS.ui, F = VS.fmt, D = VS.data, S = VS.store;

  VS.pages = VS.pages || {};
  VS.actions = VS.actions || {};

  /* ======================================================================
   * 0. 局部原语（UI.util / UI 未提供的能力）
   * ==================================================================== */

  function cap(text) { return h('div', { class: 't-caption', text: text }); }
  function strong(text) { return h('div', { class: 't-strong', text: text }); }
  function mono(text) { return h('div', { class: 't-mono', text: text }); }
  function host(cls) { return h('div', { class: cls }); }

  /** 页面统一骨架：page-head(page-titles + page-actions) + page-body */
  function pageShell(title, sub, actions, body) {
    return h('div', { class: 'page' }, [
      h('div', { class: 'page-head' }, [
        h('div', { class: 'page-titles' }, [
          h('h1', { class: 't-h1', text: title }),
          h('div', { class: 't-caption', text: sub })
        ]),
        h('div', { class: 'page-actions' }, actions || [])
      ]),
      h('div', { class: 'page-body' }, body || [])
    ]);
  }

  /** 进度条宿主：内部始终用 UI.bar 渲染，setBar 只替换这一层 */
  function barHost(pct, tone, size) {
    var node = h('div');
    node.setBar = function (p, t) {
      node.innerHTML = '';
      node.appendChild(UI.bar(p === null || p === undefined || isNaN(p) ? 0 : p, t || tone, size));
    };
    node.setBar(pct);
    return node;
  }

  /* --- 无障碍兜底：所有可交互元素必须有 title 或 aria-label --------------- */
  function a11y(node, label) {
    if (node && label && node.setAttribute) {
      node.setAttribute('title', label);
      if (!node.hasAttribute('aria-label')) node.setAttribute('aria-label', label);
    }
    return node;
  }
  /** UI.select 的触发按钮由组件内部创建，创建后补 title / aria-label */
  function a11ySelect(node, label) {
    var b = node && node.querySelector ? node.querySelector('.select-btn') : null;
    if (b) { b.setAttribute('title', label); b.setAttribute('aria-label', label); }
    return node;
  }
  /** UI.radioCards 的卡片带 role=radio 但没有可读名称，创建后补 */
  function a11yRadioCards(node, options) {
    var cards = node && node.querySelectorAll ? node.querySelectorAll('.radio-card') : [];
    for (var i = 0; i < cards.length; i++) {
      var o = options[i] || {};
      var t = (o.title || '') + (o.desc ? ' · ' + o.desc : '') + (o.disabled && o.reason ? ' · 不可用：' + o.reason : '');
      cards[i].setAttribute('title', t);
      cards[i].setAttribute('aria-label', t);
    }
    return node;
  }

  /* --- 状态三档与错误码文案 --------------------------------------------- */
  var STATE_META = {
    ok:      { tone: 'success', label: '通过' },
    warn:    { tone: 'warning', label: '注意' },
    danger:  { tone: 'danger',  label: '异常' },
    unknown: { tone: 'stale',   label: '无数据' },
    design:  { tone: 'design',  label: '设计保证' }
  };
  function normState(s) { return STATE_META[s] ? s : 'unknown'; }

  /** 仪表状态徽标：unknown / design 一律灰，不显示 0、不显示绿 */
  function stateBadge(s) {
    var st = normState(s), meta = STATE_META[st];
    if (st === 'design') {
      return UI.badge({ text: meta.label, design: true, title: '契约清单中无此接口：按设计保证标注，不代表运行时已验证' });
    }
    if (st === 'unknown') {
      return UI.badge({ text: meta.label, tone: 'stale', title: '接口未返回数据：灰态展示，不显示 0 条、不显示为通过' });
    }
    return UI.badge({ text: meta.label, tone: meta.tone });
  }

  function errText(code) {
    var e = VS.ERR[code] || {};
    return '错误码 ' + F.num(code) + ' · ' + (e.title || '不可用') + (e.hint ? '：' + e.hint : '');
  }

  /* --- 写操作闸门（VS.derive.denyWrite）-------------------------------- */
  function denyWrite() {
    var c = VS.derive.denyWrite();
    return c === null ? null : { code: c, text: errText(c) };
  }
  /** 维护态例外：擦除 / 销毁 是维护态下唯一被允许的写操作（05-02 §三.5 / 09 P6-6） */
  function denyErase() {
    if (S.get('lock') !== 'Unlocked') return { code: 5, text: errText(5) };
    if (S.get('readOnly')) return { code: 9, text: errText(9) };
    return null;
  }
  function inMaintenance() { return !!S.get('maintenance') || S.get('session') === 'Maintenance'; }

  /** 受写闸门约束的按钮：禁用时把 reason 放进 title，不用 toast 假装可用 */
  function guardedBtn(o) {
    var d = o.deny ? o.deny() : denyWrite();
    return UI.btn({
      label: o.label, icon: o.icon, variant: o.variant || 'primary', size: o.size, block: o.block,
      disabled: !!d,
      title: d ? (o.label + ' 不可用 · ' + d.text) : (o.title || o.label),
      onClick: o.onClick
    });
  }

  /* --- 只读 / 维护态横幅 ------------------------------------------------ */
  function statusBanners(opts) {
    opts = opts || {};
    var out = [];
    if (S.get('readOnly')) {
      out.push(UI.alertbar({
        tone: 'stale', icon: 'lock',
        title: '只读降级（' + errText(9) + '）',
        text: '写操作已禁用并给出原因。只读仍可用：' + VS.derive.readOnlyAllowed.join(' / '),
        actions: [UI.btn({
          label: '查看租约诊断', size: 'sm', title: '查看写租约诊断（只读）',
          onClick: function () { UI.toast({ tone: 'info', title: '租约诊断', msg: S.get('readonlyReason') || '另一实例持有 vault.lock（演示）' }); }
        })]
      }));
    }
    if (inMaintenance()) {
      out.push(UI.maintBanner({
        tone: 'maintenance',
        title: '维护态：业务写入冻结（' + errText(10) + '）',
        sub: '导入 / 编辑 / 重命名 / 打标签 / 删除 / 分享 / 同步 / 轮换 / 迁移 均被拒绝' +
          (opts.eraseException === false ? '。' : '；维护态下「擦除 / 销毁」是唯一被允许的写操作（用户要求销毁高于数据结构一致）。')
      }));
    }
    return out;
  }

  /* --- 统一导航（ctx.go 由外壳注入）------------------------------------ */
  function go(ctx, route, params) {
    if (ctx && typeof ctx.go === 'function') { ctx.go(route, params); return true; }
    UI.toast({ tone: 'info', title: '导航未接线', msg: '外壳未提供 ctx.go，无法跳转到 ' + route + '（导航层由 app.js 实现）' });
    return false;
  }

  /* --- 页面重渲染钩子（演示开关改动后刷新当前页）------------------------ */
  var currentRender = null;
  function rerender() { if (typeof currentRender === 'function') currentRender(); }

  /** 演示开关行（原型专用：只切换展示分支） */
  function demoToggle(label, sub, checked, onChange) {
    var sw = UI.switchCtl({
      checked: checked, sub: sub,
      onChange: function () { onChange(!!sw.input.checked); }
    });
    return h('div', { class: 'row-between' }, [
      h('div', { class: 'grow' }, [strong(label), sub ? cap(sub) : null]),
      sw
    ]);
  }

  /* ======================================================================
   * 1. 模块内状态（页面级 / 演示级）
   * ==================================================================== */
  var BROKEN_HEAD = D.hash(64);          /* 演示用「断点后的链头」 */
  var secTab = null;                     /* 由 S.get('secTab') 初始化 */
  var demo = {
    auditBroken: false,                  /* 审计链校验失败 → danger + brokenAt */
    auditDown: false,                    /* 审计连续失败 → 灰态「审计不可用」 */
    physicalDeleteFailed: false,         /* 物理删除失败 → 介质擦除待完成 */
    maskPlatformBit: false,              /* 去掉平台安全删除位 → 禁止出现「安全擦除」 */
    clockSkew: false,                    /* 系统时钟异常（超前 > 24h） */
    stegoLegacy: false,                  /* 验证 V1.0 兼容读提示 */
    stegoNewer: false                    /* 验证「无法读取更高版本载荷」 */
  };
  var auditFilter = { cat: 'all', result: 'all', from: '', to: '', q: '' };
  var destroyState = { scope: 'vault', delay: 'now', customAt: '', expiresDays: 7, clockAck: false };
  var embedWiz = null, extractWiz = null;
  var alarmSeen = {}, alarmActive = null, alarmSrc = 'audit_verify';
  var ALARM_IDEMPOTENT_MS = 5 * 60 * 1000;

  /* ======================================================================
   * 2. 安全中心（/security）
   * ==================================================================== */

  var SEC_TABS = [
    { value: 'checks',  label: '检测项',   icon: 'gauge' },
    { value: 'erase',   label: '擦除强度', icon: 'erase' },
    { value: 'audit',   label: '审计日志', icon: 'history' },
    { value: 'destroy', label: '紧急销毁', icon: 'danger' }
  ];

  VS.pages['security'] = function (ctx) {
    ctx = ctx || { params: {}, go: null };
    /* 路由参数优先（例如 /alarm 的「立即查看审计」→ security(tab=audit)），其次读会话内 tab */
    var wantTab = ctx.params && ctx.params.tab;
    if (wantTab && SEC_TABS.some(function (t) { return t.value === wantTab; })) secTab = wantTab;
    else if (!secTab) secTab = S.get('secTab') || 'checks';
    S.set({ secTab: secTab }, true);

    var bodyHost = host('col gap-4');
    function render() {
      bodyHost.innerHTML = '';
      statusBanners().forEach(function (b) { bodyHost.appendChild(b); });
      var node = secTab === 'erase' ? secEraseTab(ctx)
        : secTab === 'audit' ? secAuditTab(ctx)
        : secTab === 'destroy' ? secDestroyTab(ctx)
        : secChecksTab(ctx);
      bodyHost.appendChild(node);
    }

    var tabs = UI.tabs({
      items: SEC_TABS, value: secTab,
      onChange: function (v) { secTab = v; S.set({ secTab: v }, true); render(); }
    });

    render();
    currentRender = render;

    var head = [
      UI.btn({
        label: '运行全部检测', icon: 'scan', variant: 'primary',
        title: '运行全部检测（只读诊断；无数据的项保持灰态，不假装通过）',
        onClick: function () { UI.toast({ tone: 'info', title: '检测已触发', msg: '六项检测均为只读接口，结果按接口原值直出（演示）' }); }
      }),
      UI.btn({
        label: '演示开关', icon: 'settings',
        title: '打开原型演示开关：只读降级 / 维护态 / 审计失败 / 时钟异常',
        onClick: function () { openDemoPanel(); }
      })
    ];

    return pageShell(
      '安全中心',
      '检测项 / 擦除强度 / 审计日志 / 紧急销毁 · 只读降级与维护态下，审计查看 · 校验 · 导出与隐写提取仍可用',
      head,
      [
        tabs,
        bodyHost,
        cap('文案纪律：接口无数据一律灰态（不显示 0、不显示绿）；「安全擦除」字样仅在 erase_class = platform_secure 且 method_bits 含平台安全删除位时出现；引擎原值直出，不做美化。')
      ]
    );
  };

  /* ---------------- 演示开关面板 ---------------- */
  function openDemoPanel() {
    var body = host('col gap-3');
    function paint() {
      body.innerHTML = '';
      body.appendChild(UI.alertbar({
        tone: 'info', icon: 'sparkle', title: '原型演示开关',
        text: '只切换展示分支，用于验证降级与失败路径；不写入任何真实状态，也不代表引擎行为。'
      }));
      body.appendChild(demoToggle('只读降级（码 ' + F.num(9) + '）', '另一实例持有写租约：写操作禁用，审计与隐写提取仍可用',
        !!S.get('readOnly'),
        function (v) { S.set({ readOnly: v, readonlyReason: v ? '另一实例持有 vault.lock（演示）' : '' }); paint(); rerender(); }));
      body.appendChild(demoToggle('维护态（码 ' + F.num(10) + '）', '业务写入冻结；擦除 / 销毁 是唯一被允许的写操作',
        inMaintenance(),
        function (v) { S.set({ maintenance: v ? 'rotation' : null, session: v ? 'Maintenance' : 'Active' }); paint(); rerender(); }));
      body.appendChild(demoToggle('审计链校验失败', '检测项审计链转为 danger，并给出 brokenAt 段边界与取证引导（不自动修复）',
        demo.auditBroken,
        function (v) { demo.auditBroken = v; paint(); rerender(); }));
      body.appendChild(demoToggle('审计连续失败', '审计日志 tab 顶部显示灰态「审计不可用」，不显示条数',
        demo.auditDown,
        function (v) { demo.auditDown = v; paint(); rerender(); }));
      body.appendChild(demoToggle('物理删除失败', '待清理条目显示「介质擦除待完成」，不得显示「已擦除」',
        demo.physicalDeleteFailed,
        function (v) { demo.physicalDeleteFailed = v; paint(); rerender(); }));
      body.appendChild(demoToggle('去掉平台安全删除位', 'erase_class 仍为 platform_secure，但 method_bits 丢失安全删除位 → 禁止出现「安全擦除」',
        demo.maskPlatformBit,
        function (v) { demo.maskPlatformBit = v; paint(); rerender(); }));
      body.appendChild(demoToggle('系统时钟异常（超前 > ' + F.num(24) + 'h）', '紧急销毁需显式确认时钟异常后才允许提交',
        demo.clockSkew,
        function (v) { demo.clockSkew = v; paint(); rerender(); }));
    }
    paint();
    var m = UI.modal({
      title: '演示开关', sub: '仅本原型 · 不影响任何真实状态', size: 'md', body: body,
      footer: function () { return [UI.btn({ label: '关闭', title: '关闭', onClick: function () { m.close(); } })]; }
    });
  }

  /* ---------------- tab 1 · 检测项 ---------------- */
  function secChecksTab(ctx) {
    var wrap = host('col gap-4');

    if (demo.auditDown) {
      wrap.appendChild(UI.alertbar({
        tone: 'stale', icon: 'ban',
        title: '审计不可用',
        text: '审计链连续写入失败：进入灰态。既不再声称「审计通过」，也不显示 ' + F.num(0) + ' 条；业务写入不被阻断（审计失败不阻断业务），但取证能力需人工介入。',
        actions: [UI.btn({
          label: '检查磁盘', size: 'sm', title: '检查磁盘可写性与剩余空间',
          onClick: function () { UI.toast({ tone: 'warn', title: '磁盘检查', msg: 'vault_core_diagnostics().disk 写入失败（演示）' }); }
        })]
      }));
    }

    var grid = host('gauge-grid');
    D.securityChecks.forEach(function (c) {
      var state = normState(c.state), note = c.note;
      if (c.key === 'audit' && demo.auditBroken) {
        state = 'danger';
        note = '链头校验失败 · 断点段边界 brokenAt = 段 ' + F.num(12) + ' · 断点后的条数不可信';
      } else if (c.key === 'audit' && demo.auditDown) {
        state = 'unknown';
        note = '审计不可用：连续写入失败（灰态，不显示条数）';
      }

      var actions = [];
      if (c.key === 'audit') {
        actions.push(UI.btn({
          label: '校验审计链', size: 'sm', title: '校验审计链（只读；失败时不自动修复，先取证）',
          onClick: function () { openAuditVerify(ctx); }
        }));
      }
      if (c.key === 'integrity') {
        actions.push(UI.btn({
          label: '全库完整性扫描', size: 'sm', variant: 'primary',
          title: '全库完整性扫描（可取消；取消返回 ' + F.num(12) + ' 视为正常结果）',
          onClick: function () { openFullScan(ctx); }
        }));
      }

      grid.appendChild(h('div', { class: 'col gap-2' }, [
        UI.gauge({
          icon: c.icon, name: c.name, state: state, note: note, src: c.src,
          badge: h('div', { class: 'row gap-1' }, [
            stateBadge(state),
            /* 能力三档：design 表示契约清单里没有该接口 */
            UI.tierMark(c.state === 'design' ? 'design' : 'real')
          ])
        }),
        actions.length ? h('div', { class: 'row gap-2 wrap' }, actions) : null
      ]));
    });
    wrap.appendChild(grid);

    /* 审计失败引导：danger + brokenAt 段边界 + 取证引导，明确不自动修复 */
    if (demo.auditBroken) {
      wrap.appendChild(UI.alertbar({
        tone: 'danger', icon: 'shieldoff',
        title: '审计链校验失败 · 段边界 brokenAt = 段 ' + F.num(12),
        text: '断点之后的链头与条数均不可信。按纪律不自动修复、不重建链、不静默截断：先取证再处置。',
        actions: [
          UI.btn({ label: '导出审计取证', variant: 'danger', size: 'sm', title: '导出审计取证包（只读操作）', onClick: function () { exportAuditBundle(ctx); } }),
          UI.btn({ label: '检查磁盘', size: 'sm', title: '检查磁盘可写性与剩余空间', onClick: function () { UI.toast({ tone: 'warn', title: '磁盘检查', msg: '段 ' + F.num(12) + ' 尾部不完整（torn append）· 写入失败（演示）' }); } })
        ]
      }));
    } else {
      wrap.appendChild(cap('审计链状态来自 vault_core_audit_verify() 的返回值（链头 + 条数），本页不缓存、不推断。'));
    }

    /* 校验不通过时的语义说明卡 */
    wrap.appendChild(UI.card({
      title: '校验失败的语义', sub: '为什么「不自动修复」',
      body: h('div', { class: 'col gap-2' }, [
        cap('链头校验只证明链自洽，不证明记录未被整体删除；条数由接口原值给出。'),
        cap('一旦发现断点，自动重建链会覆盖取证现场，因此只提供「导出审计取证 / 检查磁盘」两条人工路径。'),
        cap('只读降级下审计校验与导出仍可用（VS.derive.readOnlyAllowed）。')
      ])
    }));

    return wrap;
  }

  /* ---------------- 校验审计链弹窗 ---------------- */
  function openAuditVerify(ctx) {
    var ok = !demo.auditBroken && !demo.auditDown;
    var head = demo.auditBroken ? BROKEN_HEAD : D.auditHead;
    var body = host('col gap-3');
    body.appendChild(UI.alertbar({
      tone: ok ? 'info' : 'danger', icon: ok ? 'check' : 'shieldoff',
      title: ok ? '链头校验通过' : '链头校验失败',
      text: ok
        ? '仅证明链自洽；条数与链头均由接口原值直出，本页不做推断。'
        : '断点段边界 brokenAt = 段 ' + F.num(12) + '；不自动修复，请先导出取证。'
    }));
    body.appendChild(UI.kv([
      { k: '链头 head', v: U.shortHash(head, 12, 8), mono: true, copy: true },
      { k: '条数 count', v: F.num(D.auditLog.length), mono: true },
      { k: 'verified', v: String(ok), mono: true },
      { k: '断点段边界 brokenAt', v: demo.auditBroken ? ('段 ' + F.num(12)) : '—', mono: true },
      { k: '数据来源接口', v: 'vault_core_audit_verify()', mono: true }
    ]));
    body.appendChild(cap('校验为只读操作：只读降级下仍可用，不置灰。'));
    if (!ok) {
      body.appendChild(h('div', { class: 'row gap-2 wrap' }, [
        UI.btn({ label: '导出审计取证', variant: 'danger', title: '导出审计取证包（只读）', onClick: function () { exportAuditBundle(ctx); } }),
        UI.btn({ label: '检查磁盘', title: '检查磁盘可写性与剩余空间', onClick: function () { UI.toast({ tone: 'warn', title: '磁盘检查', msg: '写入失败（演示）：段 ' + F.num(12) + ' 尾部不完整' }); } })
      ]));
    }
    var m = UI.modal({
      title: '校验审计链', sub: 'vault_core_audit_verify()', size: 'md', tone: ok ? '' : 'danger',
      body: body,
      footer: function () { return [UI.btn({ label: '关闭', title: '关闭', onClick: function () { m.close(); } })]; }
    });
  }

  /* ---------------- 全库完整性扫描（任务进度弹窗）---------------- */
  function openFullScan(ctx) {
    var total = D.vault.entries.length || 1;
    var done = 0, failed = 0, timer = null, finished = false;
    var bar = barHost(0, 'accent');
    var stat = h('div', { class: 't-caption' });
    var cur = h('div', { class: 't-mono', text: '等待调度…' });
    var tail = host('col gap-2');

    function refreshStat() {
      stat.textContent = '已检查 ' + F.num(done) + ' / ' + F.num(total) + ' 项 · 失败 ' + F.num(failed) + ' 项';
    }
    refreshStat();

    function stop() { if (timer) { clearInterval(timer); timer = null; } }

    var m = UI.modal({
      title: '全库完整性扫描', sub: 'vault_core_verify_all → TASK_*', size: 'md',
      body: h('div', { class: 'col gap-3' }, [
        bar, stat, cur, tail,
        cap('扫描为只读操作：校验块 CRC 与 chunk 计数，不修改任何文件；失败项不自动修复、不自动重试。')
      ]),
      footer: function () {
        return [
          UI.btn({
            label: '转后台', title: '关闭弹窗，任务继续（进度见队列页）',
            onClick: function () { m.close(); UI.toast({ tone: 'info', title: '已转后台', msg: '任务进度可在队列页查看（演示）' }); }
          }),
          UI.btn({
            label: '取消', variant: 'danger',
            title: '取消扫描：返回 ' + F.num(12) + '（已取消），视为正常结果',
            onClick: function () { stop(); m.close(); showCancelled('全库完整性扫描'); }
          })
        ];
      }
    });

    timer = setInterval(function () {
      if (finished) return;
      var item = D.vault.entries[done % total] || { name: '—' };
      done += 1;
      if (done % 5 === 0 && failed < 2) failed += 1;
      cur.textContent = '当前：' + item.name;
      bar.setBar(Math.min(1, done / total), 'accent');
      refreshStat();
      if (done >= total) {
        finished = true; stop();
        bar.setBar(1, failed ? 'danger' : 'success');
        tail.appendChild(UI.alertbar({
          tone: failed ? 'warn' : 'info', icon: failed ? 'alert' : 'check',
          title: failed ? ('扫描完成 · 失败 ' + F.num(failed) + ' 项') : '扫描完成 · 未发现失败项',
          text: failed
            ? '失败项需人工处置：可导出取证或检查磁盘；不自动修复、不自动重试。'
            : '全部块的 CRC 与 chunk 计数和索引一致。',
          actions: failed ? [UI.btn({ label: '导出取证', size: 'sm', title: '导出取证包', onClick: function () { exportAuditBundle(ctx); } })] : null
        }));
      }
    }, 240);
  }

  function showCancelled(what) {
    UI.toast({
      tone: 'info', title: '已取消',
      msg: what + '：' + errText(12) + '（视为正常结果，不报错、不计失败）', duration: 4200
    });
  }

  /* ---------------- 导出加密审计包 ---------------- */
  function exportAuditBundle(ctx) {
    var feat = D.licenseState && D.licenseState.features ? D.licenseState.features.feat_audit_export : false;
    if (!feat) {
      var body = host('col gap-3');
      body.appendChild(UI.alertbar({
        tone: 'maintenance', icon: 'award', title: '能力未授权：审计链加密导出',
        text: '当前档位（' + D.licenseState.tier + '）未开通 feat_audit_export。按证据先行原则：能力位未置位时置灰并说明原因，不假装导出成功。'
      }));
      body.appendChild(cap('本地审计查看 / 校验 不受该能力位限制，仍可用；授权档位说明见授权与订阅页。'));
      var m = UI.modal({
        title: '导出加密审计包', size: 'sm', tone: 'maintenance', body: body,
        footer: function () {
          return [
            UI.btn({ label: '关闭', title: '关闭', onClick: function () { m.close(); } }),
            UI.btn({ label: '前往授权与订阅', variant: 'primary', title: '前往授权与订阅', onClick: function () { m.close(); go(ctx, 'license'); } })
          ];
        }
      });
      return;
    }
    var line = 'vs-audit-bundle v1 · 条数 ' + F.num(D.auditLog.length) +
      ' · 链头 ' + U.shortHash(D.auditHead, 12, 8) + ' · 加密 HKDF(MK,"audit-export") + AEAD';
    var body2 = host('col gap-3');
    body2.appendChild(mono(line));
    body2.appendChild(UI.copyField(line, '复制导出载荷说明'));
    body2.appendChild(cap('导出为只读操作：只读降级下仍可用；导出内容不含目标路径、不含文件名原文、不含查询词。'));
    var m2 = UI.modal({
      title: '导出加密审计包', sub: 'feat_audit_export = true', size: 'md', body: body2,
      footer: function () { return [UI.btn({ label: '完成', variant: 'primary', title: '完成', onClick: function () { m2.close(); UI.toast({ tone: 'success', title: '已生成', msg: '审计包已写出到外部路径（演示）' }); } })]; }
    });
  }

  /* ---------------- tab 2 · 擦除强度 ---------------- */
  var ERASE_SAMPLES = [
    { cls: 1, bits: '0b0001', media: 1, note: '加密擦除（crypto_only）：密钥已销毁；介质覆写未执行。' },
    { cls: 2, bits: '0b0100', media: 2, note: '零覆写（zero_overwrite）：已执行单轮零覆写。' },
    { cls: 3, bits: '0b0011', media: 3, note: '平台安全删除指令已执行（method_bits 含平台安全删除位）。' },
    { cls: 4, bits: '0b0000', media: 4, note: '无法判定（unknown）：介质类型不可判定；介质覆写未执行。' }
  ];

  /** method_bits 的第 1 位（bit1）= 平台安全删除位 */
  function bitsHasPlatformSecure(bits) {
    var v = parseInt(String(bits).replace(/^0b/i, ''), 2);
    return !isNaN(v) && (v & 2) !== 0;
  }

  /* data.js 未提供 degradations[] 字段（见汇报）：以下按 05-02 §三.4 口径在本文件内定义 */
  var DEGRADATIONS = [
    { code: 'ERASE_MEDIA_UNKNOWN', tone: 'warning', text: '介质类型无法判定（media_kind = unknown）→ 介质覆写未执行' },
    { code: 'ERASE_METHOD_BITS_EMPTY', tone: 'warning', text: 'method_bits = 0b0000：未置位任何删除指令 → 仅完成密钥销毁' },
    { code: 'ERASE_PLATFORM_UNSUPPORTED', tone: 'stale', text: '平台不支持安全删除（文件系统无对应接口）→ 降级为零覆写或仅加密擦除' },
    { code: 'ERASE_PHYSICAL_PENDING', tone: 'danger', text: '物理删除失败 / 待完成 → 显示「介质擦除待完成」，不得显示「已擦除」' }
  ];

  function eraseSampleBits(s) {
    /* 演示：抹掉平台安全删除位，验证「安全擦除」字样必须消失 */
    return (demo.maskPlatformBit && s.cls === 3) ? '0b0001' : s.bits;
  }

  function eraseSampleCell(s) {
    var bits = eraseSampleBits(s);
    var secure = s.cls === 3 && bitsHasPlatformSecure(bits);
    var badge;
    if (secure) {
      badge = UI.eraseBadge(3);
    } else if (s.cls === 3) {
      badge = UI.badge({
        text: '加密擦除（平台安全删除位未置位）', tone: 'warning',
        title: 'erase_class = platform_secure 但 method_bits 未含平台安全删除位：按纪律不得显示「安全擦除」'
      });
    } else {
      badge = UI.eraseBadge(s.cls);
    }
    var note = s.note;
    if (s.cls === 1 || s.cls === 4 || (s.cls === 3 && !secure)) {
      note = note + '（介质覆写未执行）';
    }
    return h('div', { class: 'card' }, [
      h('div', { class: 'card-body col gap-2' }, [
        h('div', { class: 'row-between' }, [badge, h('span', { class: 't-mono', text: UI.ERASE_CLASS[s.cls] ? UI.ERASE_CLASS[s.cls].key : 'unknown' })]),
        mono('erase_class=' + F.num(s.cls) + ' · method_bits=' + bits + ' · media_kind=' + F.num(s.media)),
        cap(note),
        s.cls === 3 && !secure
          ? cap('规则命中：平台安全删除位缺失 → 引擎原值仍是 platform_secure，但界面禁止出现「安全擦除」。')
          : null
      ])
    ]);
  }

  function secEraseTab(ctx) {
    var wrap = host('col gap-4');

    wrap.appendChild(UI.alertbar({
      tone: 'warn', icon: 'erase', title: '硬约束：擦除强度按引擎原值直出',
      text: '「安全擦除」字样仅在 erase_class = platform_secure 且 method_bits 含平台安全删除位时出现；' +
        'crypto_only / unknown 文案必须包含「介质覆写未执行」；物理删除失败显示「介质擦除待完成」，不得显示「已擦除」。'
    }));

    wrap.appendChild(UI.card({
      title: '四种取值各展示一次', sub: '引擎原值直出 · 不做美化 · 不把 unknown 说成「安全擦除」',
      actions: [UI.btn({
        label: demo.maskPlatformBit ? '恢复平台安全删除位' : '演示：去掉平台安全删除位',
        size: 'sm', title: '切换 method_bits 中平台安全删除位的演示开关',
        onClick: function () { demo.maskPlatformBit = !demo.maskPlatformBit; rerender(); }
      })],
      body: h('div', { class: 'grid-2' }, ERASE_SAMPLES.map(eraseSampleCell))
    }));

    /* 降级条目示例（data.js 未提供 degradations[]，见汇报） */
    wrap.appendChild(UI.card({
      title: '降级条目（degradations[]）示例', sub: '引擎返回降级原因时逐条列出，而不是折叠成一句「已擦除」',
      body: h('ul', { class: 'col gap-2' }, DEGRADATIONS.map(function (d) {
        return h('li', { class: 'row gap-2' }, [
          UI.badge({ text: d.tone === 'danger' ? '待完成' : '降级', tone: d.tone, title: d.code }),
          mono(d.code),
          h('span', { class: 'grow', text: d.text })
        ]);
      }))
    }));

    /* 待清理条目表 */
    var tableWrap = host('div');
    function paintTable() { tableWrap.innerHTML = ''; tableWrap.appendChild(pendingEraseTable()); }
    paintTable();

    wrap.appendChild(UI.card({
      title: '待清理条目', sub: '删除保护区（24 h）与介质擦除状态分列展示',
      actions: [UI.btn({
        label: '演示：物理删除失败', size: 'sm', title: '切换物理删除失败演示（置为「介质擦除待完成」）',
        onClick: function () { demo.physicalDeleteFailed = !demo.physicalDeleteFailed; paintTable(); }
      })],
      body: h('div', { class: 'col gap-3' }, [
        tableWrap,
        cap('「介质擦除待完成」= 密钥已销毁但介质覆写未完成：界面永不显示「已擦除」；清理动作属写操作，受写闸门约束（维护态下擦除仍允许）。')
      ])
    }));

    return wrap;
  }

  function pendingEraseTable() {
    var rows = D.deletedEntries.map(function (d, i) {
      var pending = !!d.pendingCleanup || (demo.physicalDeleteFailed && i === 0);
      return {
        id: d.id, name: d.name, size: d.size, deletedMs: d.deletedMs,
        eraseClass: d.eraseClass, mediaKind: d.mediaKind, pending: pending,
        protectUntil: d.protectUntil
      };
    });
    var columns = [
      { key: 'name', label: '条目' },
      { key: 'size', label: '体积', align: 'right', render: function (r) { return F.bytes(r.size); } },
      { key: 'deletedMs', label: '删除时间', render: function (r) { return F.dateLong(r.deletedMs); } },
      { key: 'eraseClass', label: 'erase_class', class: 't-mono', render: function (r) { return F.num(r.eraseClass); } },
      { key: 'mediaKind', label: 'media_kind', class: 't-mono', render: function (r) { return F.num(r.mediaKind); } },
      {
        key: 'state', label: '介质擦除状态', render: function (r) {
          if (r.pending) {
            return UI.badge({ text: '介质擦除待完成', tone: 'danger', title: '物理删除未完成（待清理）：不得显示「已擦除」' });
          }
          return UI.badge({ text: '已加密擦除', tone: 'warning', title: '已加密擦除；介质覆写未执行' });
        }
      },
      {
        key: 'protect', label: '误删保护', render: function (r) {
          return r.protectUntil
            ? UI.badge({ text: '保护至 ' + F.dateShort(r.protectUntil), tone: 'stale' })
            : cap('已过期');
        }
      }
    ];
    return UI.table({
      columns: columns, rows: rows,
      empty: UI.empty({ icon: 'trash', title: '没有待清理条目', desc: '删除后进入保护区的条目会出现在这里' })
    });
  }

  /* ---------------- tab 3 · 审计日志 ---------------- */
  function auditRows() {
    return D.auditLog.filter(function (r) {
      if (auditFilter.cat !== 'all' && r.cat !== auditFilter.cat) return false;
      var res = r.detail ? r.detail.result : 0;
      if (auditFilter.result === 'fail') { if (res === 0) return false; }
      else if (auditFilter.result !== 'all' && String(res) !== String(auditFilter.result)) return false;
      if (auditFilter.from) {
        var from = new Date(auditFilter.from + 'T00:00:00').getTime();
        if (!isNaN(from) && r.tsMs < from) return false;
      }
      if (auditFilter.to) {
        var to = new Date(auditFilter.to + 'T23:59:59').getTime();
        if (!isNaN(to) && r.tsMs > to) return false;
      }
      if (auditFilter.q) {
        var q = auditFilter.q.toLowerCase();
        var hay = (r.op + ' ' + r.opLabel + ' ' + r.cat + ' ' + r.seq).toLowerCase();
        if (hay.indexOf(q) < 0) return false;
      }
      return true;
    });
  }

  function secAuditTab(ctx) {
    var wrap = host('col gap-4');

    if (demo.auditDown) {
      wrap.appendChild(UI.alertbar({
        tone: 'stale', icon: 'ban', title: '审计不可用',
        text: '审计链连续写入失败（灰态）：不显示条数、不显示为通过；审计失败不阻断业务，取证需人工介入。'
      }));
    }

    var tableHost = host('div');
    function paintTable() {
      tableHost.innerHTML = '';
      var rows = auditRows();
      tableHost.appendChild(UI.table({
        rows: rows.map(function (r) { return r; }),
        selectedId: undefined,
        maxHeight: '54vh',
        empty: UI.empty({ icon: 'list', title: '无可显示的审计记录', desc: '当前筛选条件下没有记录；不显示 0 条，也不视为通过。' }),
        columns: [
          { key: 'seq', label: 'seq', class: 't-mono', width: '86px', render: function (r) { return h('span', { class: 't-mono', text: F.num(r.seq) }); } },
          { key: 'tsMs', label: '时间', width: '200px', render: function (r) { return h('span', { class: 't-mono', text: F.dateLong(r.tsMs) }); } },
          { key: 'opLabel', label: '操作', render: function (r) { return h('span', { text: r.opLabel + '（' + r.op + '）' }); } },
          { key: 'cat', label: '类别', width: '120px', render: function (r) { return UI.badge({ text: r.cat, title: '操作类别：' + r.cat }); } },
          { key: 'result', label: '结果码', width: '150px', render: function (r) { return resultBadge(r.detail ? r.detail.result : 0); } },
          {
            key: 'headHash', label: '链头前缀', width: '150px', render: function (r) {
              return h('span', { class: 't-mono', title: 'headHash = ' + r.headHash, text: U.shortHash(r.headHash, 8, 4) });
            }
          },
          {
            key: 'detail', label: '', width: '92px', render: function (r) {
              return UI.btn({
                label: '详情', size: 'sm', title: '查看脱敏 detail（seq ' + F.num(r.seq) + '）',
                onClick: function () { openAuditDetail(r); }
              });
            }
          }
        ]
      }));
    }

    /* 工具条：分类 / 结果码 / 日期范围 / 关键词 / 校验 / 导出 */
    var catSel = a11ySelect(UI.select({
      value: auditFilter.cat, block: false,
      options: [
        { value: 'all', label: '全部类别' },
        { value: 'vault', label: 'vault · 保险箱' },
        { value: 'security', label: 'security · 安全' },
        { value: 'p2p', label: 'p2p · 设备' },
        { value: 'maintain', label: 'maintain · 维护' },
        { value: 'license', label: 'license · 授权' },
        { value: 'system', label: 'system · 系统' }
      ],
      onChange: function (v) { auditFilter.cat = v; paintTable(); }
    }), '按类别筛选审计记录');
    var resSel = a11ySelect(UI.select({
      value: auditFilter.result,
      options: [
        { value: 'all', label: '全部结果码' },
        { value: '0', label: '0 · 成功' },
        { value: '3', label: '3 · 磁盘不可写' },
        { value: '9', label: '9 · 租约被占 · 只读' },
        { value: '12', label: '12 · 已取消' },
        { value: 'fail', label: '仅失败' }
      ],
      onChange: function (v) { auditFilter.result = v; paintTable(); }
    }), '按结果码筛选审计记录');
    var fromInput = UI.input({
      type: 'date', value: auditFilter.from, placeholder: '起始日期',
      onInput: function () { auditFilter.from = fromInput.value; paintTable(); }
    });
    a11y(fromInput, '起始日期（含当天）');
    var toInput = UI.input({
      type: 'date', value: auditFilter.to, placeholder: '结束日期',
      onInput: function () { auditFilter.to = toInput.value; paintTable(); }
    });
    a11y(toInput, '结束日期（含当天）');
    var search = UI.searchBox({
      value: auditFilter.q, placeholder: '按操作 / 类别 / seq 过滤（在 UI 侧匹配，不写入审计）',
      onInput: function (e) { auditFilter.q = e.target.value; paintTable(); }
    });
    a11y(search.querySelector('input'), '关键词过滤（不写入审计，审计不含查询词）');

    var bar = h('div', { class: 'page-toolbar' }, [
      catSel, resSel,
      h('div', { class: 'row gap-1' }, [fromInput, h('span', { class: 't-caption', text: '→' }), toInput]),
      h('div', { class: 'grow' }, [search]),
      UI.btn({ label: '校验审计链', title: '校验审计链（只读；失败不自动修复）', onClick: function () { openAuditVerify(ctx); } }),
      UI.btn({ label: '导出加密审计包', icon: 'external', title: '导出加密审计包（导入导出/导出能力位受档位约束）', onClick: function () { exportAuditBundle(ctx); } })
    ]);
    wrap.appendChild(bar);

    paintTable();
    wrap.appendChild(tableHost);

    wrap.appendChild(UI.alertbar({
      tone: 'info', icon: 'info', title: '脱敏口径与只读可用性',
      text: '审计不含目标路径、不含文件名原文、不含查询词；只读降级与维护态下，审计查看 / 校验 / 导出仍可用（不置灰）。'
    }));

    return wrap;
  }

  function resultBadge(code) {
    var e = VS.ERR[code];
    if (!e) return UI.badge({ text: '—', tone: 'stale', title: '未记录结果码' });
    var tone = e.tone === 'maintenance' ? 'maintenance' : e.tone;
    if (tone === 'success') tone = 'success';
    return UI.badge({ text: F.num(code) + ' · ' + e.title, tone: tone, title: e.hint || e.title });
  }

  function openAuditDetail(row) {
    var d = row.detail || {};
    function v(key) {
      var x = d[key];
      if (x === null || x === undefined || x === '') return '—（未记录）';
      if (key === 'bytes') return F.bytes(x) + '（' + F.num(x) + ' B）';
      if (key === 'chunks') return F.num(x) + ' 块';
      if (key === 'result') return F.num(x) + ' · ' + (VS.ERR[x] ? VS.ERR[x].title : '未知');
      return String(x);
    }
    var body = host('col gap-3');
    body.appendChild(UI.kv([
      { k: 'seq', v: F.num(row.seq), mono: true },
      { k: '时间', v: F.dateLong(row.tsMs), mono: true },
      { k: '操作', v: row.opLabel + '（' + row.op + '）' },
      { k: '类别', v: row.cat, mono: true },
      { k: '等级', v: row.level, mono: true },
      { k: '结果码 result', v: v('result'), mono: true },
      { k: 'prevHash', v: U.shortHash(row.prevHash, 16, 8), mono: true, copy: true },
      { k: 'headHash', v: U.shortHash(row.headHash, 16, 8), mono: true, copy: true }
    ]));
    body.appendChild(strong('脱敏 detail'));
    body.appendChild(UI.kv([
      { k: 'file_id', v: v('file_id'), mono: true },
      { k: 'bytes', v: v('bytes'), mono: true },
      { k: 'chunks', v: v('chunks'), mono: true },
      { k: 'cipher_suite', v: v('cipher_suite'), mono: true },
      { k: 'erase_class', v: v('erase_class'), mono: true },
      { k: 'media_kind', v: v('media_kind'), mono: true },
      { k: 'method_bits', v: v('method_bits'), mono: true }
    ]));
    body.appendChild(UI.alertbar({
      tone: 'info', icon: 'shield',
      title: '审计不含目标路径、不含文件名原文、不含查询词',
      text: 'detail 仅保留取证所需的标识与计数；路径与名称原文不出引擎，也不进入审计导出。'
    }));
    var m = UI.modal({
      title: '审计详情 · seq ' + F.num(row.seq), sub: row.op, size: 'md', body: body,
      footer: function () {
        return [
          UI.btn({ label: '复制链头', title: '复制完整链头哈希', onClick: function () { UI.copy(row.headHash); } }),
          UI.btn({ label: '关闭', variant: 'primary', title: '关闭', onClick: function () { m.close(); } })
        ];
      }
    });
  }

  /* ---------------- tab 4 · 紧急销毁 / 危险操作 ---------------- */
  function destroyDeadline() {
    var base = Date.now();
    if (destroyState.delay === 'now') return base;
    if (destroyState.delay === '5m') return base + 5 * 60000;
    if (destroyState.delay === '1h') return base + 3600000;
    if (destroyState.delay === 'custom' && destroyState.customAt) {
      var t = new Date(destroyState.customAt).getTime();
      return isNaN(t) ? null : t;
    }
    return null;
  }

  function secDestroyTab(ctx) {
    var wrap = host('col gap-4');

    wrap.appendChild(UI.alertbar({
      tone: 'danger', icon: 'danger', title: '危险操作区',
      text: '紧急销毁不可撤销、不可取消，进度单向；远程销毁指令在维护态下同样允许（用户要求销毁高于数据结构一致）。'
    }));

    /* 范围 */
    var scopeCards = a11yRadioCards(UI.radioCards({
      value: destroyState.scope,
      options: D.destroyScopes.map(function (s) { return { value: s.value, title: s.label, desc: s.desc }; }),
      onChange: function (v) { destroyState.scope = v; paint(); }
    }), D.destroyScopes.map(function (s) { return { title: s.label, desc: s.desc }; }));

    /* 延迟销毁：绝对时刻，不是倒计时文本 */
    var delayOptions = [
      { value: 'now', title: '立即执行', desc: 'deadlineMs = 现在' },
      { value: '5m', title: F.num(5) + ' 分钟后', desc: 'deadlineMs = 现在 + ' + F.num(5) + ' 分钟' },
      { value: '1h', title: F.num(1) + ' 小时后', desc: 'deadlineMs = 现在 + ' + F.num(1) + ' 小时' },
      { value: 'custom', title: '自定义绝对时刻', desc: '按绝对 deadlineMs 落盘，重启不丢' }
    ];
    var delayCards = a11yRadioCards(UI.radioCards({
      value: destroyState.delay, options: delayOptions,
      onChange: function (v) { destroyState.delay = v; paint(); }
    }), delayOptions);

    var customInput = UI.input({
      type: 'datetime-local', value: destroyState.customAt,
      onInput: function () { destroyState.customAt = customInput.value; paint(); }
    });
    a11y(customInput, '自定义销毁绝对时刻（datetime-local）');

    /* 时效：7 天默认 / 0 = 不过期（需二次确认） */
    var expiryOptions = [
      { value: 7, title: F.num(7) + ' 天（默认）', desc: 'expires_ms = 现在 + ' + F.num(7) + ' 天' },
      { value: 0, title: F.num(0) + ' = 不过期', desc: '指令永久有效：需二次确认' }
    ];
    var expiryCards = a11yRadioCards(UI.radioCards({
      value: destroyState.expiresDays,
      options: expiryOptions,
      onChange: function (v) {
        if (String(v) === '0') {
          var m = UI.confirm({
            title: '该指令将永久有效', tone: 'danger', confirmLabel: '我确认（永久有效）',
            body: h('div', { class: 'col gap-2' }, [
              h('div', { class: 't-body', text: '该指令将永久有效（expires_ms = ' + F.num(0) + '）。' }),
              cap('签名指令不会随时间失效：任何持有匹配设备指纹的对端在上线后都会执行，直到你手动撤销。')
            ]),
            onConfirm: function () { destroyState.expiresDays = 0; paint(); },
            onCancel: function () { destroyState.expiresDays = 7; paint(); }
          });
          return;
        }
        destroyState.expiresDays = 7; paint();
      }
    }), expiryOptions);

    /* 时钟异常确认 */
    var ackBox = null;
    var body = host('col gap-4');
    function paint() {
      body.innerHTML = '';
      var deadline = destroyDeadline();

      body.appendChild(UI.field({ label: '销毁范围', control: scopeCards, hint: '范围越大影响越广；account 含撤销中继会话，不可恢复。' }));
      body.appendChild(UI.field({ label: '延迟销毁（绝对时刻）', control: delayCards, hint: '界面显示绝对 deadlineMs，不用倒计时文本；重启后按落盘的时刻续走。' }));
      if (destroyState.delay === 'custom') {
        body.appendChild(UI.field({
          label: '自定义绝对时刻', control: customInput,
          hint: deadline === null ? '尚未选择有效时刻：无法提交' : ('绝对期限 deadlineMs = ' + F.dateLong(deadline))
        }));
      }
      body.appendChild(UI.kv([
        { k: '绝对期限 deadlineMs', v: deadline === null ? '—（未选择）' : F.dateLong(deadline), mono: true },
        { k: '范围 scope', v: destroyState.scope, mono: true },
        { k: '时效 expires_ms', v: destroyState.expiresDays === 0 ? F.num(0) + '（不过期）' : (F.num(destroyState.expiresDays) + ' 天'), mono: true },
        { k: '指令类型', v: 'kind = ' + F.num(1) + '（销毁指令 · 跨设备信封，非商业订单）', mono: true }
      ]));

      body.appendChild(UI.field({ label: '指令时效', control: expiryCards, hint: '选择 ' + F.num(0) + ' 时需二次确认「该指令将永久有效」。' }));

      if (demo.clockSkew) {
        ackBox = UI.checkbox({
          checked: destroyState.clockAck, label: '检测到系统时钟异常（超前 > ' + F.num(24) + 'h）',
          sub: '绝对 deadlineMs 可能被误解析；我确认已核对系统时间后仍要提交'
        });
        a11y(ackBox, '确认系统时钟异常后仍要提交销毁指令');
        body.appendChild(UI.alertbar({
          tone: 'danger', icon: 'clock', title: '检测到系统时钟异常（超前 > ' + F.num(24) + 'h）',
          text: '绝对时刻语义可能被当前时钟偏移破坏；需显式确认后才能提交。',
          actions: [ackBox]
        }));
      } else {
        ackBox = null;
      }

      var denied = denyErase();
      var blockedByClock = demo.clockSkew && !destroyState.clockAck;
      var blockedByDeadline = deadline === null;
      var main = UI.btn({
        label: '启动紧急销毁', icon: 'danger', variant: 'danger', size: 'lg',
        disabled: !!denied || blockedByClock || blockedByDeadline,
        title: denied ? ('启动紧急销毁 不可用 · ' + denied.text)
          : blockedByClock ? '启动紧急销毁 不可用 · 需先显式确认系统时钟异常'
            : blockedByDeadline ? '启动紧急销毁 不可用 · 自定义时刻无效' : '启动紧急销毁（需输入确认短语「销毁」）',
        onClick: function () { confirmDestroy(ctx); }
      });

      body.appendChild(h('div', { class: 'row gap-2 wrap' }, [
        main,
        UI.btn({
          label: '远程锁定接收弹窗演示', icon: 'lock',
          title: '演示远程锁定指令的接收弹窗（收到即锁，无二次确认）',
          onClick: function () { openRemoteLockDialog(); }
        }),
        UI.btn({
          label: '演示：时钟异常', title: '切换系统时钟异常演示',
          onClick: function () { demo.clockSkew = !demo.clockSkew; if (!demo.clockSkew) destroyState.clockAck = false; paint(); }
        })
      ]));

      body.appendChild(cap('提交前需输入确认短语「销毁」；执行期全屏、不可取消、进度单向，完成后回到锁屏。'));
    }
    paint();

    wrap.appendChild(UI.card({ title: '紧急销毁', sub: '范围 / 绝对延迟 / 时效 / 时钟校验', body: body }));

    wrap.appendChild(UI.card({
      title: '维护态与只读降级下的行为', sub: '写闸门例外说明',
      body: h('div', { class: 'col gap-2' }, [
        cap('维护态（码 ' + F.num(10) + '）：业务写入冻结，但「擦除 / 销毁」是唯一被允许的写操作，本区按钮保持可用。'),
        cap('只读降级（码 ' + F.num(9) + '）：擦除与销毁被拒绝（另一实例持有写租约），按钮置灰并在 title 中给出原因。'),
        cap('未解锁 / 会话失效（码 ' + F.num(5) + '）：一律拒绝。')
      ])
    }));

    return wrap;
  }

  function confirmDestroy(ctx) {
    var deadline = destroyDeadline();
    var scope = D.destroyScopes.filter(function (s) { return s.value === destroyState.scope; })[0] || D.destroyScopes[0];
    var m = UI.confirmPhrase({
      title: '启动紧急销毁', sub: '不可撤销', tone: 'danger', phrase: '销毁',
      confirmLabel: '启动紧急销毁',
      body: h('div', { class: 'col gap-3' }, [
        UI.alertbar({
          tone: 'danger', icon: 'danger', title: '即将不可逆地销毁数据',
          text: '范围：' + scope.label + ' · ' + scope.desc
        }),
        UI.kv([
          { k: '绝对期限 deadlineMs', v: deadline === null ? '—' : F.dateLong(deadline), mono: true },
          { k: '时效 expires_ms', v: destroyState.expiresDays === 0 ? F.num(0) + '（不过期）' : (F.num(destroyState.expiresDays) + ' 天'), mono: true },
          { k: '范围 scope', v: destroyState.scope, mono: true }
        ]),
        cap('执行期全屏阻塞、不可取消；进度单向，失败也如实呈现。')
      ]),
      hint: '确认短语不可复制粘贴以外的方式绕过：必须手输「销毁」。',
      onConfirm: function () { openDestroyProgress(ctx, scope, deadline); }
    });
    return m;
  }

  function openDestroyProgress(ctx, scope, deadline) {
    var content = host('col gap-3');
    var phases = [
      '生成签名销毁指令（连接中继前先落盘 deadlineMs）',
      '销毁文件密钥（FSKey）与从属密钥包装副本',
      '覆写并删除保险箱数据库与索引',
      '广播销毁指令给已配对设备 / 撤销中继会话',
      '写入审计记录（审计失败不阻断）'
    ];
    var bar = barHost(0, 'danger');
    var phaseLabel = h('div', { class: 't-body', text: phases[0] });
    var stat = h('div', { class: 't-mono', text: '—' });
    content.appendChild(h('span', { style: { color: 'var(--c-danger)' }, html: VS.icon('danger', 56) }));
    content.appendChild(h('div', { class: 't-display', text: '紧急销毁进行中' }));
    content.appendChild(cap('全屏阻塞 · 不可取消 · 进度单向'));
    content.appendChild(h('div', { style: { width: '100%' } }, [bar]));
    content.appendChild(phaseLabel);
    content.appendChild(stat);
    content.appendChild(h('div', { class: 't-caption t-mono', text: 'deadlineMs = ' + (deadline === null ? '—' : F.dateLong(deadline)) + ' · scope = ' + (scope ? scope.value : '—') }));

    var fs = UI.fullscreen({ tone: 'danger', content: content });

    var pct = 0, idx = 0;
    var timer = setInterval(function () {
      pct = Math.min(1, pct + 0.02);
      if (pct >= (idx + 1) / phases.length && idx < phases.length - 1) {
        idx += 1;
        phaseLabel.textContent = phases[idx];
      }
      bar.setBar(pct, 'danger');
      stat.textContent = '进度 ' + F.pct(pct) + ' · 已处理 ' + F.num(Math.round(pct * (D.vault.entries.length + D.deletedEntries.length))) + ' 项';
      if (pct >= 1) {
        clearInterval(timer);
        phaseLabel.textContent = '销毁完成';
        stat.textContent = '全部密钥已销毁 · 审计已写入（审计失败不阻断业务）';
        showDestroyDone(ctx, fs);
      }
    }, 160);
  }

  function showDestroyDone(ctx, fs) {
    fs.close();
    var content = host('col gap-3');
    content.appendChild(h('span', { style: { color: 'var(--c-accent)' }, html: VS.icon('check', 56) }));
    content.appendChild(h('div', { class: 't-display', text: '销毁已完成' }));
    content.appendChild(cap('本机保险箱数据与密钥已销毁；已配对设备将在下次上线执行签名指令。'));
    content.appendChild(h('div', { class: 't-caption t-mono', text: '接口：vault_core_destroy() · 审计操作 security.destroy' }));
    content.appendChild(h('div', { class: 'row gap-3', style: { marginTop: '8px' } }, [
      UI.btn({
        label: '回锁屏', variant: 'danger', size: 'lg', title: '回到锁屏',
        onClick: function () {
          donescreen.close();
          S.set({ lock: 'Locked', session: 'Active', route: 'lock' });
          go(ctx, 'lock');
        }
      }),
      UI.btn({
        label: '导出销毁取证', title: '导出销毁取证记录（只读）',
        onClick: function () { exportAuditBundle(ctx); }
      })
    ]));
    var donescreen = UI.fullscreen({ tone: 'danger', content: content });
  }

  function openRemoteLockDialog() {
    var peer = D.devices.filter(function (d) { return !d.self; })[0] || { name: 'WorkBook' };
    var body = host('col gap-3');
    body.appendChild(UI.alertbar({
      tone: 'stale', icon: 'lock', title: '已由 ' + peer.name + ' 远程锁定',
      text: '锁定指令收到即锁，不做二次确认（与销毁同级通道）。本机会话已失效，需重新解锁。'
    }));
    body.appendChild(UI.kv([
      { k: '来源设备', v: peer.name, mono: true },
      { k: '设备指纹', v: U.shortHash(peer.fingerprint, 12, 8), mono: true },
      { k: '指令类型', v: 'kind = ' + F.num(2) + '（锁定指令）', mono: true },
      { k: '域串', v: 'vsync-lock:{target}:{delay_ms}:{ts}', mono: true },
      { k: '接收校验', v: '签名 / target / expires_ms / 时钟偏差 ≤ ' + F.num(300) + ' s', mono: true }
    ]));
    var m = UI.modal({
      title: '远程锁定指令', sub: '收到即锁 · 无二次确认', size: 'sm', tone: 'danger', dismissible: false,
      body: body,
      footer: function () {
        return [UI.btn({
          label: '我知道了', variant: 'danger', title: '确认已锁定（无二次确认）',
          onClick: function () { m.close(); S.set({ lock: 'Locked', session: 'Revoked' }); UI.toast({ tone: 'warn', title: '已锁定', msg: '会话已失效，请重新解锁' }); }
        })];
      }
    });
  }

  /* ======================================================================
   * 3. 隐写术（/stego）
   * ==================================================================== */

  var EMBED_STEPS = ['选择载荷', '选择载体图片', '选项', '容量估算', '结果分支', '执行与进度', '完成'];
  var EXTRACT_STEPS = ['选择图像', '还原目标路径', '执行', '结果'];
  var STEGO_FLAGS = [
    { bit: 0, name: '首片', desc: 'bit0 = 1：分片的第一片，额外携带 file_hdr' },
    { bit: 1, name: '单图', desc: 'bit1 = 1：单图模式（shard_total 应为 1）' },
    { bit: 2, name: '位分散', desc: 'bit2 = 1：已启用位分散（确定性双射，不依赖密钥，默认启用）' },
    { bit: 3, name: '位序置乱', desc: 'bit3 = 1：已启用位序置乱（默认关闭；任一位丢失即整包不可解）' }
  ];

  /* data.js 的 stegoPaths 未提供 file_hdr 字段（见汇报）：此处为原型示例，仅用于展示字段清单 */
  var STEGO_FILE_HDR_SAMPLE = {
    magic: '0x56535354 ("VSST")',
    payload_ver: 2,
    flags: '0b0111',
    shard_index: 0,
    shard_total: 3,
    file_len: 1258291,
    kdf_ver: 2,
    note: '仅首片携带；来源为 vault_core_stego_embed_multi 的载荷头（原型示例）'
  };

  function flagBits(flags) {
    var v = parseInt(String(flags).replace(/^0b/i, ''), 2);
    return isNaN(v) ? 0 : v;
  }
  function hasFlag(flags, bit) { return (flagBits(flags) & (1 << bit)) !== 0; }
  function flagChips(flags) {
    return h('div', { class: 'row gap-1 wrap' }, STEGO_FLAGS.map(function (f) {
      return hasFlag(flags, f.bit)
        ? UI.chip({ text: f.name, tone: 'accent', title: f.desc })
        : UI.chip({ text: f.name + '·关', title: f.desc });
    }));
  }
  function shardIndexOf(name) {
    for (var i = 0; i < D.stegoPaths.length; i++) {
      if (D.stegoPaths[i].carrier === name) return D.stegoPaths[i].shardIndex;
    }
    return null;
  }

  VS.pages['stego'] = function (ctx) {
    ctx = ctx || { params: {}, go: null };
    /* 本页没有内部重渲染容器，刷新交给外壳的 store 订阅 */
    currentRender = null;
    return S.get('stegoEnabled') ? stegoEnabledPage(ctx) : stegoDisabledPage(ctx);
  };

  /* ---------------- 未启用：引导页 ---------------- */
  function stegoDisabledPage(ctx) {
    var body = [
      UI.empty({
        icon: 'stego',
        title: '隐写术未启用',
        desc: '隐写术默认不启用：语义上「默认隐藏」由导航层实现（导航不渲染隐写入口），本页只做启用 / 停用。',
        actions: [
          UI.btn({
            label: '启用隐写术', icon: 'stego', variant: 'primary', title: '启用隐写术（需二次确认；启用即受审计）',
            onClick: function () { confirmEnableStego(ctx); }
          })
        ]
      }),
      UI.alertbar({
        tone: 'info', icon: 'info', title: '未启用时的接口语义',
        text: '未启用时嵌入 / 提取接口返回 ' + F.num(7) + '（参数非法），并给出 dialog.stegoDisabled 文案；本页不伪造成功。'
      }),
      UI.card({
        title: '启用后会发生什么', sub: '先看边界再决定',
        body: h('ul', { class: 'col gap-2' }, D.stegoBoundaries.map(function (t) {
          return h('li', { class: 'row gap-2' }, [UI.icon('stego', 14), h('span', { class: 'grow', text: t })]);
        }))
      })
    ];
    return pageShell('隐写术', '未启用 · 启用即受审计（开关切换、每次嵌入 / 提取均写 vault-audit 的 security 类别）', [], body);
  }

  function confirmEnableStego(ctx) {
    var m = UI.confirm({
      title: '启用隐写术', size: 'sm', tone: 'danger', confirmLabel: '启用',
      body: h('div', { class: 'col gap-3' }, [
        UI.alertbar({
          tone: 'danger', icon: 'alert', title: '启用即受审计',
          text: '开关切换、每次嵌入 / 提取都会写入 vault-audit 的 security 类别；审计失败不阻断业务，但会发 ERROR_DIAG。'
        }),
        cap('隐写不替代保险箱；不可检测性没有安全保证。启用开关落盘 settings.enc（VSSG v1, kind=0，HKDF(MK,"settings")）。')
      ]),
      onConfirm: function () {
        S.set({ stegoEnabled: true });
        UI.toast({ tone: 'success', title: '隐写术已启用', msg: '已写入审计（security 类别）· 入口在导航层出现' });
        rerender();
      }
    });
    return m;
  }

  function confirmDisableStego(ctx) {
    var m = UI.confirm({
      title: '停用隐写术', size: 'sm', tone: 'danger', confirmLabel: '停用',
      body: h('div', { class: 'col gap-3' }, [
        strong('停用后的后果'),
        h('ul', { class: 'col gap-1' }, [
          h('li', { class: 't-caption', text: '嵌入 / 提取入口关闭；未启用的接口调用返回 ' + F.num(7) + '。' }),
          h('li', { class: 't-caption', text: '已生成的载体图片与其内载荷不受影响，仍可用其它客户端提取。' }),
          h('li', { class: 't-caption', text: '审计记录不会被删除：停用动作本身也写审计。' })
        ])
      ]),
      onConfirm: function () {
        S.set({ stegoEnabled: false });
        UI.toast({ tone: 'warn', title: '隐写术已停用', msg: '停用动作已写入审计（security 类别）' });
        rerender();
      }
    });
    return m;
  }

  /* ---------------- 已启用：主页面 ---------------- */
  function stegoEnabledPage(ctx) {
    var body = [
      /* 1. 安全边界声明区 */
      UI.card({
        title: '安全边界声明', sub: '逐条渲染 D.stegoBoundaries（' + F.num(D.stegoBoundaries.length) + ' 条）· 不接受「设计保证」式对外承诺',
        body: h('ul', { class: 'col gap-2' }, D.stegoBoundaries.map(function (t, i) {
          var title = String(t).split('：')[0];
          var tone = i === 1 ? 'warn' : (i === 4 ? 'info' : 'info');
          return UI.alertbar({ tone: tone, icon: i === 1 ? 'alert' : 'shield', title: title, text: t });
        }))
      }),

      /* 2. 单图容量说明 */
      UI.card({
        title: '单图容量说明', sub: '公式只作解释；容量真值只来自 vault_core_stego_plan',
        body: h('div', { class: 'col gap-3' }, [
          mono('capacity  = floor(width × height × ' + F.num(3) + ' / ' + F.num(8) + ') − ' + F.num(4) + '   （三通道各取 LSB，扣减 ' + F.num(4) + ' B 头）'),
          mono('usable    = floor(capacity × fill_ratio)'),
          mono('示例：' + F.num(1920) + '×' + F.num(1080) + ' = ' + F.num(777596) + ' B（' + F.bytes(777596) + '）· fill_ratio = ' + F.num(0.8) + ' → usable ≈ ' + F.num(622076) + ' B'),
          UI.alertbar({
            tone: 'danger', icon: 'ban', title: '硬约束：UI 不自行计算容量',
            text: '容量真值只能来自 vault_core_stego_plan（结果不缓存，两次调用之间换图则结果随之变化）；界面上的公式仅用于解释口径，不作为任何判断依据。'
          })
        ])
      }),

      /* 3. 反检测说明 */
      UI.card({
        title: '反检测说明', sub: '三项开关的真实语义与默认值',
        body: h('div', { class: 'col gap-3' }, [
          h('div', { class: 'row gap-2 wrap' }, [
            h('div', { class: 'grow' }, [strong('位分散 disperse'), cap('确定性双射，把载荷位分散到整幅图；不依赖密钥，因此同样可被持原图者比对。默认启用。')]),
            UI.badge({ text: '默认启用', tone: 'success', title: 'disperse = true' })
          ]),
          h('div', { class: 'row gap-2 wrap' }, [
            h('div', { class: 'grow' }, [strong('填充率 fill_ratio'), cap('只占用 usable = capacity × fill_ratio 的容量；默认 ' + F.num(0.8) + '（上限 ' + F.pct(0.8) + '），留出裕量降低统计显著性。')]),
            UI.badge({ text: F.pct(0.8) + ' 上限', tone: 'info', title: 'fill_ratio ≤ 0.8' })
          ]),
          h('div', { class: 'row gap-2 wrap' }, [
            h('div', { class: 'grow' }, [strong('位序置乱 shuffle'), cap('启用后任一位丢失即整包不可解，鲁棒性下降；因此默认关闭，界面在开启时给警告。')]),
            UI.badge({ text: '默认关闭', tone: 'stale', title: 'shuffle = false' })
          ]),
          UI.alertbar({ tone: 'warn', icon: 'alert', title: '不可检测性没有安全保证', text: '三项开关只降低统计检测显著性，不承诺「无法被检测」；持原图比对必然可检出。' })
        ])
      }),

      /* 4 / 5. 向导入口 */
      h('div', { class: 'grid-2' }, [
        UI.card({
          title: '嵌入向导', sub: F.num(EMBED_STEPS.length) + ' 步：载荷 → 载体 → 选项 → 容量估算 → 分支 → 执行 → 完成',
          body: h('div', { class: 'col gap-3' }, [
            cap('多图嵌入走 vault_core_stego_plan → vault_core_stego_embed_multi；' +
              'fits=false 时如实拒绝（错误码 ' + F.num(7) + ' + last_error），不截断、不降质。'),
            UI.btn({ label: '打开嵌入向导', icon: 'stego', variant: 'primary', title: '打开隐写嵌入向导', onClick: function () { openEmbedWizard(ctx); } })
          ])
        }),
        UI.card({
          title: '提取向导', sub: F.num(EXTRACT_STEPS.length) + ' 步：选图（顺序无关）→ 目标路径 → 执行 → 结果',
          body: h('div', { class: 'col gap-3' }, [
            cap('乱序重组：按 shard_index 排序后再拼接；重复 shard_index 拒绝（不做「后到者胜」）；缺片报 missing[]。'),
            UI.btn({ label: '打开提取向导', icon: 'import', title: '打开隐写提取向导', onClick: function () { openExtractWizard(ctx); } })
          ])
        })
      ]),

      /* 6. 单图模式 */
      UI.card({
        title: '单图模式', sub: 'vault_core_stego_capacity（负值即错误码，仅供 UI 预估）',
        actions: [
          UI.btn({ label: '单图容量估算', title: '单图容量估算（负值即错误码，供 UI 预估）', onClick: function () { openSingleImage('capacity'); } }),
          UI.btn({ label: '单图嵌入', title: '单图嵌入（超容量报错误码 ' + F.num(6) + '）', onClick: function () { openSingleImage('embed'); } }),
          UI.btn({ label: '单图提取', title: '单图提取（flags bit1 = 单图）', onClick: function () { openSingleImage('extract'); } })
        ],
        body: h('div', { class: 'col gap-2' }, [
          cap('单图嵌入 / 提取走 vault_core_stego_embed / _extract；容量估算走 vault_core_stego_capacity。'),
          cap('返回值语义：正数 = 可用字节数；负值 = 错误码（例如 −' + F.num(6) + ' 即错误码 ' + F.num(6) + ' 格式错误）。'),
          UI.alertbar({ tone: 'danger', icon: 'ban', title: '超容量绝不截断', text: '载荷超出单图容量 → 错误码 ' + F.num(6) + ' + 「payload exceeds image capacity」，如实拒绝。' })
        ])
      }),

      /* 7. 批量区 */
      UI.card({
        title: '批量区', sub: '外壳侧顺序调用：逐项状态，不聚合为一个总状态',
        actions: [
          UI.btn({ label: '批量嵌入', title: '批量嵌入（顺序调用，逐项显示成功 / 失败）', onClick: function () { openBatch('embed'); } }),
          UI.btn({ label: '批量提取', title: '批量提取（顺序调用，逐项显示成功 / 失败）', onClick: function () { openBatch('extract'); } })
        ],
        body: h('div', { class: 'col gap-2' }, [
          cap('批量 = 外壳侧对每个条目顺序调用单次接口；UI 必须逐项展示成功 / 失败与错误码，不得折叠成一个「批量完成」。'),
          cap('单项失败不影响其余项，也不回滚已成功的产物。')
        ])
      }),

      /* 8. 隐写产物详情 */
      stegoPathsCard()
    ];

    var actions = [
      UI.btn({ label: '嵌入向导', icon: 'stego', variant: 'primary', title: '打开嵌入向导', onClick: function () { openEmbedWizard(ctx); } }),
      UI.btn({ label: '提取向导', icon: 'import', title: '打开提取向导', onClick: function () { openExtractWizard(ctx); } }),
      UI.btn({ label: '停用隐写术', variant: 'outline-danger', title: '停用隐写术（会说明后果）', onClick: function () { confirmDisableStego(ctx); } })
    ];

    return pageShell('隐写术', '已启用 · 启用即受审计 · 容量真值只来自 vault_core_stego_plan（UI 不自行计算）', actions, body);
  }

  /* ---------------- 产物详情 ---------------- */
  function stegoPathsCard() {
    var rows = D.stegoPaths.map(function (p) {
      var single = hasFlag(p.flags, 1);
      var inconsistent = single && p.shardTotal > 1;
      return {
        id: p.shardIndex, p: p, single: single, inconsistent: inconsistent
      };
    });
    var columns = [
      { key: 'shard', label: 'shard_index / total', width: '170px', class: 't-mono', render: function (r) { return F.num(r.p.shardIndex) + ' / ' + F.num(r.p.shardTotal); } },
      { key: 'carrier', label: '载体', render: function (r) { return h('span', { text: r.p.carrier }); } },
      { key: 'fileLen', label: 'file_len', width: '140px', class: 't-mono', render: function (r) { return F.num(r.p.fileLen); } },
      { key: 'ver', label: 'payload_ver', width: '120px', class: 't-mono', render: function (r) { return F.num(r.p.payloadVer); } },
      {
        key: 'flags', label: 'flags 位含义', render: function (r) {
          return h('div', { class: 'col gap-1' }, [
            h('span', { class: 't-mono', text: r.p.flags }),
            flagChips(r.p.flags),
            r.inconsistent
              ? UI.badge({ text: 'flags 与 shard_total 不一致（bit1 单图置位但 shard_total = ' + F.num(r.p.shardTotal) + '）· 数据样例需核对', tone: 'danger' })
              : null
          ]);
        }
      },
      { key: 'crc', label: 'shard_crc32', width: '130px', class: 't-mono', render: function (r) { return U.shortHash(r.p.shardCrc32, 10, 6); } },
      {
        key: 'hdr', label: 'file_hdr', render: function (r) {
          return r.p.shardIndex === 0
            ? mono(JSON.stringify(STEGO_FILE_HDR_SAMPLE))
            : cap('—（非首片不携带 file_hdr）');
        }
      }
    ];
    return UI.card({
      title: '隐写产物详情', sub: 'payload_ver = ' + F.num(2) + ' 分片布局 · flags / shard_index / file_len / shard_crc32 / file_hdr',
      body: h('div', { class: 'col gap-3' }, [
        UI.table({ columns: columns, rows: rows, empty: UI.empty({ icon: 'stego', title: '没有隐写产物', desc: '完成一次嵌入后会在这里列出分片布局' }) }),
        h('div', { class: 'col gap-1' }, [
          strong('flags 位含义'),
          h('ul', { class: 'col gap-1' }, STEGO_FLAGS.map(function (f) {
            return h('li', { class: 't-caption', text: 'bit' + F.num(f.bit) + ' · ' + f.name + '：' + f.desc });
          }))
        ]),
        cap('file_hdr 只有首片携带；data.js 的 stegoPaths 未提供 file_hdr 字段，上表首行为原型示例（见汇报）。')
      ])
    });
  }

  /* ---------------- 嵌入向导 ---------------- */
  function newEmbedState() {
    return {
      step: 0,
      payloadId: (D.vault.entries[0] || {}).id,
      carriers: [],
      badAdded: false,
      fillRatio: 0.8, disperse: true, shuffle: false, name: '', codec: 'png',
      maxBytes: 4294967296, maxImages: 8,
      produced: 0, cancelled: false, done: false
    };
  }

  function imageOptions() {
    var list = D.vault.entries.filter(function (e) { return e.type === 'image'; }).map(function (e) {
      return { key: 'e' + e.id, id: e.id, name: e.name, size: e.size, error: null };
    });
    if (embedWiz && embedWiz.badAdded) {
      list.push({ key: 'bad', id: null, name: '灰度图-16bit.png（演示）', size: 65536, error: 'unsupported png format' });
    }
    return list;
  }

  function openEmbedWizard(ctx) {
    if (!S.get('stegoEnabled')) { openStegoDisabledDialog(); return; }
    embedWiz = newEmbedState();
    var modalHost = host('div');
    var m = UI.modal({
      title: '隐写嵌入向导', sub: 'vault_core_stego_plan → vault_core_stego_embed_multi', size: 'lg',
      body: modalHost,
      onClose: stopEmbedRun,
      footer: function () { return [UI.btn({ label: '关闭', title: '关闭向导', onClick: function () { m.close(); } })]; }
    });
    paintEmbed(modalHost, m, ctx);
  }

  function embedNav(w, modalHost, m, ctx) {
    var blocked = false, blockReason = '';
    if (w.step === 4) {
      var p = planSummary(w);
      if (!p.fits) { blocked = true; blockReason = '需要 ' + F.num(p.imagesNeeded) + ' 张图，已选 ' + F.num(p.imagesGiven); }
      else if (p.errors.length) { blocked = true; blockReason = '存在不支持的图片形态（' + F.num(p.errors.length) + ' 张）'; }
    }
    var nextLabel = w.step === 4 ? '开始嵌入' : '下一步';
    return h('div', { class: 'row gap-2', style: { marginTop: '4px' } }, [
      UI.btn({
        label: '上一步', title: '上一步', disabled: w.step === 0 || w.step === 5,
        onClick: function () { w.step = Math.max(0, w.step - 1); paintEmbed(modalHost, m, ctx); }
      }),
      w.step < EMBED_STEPS.length - 1
        ? UI.btn({
          label: nextLabel, variant: 'primary', disabled: blocked,
          title: blocked ? (nextLabel + ' 不可用 · ' + blockReason) : nextLabel,
          onClick: function () {
            var cur = planSummary(w);
            var repaint = function () { paintEmbed(modalHost, m, ctx); };
            if (w.step === 1 && !w.carriers.length) { UI.toast({ tone: 'warn', title: '请先选择载体图片', msg: '至少选择 ' + F.num(1) + ' 张 PNG 载体' }); return; }
            /* ⑤ 结果分支：不满足即阻止提交（弹窗给出所需图数 / 逐条列出不支持的图） */
            if (w.step === 4 && cur.errors.length) { openBadImagesDialog(cur.errors, repaint); return; }
            if (w.step === 4 && !cur.fits) { openNotFitsDialog(cur, repaint); return; }
            w.step = Math.min(EMBED_STEPS.length - 1, w.step + 1);
            paintEmbed(modalHost, m, ctx);
            if (w.step === 5) runEmbed(w, modalHost, m, ctx);
          }
        })
        : null,
      UI.btn({
        label: '取消向导', title: '取消向导（不产生任何产物）',
        onClick: function () {
          stopEmbedRun();
          m.close();
          UI.toast({ tone: 'info', title: '已取消向导', msg: embedWiz && embedWiz.produced ? ('已生成的图数 ' + F.num(embedWiz.produced) + ' · ' + errText(12)) : errText(12) });
        }
      })
    ]);
  }

  function paintEmbed(modalHost, m, ctx) {
    modalHost.innerHTML = '';
    var w = embedWiz;
    var box = host('col gap-4');
    box.appendChild(UI.steps({
      items: EMBED_STEPS.map(function (label, i) {
        return { label: label, state: i < w.step ? 'done' : (i === w.step ? 'active' : 'todo') };
      })
    }));
    box.appendChild(embedStepBody(w, modalHost, m, ctx));
    box.appendChild(embedNav(w, modalHost, m, ctx));
    modalHost.appendChild(box);
  }

  function embedStepBody(w, modalHost, m, ctx) {
    var box = host('col gap-3');
    var repaint = function () { paintEmbed(modalHost, m, ctx); };

    if (w.step === 0) {
      box.appendChild(cap('载荷取自保险箱文件（原型取前若干条目）；载荷以 AEAD 密文形式嵌入，外壳侧不见明文，Dart 只传路径。'));
      var payloadOpts = D.vault.entries.slice(0, 8).map(function (e) {
        return { value: e.id, title: e.name, desc: F.bytes(e.size) + ' · ' + e.type };
      });
      box.appendChild(a11yRadioCards(UI.radioCards({
        value: w.payloadId, options: payloadOpts,
        onChange: function (v) { w.payloadId = v; }
      }), payloadOpts));
      return box;
    }

    if (w.step === 1) {
      box.appendChild(cap('多选 PNG 载体；选择顺序不影响嵌入结果（分片按 shard_index 排序）。'));
      var opts = imageOptions();
      var grid = host('col gap-2');
      opts.forEach(function (o) {
        var checked = w.carriers.indexOf(o.key) >= 0;
        var cb = UI.checkbox({
          checked: checked,
          onChange: function () {
            if (cb.input.checked) { if (w.carriers.indexOf(o.key) < 0) w.carriers.push(o.key); }
            else { w.carriers = w.carriers.filter(function (k) { return k !== o.key; }); }
            repaint();
          }
        });
        grid.appendChild(a11y(h('div', { class: 'row-between' }, [
          h('div', { class: 'grow' }, [
            cb,
            cap(F.bytes(o.size) + (o.error ? ' · error = ' + o.error : ' · 形态受支持'))
          ]),
          o.error ? UI.badge({ text: '不支持', tone: 'danger', title: o.error }) : UI.badge({ text: 'PNG', tone: 'info' })
        ]), o.name + (o.error ? '（' + o.error + '）' : '')));
      });
      box.appendChild(grid);
      box.appendChild(h('div', { class: 'row gap-2 wrap' }, [
        UI.btn({
          label: '加入一张不支持的 PNG', icon: 'alert',
          title: '加入一张灰度 / 调色板 / 16 位 PNG（error = unsupported png format）',
          onClick: function () { w.badAdded = true; if (w.carriers.indexOf('bad') < 0) w.carriers.push('bad'); repaint(); }
        }),
        UI.btn({ label: '全选支持项', title: '选择全部形态受支持的图片', onClick: function () {
          w.carriers = imageOptions().filter(function (o) { return !o.error; }).map(function (o) { return o.key; });
          repaint();
        } })
      ]));
      box.appendChild(cap('图片形态不支持（灰度 / 调色板 / 16 位）时引擎返回 error，界面必须逐条列出并阻止提交。'));
      return box;
    }

    if (w.step === 2) {
      var ratio = UI.slider({
        min: 0.1, max: 0.8, step: 0.05, value: w.fillRatio,
        onInput: function () { w.fillRatio = parseFloat(ratio.value); ratioLabel.textContent = 'fill_ratio = ' + F.num(w.fillRatio) + '（上限 ' + F.pct(0.8) + '）'; }
      });
      a11y(ratio, '填充率 fill_ratio（默认 0.8，上限 80%）');
      var ratioLabel = h('div', { class: 't-caption', text: 'fill_ratio = ' + F.num(w.fillRatio) + '（上限 ' + F.pct(0.8) + '）' });
      box.appendChild(UI.field({ label: '填充率 fill_ratio', control: h('div', { class: 'col gap-1' }, [ratio, ratioLabel]), hint: '只占用 usable = capacity × fill_ratio 的容量；默认 ' + F.num(0.8) + '。' }));

      var disperse = UI.switchCtl({
        checked: w.disperse, label: '位分散 disperse',
        sub: '确定性双射，不依赖密钥；默认开启', onChange: function () { w.disperse = !!disperse.input.checked; }
      });
      a11y(disperse, '位分散 disperse（默认开启）');
      box.appendChild(a11y(h('div', { class: 'field' }, [disperse]), '位分散 disperse（默认开启）'));

      var shuffle = UI.switchCtl({
        checked: w.shuffle, label: '位序置乱 shuffle',
        sub: '默认关闭', onChange: function () { w.shuffle = !!shuffle.input.checked; repaint(); }
      });
      a11y(shuffle, '位序置乱 shuffle（默认关闭）');
      box.appendChild(a11y(h('div', { class: 'field' }, [shuffle]), '位序置乱 shuffle（默认关闭）'));
      if (w.shuffle) {
        box.appendChild(UI.alertbar({ tone: 'danger', icon: 'alert', title: '任一位丢失即整包不可解', text: '位序置乱提升隐蔽性但牺牲鲁棒性：任何一位翻转都会导致整包无法还原，且不支持部分恢复。' }));
      }

      var nameInput = UI.input({ value: w.name, placeholder: '（默认空：不写入原始名称）', onInput: function () { w.name = nameInput.value; } });
      a11y(nameInput, '可选名称（默认空）');
      box.appendChild(UI.field({ label: '名称 name（默认空）', control: nameInput, hint: '默认不写入名称，避免在载体中留下可识别信息。' }));

      var codecSel = a11ySelect(UI.select({
        value: w.codec,
        options: [{ value: 'png', label: 'png' }, { value: 'auto', label: 'auto' }],
        onChange: function (v) { w.codec = v; repaint(); }
      }), '编解码 codec（png / auto）');
      box.appendChild(UI.field({ label: '编解码 codec', control: codecSel, hint: 'JPEG 鲁棒路线不做：auto 一律解析为 png。' }));
      if (w.codec === 'auto') {
        box.appendChild(UI.alertbar({ tone: 'info', icon: 'info', title: 'auto 一律解析为 png（codec_fallback=png）', text: '不承诺 JPEG 鲁棒性；选择 auto 与选择 png 的实际行为一致。' }));
      }

      var maxBytesInput = UI.input({ type: 'number', value: w.maxBytes, onInput: function () { w.maxBytes = parseInt(maxBytesInput.value, 10) || 0; } });
      a11y(maxBytesInput, '载荷上限 maxBytes（默认 4294967296）');
      box.appendChild(UI.field({
        label: 'maxBytes（默认 ' + F.num(4294967296) + ' = ' + F.bytes(4294967296) + '）',
        control: maxBytesInput, hint: '超过该值的载荷直接拒绝，不做截断。'
      }));

      var maxImagesInput = UI.input({ type: 'number', value: w.maxImages, onInput: function () { w.maxImages = parseInt(maxImagesInput.value, 10) || 0; } });
      a11y(maxImagesInput, '最大图数 maxImages');
      box.appendChild(UI.field({ label: 'maxImages', control: maxImagesInput, hint: '一次嵌入允许使用的最大载体数量。' }));
      return box;
    }

    if (w.step === 3) {
      box.appendChild(UI.alertbar({
        tone: 'info', icon: 'info', title: '容量真值来自 vault_core_stego_plan',
        text: '下表为引擎 plan 的原值直出（D.stegoPlanSample）；UI 不自行计算容量，也不缓存结果（换图后需重新调用）。'
      }));
      var per = D.stegoPlanSample.perImage.map(function (p) { return p; });
      box.appendChild(UI.table({
        rows: per,
        columns: [
          { key: 'path', label: '载体 path' },
          { key: 'width', label: 'width', align: 'right', class: 't-mono', render: function (r) { return F.num(r.width); } },
          { key: 'height', label: 'height', align: 'right', class: 't-mono', render: function (r) { return F.num(r.height); } },
          { key: 'capacity', label: 'capacity', align: 'right', class: 't-mono', render: function (r) { return F.num(r.capacity); } },
          { key: 'usable', label: 'usable', align: 'right', class: 't-mono', render: function (r) { return F.num(r.usable); } },
          {
            key: 'shardBytes', label: 'shardBytes', align: 'right', class: 't-mono', render: function (r) {
              return r.error ? UI.badge({ text: r.error, tone: 'danger' }) : F.num(r.shardBytes);
            }
          }
        ]
      }));
      var s = planSummary(w);
      box.appendChild(UI.kv([
        { k: 'imagesNeeded', v: F.num(D.stegoPlanSample.imagesNeeded), mono: true },
        { k: 'imagesGiven（已选）', v: F.num(s.imagesGiven), mono: true },
        { k: 'fits', v: String(s.fits), mono: true },
        { k: 'shortfallBytes', v: F.num(s.fits ? D.stegoPlanSample.shortfallBytes : s.shortfallBytes), mono: true },
        { k: 'notes[]', v: D.stegoPlanSample.notes.length ? D.stegoPlanSample.notes.join(' / ') : '（空）', mono: true }
      ]));
      box.appendChild(mono('容量账本示例行：file_len=' + F.num(1258291) + ' → shard0=' + F.num(621990) +
        ' + shard1=' + F.num(622034) + ' + shard2=' + F.num(14267) + ' = ' + F.num(1258291)));
      box.appendChild(cap('逐片 shardBytes 之和 = file_len；末片不足按实际写入，不补齐、不填充占位。'));
      if (!s.fits) {
        box.appendChild(UI.alertbar({
          tone: 'danger', icon: 'ban', title: '容量不足：如实拒绝', text: '需要 ' + F.num(s.imagesNeeded) + ' 张图，已选 ' + F.num(s.imagesGiven) + '；不截断、不降质，请继续选图。'
        }));
      }
      if (s.errors.length) {
        box.appendChild(UI.alertbar({
          tone: 'danger', icon: 'alert', title: '存在不支持的图片形态',
          text: s.errors.map(function (e) { return e.name + ' → ' + e.error; }).join('；') + '（阻止提交）'
        }));
      }
      return box;
    }

    if (w.step === 4) {
      var s2 = planSummary(w);
      box.appendChild(strong('结果分支'));
      if (!s2.fits) {
        box.appendChild(UI.alertbar({ tone: 'danger', icon: 'ban', title: '需要 ' + F.num(s2.imagesNeeded) + ' 张图，已选 ' + F.num(s2.imagesGiven), text: '缺口 ' + F.bytes(s2.shortfallBytes) + '；提交已被阻止。' }));
        box.appendChild(h('div', { class: 'row gap-2 wrap' }, [
          UI.btn({ label: '查看所需图数', variant: 'danger', title: '查看 imagesNeeded / imagesGiven / shortfallBytes', onClick: function () { openNotFitsDialog(s2, repaint); } }),
          UI.btn({ label: '继续选图', variant: 'primary', title: '返回载体选择继续选图', onClick: function () { w.step = 1; repaint(); } })
        ]));
      } else if (s2.errors.length) {
        box.appendChild(UI.alertbar({ tone: 'danger', icon: 'alert', title: '不支持的图片形态阻止提交', text: s2.errors.map(function (e) { return e.name + ' → ' + e.error; }).join('；') }));
        box.appendChild(h('div', { class: 'row gap-2 wrap' }, [
          UI.btn({ label: '查看不支持的图', variant: 'danger', title: '逐条查看 error', onClick: function () { openBadImagesDialog(s2.errors, repaint); } }),
          UI.btn({ label: '移除不支持的图片', title: '从已选载体中移除 error 项', onClick: function () { w.carriers = w.carriers.filter(function (k) { return k !== 'bad'; }); repaint(); } })
        ]));
      } else {
        box.appendChild(UI.alertbar({ tone: 'info', icon: 'check', title: '容量与形态检查通过', text: '下一步开始嵌入：写入过程可取消，取消时如实提示已生成的图数。' }));
      }
      return box;
    }

    /* ⑥ 执行与进度：UI.bar + 已生成图数 + 取消（取消如实提示已生成的图数）*/
    if (w.step === 5) {
      var total = D.stegoPlanSample.perImage.length;
      box.appendChild(strong(w.done ? '嵌入完成' : '正在按 shard_index 顺序写入载体…'));
      box.appendChild(UI.bar(total ? (w.produced / total) : 0, w.done ? 'success' : 'accent'));
      box.appendChild(h('div', { class: 't-caption', text: '已生成 ' + F.num(w.produced) + ' / ' + F.num(total) + ' 张 · 当前：' + ((D.stegoPlanSample.perImage[Math.min(w.produced, total - 1)] || {}).path || '—') }));
      box.appendChild(h('div', { class: 'row gap-2 wrap' }, [
        UI.btn({
          label: '取消', variant: 'danger', title: '取消写入（返回 ' + F.num(12) + '；如实提示已生成的图数，不回滚）',
          onClick: function () { cancelEmbedRun(w, modalHost, m, ctx); }
        }),
        cap('取消不可撤销已写出的载体：界面只如实报告，不假装回滚。')
      ]));
      return box;
    }

    /* step 6 完成 */
    box.appendChild(strong(w.cancelled ? '嵌入已取消' : '嵌入完成'));
    box.appendChild(UI.alertbar({
      tone: w.cancelled ? 'warn' : 'info', icon: w.cancelled ? 'alert' : 'check',
      title: w.cancelled ? ('已取消 · 已生成的图数 ' + F.num(w.produced)) : ('已生成 ' + F.num(w.produced) + ' 张载体图片'),
      text: w.cancelled
        ? errText(12) + '；已生成的载体不会被回滚，也不会删除。'
        : '每次嵌入均写 vault-audit（security 类别）；审计失败不阻断业务但发 ERROR_DIAG。'
    }));
    var per2 = D.stegoPlanSample.perImage.slice(0, Math.max(1, w.produced));
    box.appendChild(UI.table({
      rows: per2,
      columns: [
        { key: 'path', label: '生成的图片' },
        { key: 'shardBytes', label: '逐图占用', align: 'right', class: 't-mono', render: function (r) { return F.bytes(r.shardBytes); } },
        { key: 'usable', label: 'usable', align: 'right', class: 't-mono', render: function (r) { return F.num(r.usable); } }
      ]
    }));
    box.appendChild(cap('产物详情（flags / shard_index / shard_crc32 / file_hdr）见页面「隐写产物详情」区。'));
    return box;
  }

  function planSummary(w) {
    var opts = imageOptions();
    var selected = opts.filter(function (o) { return w.carriers.indexOf(o.key) >= 0; });
    var errors = selected.filter(function (o) { return !!o.error; });
    var imagesGiven = selected.length;
    var imagesNeeded = D.stegoPlanSample.imagesNeeded;
    var fits = imagesGiven >= imagesNeeded && errors.length === 0;
    var shortfall = 0;
    if (imagesGiven < imagesNeeded) {
      var left = D.stegoPlanSample.perImage.slice(imagesGiven);
      left.forEach(function (p) { shortfall += (p.usable || 0); });
    }
    return { imagesGiven: imagesGiven, imagesNeeded: imagesNeeded, fits: fits, errors: errors, shortfallBytes: shortfall };
  }

  function openNotFitsDialog(s, onContinue) {
    var body = host('col gap-3');
    body.appendChild(UI.kv([
      { k: 'imagesNeeded', v: F.num(s.imagesNeeded), mono: true },
      { k: 'imagesGiven', v: F.num(s.imagesGiven), mono: true },
      { k: 'shortfallBytes', v: F.num(s.shortfallBytes) + '（' + F.bytes(s.shortfallBytes) + '）', mono: true }
    ]));
    body.appendChild(UI.alertbar({ tone: 'danger', icon: 'ban', title: '提交已被阻止', text: '不截断、不降质、不更换编码：请继续选图后重新估算。' }));
    var m = UI.modal({
      title: '需要 ' + F.num(s.imagesNeeded) + ' 张图，已选 ' + F.num(s.imagesGiven), size: 'sm', tone: 'danger', body: body,
      footer: function () {
        return [
          UI.btn({ label: '取消', title: '留在当前步骤', onClick: function () { m.close(); } }),
          UI.btn({
            label: '继续选图', variant: 'primary', title: '返回载体选择继续选图',
            onClick: function () { m.close(); if (embedWiz) embedWiz.step = 1; if (onContinue) onContinue(); }
          })
        ];
      }
    });
    return m;
  }

  function openBadImagesDialog(errors, onFix) {
    var body = host('col gap-3');
    body.appendChild(UI.alertbar({ tone: 'danger', icon: 'alert', title: '不支持的图片形态', text: '灰度 / 调色板 / 16 位 PNG 不支持嵌入；逐条列出并阻止提交。' }));
    body.appendChild(UI.table({
      rows: errors,
      columns: [
        { key: 'name', label: '图片' },
        { key: 'error', label: 'error', class: 't-mono' }
      ]
    }));
    var m = UI.modal({
      title: '存在不支持的图片', size: 'sm', tone: 'danger', body: body,
      footer: function () {
        return [
          UI.btn({ label: '关闭', title: '关闭', onClick: function () { m.close(); } }),
          UI.btn({
            label: '移除不支持的图片', variant: 'primary', title: '从已选载体中移除 error 项',
            onClick: function () {
              m.close();
              if (embedWiz) embedWiz.carriers = embedWiz.carriers.filter(function (k) { return k !== 'bad'; });
              if (onFix) onFix();
            }
          })
        ];
      }
    });
    return m;
  }

  /* ---------------- 嵌入执行与取消（⑥ 执行与进度）---------------- */
  var embedTimer = null;

  function stopEmbedRun() {
    if (embedTimer) { clearInterval(embedTimer); embedTimer = null; }
  }

  function runEmbed(w, modalHost, m, ctx) {
    var total = D.stegoPlanSample.perImage.length;
    stopEmbedRun();
    w.produced = 0;
    w.cancelled = false;
    w.done = false;
    embedTimer = setInterval(function () {
      w.produced += 1;
      if (w.produced >= total) {
        w.produced = total;
        w.done = true;
        stopEmbedRun();
        w.step = 6;
      }
      paintEmbed(modalHost, m, ctx);
    }, 420);
  }

  /** 用户点「取消」：停止写入，如实提示已生成的图数（不回滚、不删除） */
  function cancelEmbedRun(w, modalHost, m, ctx) {
    stopEmbedRun();
    w.cancelled = true;
    w.done = false;
    w.step = 6;
    paintEmbed(modalHost, m, ctx);
    UI.toast({
      tone: 'warn', title: '嵌入已取消',
      msg: '已生成的图数 ' + F.num(w.produced) + ' · ' + errText(12) + '（已生成的载体保留，不回滚）', duration: 4200
    });
  }

  /* ---------------- 提取向导 ---------------- */
  function newExtractState() {
    return { step: 0, picks: [], dropSecond: false, target: '', name: '还原-输出.png', progress: 0, done: false };
  }

  function openExtractWizard(ctx) {
    if (!S.get('stegoEnabled')) { openStegoDisabledDialog(); return; }
    extractWiz = newExtractState();
    var modalHost = host('div');
    var m = UI.modal({
      title: '隐写提取向导', sub: 'vault_core_stego_extract_multi（乱序重组 / 缺片报 missing[]）', size: 'lg',
      body: modalHost,
      footer: function () { return [UI.btn({ label: '关闭', title: '关闭向导', onClick: function () { m.close(); } })]; }
    });
    paintExtract(modalHost, m, ctx);
  }

  function paintExtract(modalHost, m, ctx) {
    modalHost.innerHTML = '';
    var w = extractWiz;
    var box = host('col gap-4');
    box.appendChild(UI.steps({
      items: EXTRACT_STEPS.map(function (label, i) {
        return { label: label, state: i < w.step ? 'done' : (i === w.step ? 'active' : 'todo') };
      })
    }));

    if (w.step === 0) {
      box.appendChild(cap('选图顺序无关：重组时按 shard_index 排序；重复 shard_index 拒绝（不做「后到者胜」）。'));
      var opts = D.vault.entries.filter(function (e) { return e.type === 'image'; }).map(function (e) {
        return { key: 'e' + e.id, name: e.name, size: e.size };
      });
      opts.forEach(function (o) {
        var cb = UI.checkbox({
          checked: w.picks.indexOf(o.key) >= 0,
          onChange: function () {
            if (cb.input.checked) { if (w.picks.indexOf(o.key) < 0) w.picks.push(o.key); }
            else { w.picks = w.picks.filter(function (k) { return k !== o.key; }); }
            paintExtract(modalHost, m, ctx);
          }
        });
        box.appendChild(a11y(h('div', { class: 'row-between' }, [
          h('div', { class: 'grow' }, [cb, cap(F.bytes(o.size))]),
          h('span', { class: 't-mono', text: 'shard_index = ' + (shardIndexOf(o.name) === null ? '—' : F.num(shardIndexOf(o.name))) })
        ]), o.name + '（选择顺序无关）'));
      });
      var picked = opts.filter(function (o) { return w.picks.indexOf(o.key) >= 0; });
      box.appendChild(UI.kv([
        { k: '选择顺序', v: picked.map(function (o) { return o.name; }).join(' → ') || '—', mono: true },
        {
          k: '按 shard_index 排序后', v: picked.slice().sort(function (a, b) {
            var ia = shardIndexOf(a.name), ib = shardIndexOf(b.name);
            return (ia === null ? 99 : ia) - (ib === null ? 99 : ib);
          }).map(function (o) { return o.name + '(' + (shardIndexOf(o.name) === null ? '?' : F.num(shardIndexOf(o.name))) + ')'; }).join(' → ') || '—', mono: true
        }
      ]));
      box.appendChild(h('div', { class: 'row gap-2 wrap' }, [
        UI.btn({ label: '全选载体', title: '选择全部 PNG 载体', onClick: function () { w.picks = opts.map(function (o) { return o.key; }); paintExtract(modalHost, m, ctx); } }),
        UI.btn({
          label: '移除第 ' + F.num(2) + ' 张', variant: 'outline-danger',
          title: '演示缺片：移除已选中的第 ' + F.num(2) + ' 张图，触发 missing[]',
          onClick: function () { w.dropSecond = true; if (w.picks.length >= 2) w.picks.splice(1, 1); paintExtract(modalHost, m, ctx); }
        })
      ]));
      /* 兼容读提示（两态演示） */
      var legacy = UI.switchCtl({
        checked: demo.stegoLegacy, label: '演示：载荷为 V1.0 兼容格式',
        sub: '单向兼容：可读 V1.0，V1.0 客户端读不了 V2.0',
        onChange: function () { demo.stegoLegacy = !!legacy.input.checked; demo.stegoNewer = false; paintExtract(modalHost, m, ctx); }
      });
      a11y(legacy, '演示 V1.0 兼容读');
      var newer = UI.switchCtl({
        checked: demo.stegoNewer, label: '演示：更高版本载荷',
        sub: '当前客户端无法读取更高版本载荷',
        onChange: function () { demo.stegoNewer = !!newer.input.checked; demo.stegoLegacy = false; paintExtract(modalHost, m, ctx); }
      });
      a11y(newer, '演示更高版本载荷');
      box.appendChild(h('div', { class: 'col gap-2' }, [legacy, newer]));
      box.appendChild(UI.alertbar({
        tone: demo.stegoNewer ? 'danger' : (demo.stegoLegacy ? 'info' : 'info'),
        icon: demo.stegoNewer ? 'ban' : 'info',
        title: demo.stegoNewer ? '当前客户端无法读取更高版本载荷' : (demo.stegoLegacy ? '该载荷为 V1.0 兼容格式' : '兼容读提示（未触发）'),
        text: demo.stegoNewer
          ? '版本高于本客户端支持的 payload_ver：如实拒绝（错误码 ' + F.num(6) + '），不尝试降级解析。'
          : (demo.stegoLegacy ? '按 ' + F.num(1) + ' 版布局解析：' + F.num(0) + 'x00 / 0x02 首字节判定，未知首字节返回 ' + F.num(6) + '。' : 'V1.0 兼容读为单向兼容；V1.0 客户端无法读取 V2.0 产物。')
      }));
    } else if (w.step === 1) {
      box.appendChild(cap('还原目标路径由用户选择；引擎只接受路径，不接收明文。'));
      var folderOptions = [{ value: '0', label: '保险箱根目录' }].concat(D.vault.folders.map(function (f) {
        return { value: String(f.id), label: f.name };
      }));
      var sel = a11ySelect(UI.select({ value: w.target || '0', options: folderOptions, onChange: function (v) { w.target = v; } }), '还原目标文件夹');
      var nameInput = UI.input({ value: w.name, onInput: function () { w.name = nameInput.value; } });
      a11y(nameInput, '还原后的文件名');
      box.appendChild(UI.field({ label: '还原目标文件夹', control: sel }));
      box.appendChild(UI.field({ label: '还原文件名', control: nameInput, hint: '默认使用还原后的名称；不带原始路径。' }));
      box.appendChild(UI.alertbar({ tone: 'info', icon: 'info', title: '终验不一致则删除已写出的目标文件', text: '重组完成后做整体终验（file_len + shard_crc32）；不一致时删除已写出的目标文件，不留半成品。' }));
    } else if (w.step === 2) {
      var bar = barHost(0, 'accent');
      var label = h('div', { class: 't-caption', text: '正在按 shard_index 重组…' });
      box.appendChild(bar);
      box.appendChild(label);
      box.appendChild(cap('绝不静默补零或跳过：缺片时直接失败并列出 missing[]。'));
      setTimeout(function () {
        bar.setBar(1, 'success');
        label.textContent = '重组完成';
      }, 600);
    } else {
      var missing = computeMissing();
      if (missing.length) {
        box.appendChild(UI.alertbar({
          tone: 'danger', icon: 'ban', title: '缺片 · 提取失败',
          text: 'missing: [' + missing.map(function (m) { return F.num(m); }).join(', ') + '] · 绝不静默补零或跳过'
        }));
        box.appendChild(UI.btn({
          label: '查看失败详情', variant: 'danger', title: '查看 badImages 详情（missing[]）',
          onClick: function () { showBadImagesDialog(missing); }
        }));
      } else {
        box.appendChild(UI.alertbar({ tone: 'info', icon: 'check', title: '提取完成', text: '逐片 CRC 与整体 file_len 校验通过。' }));
        box.appendChild(UI.btn({
          label: '查看还原结果', variant: 'primary', title: '查看 showRestored 弹窗',
          onClick: function () { showRestoredDialog(); }
        }));
      }
    }

    box.appendChild(h('div', { class: 'row gap-2' }, [
      UI.btn({ label: '上一步', title: '上一步', disabled: w.step === 0, onClick: function () { w.step = Math.max(0, w.step - 1); paintExtract(modalHost, m, ctx); } }),
      w.step < EXTRACT_STEPS.length - 1
        ? UI.btn({
          label: w.step === 2 ? '完成重组' : '下一步', variant: 'primary',
          title: '下一步',
          onClick: function () {
            if (w.step === 0 && !w.picks.length) { UI.toast({ tone: 'warn', title: '请先选择图像', msg: '至少选择 ' + F.num(1) + ' 张载体图片' }); return; }
            w.step = Math.min(EXTRACT_STEPS.length - 1, w.step + 1);
            paintExtract(modalHost, m, ctx);
          }
        })
        : null,
      UI.btn({ label: '取消', title: '取消向导', onClick: function () { m.close(); UI.toast({ tone: 'info', title: '已取消', msg: errText(12) }); } })
    ]));
    modalHost.appendChild(box);
  }

  function computeMissing() {
    var total = D.stegoPaths.length;
    var present = {};
    var opts = D.vault.entries.filter(function (e) { return e.type === 'image'; });
    extractWiz.picks.forEach(function (k) {
      var e = opts.filter(function (o) { return 'e' + o.id === k; })[0];
      if (!e) return;
      var idx = shardIndexOf(e.name);
      if (idx !== null) present[idx] = true;
    });
    var missing = [];
    for (var i = 0; i < total; i++) if (!present[i]) missing.push(i);
    return missing;
  }

  function showBadImagesDialog(missing) {
    var body = host('col gap-3');
    body.appendChild(UI.kv([
      { k: 'missing[]', v: '[' + missing.map(function (m) { return F.num(m); }).join(', ') + ']', mono: true },
      { k: 'shard_total', v: F.num(D.stegoPaths.length), mono: true },
      { k: '已收到', v: F.num(D.stegoPaths.length - missing.length), mono: true }
    ]));
    body.appendChild(UI.alertbar({
      tone: 'danger', icon: 'ban', title: '绝不静默补零或跳过',
      text: '缺片时整包不可解：不做补零、不做跳过、不做部分还原；已写出的目标文件会被删除。'
    }));
    var m = UI.modal({
      title: 'badImages · 提取失败', size: 'sm', tone: 'danger', body: body,
      footer: function () { return [UI.btn({ label: '关闭', title: '关闭', onClick: function () { m.close(); } })]; }
    });
    return m;
  }

  function showRestoredDialog() {
    var body = host('col gap-3');
    body.appendChild(UI.kv([
      { k: 'name', v: '客户名单.csv', mono: true },
      { k: 'size', v: F.bytes(D.stegoPlanSample.fileLen) + '（' + F.num(D.stegoPlanSample.fileLen) + ' B）', mono: true },
      { k: 'shards', v: F.num(D.stegoPaths.length), mono: true }
    ]));
    body.appendChild(UI.alertbar({ tone: 'info', icon: 'check', title: '还原完成', text: '逐片 shard_crc32 与整体 file_len 一致。' }));
    var m = UI.modal({
      title: 'showRestored', size: 'sm', body: body,
      footer: function () { return [UI.btn({ label: '完成', variant: 'primary', title: '完成', onClick: function () { m.close(); } })]; }
    });
    return m;
  }

  function openStegoDisabledDialog() {
    var body = host('col gap-3');
    body.appendChild(UI.alertbar({
      tone: 'stale', icon: 'ban', title: 'dialog.stegoDisabled',
      text: '隐写术未启用：嵌入 / 提取接口返回 ' + F.num(7) + '（参数非法）。界面不伪造成功，也不静默降级。'
    }));
    body.appendChild(cap('启用即受审计：开关切换与每次嵌入 / 提取都会写入 vault-audit 的 security 类别。'));
    var m = UI.modal({
      title: '隐写术未启用', size: 'sm', body: body,
      footer: function () {
        return [
          UI.btn({ label: '关闭', title: '关闭', onClick: function () { m.close(); } }),
          UI.btn({ label: '前往启用', variant: 'primary', title: '前往隐写术页启用', onClick: function () { m.close(); confirmEnableStego(null); } })
        ];
      }
    });
    return m;
  }

  /* ---------------- 单图模式 ---------------- */
  function openSingleImage(kind) {
    if (!S.get('stegoEnabled')) { openStegoDisabledDialog(); return; }
    var body = host('col gap-3');
    var m;
    if (kind === 'capacity') {
      var reqInput = UI.input({ type: 'number', value: D.stegoPlanSample.perImage[0].capacity + 1 });
      a11y(reqInput, '请求字节数（用于演示超容量）');
      var result = h('div', { class: 'col gap-2' });
      function evaluate() {
        var req = parseInt(reqInput.value, 10) || 0;
        var capBytes = D.stegoPlanSample.perImage[0].capacity;
        result.innerHTML = '';
        if (req > capBytes) {
          result.appendChild(UI.alertbar({
            tone: 'danger', icon: 'ban', title: '错误码 ' + F.num(6) + ' · ' + (VS.ERR[6] ? VS.ERR[6].title : '格式错误'),
            text: 'payload exceeds image capacity'
          }));
          result.appendChild(mono('vault_core_stego_capacity 返回 ' + F.num(-6) + '（负值即错误码）· 绝不截断'));
        } else {
          result.appendChild(UI.alertbar({
            tone: 'info', icon: 'info', title: '可用容量 ' + F.bytes(capBytes),
            text: 'vault_core_stego_capacity 返回 ' + F.num(capBytes) + '（正数 = 可用字节数）'
          }));
        }
      }
      reqInput.addEventListener('input', evaluate);
      evaluate();
      body.appendChild(UI.field({ label: '请求字节数', control: reqInput, hint: '单图容量估算接口：vault_core_stego_capacity（负值即错误码，仅供 UI 预估）。' }));
      body.appendChild(result);
      body.appendChild(cap('预估结果不替代 vault_core_stego_plan：容量真值仍以 plan 为准。'));
    } else if (kind === 'embed') {
      body.appendChild(UI.kv([
        { k: '接口', v: 'vault_core_stego_embed', mono: true },
        { k: '默认 codec', v: 'png（auto 一律解析为 png）', mono: true },
        { k: '载体', v: (D.stegoPlanSample.perImage[0] || {}).path || '—', mono: true },
        { k: '可用容量', v: F.num((D.stegoPlanSample.perImage[0] || {}).capacity || 0) + ' B', mono: true }
      ]));
      body.appendChild(UI.alertbar({ tone: 'danger', icon: 'ban', title: '超容量 → 错误码 ' + F.num(6), text: 'payload exceeds image capacity · 绝不截断' }));
    } else {
      body.appendChild(UI.kv([
        { k: '接口', v: 'vault_core_stego_extract', mono: true },
        { k: '判据', v: 'flags bit1 = ' + F.num(1) + '（单图）', mono: true },
        { k: '校验', v: 'file_len + shard_crc32 终验一致后才写出', mono: true }
      ]));
      body.appendChild(UI.alertbar({ tone: 'info', icon: 'info', title: '单图提取', text: '终验不一致则删除已写出的目标文件，不留半成品。' }));
    }
    m = UI.modal({
      title: kind === 'capacity' ? '单图容量估算' : (kind === 'embed' ? '单图嵌入' : '单图提取'),
      size: 'sm', body: body,
      footer: function () { return [UI.btn({ label: '关闭', title: '关闭', onClick: function () { m.close(); } })]; }
    });
  }

  /* ---------------- 批量区 ---------------- */
  function openBatch(kind) {
    if (!S.get('stegoEnabled')) { openStegoDisabledDialog(); return; }
    var items = D.vault.entries.slice(0, 5).map(function (e, i) {
      var ok = !(i === 2) && !(i === 4);
      return {
        id: e.id, name: e.name, size: e.size, ok: ok,
        code: ok ? 0 : (i === 2 ? 6 : 12),
        note: ok ? (kind === 'embed' ? '已写出载体' : '已还原到目标路径')
          : (i === 2 ? '格式错误（载体形态不支持）· 未写出任何文件' : '会话取消 · 已取消（' + F.num(12) + '）')
      };
    });
    var body = host('col gap-3');
    body.appendChild(UI.alertbar({
      tone: 'info', icon: 'info', title: '批量 = 外壳侧顺序调用',
      text: '逐项显示成功 / 失败与错误码，不得聚合成一个总状态；失败项不影响其它项，也不回滚已成功的产物。'
    }));
    body.appendChild(UI.table({
      rows: items,
      columns: [
        { key: 'name', label: '条目' },
        { key: 'size', label: '体积', align: 'right', class: 't-mono', render: function (r) { return F.bytes(r.size); } },
        { key: 'code', label: '结果码', width: '170px', render: function (r) { return resultBadge(r.code); } },
        { key: 'note', label: '逐项说明', render: function (r) { return h('span', { class: r.ok ? 't-caption' : '', text: r.note }); } }
      ]
    }));
    body.appendChild(cap('汇总行只统计条数，不代替逐项状态。'));
    var m = UI.modal({
      title: kind === 'embed' ? '批量嵌入（顺序调用）' : '批量提取（顺序调用）',
      size: 'md', body: body,
      footer: function () { return [UI.btn({ label: '关闭', title: '关闭', onClick: function () { m.close(); } })]; }
    });
  }

  /* ======================================================================
   * 4. 全屏强告警（/alarm）
   * ==================================================================== */

  var ALARM_SOURCES = {
    audit_verify: {
      key: 'audit_verify',
      tone: 'danger',
      icon: 'shieldoff',
      conclusion: '审计链校验失败，取证能力存疑',
      reasons: [
        '链头校验返回失败，断点段边界 brokenAt = 段 ' + F.num(12),
        '断点之后的链头与条数均不可信，已停止自动修复',
        '业务写入未被阻断（审计失败不阻断业务）'
      ],
      source: '事件类型 audit_verify_failed · 接口 vault_core_audit_verify()',
      primary: { label: '立即查看审计', route: 'security', params: { tab: 'audit' } },
      secondary: { label: '导出取证', action: 'security.exportAudit' }
    },
    need_recovery: {
      key: 'need_recovery',
      tone: 'danger',
      icon: 'recover',
      conclusion: '启动恢复未完成，无法给出确定视图',
      reasons: [
        '错误码 ' + F.num(11) + '：需要恢复，启动自检未通过',
        '检测到未提交变更 ' + F.num(3) + ' 处，段 ' + F.num(12) + ' 尾部不完整（torn append）',
        '建议动作：回滚或重放，由你在恢复向导中确认'
      ],
      source: '事件类型 ENGINE_READY(1) · 接口 vault_core_recover_status() = ' + F.num(11),
      primary: { label: '前往恢复向导', route: 'recover' },
      secondary: { label: '我知道了', close: true }
    }
  };

  VS.pages['alarm'] = function (ctx) {
    ctx = ctx || { params: {}, go: null };
    currentRender = null;
    var srcKey = (ctx.params && (ctx.params.src || ctx.params.source)) || alarmSrc;
    var src = ALARM_SOURCES[srcKey] || ALARM_SOURCES.audit_verify;
    alarmSrc = src.key;

    var body = [
      UI.alertbar({
        tone: 'danger', icon: 'danger', title: '阻塞式全屏告警（/alarm）',
        text: 'danger 级事件以全屏阻塞方式呈现：无关闭按钮、Esc 无效、导航栈不可返回；必须选择一个有语义的动作才能离开。'
      }),
      UI.kv([
        { k: '当前来源', v: src.key, mono: true },
        { k: '数据来源', v: src.source, mono: true },
        { k: '幂等窗口', v: F.num(5) + ' 分钟（同源事件）', mono: true }
      ]),
      UI.card({
        title: '演示：另一种来源', sub: '审计链中断 / 启动恢复未完成（码 ' + F.num(11) + '）',
        body: h('div', { class: 'row gap-2 wrap' }, Object.keys(ALARM_SOURCES).map(function (k) {
          return UI.btn({
            label: k === 'audit_verify' ? '审计链中断' : '启动恢复未完成',
            variant: k === src.key ? 'primary' : 'default',
            title: '以「' + ALARM_SOURCES[k].key + '」为来源弹出全屏告警',
            onClick: function () { alarmSrc = k; openAlarmFullscreen(ctx, ALARM_SOURCES[k], true); }
          });
        }))
      }),
      UI.card({
        title: '同源事件 ' + F.num(5) + ' 分钟幂等',
        body: h('div', { class: 'col gap-2' }, [
          cap('同一来源事件在 ' + F.num(5) + ' 分钟内只弹一次；重复到达的事件被幂等拦截并记录，不重复打扰用户。'),
          h('div', { class: 'row gap-2 wrap' }, [
            UI.btn({
              label: '尝试重复触发（应被拦截）', title: '演示幂等：重复触发同源事件应被拦截',
              onClick: function () {
                var last = alarmSeen[src.key];
                var now = Date.now();
                if (last && now - last < ALARM_IDEMPOTENT_MS) {
                  UI.toast({
                    tone: 'info', title: '已被幂等拦截',
                    msg: '同源事件 ' + src.key + ' 距上次弹出 ' + F.num(Math.round((now - last) / 60000)) + ' 分钟，未再次弹出'
                  });
                } else {
                  UI.toast({ tone: 'warn', title: '未命中幂等窗口', msg: '距上次弹出已超过 ' + F.num(5) + ' 分钟，将再次弹出' });
                }
              }
            }),
            UI.btn({ label: '重置幂等记录', title: '清空幂等记录（演示）', onClick: function () { alarmSeen = {}; UI.toast({ tone: 'info', title: '已重置', msg: '幂等记录已清空（演示）' }); } })
          ])
        ])
      })
    ];

    /* 页面挂载时按幂等规则弹出阻塞式全屏（同源 5 分钟内不重复弹出） */
    if (!alarmActive) openAlarmFullscreen(ctx, src, false);

    return pageShell('全屏强告警', '/alarm · 阻塞式 · 语义化动作 · 同源事件 ' + F.num(5) + ' 分钟幂等', [], body);
  };

  function openAlarmFullscreen(ctx, src, force) {
    var now = Date.now();
    var last = alarmSeen[src.key];
    if (!force && last && now - last < ALARM_IDEMPOTENT_MS) {
      UI.toast({ tone: 'info', title: '同源事件已幂等拦截', msg: src.key + '：距上次弹出 ' + F.num(Math.round((now - last) / 60000)) + ' 分钟，不再重复弹出' });
      return null;
    }
    alarmSeen[src.key] = now;
    if (alarmActive) { alarmActive.close(); alarmActive = null; }

    var content = host('col gap-3');
    content.appendChild(h('span', { style: { color: 'var(--c-danger)' }, html: VS.icon(src.icon, 56) }));
    content.appendChild(h('div', { class: 't-display', text: src.conclusion }));
    src.reasons.slice(0, 3).forEach(function (r) {
      content.appendChild(h('div', { class: 't-body', text: r }));
    });
    content.appendChild(h('div', { class: 't-caption t-mono', text: '数据来源：' + src.source }));
    content.appendChild(h('div', { class: 't-caption', text: '同源事件 ' + F.num(5) + ' 分钟内幂等 · 无「点任意处关闭」' }));

    var primary = UI.btn({
      label: src.primary.label, variant: 'danger', size: 'lg', title: src.primary.label,
      onClick: function () {
        if (alarmActive) { alarmActive.close(); alarmActive = null; }
        if (src.primary.route) go(ctx, src.primary.route, src.primary.params);
        if (src.primary.action && VS.actions[src.primary.action]) VS.actions[src.primary.action](ctx);
      }
    });
    var secondary = UI.btn({
      label: src.secondary.label, size: 'lg', title: src.secondary.label,
      onClick: function () {
        if (alarmActive) { alarmActive.close(); alarmActive = null; }
        if (src.secondary.route) go(ctx, src.secondary.route);
        if (src.secondary.action && VS.actions[src.secondary.action]) VS.actions[src.secondary.action](ctx);
      }
    });
    content.appendChild(h('div', { class: 'row gap-3', style: { marginTop: '8px' } }, [primary, secondary]));

    alarmActive = UI.fullscreen({ tone: 'danger', content: content });
    return alarmActive;
  }

  /* ======================================================================
   * 5. 动作注册（外壳 / 命令面板 / 通知可调用）
   *    签名统一为 function (ctx)，ctx 可省略
   * ==================================================================== */
  VS.actions['security.scanIntegrity'] = function (ctx) { openFullScan(ctx || { params: {}, go: null }); };
  VS.actions['security.verifyAudit'] = function (ctx) { openAuditVerify(ctx || { params: {}, go: null }); };
  VS.actions['security.exportAudit'] = function (ctx) { exportAuditBundle(ctx || { params: {}, go: null }); };
  VS.actions['security.demoPanel'] = function () { openDemoPanel(); };
  VS.actions['security.destroy'] = function (ctx) {
    secTab = 'destroy'; S.set({ secTab: 'destroy' }, true);
    go(ctx || {}, 'security');
    rerender();
  };
  VS.actions['alarm.open'] = function (ctx) {
    ctx = ctx || { params: {}, go: null };
    var key = (ctx.params && (ctx.params.src || ctx.params.source)) || alarmSrc;
    var src = ALARM_SOURCES[key] || ALARM_SOURCES.audit_verify;
    openAlarmFullscreen(ctx, src, true);
  };
  VS.actions['stego.enable'] = function () { if (!S.get('stegoEnabled')) confirmEnableStego(null); };
  VS.actions['stego.disable'] = function () { if (S.get('stegoEnabled')) confirmDisableStego(null); };
  VS.actions['stego.embed'] = function (ctx) {
    ctx = ctx || { params: {}, go: null };
    if (!S.get('stegoEnabled')) { openStegoDisabledDialog(); return; }
    go(ctx, 'stego');
    openEmbedWizard(ctx);
  };
  VS.actions['stego.extract'] = function (ctx) {
    ctx = ctx || { params: {}, go: null };
    if (!S.get('stegoEnabled')) { openStegoDisabledDialog(); return; }
    go(ctx, 'stego');
    openExtractWizard(ctx);
  };
  VS.actions['stego.capacity'] = function () { openSingleImage('capacity'); };

})(window);
