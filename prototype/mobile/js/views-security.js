/* ============================================================================
 * VaultSync V2.0 原型 · 移动端视图：安全中心 / 审计详情 / 隐写术 / 全屏强告警
 * 纯静态、零依赖、经典脚本（不使用 ES Module / fetch / 外部库）
 *
 * 与桌面端 ../v2.0/js/views-security.js：同一套内核与数据（core/ui/data），只换外壳与布局。
 * 移动端形态依据 docs/v2.0/03 §2.2 / §4.3、docs/v2.0/04 §1.2 / §三.5 / §六：
 *   · 审计日志在手机上为「卡片流」（不是完整表格）
 *   · 二级 / 三级界面一律 MUI.sheet / actionSheet / confirmSheet（禁止 UI.modal）
 *   · 列表项长按 → MUI.actionSheet；危险全屏 → MUI.mfullscreen（dismissible:false）
 *   · 触控目标 ≥ 44×44；安全中心显示「本端能力」（截屏保护 FLAG_SECURE / iOS 遮罩）
 *
 * 注册契约：
 *   VS.pages['security'|'audit'|'stego'|'alarm'] = function (ctx) -> MUI.page
 *   VS.actions['security.*'|'stego.*'|'alarm.open'] = function (ctx)
 *
 * 语料：docs/v2.0/04 §三.5；05-02 §三.4-.5 / §七.1；05-05；09 P6-6 / P7-5 / P7-10 / P7-11 / P8-10 / P9-5。
 *
 * 纪律（逐条可在界面上验收）：
 *   - 无数据一律灰：unknown / design 走 stale 灰，不显示 0、不显示绿
 *   - 「安全擦除」字样仅在 erase_class = platform_secure 且 method_bits 含平台安全删除位时出现
 *   - crypto_only / unknown 文案必须包含「介质覆写未执行」
 *   - 物理删除失败 → 「介质擦除待完成」，不得显示「已擦除」；引擎原值直出
 *   - 容量真值只来自 vault_core_stego_plan，UI 不自行计算容量
 *   - 只读降级：写操作 disabled + reason（VS.derive.denyWrite()）
 *   - 维护态例外：「擦除 / 销毁」是维护态下唯一被允许的写操作
 *   - 能力三档：未置位 → 整块不渲染（返回 null）；其次禁用并写明原因
 *   - 审计连续写入失败 → 灰态「审计不可用」，不阻断业务，也不声称「审计通过」
 * ==========================================================================*/
(function (global) {
  'use strict';

  var VS = global.VS;
  var h = VS.util.h, U = VS.util, UI = VS.ui, MUI = VS.mui, F = VS.fmt, D = VS.data, S = VS.store;

  VS.pages = VS.pages || {};
  VS.actions = VS.actions || {};

  /* ======================================================================
   * 0. 局部原语
   * ==================================================================== */
  function cap(t) { return h('div', { class: 't-caption', text: t }); }
  function strong(t) { return h('div', { class: 't-strong', text: t }); }
  function mono(t) { return h('div', { class: 't-mono', text: t }); }
  function col(children, gap) { return h('div', { class: 'col' + (gap ? ' gap-' + gap : '') }, children); }
  function grid2(children) {
    return h('div', { style: { display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(150px, 1fr))', gap: '10px' } }, children);
  }
  /** 无障碍兜底：所有可交互元素必须有 title / aria-label */
  function a11y(node, label) {
    if (node && label && node.setAttribute) {
      node.setAttribute('title', label);
      if (!node.hasAttribute('aria-label')) node.setAttribute('aria-label', label);
    }
    return node;
  }
  function a11ySelect(node, label) {
    var b = node && node.querySelector ? node.querySelector('.select-btn') : null;
    if (b) { b.setAttribute('title', label); b.setAttribute('aria-label', label); }
    return node;
  }
  function a11yRadio(node, options) {
    var cards = node && node.querySelectorAll ? node.querySelectorAll('.radio-card') : [];
    for (var i = 0; i < cards.length; i++) {
      var o = options[i] || {};
      var t = (o.title || '') + (o.desc ? ' · ' + o.desc : '') + (o.disabled && o.reason ? ' · 不可用：' + o.reason : '');
      cards[i].setAttribute('title', t);
      cards[i].setAttribute('aria-label', t);
    }
    return node;
  }
  /** 整宽按钮：触控目标 >= 44x44（mbtn-block 最小高度 48px） */
  function mbtn(label, opts) {
    opts = opts || {};
    var dis = !!opts.disabled;
    return a11y(h('button', {
      class: 'mbtn-block', type: 'button',
      dataset: { variant: opts.variant || 'ghost' },
      disabled: dis,
      style: opts.flex ? { width: 'auto', flex: opts.flex } : (opts.style || null),
      onclick: dis ? null : opts.onClick
    }, [
      opts.icon ? h('span', { html: VS.icon(opts.icon, 17) }) : null,
      h('span', { text: label })
    ]), opts.title || (label + (dis && opts.reason ? ' 不可用 · ' + opts.reason : '')));
  }

  /* --- 状态三档：unknown / design 一律灰，不显示 0、不显示绿 --- */
  var STATE_META = {
    ok: { tone: 'success', mark: 'OK', label: '通过' },
    warn: { tone: 'warning', mark: '!', label: '注意' },
    danger: { tone: 'danger', mark: 'X', label: '异常' },
    unknown: { tone: 'stale', mark: '?', label: '无数据' },
    design: { tone: 'stale', mark: '~', label: '设计保证' }
  };
  function normState(s) { return STATE_META[s] ? s : 'unknown'; }
  function stateBadge(s) {
    var st = normState(s), meta = STATE_META[st];
    if (st === 'design') return UI.badge({ text: meta.label, design: true, title: '契约清单中无此接口：按设计保证标注，不代表运行时已验证' });
    if (st === 'unknown') return UI.badge({ text: meta.label, tone: 'stale', title: '接口未返回数据：灰态展示，不显示 0 条、不显示为通过' });
    return UI.badge({ text: meta.label, tone: meta.tone });
  }
  function errText(code) {
    var e = VS.ERR[code] || {};
    return '错误码 ' + F.num(code) + ' · ' + (e.title || '不可用') + (e.hint ? '：' + e.hint : '');
  }
  function resultBadge(code) {
    var e = VS.ERR[code];
    if (!e) return UI.badge({ text: '-', tone: 'stale', title: '未记录结果码' });
    return UI.badge({ text: F.num(code) + ' · ' + e.title, tone: e.tone === 'maintenance' ? 'maintenance' : e.tone, title: e.hint || e.title });
  }

  /* --- 写闸门（维护态例外见 denyErase） --- */
  function denyWrite() { var c = VS.derive.denyWrite(); return c === null ? null : { code: c, text: errText(c) }; }
  /** 维护态例外：擦除 / 销毁 是维护态下唯一被允许的写操作（05-02 §三.5 / 09 P6-6） */
  function denyErase() {
    if (S.get('lock') !== 'Unlocked') return { code: 5, text: errText(5) };
    if (S.get('readOnly')) return { code: 9, text: errText(9) };
    return null;
  }
  function inMaintenance() { return !!S.get('maintenance') || S.get('session') === 'Maintenance'; }
  function statusBanners(opts) {
    opts = opts || {};
    var out = [];
    if (S.get('readOnly')) {
      out.push(MUI.limitedNote('只读降级（' + errText(9) + '）：写操作已禁用并写明原因。仍可用：' + VS.derive.readOnlyAllowed.join(' / ')));
    }
    if (inMaintenance()) {
      out.push(MUI.maintBanner('maintenance', '维护态：业务写入冻结（' + errText(10) + '）',
        opts.eraseException === false
          ? '导入 / 编辑 / 重命名 / 打标签 / 删除 / 分享 / 同步 / 轮换 / 迁移 均被拒绝。'
          : '维护态下「擦除 / 销毁」是唯一被允许的写操作（用户要求销毁高于数据结构一致）；其余写入冻结。'));
    }
    return out;
  }
  function go(ctx, route, params) {
    if (ctx && typeof ctx.go === 'function') { ctx.go(route, params); return true; }
    UI.toast({ tone: 'info', title: '导航未接线', msg: '外壳未提供 ctx.go，无法跳转到 ' + route });
    return false;
  }
  /** 页面重建钩子：各页在渲染时注册，供演示开关切换后原地重绘 */
  var rebuild = null;
  function attr() { if (typeof rebuild === 'function') rebuild(); }
  function makeRebuild(factory, ctx) {
    return function () {
      if (!VS.paint) return;
      try { VS.paint(factory(ctx), { params: (ctx && ctx.params) || {}, scrollTop: 0, route: (ctx && ctx.route) || '' }); }
      catch (e) { console.error('[VS] 重绘失败', e); }
    };
  }
  function demoToggle(label, sub, checked, onChange) {
    var sw = UI.switchCtl({ checked: checked, sub: sub, onChange: function () { onChange(!!sw.input.checked); } });
    return h('div', { class: 'row-between', style: { minHeight: '44px' } }, [
      h('div', { class: 'grow' }, [strong(label), sub ? cap(sub) : null]), sw
    ]);
  }

  /* ======================================================================
   * 1. 模块内状态
   * ==================================================================== */
  var BROKEN_HEAD = D.hash(64);                 /* 演示用「断点后的链头」 */
  var demo = {
    auditBroken: false,                         /* 审计链校验失败 -> danger + brokenAt */
    auditDown: false,                           /* 审计连续失败 -> 灰态「审计不可用」 */
    physicalDeleteFailed: false,                /* 物理删除失败 -> 介质擦除待完成 */
    maskPlatformBit: false,                     /* 去掉平台安全删除位 -> 禁止「安全擦除」字样 */
    clockSkew: false,                           /* 系统时钟异常（超前 > 24h） */
    stegoLegacy: false, stegoNewer: false       /* 兼容读两态 */
  };
  var auditFilter = { cat: 'all', result: 'all' };
  var destroyState = { scope: 'vault', delay: 'now', customAt: '', expiresDays: 7, clockAck: false };
  var embedWiz = null, extractWiz = null, embedTimer = null;
  var alarmSeen = {}, alarmActive = null, alarmSrc = 'audit_verify';
  var ALARM_IDEMPOTENT_MS = 5 * 60 * 1000;

  /* ======================================================================
   * 2. 安全中心（/security）· Tab · 四个分段用 MUI.mseg
   * ==================================================================== */
  var SEC_TABS = [
    { value: 'checks', label: '检测项', icon: 'gauge' },
    { value: 'erase', label: '擦除强度', icon: 'erase' },
    { value: 'audit', label: '审计', icon: 'history' },
    { value: 'destroy', label: '紧急销毁', icon: 'danger' }
  ];

  var securityPage = function (ctx) {
    ctx = ctx || { params: {}, go: null };
    var want = ctx.params && ctx.params.tab;
    var tab = SEC_TABS.some(function (t) { return t.value === want; }) ? want
      : (SEC_TABS.some(function (t) { return t.value === S.get('secTab'); }) ? S.get('secTab') : 'checks');
    if (tab !== S.get('secTab')) S.set({ secTab: tab }, true);
    rebuild = makeRebuild(securityPage, ctx);

    var seg = MUI.mseg({
      value: tab, items: SEC_TABS,
      onChange: function (v) {
        S.set({ secTab: v }, true);
        try {
          VS.paint(securityPage({ params: { tab: v }, go: ctx.go }), { params: { tab: v }, scrollTop: 0, route: 'security' });
        } catch (e) { console.error('[VS] 切段失败', e); }
      }
    });

    var body = [seg];
    if (tab === 'checks') body = body.concat(secChecks(ctx));
    else if (tab === 'erase') body = body.concat(secErase());
    else if (tab === 'audit') body = body.concat(secAudit(ctx));
    else body = body.concat(secDestroy(ctx));

    return MUI.page({
      title: '安全中心',
      sub: '本端：移动装置 · 截屏保护见「本端能力」',
      back: false,
      actions: [{ icon: 'history', label: '审计', onClick: function () { go(ctx, 'security', { tab: 'audit' }); } }],
      overflow: [
        { label: '校验审计链', sub: 'vault_core_audit_verify()', icon: 'link', onClick: function () { openAuditVerify(ctx); } },
        { label: '导出加密审计包', sub: '只读 · 能力位受档位约束', icon: 'external', onClick: function () { exportAuditBundle(ctx); } },
        { label: '全库完整性扫描', sub: '可取消（码 ' + F.num(12) + ' 视为正常）', icon: 'layers', onClick: function () { openFullScan(ctx); } },
        { label: '演示控制台', sub: '只读 / 维护态 / 审计失败 / 时钟异常', icon: 'sparkle', onClick: function () { openDemoSheet(); } },
        { label: '前往隐写术', sub: '进入前需启用', icon: 'stego', onClick: function () { go(ctx, 'stego'); } }
      ],
      body: MUI.screen(body),
      tab: true, fab: null, onBack: null
    });
  };
  VS.pages['security'] = securityPage;

  /* ---------------- 段 1 · 检测项 ---------------- */
  function secChecks(ctx) {
    var out = statusBanners();
    if (demo.auditDown) {
      out.push(UI.alertbar({
        tone: 'stale', icon: 'ban', title: '审计不可用',
        text: '审计链连续写入失败：进入灰态。既不再声称「审计通过」，也不显示 ' + F.num(0) +
          ' 条；业务写入不被阻断（审计失败不阻断业务），但取证能力需人工介入。'
      }));
    }

    /* 本端能力：截屏保护（手机上支持，不照抄桌面的「不适用」） */
    var ss = (S.get('settings') && S.get('settings').screenshotProtect !== undefined) ? S.get('settings').screenshotProtect : true;
    out.push(MUI.mcard({
      title: '本端能力 · 截屏保护',
      sub: '本端增配 / 减配项必须逐条如实声明',
      body: col([
        MUI.mrow({
          icon: 'shield', chevron: false,
          title: ss ? '已启用 · 系统级截屏阻断' : '未启用',
          sub: 'Android：FLAG_SECURE（窗口级）；iOS：切后台遮罩 + 截屏事件回调',
          trail: [UI.badge({ text: '本端支持', tone: ss ? 'success' : 'stale', title: '平台提供系统级截屏保护接口：运行时生效，非设计保证' })]
        }),
        MUI.mrow({
          icon: 'info', chevron: false,
          title: '来源接口',
          sub: 'platform_secure_window.setSecure(true) · 不支持的机型回落为切后台遮罩',
          trail: [UI.tierMark('real')]
        }),
        cap('与本端减配项区分：后台同步尽力而为、入站监听仅前台、配对以扫码为主 —— 这些是外壳固有约束，与本行能力无关。')
      ], 2)
    }));

    /* 六格：每格必须有来源接口名 + 能力三档标记 */
    out.push(MUI.mcard({
      title: '检测项 · ' + F.num(D.securityChecks.length) + ' 项',
      sub: '每格标注来源接口名；未知 / 设计保证一律灰',
      body: grid2(D.securityChecks.map(function (c) {
        var state = normState(c.state), note = c.note;
        if (c.key === 'audit' && demo.auditBroken) {
          state = 'danger';
          note = '链头校验失败 · 断点段边界 brokenAt = 段 ' + F.num(12) + ' · 断点后的条数不可信';
        } else if (c.key === 'audit' && demo.auditDown) {
          state = 'unknown'; note = '审计不可用：连续写入失败（灰态，不显示条数）';
        }
        return UI.gauge({
          icon: c.icon, name: c.name, state: state, note: note, src: '来源：' + c.src,
          badge: h('div', { class: 'row gap-1' }, [
            stateBadge(state),
            UI.tierMark(c.state === 'design' ? 'design' : (c.state === 'unknown' ? 'design' : 'real'))
          ])
        });
      }))
    }));

    out.push(MUI.mcard({
      title: '校验与扫描', sub: '两者均为只读操作：只读降级下不置灰',
      body: MUI.mlist([
        MUI.mrow({ icon: 'link', title: '校验审计链', sub: 'vault_core_audit_verify() · 失败不自动修复，先取证', onClick: function () { openAuditVerify(ctx); } }),
        MUI.mrow({ icon: 'layers', title: '全库完整性扫描', sub: 'vault_core_verify_all -> TASK_* · 可取消', onClick: function () { openFullScan(ctx); } })
      ])
    }));

    if (demo.auditBroken) {
      out.push(UI.alertbar({
        tone: 'danger', icon: 'shieldoff', title: '审计链校验失败 · 段边界 brokenAt = 段 ' + F.num(12),
        text: '断点之后的链头与条数均不可信。按纪律不自动修复、不重建链、不静默截断：先取证再处置。',
        actions: [
          mbtn('导出审计取证', { variant: 'danger', flex: '1 1 130px', title: '导出审计取证包（只读）', onClick: function () { exportAuditBundle(ctx); } }),
          mbtn('检查磁盘', { flex: '1 1 110px', title: '检查磁盘可写性与剩余空间', onClick: checkDisk })
        ]
      }));
    } else {
      out.push(cap('审计链状态来自 vault_core_audit_verify() 的返回值（链头 + 条数）；本页不缓存、不推断。'));
    }

    out.push(MUI.mcard({
      title: '校验失败的语义', sub: '为什么「不自动修复」',
      body: col([
        cap('链头校验只证明链自洽，不证明记录未被整体删除；条数由接口原值给出。'),
        cap('一旦发现断点，自动重建链会覆盖取证现场，因此只提供「导出审计取证 / 检查磁盘」两条人工路径。'),
        cap('只读降级下审计校验与导出仍可用（VS.derive.readOnlyAllowed）。')
      ], 2)
    }));
    return out;
  }
  function checkDisk() {
    UI.toast({ tone: 'warn', title: '磁盘检查', msg: 'vault_core_diagnostics().disk 写入失败（演示）· 段 ' + F.num(12) + ' 尾部不完整' });
  }

  /* ---------------- 校验审计链（sheet） ---------------- */
  function openAuditVerify(ctx) {
    var ok = !demo.auditBroken && !demo.auditDown;
    var head = demo.auditBroken ? BROKEN_HEAD : D.auditHead;
    var body = col([
      UI.alertbar({
        tone: ok ? 'info' : 'danger', icon: ok ? 'check' : 'shieldoff',
        title: ok ? '链头校验通过' : '链头校验失败',
        text: ok ? '仅证明链自洽；条数与链头均由接口原值直出，本页不做推断。'
          : '断点段边界 brokenAt = 段 ' + F.num(12) + '；不自动修复，请先导出取证。'
      }),
      MUI.mkv([
        { k: '链头 head', v: U.shortHash(head, 16, 8), mono: true },
        { k: '条数 count', v: F.num(D.auditLog.length), mono: true },
        { k: 'verified', v: String(ok), mono: true },
        { k: 'brokenAt', v: demo.auditBroken ? ('段 ' + F.num(12)) : '—（无断点）', mono: true },
        { k: '来源接口', v: 'vault_core_audit_verify()', mono: true }
      ]),
      cap('校验为只读操作：只读降级下仍可用，不置灰。')
    ], 3);
    if (!ok) {
      body.appendChild(h('div', { class: 'row gap-2 wrap' }, [
        mbtn('导出审计取证', { variant: 'danger', flex: '1 1 130px', title: '导出审计取证包（只读）', onClick: function () { exportAuditBundle(ctx); } }),
        mbtn('检查磁盘', { flex: '1 1 110px', title: '检查磁盘可写性与剩余空间', onClick: checkDisk })
      ]));
    }
    MUI.sheet({
      title: '校验审计链', sub: 'vault_core_audit_verify()', size: 'tall', body: body,
      footer: function (close) { return [mbtn('关闭', { onClick: close, title: '关闭校验面板' })]; }
    });
  }

  /* ---------------- 全库完整性扫描（ring + bar + 取消 -> 码 12） ------- */
  function openFullScan(ctx) {
    var total = D.vault.entries.length || 1, done = 0, failed = 0, timer = null, finished = false;
    var ringHost = h('div', { style: { display: 'grid', placeItems: 'center' } });
    var barHost = h('div');
    var stat = h('div', { class: 't-caption' });
    var cur = h('div', { class: 't-mono', text: '等待调度…' });
    var tail = col([]);
    function paintRing(p, label) {
      ringHost.innerHTML = '';
      ringHost.appendChild(UI.ring({ pct: p, size: 96, stroke: 8, tone: 'accent', label: label }));
    }
    function paintBar(p, tone) { barHost.innerHTML = ''; barHost.appendChild(UI.bar(p, tone)); }
    function refresh() { stat.textContent = '已检查 ' + F.num(done) + ' / ' + F.num(total) + ' 项 · 失败 ' + F.num(failed) + ' 项'; }
    paintRing(0, F.pct(0)); paintBar(0, 'accent'); refresh();
    function stop() { if (timer) { clearInterval(timer); timer = null; } }

    MUI.sheet({
      title: '全库完整性扫描', sub: 'vault_core_verify_all -> TASK_*（只读）', size: 'tall', dismissible: false,
      body: col([ringHost, barHost, stat, cur, tail,
        cap('扫描为只读操作：校验块 CRC 与 chunk 计数，不修改任何文件；失败项不自动修复、不自动重试。')], 3),
      footer: function (close) {
        return [
          mbtn('转后台', { title: '关闭面板，扫描继续（进度见队列页）', onClick: function () { close(); UI.toast({ tone: 'info', title: '已转后台', msg: '任务进度可在队列页查看（演示）' }); } }),
          mbtn('取消扫描', { variant: 'danger', title: '取消扫描：返回码 ' + F.num(12) + '（已取消），视为正常结果', onClick: function () { stop(); close(); showCancelled('全库完整性扫描'); } })
        ];
      }
    });

    timer = setInterval(function () {
      if (finished) return;
      var item = D.vault.entries[done % total] || { name: '—' };
      done += 1;
      if (done % 5 === 0 && failed < 2) failed += 1;
      cur.textContent = '当前：' + item.name;
      var p = Math.min(1, done / total);
      paintRing(p, F.pct(p)); paintBar(p, failed ? 'danger' : 'accent'); refresh();
      if (done >= total) {
        finished = true; stop();
        paintRing(1, failed ? '有空' : '通过');
        tail.appendChild(UI.alertbar({
          tone: failed ? 'warn' : 'info', icon: failed ? 'alert' : 'check',
          title: failed ? ('扫描完成 · 失败 ' + F.num(failed) + ' 项') : '扫描完成 · 未发现失败项',
          text: failed ? '失败项需人工处置：可导出取证或检查磁盘；不自动修复、不自动重试。' : '全部块的 CRC 与 chunk 计数和索引一致。'
        }));
      }
    }, 240);
  }
  function showCancelled(what) {
    UI.toast({ tone: 'info', title: '已取消', msg: what + '：' + errText(12) + '（视为正常结果，不报错、不计失败）', duration: 4200 });
  }

  /* ---------------- 导出加密审计包（sheet） ---------------- */
  function exportAuditBundle(ctx) {
    var feat = D.licenseState && D.licenseState.features ? D.licenseState.features.feat_audit_export : false;
    if (!feat) {
      MUI.sheet({
        title: '导出加密审计包', sub: '能力未授权', size: 'tall',
        body: col([
          UI.alertbar({
            tone: 'maintenance', icon: 'award', title: '能力未授权：审计链加密导出',
            text: '当前档位（' + D.licenseState.tier + '）未开通 feat_audit_export。按证据先行原则：能力位未置位时置灰并说明原因，不假装导出成功。'
          }),
          cap('本地审计查看 / 校验 不受该能力位限制，仍可用；授权档位说明见授权与订阅页。')
        ], 3),
        footer: function (close) {
          return [
            mbtn('前往授权与订阅', { variant: 'primary', title: '前往授权与订阅页', onClick: function () { close(); go(ctx, 'license'); } }),
            mbtn('关闭', { onClick: close })
          ];
        }
      });
      return;
    }
    var line = 'vs-audit-bundle v1 · 条数 ' + F.num(D.auditLog.length) + ' · 链头 ' + U.shortHash(D.auditHead, 12, 8) +
      ' · 加密 HKDF(MK,"audit-export") + AEAD';
    MUI.sheet({
      title: '导出加密审计包', sub: 'feat_audit_export = true', size: 'tall',
      body: col([
        mono(line), UI.copyField(line, '复制导出载荷说明'),
        cap('导出为只读操作：只读降级下仍可用；导出内容不含目标路径、不含文件名原文、不含查询词。')
      ], 3),
      footer: function (close) {
        return [mbtn('完成', { variant: 'primary', title: '完成导出', onClick: function () { close(); UI.toast({ tone: 'success', title: '已生成', msg: '审计包已写出到外部路径（演示）' }); } })];
      }
    });
  }

  /* ---------------- 段 2 · 擦除强度 ---------------- */
  var ERASE_SAMPLES = [
    { cls: 1, bits: '0b0001', media: 1, key: 'crypto_only' },
    { cls: 2, bits: '0b0100', media: 2, key: 'zero_overwrite' },
    { cls: 3, bits: '0b0011', media: 3, key: 'platform_secure' },
    { cls: 4, bits: '0b0000', media: 4, key: 'unknown' }
  ];
  /** method_bits 的第 1 位（bit1）= 平台安全删除位 */
  function bitsHasPlatformSecure(bits) {
    var v = parseInt(String(bits).replace(/^0b/i, ''), 2);
    return !isNaN(v) && (v & 2) !== 0;
  }
  /* data.js 未提供 degradations[] 字段（见汇报）：按 05-02 §三.4 口径在本文件内定义 */
  var DEGRADATIONS = [
    { code: 'ERASE_MEDIA_UNKNOWN', tone: 'warning', text: '介质类型无法判定（media_kind = unknown）-> 介质覆写未执行' },
    { code: 'ERASE_METHOD_BITS_EMPTY', tone: 'warning', text: 'method_bits = 0b0000：未置位任何删除指令 -> 仅完成密钥销毁' },
    { code: 'ERASE_PLATFORM_UNSUPPORTED', tone: 'stale', text: '平台不支持安全删除（文件系统无对应接口）-> 降级为零覆写或仅加密擦除' },
    { code: 'ERASE_PHYSICAL_PENDING', tone: 'danger', text: '物理删除失败 / 待完成 -> 显示「介质擦除待完成」，不得显示「已擦除」' }
  ];
  function eraseSampleBits(s) { return (demo.maskPlatformBit && s.cls === 3) ? '0b0001' : s.bits; }
  function eraseSampleCell(s) {
    var bits = eraseSampleBits(s), secure = s.cls === 3 && bitsHasPlatformSecure(bits), badge, note;
    if (secure) badge = UI.eraseBadge(3);
    else if (s.cls === 3) badge = UI.badge({ text: '加密擦除（平台安全删除位未置位）', tone: 'warning', title: 'erase_class = platform_secure 但 method_bits 未含平台安全删除位：按纪律不得显示「安全擦除」' });
    else badge = UI.eraseBadge(s.cls);

    if (s.cls === 1) note = 'crypto_only：已加密擦除；介质覆写未执行。';
    else if (s.cls === 2) note = 'zero_overwrite：已执行单轮零覆写。';
    else if (s.cls === 3 && secure) note = 'platform_secure：平台安全删除指令已执行（method_bits 含平台安全删除位）。';
    else if (s.cls === 3) note = '引擎原值 = platform_secure，但方法位缺失：介质覆写未执行，且不得显示「安全擦除」。';
    else note = 'unknown：无法判定介质类型；介质覆写未执行。';

    return MUI.mcard({
      title: UI.ERASE_CLASS[s.cls] ? UI.ERASE_CLASS[s.cls].label : '无法判定',
      sub: 'erase_class = ' + F.num(s.cls) + ' · ' + s.key,
      body: col([
        h('div', { class: 'row-between' }, [badge, h('span', { class: 't-mono', text: s.key })]),
        mono('method_bits = ' + bits + ' · media_kind = ' + F.num(s.media)),
        cap(note)
      ], 2)
    });
  }
  function secErase() {
    /* 能力三档：CAP_ERASE_CLASS 未置位 -> 整块不渲染（返回 null） */
    if (S.get('caps').CAP_ERASE_CLASS === false) {
      return [MUI.empty({
        icon: 'ban', title: '擦除强度探测未置位',
        desc: '能力位 CAP_ERASE_CLASS（bit ' + F.num(VS.CAP.CAP_ERASE_CLASS.bit) + '）未置位：本端不渲染擦除强度区，也不以任何形式推断擦除等级。'
      })];
    }
    var out = [UI.alertbar({
      tone: 'warn', icon: 'erase', title: '硬约束：擦除强度按引擎原值直出',
      text: '「安全擦除」字样仅在 erase_class = platform_secure 且 method_bits 含平台安全删除位时出现；' +
        'crypto_only / unknown 文案必须包含「介质覆写未执行」；物理删除失败显示「介质擦除待完成」，不得显示「已擦除」。'
    })];

    out.push(MUI.mcard({
      title: '四种取值各展示一次', sub: '引擎原值直出 · 不做美化 · 不把 unknown 说成「安全擦除」',
      actions: [mbtn(demo.maskPlatformBit ? '恢复方法位' : '去掉平台安全位', {
        style: { width: 'auto', minHeight: '36px', padding: '0 10px', fontSize: '12px' },
        title: '演示：切换 method_bits 中平台安全删除位',
        onClick: function () { demo.maskPlatformBit = !demo.maskPlatformBit; attr(); }
      })],
      body: grid2(ERASE_SAMPLES.map(eraseSampleCell))
    }));

    out.push(MUI.mcard({
      title: '降级条目（degradations[]）示例', sub: '引擎返回降级原因时逐条列出，而不是折叠成一句「已擦除」',
      body: col(DEGRADATIONS.map(function (d) {
        return h('div', { class: 'row gap-2', style: { alignItems: 'flex-start' } }, [
          UI.badge({ text: d.tone === 'danger' ? '待完成' : '降级', tone: d.tone, title: d.code }),
          h('div', { class: 'grow' }, [mono(d.code), cap(d.text)])
        ]);
      }), 3)
    }));

    var listHost = h('div');
    function paintList() { listHost.innerHTML = ''; listHost.appendChild(pendingCleanupList()); }
    paintList();
    out.push(MUI.mcard({
      title: '待清理条目', sub: 'D.deletedEntries · 误删保护区（24 h）与介质擦除状态分列展示',
      actions: [mbtn('演示：物理删除失败', {
        style: { width: 'auto', minHeight: '36px', padding: '0 10px', fontSize: '12px' },
        title: '切换物理删除失败演示（置为「介质擦除待完成」）',
        onClick: function () { demo.physicalDeleteFailed = !demo.physicalDeleteFailed; paintList(); }
      })],
      body: col([listHost,
        cap('「介质擦除待完成」= 密钥已销毁但介质覆写未完成：界面永不显示「已擦除」；清理动作属写操作，受写闸门约束，但维护态下擦除仍被允许。'),
        cap('长按条目可弹出动作面板（移动端形态）。')], 3)
    }));
    return out;
  }
  function pendingCleanupList() {
    var rows = D.deletedEntries.map(function (d, i) {
      var pending = !!d.pendingCleanup || (demo.physicalDeleteFailed && i === 0);
      return MUI.mrow({
        icon: 'trash', danger: pending, chevron: false, title: d.name,
        sub: F.bytes(d.size) + ' · 删除于 ' + F.dateLong(d.deletedMs) + ' · erase_class=' + F.num(d.eraseClass) + ' · media_kind=' + F.num(d.mediaKind),
        trail: [pending
          ? UI.badge({ text: '介质擦除待完成', tone: 'danger', title: '物理删除未完成（待清理）：不得显示「已擦除」' })
          : UI.badge({ text: '已加密擦除', tone: 'warning', title: '已加密擦除；介质覆写未执行' })],
        onLongPress: function () { pendingActions(d, pending); }
      });
    });
    return rows.length ? MUI.mlist(rows)
      : MUI.empty({ icon: 'trash', title: '没有待清理条目', desc: '删除后进入保护区的条目会出现在这里' });
  }
  /** 列表项长按 -> MUI.actionSheet（移动端形态） */
  function pendingActions(d, pending) {
    var denied = denyErase();
    MUI.actionSheet({
      title: d.name, sub: pending ? '介质擦除待完成' : '已加密擦除（介质覆写未执行）',
      items: [
        { group: '条目操作' },
        {
          label: '立即完成介质擦除', icon: 'erase', danger: pending, sub: pending ? '把待完成项推进到完成' : '当前无需处理',
          disabled: !pending || !!denied, reason: denied ? ('写操作被拒绝：' + denied.text) : '介质擦除尚未待完成',
          onClick: function () { UI.toast({ tone: 'info', title: '已排队', msg: '演示：介质擦除任务已入队' }); }
        },
        {
          label: '查看条目详情', icon: 'info',
          onClick: function () {
            MUI.sheet({
              title: '待清理条目详情', sub: 'seq ' + F.num(d.id), size: 'tall',
              body: MUI.mkv([
                { k: '名称', v: d.name },
                { k: '体积', v: F.bytes(d.size) + '（' + F.num(d.size) + ' B）', mono: true },
                { k: '删除时间', v: F.dateLong(d.deletedMs), mono: true },
                { k: 'erase_class', v: F.num(d.eraseClass), mono: true },
                { k: 'media_kind', v: F.num(d.mediaKind), mono: true },
                { k: 'erase_ts_ms', v: F.dateLong(d.eraseTsMs), mono: true },
                { k: 'fs_key_destroyed_ms', v: F.dateLong(d.fsKeyDestroyedMs), mono: true },
                { k: '误删保护至', v: d.protectUntil ? F.dateLong(d.protectUntil) : '—（已过期）', mono: true },
                { k: '介质擦除状态', v: pending ? '介质擦除待完成（不得显示「已擦除」）' : '已加密擦除 · 介质覆写未执行' }
              ])
            });
          }
        }
      ]
    });
  }

  /* ---------------- 段 3 · 审计（卡片流） ---------------- */
  var AUDIT_CATS = [
    { value: 'all', label: '全部' }, { value: 'vault', label: 'vault' }, { value: 'security', label: 'security' },
    { value: 'p2p', label: 'p2p' }, { value: 'maintain', label: 'maintain' },
    { value: 'license', label: 'license' }, { value: 'system', label: 'system' }
  ];
  function auditRows() {
    return D.auditLog.filter(function (r) {
      if (auditFilter.cat !== 'all' && r.cat !== auditFilter.cat) return false;
      var res = r.detail ? r.detail.result : 0;
      if (auditFilter.result === 'fail') return res !== 0;
      if (auditFilter.result !== 'all' && String(res) !== String(auditFilter.result)) return false;
      return true;
    });
  }
  /** 审计卡片（手机端卡片流，不是完整表格） */
  function auditCard(r) {
    var res = r.detail ? r.detail.result : 0;
    return h('button', {
      class: 'mcard', type: 'button', style: { width: '100%', textAlign: 'left', cursor: 'pointer' },
      'aria-label': '审计 seq ' + F.num(r.seq) + '，' + r.opLabel + '，类别 ' + r.cat + '，结果码 ' + F.num(res),
      title: '查看脱敏 detail（seq ' + F.num(r.seq) + '）',
      onclick: function () { go(null, 'audit', { seq: r.seq }); }
    }, [
      h('div', { class: 'mcard-head' }, [
        h('div', { class: 'grow' }, [
          h('div', { class: 't-strong' }, [
            h('span', { class: 't-mono', text: '#' + F.num(r.seq) + ' ' }), h('span', { text: r.opLabel })
          ]),
          h('div', { class: 't-caption', text: F.dateLong(r.tsMs) + ' · ' + r.op })
        ]),
        resultBadge(res)
      ]),
      h('div', { class: 'mcard-body', style: { paddingTop: '10px', paddingBottom: '10px' } }, [
        h('div', { class: 'row gap-2 wrap' }, [
          UI.badge({ text: r.cat, title: '操作类别：' + r.cat }),
          UI.chip({ text: U.shortHash(r.headHash, 10, 6), title: 'headHash = ' + r.headHash }),
          h('span', { class: 't-caption', text: '链头前缀' })
        ])
      ])
    ]);
  }
  function secAudit(ctx) {
    var out = [];
    if (demo.auditDown) {
      out.push(UI.alertbar({
        tone: 'stale', icon: 'ban', title: '审计不可用',
        text: '审计链连续写入失败（灰态）：不显示条数、不显示为通过；审计失败不阻断业务，取证需人工介入。'
      }));
    }
    var listHost = h('div', { class: 'col gap-2' });
    function paintAudit() {
      listHost.innerHTML = '';
      if (demo.auditDown) {
        listHost.appendChild(MUI.empty({ icon: 'ban', title: '审计不可用（灰态）', desc: '连续写入失败时不展示条数，也不视为通过；恢复前请人工介入取证。' }));
        return;
      }
      var rows = auditRows();
      if (!rows.length) {
        listHost.appendChild(MUI.empty({ icon: 'history', title: '无可显示的审计记录', desc: '当前筛选条件下没有记录；不显示 0 条，也不视为通过。' }));
        return;
      }
      rows.forEach(function (r) { listHost.appendChild(auditCard(r)); });
    }
    paintAudit();

    out.push(MUI.mcard({
      title: '筛选', sub: '分类 / 结果码（筛选在 UI 侧完成，不写入审计）',
      body: col([
        MUI.mtabs({ value: auditFilter.cat, items: AUDIT_CATS, onChange: function (v) { auditFilter.cat = v; paintAudit(); } }),
        a11ySelect(MUI.select({
          value: auditFilter.result, block: true,
          options: [
            { value: 'all', label: '全部结果码' }, { value: '0', label: '0 · 成功' },
            { value: '3', label: '3 · 磁盘不可写' }, { value: '9', label: '9 · 租约被占 · 只读' },
            { value: '12', label: '12 · 已取消' }, { value: 'fail', label: '仅失败（result != 0）' }
          ],
          onChange: function (v) { auditFilter.result = v; paintAudit(); }
        }), '按结果码筛选审计记录')
      ], 2)
    }));

    out.push(h('div', { class: 'row gap-2 wrap' }, [
      mbtn('校验审计链', { icon: 'link', flex: '1 1 150px', title: '校验审计链（只读；失败不自动修复）', onClick: function () { openAuditVerify(ctx); } }),
      mbtn('导出加密审计包', { icon: 'external', flex: '1 1 150px', title: '导出加密审计包（能力位受档位约束）', onClick: function () { exportAuditBundle(ctx); } })
    ]));
    out.push(listHost);
    out.push(cap('手机端以卡片流呈现（不做完整表格）：单击卡片进入审计详情页。'));
    out.push(UI.alertbar({
      tone: 'info', icon: 'info', title: '脱敏口径与只读可用性',
      text: '审计不含目标路径、不含文件名原文、不含查询词；只读降级与维护态下，审计查看 / 校验 / 导出仍可用（不置灰）。'
    }));
    out.push(MUI.mcard({
      title: '演示开关', sub: '仅切换展示分支，不改引擎语义',
      body: col([
        demoToggle('审计链校验失败', '检测项与详情显示 danger + brokenAt 段边界', demo.auditBroken, function (v) { demo.auditBroken = v; attr(); }),
        demoToggle('审计连续写入失败', '该段顶部进入灰态「审计不可用」', demo.auditDown, function (v) { demo.auditDown = v; attr(); })
      ], 3)
    }));
    return out;
  }

  /* ---------------- 段 4 · 紧急销毁 ---------------- */
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
  function delayLabel() {
    if (destroyState.delay === 'now') return '立即执行';
    if (destroyState.delay === '5m') return F.num(5) + ' 分钟后执行';
    if (destroyState.delay === '1h') return F.num(1) + ' 小时后执行';
    var d = destroyDeadline();
    return d === null ? '自定义（未选择）' : ('自定义 · ' + F.dateLong(d));
  }
  function secDestroy(ctx) {
    var out = [UI.alertbar({
      tone: 'danger', icon: 'danger', title: '危险操作区',
      text: '紧急销毁不可撤销、不可取消，进度单向；执行期全屏阻塞。远程销毁 / 锁定指令在维护态下同样允许。'
    })];

    var scopeCards = a11yRadio(MUI.radioCards({
      value: destroyState.scope,
      options: D.destroyScopes.map(function (s) { return { value: s.value, title: s.label, desc: s.desc }; }),
      onChange: function (v) { destroyState.scope = v; paintDestroy(); }
    }), D.destroyScopes.map(function (s) { return { title: s.label, desc: s.desc }; }));

    var delayOptions = [
      { value: 'now', title: '立即执行', desc: 'deadlineMs = 现在' },
      { value: '5m', title: '5 分钟后执行', desc: 'deadlineMs = 现在 + ' + F.num(5) + ' 分钟' },
      { value: '1h', title: '1 小时后执行', desc: 'deadlineMs = 现在 + ' + F.num(1) + ' 小时' },
      { value: 'custom', title: '自定义绝对时刻', desc: '按绝对 deadlineMs 落盘，重启不丢' }
    ];
    var delayCards = a11yRadio(MUI.radioCards({
      value: destroyState.delay, options: delayOptions,
      onChange: function (v) { destroyState.delay = v; paintDestroy(); }
    }), delayOptions);

    var customInput = a11y(UI.input({
      type: 'datetime-local', value: destroyState.customAt,
      onInput: function () { destroyState.customAt = customInput.value; paintDestroy(); }
    }), '自定义销毁绝对时刻（datetime-local）');

    /* 时效：7 天默认 / 0 = 不过期（选 0 时二次确认「该指令将永久有效」） */
    var expiryOptions = [
      { value: 7, title: F.num(7) + ' 天（默认）', desc: 'expires_ms = 现在 + ' + F.num(7) + ' 天' },
      { value: 0, title: F.num(0) + ' = 不过期', desc: '指令永久有效：需二次确认' }
    ];
    var expiryCards = a11yRadio(MUI.radioCards({
      value: destroyState.expiresDays, options: expiryOptions,
      onChange: function (v) {
        if (String(v) !== '0') { destroyState.expiresDays = 7; paintDestroy(); return; }
        MUI.confirmSheet({
          title: '该指令将永久有效', sub: 'expires_ms = ' + F.num(0), tone: 'danger',
          confirmLabel: '我确认（永久有效）', cancelLabel: '改为 ' + F.num(7) + ' 天',
          body: col([
            h('div', { class: 't-body', text: '该指令将永久有效（expires_ms = ' + F.num(0) + '）。' }),
            cap('签名指令不会随时间失效：任何持有匹配设备指纹的对端在上线后都会执行，直到你手动撤销。')
          ], 2),
          onConfirm: function () { destroyState.expiresDays = 0; paintDestroy(); },
          onCancel: function () { destroyState.expiresDays = 7; paintDestroy(); }
        });
      }
    }), expiryOptions);

    var bodyHost = h('div');
    function paintDestroy() {
      bodyHost.innerHTML = '';
      var deadline = destroyDeadline();
      bodyHost.appendChild(UI.field({ label: '销毁范围', control: scopeCards, hint: '范围越大影响越广；account 含撤销中继会话，不可恢复。' }));
      bodyHost.appendChild(h('div', { class: 'mt-3' }, [UI.field({
        label: '延迟销毁（绝对时刻）', control: delayCards,
        hint: '界面显示绝对 deadlineMs，不用倒计时文本；重启后按落盘的时刻续走。'
      })]));
      if (destroyState.delay === 'custom') {
        bodyHost.appendChild(h('div', { class: 'mt-3' }, [UI.field({
          label: '自定义绝对时刻', control: customInput,
          hint: deadline === null ? '尚未选择有效时刻：无法提交' : ('绝对期限 deadlineMs = ' + F.dateLong(deadline))
        })]));
      }
      bodyHost.appendChild(h('div', { class: 'mt-3' }, [MUI.mkv([
        { k: '绝对期限 deadlineMs', v: deadline === null ? '—（未选择）' : F.dateLong(deadline), mono: true },
        { k: '延迟档位', v: delayLabel(), mono: true },
        { k: '范围 scope', v: destroyState.scope, mono: true },
        { k: '时效 expires_ms', v: destroyState.expiresDays === 0 ? (F.num(0) + '（不过期）') : (F.num(destroyState.expiresDays) + ' 天'), mono: true },
        { k: '指令类型', v: 'kind = ' + F.num(1) + '（销毁指令 · 跨设备信封，非商业订单）', mono: true }
      ])]));
      bodyHost.appendChild(h('div', { class: 'mt-3' }, [UI.field({
        label: '指令时效', control: expiryCards,
        hint: '选择 ' + F.num(0) + ' 时需二次确认「该指令将永久有效」。'
      })]));

      if (demo.clockSkew) {
        var ack = UI.checkbox({
          checked: destroyState.clockAck, label: '检测到系统时钟异常（超前 > ' + F.num(24) + 'h）',
          sub: '绝对 deadlineMs 可能被误解析；我确认已核对系统时间后仍要提交'
        });
        a11y(ack, '确认系统时钟异常后仍要提交销毁指令');
        ack.input.addEventListener('change', function () { destroyState.clockAck = !!ack.input.checked; paintDestroy(); });
        bodyHost.appendChild(h('div', { class: 'mt-3' }, [UI.alertbar({
          tone: 'danger', icon: 'clock', title: '检测到系统时钟异常（超前 > ' + F.num(24) + 'h）',
          text: '绝对时刻语义可能被当前时钟偏移破坏；需显式确认后才能提交。', actions: [ack]
        })]));
      }

      var denied = denyErase();
      var byClock = demo.clockSkew && !destroyState.clockAck;
      var byDeadline = deadline === null;
      var disabled = !!denied || byClock || byDeadline;
      var reason = denied ? ('写操作被拒绝：' + denied.text) : byClock ? '需先显式确认系统时钟异常'
        : byDeadline ? '自定义时刻无效或未选择' : '';

      bodyHost.appendChild(h('div', { class: 'mt-3' }, [a11y(h('button', {
        class: 'mbtn-block', type: 'button', dataset: { variant: 'danger' }, disabled: disabled,
        style: { minHeight: '52px', fontSize: '16px' },
        onclick: function () { if (!disabled) confirmDestroy(ctx); }
      }, [h('span', { html: VS.icon('danger', 18) }), h('span', { text: '启动紧急销毁' })]),
      disabled ? ('启动紧急销毁 不可用 · ' + reason) : '启动紧急销毁（需输入确认短语「销毁」）')]));
      if (disabled) bodyHost.appendChild(h('div', { class: 'mt-2' }, [cap('按钮已禁用并写明原因：' + reason + '。')]));
      if (inMaintenance()) bodyHost.appendChild(h('div', { class: 'mt-2' }, [cap('当前为维护态：本按钮仍可用 —— 「擦除 / 销毁」是维护态下唯一被允许的写操作。')]));

      bodyHost.appendChild(h('div', { class: 'mt-3' }, [h('div', { class: 'row gap-2 wrap' }, [
        mbtn('演示：远程锁定接收', { icon: 'lock', flex: '1 1 150px', title: '演示远程锁定指令的接收（收到即锁，无二次确认）', onClick: function () { openRemoteLockSheet(); } }),
        mbtn('演示：时钟异常', { icon: 'clock', flex: '1 1 130px', title: '切换系统时钟异常演示', onClick: function () {
          demo.clockSkew = !demo.clockSkew;
          if (!demo.clockSkew) destroyState.clockAck = false;
          paintDestroy();
        } })
      ])]));
    }
    paintDestroy();

    out.push(MUI.mcard({ title: '紧急销毁', sub: '范围 / 绝对延迟 / 时效 / 时钟校验', body: bodyHost }));
    out.push(MUI.mcard({
      title: '维护态与只读降级下的行为', sub: '写闸门例外说明（用户要求销毁高于数据结构一致）',
      body: col([
        cap('维护态（码 ' + F.num(10) + '）：业务写入冻结，但「擦除 / 销毁」是唯一被允许的写操作，本区按钮保持可用。'),
        cap('只读降级（码 ' + F.num(9) + '）：擦除与销毁被拒绝（另一实例持有写租约），按钮置灰并在 title 中给出原因。'),
        cap('未解锁 / 会话失效（码 ' + F.num(5) + '）：一律拒绝。')
      ], 2)
    }));
    return out;
  }
  function confirmDestroy(ctx) {
    var deadline = destroyDeadline();
    var scope = D.destroyScopes.filter(function (s) { return s.value === destroyState.scope; })[0] || D.destroyScopes[0];
    MUI.confirmSheet({
      title: '启动紧急销毁', sub: '不可撤销 · 请手输确认短语', tone: 'danger',
      phrase: '销毁', confirmLabel: '启动紧急销毁', confirmIcon: 'danger',
      hint: '确认短语必须手工输入「销毁」；任何绕过方式都不生效。',
      body: col([
        UI.alertbar({ tone: 'danger', icon: 'danger', title: '即将不可逆地销毁数据', text: '范围：' + scope.label + ' · ' + scope.desc }),
        MUI.mkv([
          { k: '绝对期限 deadlineMs', v: deadline === null ? '—' : F.dateLong(deadline), mono: true },
          { k: '时效 expires_ms', v: destroyState.expiresDays === 0 ? (F.num(0) + '（不过期）') : (F.num(destroyState.expiresDays) + ' 天'), mono: true },
          { k: '范围 scope', v: destroyState.scope, mono: true }
        ]),
        cap('执行期全屏阻塞、不可取消；进度单向，失败也如实呈现。')
      ], 3),
      onConfirm: function () { openDestroyProgress(ctx, scope, deadline); }
    });
  }
  /** 执行期：MUI.mfullscreen 阻塞式（dismissible:false），不可取消 */
  function openDestroyProgress(ctx, scope, deadline) {
    var phases = [
      '生成签名销毁指令（连接中继前先落盘 deadlineMs）',
      '销毁文件密钥（FSKey）与从属密钥包装副本',
      '覆写并删除保险箱数据库与索引',
      '广播销毁指令给已配对设备 / 撤销中继会话',
      '写入审计记录（审计失败不阻断）'
    ];
    var barHost = h('div', { style: { width: '100%' } });
    barHost.appendChild(UI.bar(0, 'danger'));
    var phaseLabel = h('div', { class: 't-body', text: phases[0] });
    var stat = h('div', { class: 't-mono', text: '—' });
    var content = col([
      h('span', { style: { color: 'var(--c-danger)' }, html: VS.icon('danger', 52) }),
      h('div', { class: 't-h1', text: '紧急销毁进行中' }),
      cap('全屏阻塞 · 不可取消 · 进度单向'),
      barHost, phaseLabel, stat,
      h('div', { class: 't-caption t-mono', text: 'deadlineMs = ' + (deadline === null ? '—' : F.dateLong(deadline)) + ' · scope = ' + (scope ? scope.value : '—') })
    ], 3);
    var fs = MUI.mfullscreen({ tone: 'danger', content: content, dismissible: false });

    var pct = 0, idx = 0;
    var timer = setInterval(function () {
      pct = Math.min(1, pct + 0.02);
      if (pct >= (idx + 1) / phases.length && idx < phases.length - 1) { idx += 1; phaseLabel.textContent = phases[idx]; }
      barHost.innerHTML = '';
      barHost.appendChild(UI.bar(pct, 'danger'));
      stat.textContent = '进度 ' + F.pct(pct) + ' · 已处理 ' + F.num(Math.round(pct * (D.vault.entries.length + D.deletedEntries.length))) + ' 项';
      if (pct >= 1) {
        clearInterval(timer);
        phaseLabel.textContent = '销毁完成';
        stat.textContent = '全部密钥已销毁 · 审计已写入（审计失败不阻断业务）';
        fs.close();
        showDestroyDone(ctx);
      }
    }, 160);
  }
  function showDestroyDone(ctx) {
    var content = col([
      h('span', { style: { color: 'var(--c-accent)' }, html: VS.icon('check', 52) }),
      h('div', { class: 't-h1', text: '销毁已完成' }),
      cap('本机保险箱数据与密钥已销毁；已配对设备将在下次上线执行签名指令。'),
      h('div', { class: 't-caption t-mono', text: '接口：vault_core_destroy() · 审计操作 security.destroy' }),
      h('div', { class: 'col gap-2', style: { width: '100%' } }, [
        mbtn('回锁屏', { variant: 'danger', title: '回到锁屏', onClick: function () {
          done.close();
          if (VS.actions['lock']) { VS.actions['lock'](ctx); return; }
          if (VS.app && typeof VS.app.lockNow === 'function') { VS.app.lockNow(); return; }
          S.set({ lock: 'Locked', session: 'Active' }, true);
          go(ctx, 'lock');
        } }),
        mbtn('导出销毁取证', { title: '导出销毁取证记录（只读）', onClick: function () { exportAuditBundle(ctx); } })
      ])
    ], 3);
    var done = MUI.mfullscreen({ tone: 'danger', content: content, dismissible: false });
  }
  /** 远程锁定接收：sheet 显示来源设备 + 「已由 <设备名> 远程锁定」，无二次确认 */
  function openRemoteLockSheet() {
    var peer = D.devices.filter(function (d) { return !d.self; })[0] || { name: 'WorkBook', fingerprint: '' };
    MUI.sheet({
      title: '远程锁定指令', sub: '收到即锁 · 无二次确认', size: 'tall',
      body: col([
        UI.alertbar({
          tone: 'stale', icon: 'lock', title: '已由 ' + peer.name + ' 远程锁定',
          text: '锁定指令收到即锁，不做二次确认（与销毁同级通道）。本机会话已失效，需重新解锁。'
        }),
        MUI.mkv([
          { k: '来源设备', v: peer.name, mono: true },
          { k: '设备指纹', v: U.shortHash(peer.fingerprint, 12, 8), mono: true },
          { k: '指令类型', v: 'kind = ' + F.num(2) + '（锁定指令）', mono: true },
          { k: '域串', v: 'vsync-lock:{target}:{delay_ms}:{ts}', mono: true },
          { k: '接收校验', v: '签名 / target / expires_ms / 时钟偏差 <= ' + F.num(300) + ' s', mono: true }
        ]),
        cap('无二次确认：界面不提供「稍后再说」或「取消锁定」按钮。')
      ], 3),
      footer: function (close) {
        return [mbtn('我知道了', { variant: 'danger', title: '确认已锁定（无二次确认）', onClick: function () {
          close();
          S.set({ lock: 'Locked', session: 'Revoked' }, true);
          UI.toast({ tone: 'warn', title: '已锁定', msg: '会话已失效，请重新解锁' });
          if (VS.app && typeof VS.app.lockNow === 'function') VS.app.lockNow();
        } })];
      }
    });
  }

  /* ======================================================================
   * 3. 审计详情（/audit）· tab:false · back:true
   * ==================================================================== */
  var auditPage = function (ctx) {
    ctx = ctx || { params: {}, go: null };
    rebuild = null;
    var seq = ctx.params && ctx.params.seq, row = null;
    for (var i = 0; i < D.auditLog.length; i++) if (String(D.auditLog[i].seq) === String(seq)) { row = D.auditLog[i]; break; }

    if (!row) {
      return MUI.page({
        title: '审计详情', back: true, tab: false,
        body: MUI.screen([MUI.empty({
          icon: 'history', title: '未找到该审计记录',
          desc: 'seq = ' + (seq === undefined ? '—' : F.num(seq)) + ' 不在当前审计链视图中；不显示 0 条，也不推断。',
          actions: [mbtn('返回审计段', { variant: 'primary', style: { width: 'auto', padding: '0 18px' }, title: '返回安全中心的审计段', onClick: function () { go(ctx, 'security', { tab: 'audit' }); } })]
        })]),
        onBack: function () { if (VS.nav) VS.nav.back(); }
      });
    }

    var d = row.detail || {};
    function v(key) {
      var x = d[key];
      if (x === null || x === undefined || x === '') return '—（未记录）';
      if (key === 'bytes') return F.bytes(x) + '（' + F.num(x) + ' B）';
      if (key === 'chunks') return F.num(x) + ' 块';
      if (key === 'result') return F.num(x) + ' · ' + (VS.ERR[x] ? VS.ERR[x].title : '未知');
      return String(x);
    }
    var body = [
      MUI.mcard({
        title: '审计记录 · seq ' + F.num(row.seq), sub: row.op,
        body: MUI.mkv([
          { k: 'seq', v: F.num(row.seq), mono: true },
          { k: '时间', v: F.dateLong(row.tsMs), mono: true },
          { k: '操作', v: row.opLabel + '（' + row.op + '）' },
          { k: '类别', v: row.cat, mono: true },
          { k: '等级 level', v: row.level, mono: true },
          { k: '结果码 result', v: v('result'), mono: true }
        ])
      }),
      MUI.mcard({
        title: '脱敏 detail', sub: '引擎原值直出 · 不补齐、不推断',
        body: MUI.mkv([
          { k: 'file_id', v: v('file_id'), mono: true },
          { k: 'bytes', v: v('bytes'), mono: true },
          { k: 'chunks', v: v('chunks'), mono: true },
          { k: 'cipher_suite', v: v('cipher_suite'), mono: true },
          { k: 'erase_class', v: v('erase_class'), mono: true },
          { k: 'media_kind', v: v('media_kind'), mono: true },
          { k: 'method_bits', v: v('method_bits'), mono: true },
          { k: 'result', v: v('result'), mono: true }
        ])
      }),
      UI.alertbar({
        tone: 'info', icon: 'shield', title: '审计不含目标路径、不含文件名原文、不含查询词',
        text: 'detail 仅保留取证所需的标识与计数；路径与名称原文不出引擎，也不进入审计导出。'
      }),
      MUI.mcard({
        title: '链式校验', sub: 'prevHash / headHash / seq 连续性（只读）',
        body: col([
          MUI.mkv([
            { k: 'prevHash', v: U.shortHash(row.prevHash, 20, 10), mono: true },
            { k: 'headHash', v: U.shortHash(row.headHash, 20, 10), mono: true },
            { k: 'headHash 前缀', v: U.shortHash(row.headHash, 8, 4), mono: true },
            { k: 'seq', v: F.num(row.seq), mono: true },
            { k: 'seq 连续性', v: '与前序记录相差 ' + F.num(1) + '（连续）· 段边界无断点', mono: true }
          ]),
          UI.alertbar({
            tone: demo.auditBroken ? 'danger' : 'info', icon: demo.auditBroken ? 'shieldoff' : 'check',
            title: demo.auditBroken ? ('链头校验失败 · 断点段边界 brokenAt = 段 ' + F.num(12)) : '链头校验通过',
            text: demo.auditBroken ? '断点之后的链头与条数均不可信：不自动修复、不重建链、不静默截断。'
              : '仅证明链自洽；条数与链头均由接口原值直出（vault_core_audit_verify()）。'
          }),
          h('div', { class: 'row gap-2 wrap' }, [
            mbtn('复制完整链头', { flex: '1 1 150px', title: '复制完整链头哈希', onClick: function () { UI.copy(row.headHash); } }),
            mbtn('校验审计链', { flex: '1 1 130px', title: '校验审计链（只读）', onClick: function () { openAuditVerify(ctx); } })
          ])
        ], 3)
      }),
      cap('审计查看为只读能力：只读降级与维护态下仍可用（不置灰）。')
    ];

    return MUI.page({
      title: '审计详情', sub: 'seq ' + F.num(row.seq) + ' · ' + row.op, back: true, tab: false,
      actions: [{ icon: 'link', label: '校验审计链', onClick: function () { openAuditVerify(ctx); } }],
      overflow: [
        { label: '导出加密审计包', icon: 'external', sub: '只读 · 能力位受档位约束', onClick: function () { exportAuditBundle(ctx); } },
        { label: '复制链头', icon: 'copy', onClick: function () { UI.copy(row.headHash); } }
      ],
      body: MUI.screen(body),
      onBack: function () { if (VS.nav) VS.nav.back(); }
    });
  };
  VS.pages['audit'] = auditPage;

  /* ======================================================================
   * 4. 隐写术（/stego）· tab:false · back:true
   * ==================================================================== */
  var EMBED_STEPS = ['选载荷', '选载体', '选项', '容量估算', '结果分支', '执行', '完成'];
  var EXTRACT_STEPS = ['选图', '目标路径', '执行', '结果'];
  var STEGO_FLAGS = [
    { bit: 0, name: '首片', desc: 'bit0 = 1：分片的第一片，额外携带 file_hdr' },
    { bit: 1, name: '单图', desc: 'bit1 = 1：单图模式（shard_total 应为 1）' },
    { bit: 2, name: '位分散', desc: 'bit2 = 1：已启用位分散（确定性双射，不依赖密钥，默认启用）' },
    { bit: 3, name: '位序置乱', desc: 'bit3 = 1：已启用位序置乱（默认关闭；任一位丢失即整包不可解）' }
  ];
  /* data.js 的 stegoPaths 未提供 file_hdr 字段（见汇报）：原型示例，仅用于展示字段清单 */
  var STEGO_FILE_HDR_SAMPLE = {
    magic: '0x56535354 ("VSST")', payload_ver: 2, flags: '0b0111', shard_index: 0,
    shard_total: 3, file_len: 1258291, kdf_ver: 2,
    note: '仅首片携带；来源为 vault_core_stego_embed_multi 的载荷头（原型示例）'
  };
  function flagBits(flags) { var v = parseInt(String(flags).replace(/^0b/i, ''), 2); return isNaN(v) ? 0 : v; }
  function hasFlag(flags, bit) { return (flagBits(flags) & (1 << bit)) !== 0; }
  function flagChips(flags) {
    return h('div', { class: 'row gap-1 wrap' }, STEGO_FLAGS.map(function (f) {
      return hasFlag(flags, f.bit) ? UI.chip({ text: f.name, tone: 'accent', title: f.desc })
        : UI.chip({ text: f.name + '·关', title: f.desc });
    }));
  }

  var stegoPage = function (ctx) {
    ctx = ctx || { params: {}, go: null };
    rebuild = makeRebuild(stegoPage, ctx);
    return S.get('stegoEnabled') ? stegoEnabledPage(ctx) : stegoDisabledPage(ctx);
  };
  VS.pages['stego'] = stegoPage;

  function stegoDisabledPage(ctx) {
    return MUI.page({
      title: '隐写术', sub: '未启用 · 启用即受审计', back: true, tab: false,
      actions: [{ icon: 'stego', label: '启用', onClick: function () { confirmEnableStego(); } }],
      body: MUI.screen([
        MUI.empty({
          icon: 'stego', title: '隐写术未启用',
          desc: '隐写术默认不启用：导航层不渲染隐写入口，本页只做启用 / 停用。',
          actions: [mbtn('启用隐写术', { icon: 'stego', variant: 'primary', style: { width: 'auto', padding: '0 18px' }, title: '启用隐写术（需二次确认；启用即受审计）', onClick: function () { confirmEnableStego(); } })]
        }),
        UI.alertbar({
          tone: 'info', icon: 'info', title: '未启用时的接口语义',
          text: '未启用时嵌入 / 提取接口返回 ' + F.num(7) + '（参数非法），并给出 dialog.stegoDisabled 文案；本页不伪造成功。'
        }),
        MUI.mcard({
          title: '启用后会发生什么', sub: '先看边界再决定',
          body: col(D.stegoBoundaries.map(function (t) {
            return h('div', { class: 'row gap-2', style: { alignItems: 'flex-start' } }, [
              h('span', { html: VS.icon('stego', 14) }), h('span', { class: 'grow t-caption', text: t })
            ]);
          }), 3)
        })
      ]),
      onBack: function () { if (VS.nav) VS.nav.back(); }
    });
  }
  function confirmEnableStego() {
    MUI.confirmSheet({
      title: '启用隐写术', sub: '启用即受审计', tone: 'danger', confirmLabel: '启用', confirmIcon: 'check',
      body: col([
        UI.alertbar({
          tone: 'danger', icon: 'alert', title: '启用即受审计',
          text: '开关切换、每次嵌入 / 提取都会写入 vault-audit 的 security 类别；审计失败不阻断业务，但会发 ERROR_DIAG。'
        }),
        cap('隐写不替代保险箱；不可检测性没有安全保证。启用开关落盘 settings.enc（VSSG v1, kind=0，HKDF(MK,"settings")）。')
      ], 3),
      onConfirm: function () {
        S.set({ stegoEnabled: true });
        UI.toast({ tone: 'success', title: '隐写术已启用', msg: '已写入审计（security 类别）· 入口在导航层出现' });
        attr();
      }
    });
  }
  function confirmDisableStego() {
    MUI.confirmSheet({
      title: '停用隐写术', sub: '会说明后果', tone: 'danger', confirmLabel: '停用',
      body: col([
        strong('停用后的后果'),
        h('ul', { class: 'col gap-1', style: { paddingLeft: '18px' } }, [
          h('li', { class: 't-caption', text: '嵌入 / 提取入口关闭；未启用的接口调用返回 ' + F.num(7) + '。' }),
          h('li', { class: 't-caption', text: '已生成的载体图片与其内载荷不受影响，仍可用其它客户端提取。' }),
          h('li', { class: 't-caption', text: '审计记录不会被删除：停用动作本身也写审计（security 类别）。' })
        ])
      ], 3),
      onConfirm: function () {
        S.set({ stegoEnabled: false });
        UI.toast({ tone: 'warn', title: '隐写术已停用', msg: '停用动作已写入审计（security 类别）' });
        attr();
      }
    });
  }
  function openStegoDisabledSheet() {
    MUI.sheet({
      title: '隐写术未启用', sub: 'dialog.stegoDisabled', size: 'tall',
      body: col([
        UI.alertbar({
          tone: 'danger', icon: 'ban', title: errText(7),
          text: '未启用时嵌入 / 提取接口返回 ' + F.num(7) + '，并给出 dialog.stegoDisabled 文案；本界面不伪造成功。'
        }),
        cap('启用即受审计：开关切换、每次嵌入 / 提取均写 vault-audit 的 security 类别。')
      ], 3),
      footer: function (close) {
        return [
          mbtn('前往启用', { variant: 'primary', title: '前往启用隐写术', onClick: function () { close(); confirmEnableStego(); } }),
          mbtn('关闭', { onClick: close })
        ];
      }
    });
  }

  function stegoEnabledPage(ctx) {
    var body = [
      /* 1. 安全边界 6 条（原样包含「隐写不替代保险箱」「不可检测性没有安全保证」「启用即受审计」） */
      MUI.mcard({
        title: '安全边界声明',
        sub: '逐条渲染 D.stegoBoundaries（' + F.num(D.stegoBoundaries.length) + ' 条）· 不接受「设计保证」式对外承诺',
        body: col(D.stegoBoundaries.map(function (t, i) {
          return UI.alertbar({ tone: i === 1 ? 'warn' : 'info', icon: i === 1 ? 'alert' : 'shield', title: String(t).split('：')[0], text: t });
        }), 2)
      }),
      /* 2. 单图容量说明 */
      MUI.mcard({
        title: '单图容量说明', sub: '公式只作解释；容量真值只来自 vault_core_stego_plan',
        body: col([
          mono('capacity = floor(width x height x 3 / 8) - 4'),
          mono('usable   = floor(capacity x fill_ratio)'),
          mono('示例：' + F.num(1920) + 'x' + F.num(1080) + ' = ' + F.num(777596) + ' B（' + F.bytes(777596) + '）'),
          UI.alertbar({
            tone: 'danger', icon: 'ban', title: '硬约束：UI 不自行计算容量',
            text: '容量真值只能来自 vault_core_stego_plan（结果不缓存，两次调用之间换图则结果随之变化）；界面上的公式仅用于解释口径，不作为任何判断依据。'
          })
        ], 3)
      }),
      /* 3. 反检测说明 */
      MUI.mcard({
        title: '反检测说明', sub: '位分散 / 填充率 / 位序置乱：三项开关的真实语义与默认值',
        body: col([
          h('div', { class: 'row gap-2 wrap' }, [
            h('div', { class: 'grow' }, [strong('位分散 disperse'), cap('确定性双射，把载荷位分散到整幅图；不依赖密钥，因此同样可被持原图者比对。默认启用。')]),
            UI.badge({ text: '默认启用', tone: 'success', title: 'disperse = true' })
          ]),
          h('div', { class: 'row gap-2 wrap' }, [
            h('div', { class: 'grow' }, [strong('填充率 fill_ratio'), cap('只占用 usable = capacity x fill_ratio 的容量；默认 ' + F.num(0.8) + '，留出裕量降低统计显著性。')]),
            UI.badge({ text: F.pct(0.8) + ' 上限', tone: 'info', title: 'fill_ratio <= 0.8' })
          ]),
          h('div', { class: 'row gap-2 wrap' }, [
            h('div', { class: 'grow' }, [strong('位序置乱 shuffle'), cap('启用后任一位丢失即整包不可解，鲁棒性下降；因此默认关闭，界面在开启时给警告。')]),
            UI.badge({ text: '默认关闭', tone: 'stale', title: 'shuffle = false' })
          ]),
          UI.alertbar({ tone: 'warn', icon: 'alert', title: '不可检测性没有安全保证', text: '三项开关只降低统计检测显著性，不承诺「无法被检测」；持原图比对必然可检出。' })
        ], 3)
      }),
      /* 4 / 5. 向导入口 */
      MUI.mcard({
        title: '嵌入向导', sub: F.num(EMBED_STEPS.length) + ' 步：选载荷 -> 选载体 -> 选项 -> 容量估算 -> 分支 -> 执行 -> 完成',
        body: col([
          cap('多图嵌入走 vault_core_stego_plan -> vault_core_stego_embed_multi；fits=false 时如实拒绝（错误码 ' + F.num(7) + ' + last_error），不截断、不降质。'),
          mbtn('打开嵌入向导', { icon: 'stego', variant: 'primary', title: '打开隐写嵌入向导', onClick: function () { openEmbedWizard(ctx); } })
        ], 3)
      }),
      MUI.mcard({
        title: '提取向导', sub: F.num(EXTRACT_STEPS.length) + ' 步：选图（顺序无关）-> 目标路径 -> 执行 -> 结果',
        body: col([
          cap('乱序重组：按 shard_index 排序后再拼接；重复 shard_index 拒绝（不做「后到者胜」）；缺片报 missing[]。'),
          mbtn('打开提取向导', { icon: 'import', title: '打开隐写提取向导', onClick: function () { openExtractWizard(); } })
        ], 3)
      }),
      /* 6. 单图模式 */
      MUI.mcard({
        title: '单图模式', sub: 'vault_core_stego_capacity（负值即错误码，仅供 UI 预估）',
        body: col([
          cap('返回值语义：正数 = 可用字节数；负值 = 错误码（例如 -' + F.num(6) + ' 即错误码 ' + F.num(6) + '）。'),
          h('div', { class: 'row gap-2 wrap' }, [
            mbtn('单图容量估算', { flex: '1 1 110px', title: '单图容量估算（负值即错误码，供 UI 预估）', onClick: function () { openSingleImage('capacity'); } }),
            mbtn('单图嵌入', { flex: '1 1 100px', title: '单图嵌入（超容量报错误码 ' + F.num(6) + '）', onClick: function () { openSingleImage('embed'); } }),
            mbtn('单图提取', { flex: '1 1 100px', title: '单图提取（flags bit1 = 单图）', onClick: function () { openSingleImage('extract'); } })
          ]),
          UI.alertbar({ tone: 'danger', icon: 'ban', title: '超容量绝不截断', text: '载荷超出单图容量 -> 错误码 ' + F.num(6) + ' + 「payload exceeds image capacity」，如实拒绝。' })
        ], 3)
      }),
      /* 7. 批量区 */
      MUI.mcard({
        title: '批量区', sub: '外壳侧顺序调用：逐项状态，不聚合为一个总状态',
        body: col([
          cap('批量 = 外壳侧对每个条目顺序调用单次接口；UI 必须逐项展示成功 / 失败与错误码，不得折叠成一个「批量完成」。'),
          cap('单项失败不影响其余项，也不回滚已成功的产物。'),
          h('div', { class: 'row gap-2 wrap' }, [
            mbtn('批量嵌入', { flex: '1 1 130px', title: '批量嵌入（顺序调用，逐项显示成功 / 失败）', onClick: function () { openBatch('embed'); } }),
            mbtn('批量提取', { flex: '1 1 130px', title: '批量提取（顺序调用，逐项显示成功 / 失败）', onClick: function () { openBatch('extract'); } })
          ])
        ], 3)
      }),
      /* 8. 隐写产物详情 */
      stegoPathsCard(),
      /* 9. 演示开关 */
      MUI.mcard({
        title: '演示开关', sub: '仅切换展示分支',
        body: col([
          demoToggle('兼容读：V1.0 兼容格式', '提取结果提示「该载荷为 V1.0 兼容格式」', demo.stegoLegacy, function (v) { demo.stegoLegacy = v; if (v) demo.stegoNewer = false; }),
          demoToggle('兼容读：更高版本载荷', '提取结果提示「当前客户端无法读取更高版本载荷」', demo.stegoNewer, function (v) { demo.stegoNewer = v; if (v) demo.stegoLegacy = false; })
        ], 3)
      })
    ];

    return MUI.page({
      title: '隐写术',
      sub: '已启用 · 启用即受审计 · 容量真值只来自 vault_core_stego_plan（UI 不自行计算）',
      back: true, tab: false,
      actions: [{ icon: 'stego', label: '嵌入向导', onClick: function () { openEmbedWizard(ctx); } }],
      overflow: [
        { label: '提取向导', icon: 'import', sub: '顺序无关 · 缺片报 missing[]', onClick: function () { openExtractWizard(); } },
        { label: '单图容量估算', icon: 'layers', onClick: function () { openSingleImage('capacity'); } },
        { label: '批量嵌入（顺序调用）', icon: 'queue', onClick: function () { openBatch('embed'); } },
        { label: '批量提取（顺序调用）', icon: 'import', onClick: function () { openBatch('extract'); } },
        { label: '停用隐写术', icon: 'ban', danger: true, sub: '会说明后果', onClick: function () { confirmDisableStego(); } }
      ],
      body: MUI.screen(body),
      onBack: function () { if (VS.nav) VS.nav.back(); }
    });
  }

  function stegoPathsCard() {
    return MUI.mcard({
      title: '隐写产物详情', sub: 'D.stegoPaths · flags 位含义逐位展开',
      body: col(D.stegoPaths.map(function (p) {
        var inconsistent = hasFlag(p.flags, 1) && p.shardTotal > 1;
        return MUI.mcard({
          title: p.carrier, sub: 'shard_index ' + F.num(p.shardIndex) + ' / ' + F.num(p.shardTotal),
          body: col([
            MUI.mkv([
              { k: 'payload_ver', v: F.num(p.payloadVer), mono: true },
              { k: 'flags', v: p.flags, mono: true },
              { k: 'shard_index', v: F.num(p.shardIndex), mono: true },
              { k: 'shard_total', v: F.num(p.shardTotal), mono: true },
              { k: 'file_len', v: F.num(p.fileLen) + '（' + F.bytes(p.fileLen) + '）', mono: true },
              { k: 'shard_crc32', v: U.shortHash(p.shardCrc32, 10, 6), mono: true }
            ]),
            flagChips(p.flags),
            inconsistent ? UI.badge({
              text: 'flags 与 shard_total 不一致（bit1 单图置位但 shard_total = ' + F.num(p.shardTotal) + '）',
              tone: 'danger', title: '数据样例需核对：flags 与分片计数矛盾'
            }) : null,
            p.shardIndex === 0 ? mono('file_hdr = ' + JSON.stringify(STEGO_FILE_HDR_SAMPLE)) : cap('—（非首片不携带 file_hdr）')
          ], 3)
        });
      }), 3)
    });
  }

  /* ---------------- 嵌入向导（sheet size tall，7 步） ---------------- */
  function newEmbedState() {
    return {
      step: 0, payloadId: (D.vault.entries[0] || {}).id, carriers: [], badAdded: false,
      fillRatio: 0.8, disperse: true, shuffle: false, name: '', codec: 'png',
      maxBytes: 4294967296, maxImages: 8, produced: 0, cancelled: false, done: false
    };
  }
  function imageOptions() {
    var list = D.vault.entries.filter(function (e) { return e.type === 'image'; }).map(function (e) {
      return { key: 'e' + e.id, id: e.id, name: e.name, size: e.size, error: null };
    });
    if (embedWiz && embedWiz.badAdded) list.push({ key: 'bad', id: null, name: '灰度图-16bit.png（演示）', size: 65536, error: 'unsupported png format' });
    return list;
  }
  function planSummary(w) {
    var selected = imageOptions().filter(function (o) { return w.carriers.indexOf(o.key) >= 0; });
    var errors = selected.filter(function (o) { return !!o.error; });
    var imagesGiven = selected.length, imagesNeeded = D.stegoPlanSample.imagesNeeded;
    var shortfall = 0;
    if (imagesGiven < imagesNeeded) D.stegoPlanSample.perImage.slice(imagesGiven).forEach(function (p) { shortfall += (p.usable || 0); });
    return { imagesGiven: imagesGiven, imagesNeeded: imagesNeeded, fits: imagesGiven >= imagesNeeded && errors.length === 0, errors: errors, shortfallBytes: shortfall };
  }
  function stopEmbedRun() { if (embedTimer) { clearInterval(embedTimer); embedTimer = null; } }

  function openEmbedWizard(ctx) {
    if (!S.get('stegoEnabled')) { openStegoDisabledSheet(); return; }
    embedWiz = newEmbedState();
    var hostEl = h('div', { class: 'col gap-4' });
    var sheet = null;
    function paint() { hostEl.innerHTML = ''; hostEl.appendChild(stepBody()); hostEl.appendChild(nav()); }

    function nav() {
      var blocked = false, blockReason = '';
      if (embedWiz.step === 4) {
        var p = planSummary(embedWiz);
        if (!p.fits) { blocked = true; blockReason = '需要 ' + F.num(p.imagesNeeded) + ' 张图，已选 ' + F.num(p.imagesGiven); }
        else if (p.errors.length) { blocked = true; blockReason = '存在不支持的图片形态（' + F.num(p.errors.length) + ' 张）'; }
      }
      var nextLabel = embedWiz.step === 4 ? '开始嵌入' : '下一步';
      var next = a11y(h('button', {
        class: 'mbtn-block', type: 'button', dataset: { variant: 'primary' },
        disabled: blocked || embedWiz.step >= EMBED_STEPS.length - 1,
        onclick: function () {
          var cur = planSummary(embedWiz);
          if (embedWiz.step === 1 && !embedWiz.carriers.length) { UI.toast({ tone: 'warn', title: '请先选择载体图片', msg: '至少选择 ' + F.num(1) + ' 张 PNG 载体' }); return; }
          if (embedWiz.step === 4 && cur.errors.length) { openBadImagesSheet(cur.errors, paint); return; }
          if (embedWiz.step === 4 && !cur.fits) { openNotFitsSheet(cur, paint); return; }
          embedWiz.step = Math.min(EMBED_STEPS.length - 1, embedWiz.step + 1);
          paint();
          if (embedWiz.step === 5) runEmbed(paint);
        }
      }, [h('span', { text: nextLabel })]),
      blocked ? (nextLabel + ' 不可用 · ' + blockReason) : (embedWiz.step === 4 ? '开始嵌入（容量与形态检查通过）' : nextLabel));

      return col([
        MUI.steps({ current: embedWiz.step, items: EMBED_STEPS.map(function (l) { return { label: l }; }) }),
        next,
        mbtn('上一步', { disabled: embedWiz.step === 0 || embedWiz.step === 5, onClick: function () { embedWiz.step = Math.max(0, embedWiz.step - 1); paint(); } }),
        mbtn('取消向导', { title: '取消向导（不产生任何产物）', onClick: function () {
          stopEmbedRun();
          if (sheet) sheet.close();
          UI.toast({ tone: 'info', title: '已取消向导', msg: embedWiz && embedWiz.produced ? ('已生成的图数 ' + F.num(embedWiz.produced) + ' · ' + errText(12)) : errText(12) });
        } })
      ], 2);
    }

    function stepBody() {
      var w = embedWiz, box = col([], 3);

      /* 1 选载荷（保险箱文件） */
      if (w.step === 0) {
        box.appendChild(cap('1 选载荷：来自保险箱的文件；名称仅用于 UI 选择，不进审计（审计不含文件名原文）。'));
        D.vault.entries.slice(0, 8).forEach(function (e) {
          var on = String(w.payloadId) === String(e.id);
          box.appendChild(a11y(h('button', {
            class: 'mrow', type: 'button', style: { borderRadius: '10px', border: '1px solid var(--c-border)' },
            onclick: function () { w.payloadId = e.id; paint(); }
          }, [
            h('span', { class: 'mr-ico', html: VS.icon('file', 22) }),
            h('div', { class: 'mr-main' }, [
              h('div', { class: 'mr-title', text: e.name }),
              h('div', { class: 'mr-sub', text: F.bytes(e.size) + ' · ' + F.num(e.chunkCount) + ' 块' })
            ]),
            on ? h('span', { html: VS.icon('check-circle', 18) }) : null
          ]), '选择载荷 ' + e.name + (on ? '（已选）' : '')));
        });
        return box;
      }

      /* 2 选载体图片（多选 PNG） */
      if (w.step === 1) {
        box.appendChild(cap('2 选载体图片（可多选 PNG）：选择顺序不重要，嵌入按你给出的集合一次性 plan。'));
        imageOptions().forEach(function (o) {
          var on = w.carriers.indexOf(o.key) >= 0;
          box.appendChild(a11y(h('button', {
            class: 'mrow', type: 'button', style: { borderRadius: '10px', border: '1px solid var(--c-border)' },
            onclick: function () {
              if (o.error) { UI.toast({ tone: 'danger', title: '不支持该图片', msg: o.name + ' -> ' + o.error }); return; }
              var i = w.carriers.indexOf(o.key);
              if (i >= 0) w.carriers.splice(i, 1); else w.carriers.push(o.key);
              paint();
            }
          }, [
            h('span', { class: 'mr-ico', html: VS.icon('image', 22) }),
            h('div', { class: 'mr-main' }, [
              h('div', { class: 'mr-title', text: o.name }),
              h('div', { class: 'mr-sub', text: F.bytes(o.size) + (o.error ? (' · ' + o.error) : '') })
            ]),
            o.error ? UI.badge({ text: 'error', tone: 'danger', title: o.error }) : null,
            on ? h('span', { html: VS.icon('check-circle', 18) }) : null
          ]), '选择载体 ' + o.name + (on ? '（已选）' : '')));
        });
        box.appendChild(h('div', { class: 'row gap-2 wrap' }, [
          mbtn(w.badAdded ? '移除不支持的 PNG' : '加入一张不支持的 PNG', {
            flex: '1 1 170px', title: '把某条置为 unsupported png format 以便演示',
            onClick: function () {
              w.badAdded = !w.badAdded;
              if (w.badAdded) w.carriers.push('bad');
              else w.carriers = w.carriers.filter(function (k) { return k !== 'bad'; });
              paint();
            }
          }),
          cap('已选 ' + F.num(w.carriers.length) + ' 张')
        ]));
        return box;
      }

      /* 3 选项 */
      if (w.step === 2) {
        box.appendChild(cap('3 选项：默认值即安全默认；shuffle 默认关闭并在开启时警告。'));
        var ratioOut = h('span', { class: 't-mono', text: F.num(w.fillRatio) });
        var ratio = UI.slider({ min: 0.1, max: 0.8, step: 0.05, value: w.fillRatio, onInput: function () { w.fillRatio = parseFloat(ratio.value); ratioOut.textContent = F.num(w.fillRatio); } });
        box.appendChild(UI.field({
          label: '填充率 fillRatio（默认 ' + F.num(0.8) + '）',
          control: h('div', { class: 'row gap-3' }, [a11y(ratio, '填充率 fillRatio（默认 ' + F.num(0.8) + '）'), ratioOut]),
          hint: '只占用 usable = capacity x fillRatio；上限 ' + F.pct(0.8) + '，默认 ' + F.num(0.8) + '。'
        }));
        box.appendChild(a11y(UI.switchCtl({ checked: w.disperse, label: '位分散 disperse', sub: '默认开启：确定性双射，不依赖密钥', onChange: function (e) { w.disperse = !!e.target.checked; } }), '位分散 disperse（默认开启）'));
        box.appendChild(a11y(UI.switchCtl({ checked: w.shuffle, label: '位序置乱 shuffle', sub: '默认关闭：开启后任一位丢失即整包不可解', onChange: function (e) { w.shuffle = !!e.target.checked; paint(); } }), '位序置乱 shuffle（默认关闭）'));
        if (w.shuffle) {
          box.appendChild(UI.alertbar({ tone: 'danger', icon: 'alert', title: '任一位丢失即整包不可解', text: '位序置乱提升隐蔽性但牺牲鲁棒性：任何一位翻转都会导致整包无法还原，且不支持部分恢复。' }));
        }
        var nameInput = UI.input({ value: w.name, placeholder: '（默认空：不写入原始名称）', onInput: function () { w.name = nameInput.value; } });
        box.appendChild(UI.field({ label: '名称 name（默认空）', control: a11y(nameInput, '可选名称（默认空）'), hint: '默认不写入名称，避免在载体中留下可识别信息。' }));
        var codecSel = a11ySelect(UI.select({
          value: w.codec, block: true, options: [{ value: 'png', label: 'png' }, { value: 'auto', label: 'auto' }],
          onChange: function (v) { w.codec = v; paint(); }
        }), '编解码 codec（png / auto）');
        box.appendChild(UI.field({ label: '编解码 codec', control: codecSel, hint: 'JPEG 鲁棒路线不做：auto 一律解析为 png。' }));
        if (w.codec === 'auto') {
          box.appendChild(UI.alertbar({ tone: 'info', icon: 'info', title: 'auto 一律解析为 png（codec_fallback=png）', text: '不承诺 JPEG 鲁棒性；选择 auto 与选择 png 的实际行为一致。' }));
        }
        var maxBytesInput = UI.input({ type: 'number', value: w.maxBytes, onInput: function () { w.maxBytes = parseInt(maxBytesInput.value, 10) || 0; } });
        box.appendChild(UI.field({
          label: 'maxBytes（默认 ' + F.num(4294967296) + ' = ' + F.bytes(4294967296) + '）',
          control: a11y(maxBytesInput, '载荷上限 maxBytes（默认 ' + F.num(4294967296) + '）'),
          hint: '超过该值的载荷直接拒绝，不做截断。'
        }));
        var maxImagesInput = UI.input({ type: 'number', value: w.maxImages, onInput: function () { w.maxImages = parseInt(maxImagesInput.value, 10) || 0; } });
        box.appendChild(UI.field({ label: 'maxImages', control: a11y(maxImagesInput, '最大图数 maxImages'), hint: '一次嵌入允许使用的最大载体数量。' }));
        return box;
      }

      /* 4 容量估算（引擎 plan 原值直出） */
      if (w.step === 3) {
        box.appendChild(UI.alertbar({
          tone: 'info', icon: 'info', title: '容量真值来自 vault_core_stego_plan',
          text: '下列为引擎 plan 的原值直出（D.stegoPlanSample）；UI 不自行计算容量，也不缓存结果（换图后需重新调用）。'
        }));
        D.stegoPlanSample.perImage.forEach(function (p) {
          box.appendChild(MUI.mcard({
            title: p.path, sub: 'width x height = ' + F.num(p.width) + ' x ' + F.num(p.height),
            body: MUI.mkv([
              { k: 'capacity', v: F.num(p.capacity), mono: true },
              { k: 'usable', v: F.num(p.usable), mono: true },
              { k: 'shardBytes', v: p.error ? p.error : F.num(p.shardBytes), mono: true }
            ])
          }));
        });
        var s = planSummary(w);
        box.appendChild(MUI.mkv([
          { k: 'imagesNeeded', v: F.num(D.stegoPlanSample.imagesNeeded), mono: true },
          { k: 'imagesGiven（已选）', v: F.num(s.imagesGiven), mono: true },
          { k: 'fits', v: String(s.fits), mono: true },
          { k: 'shortfallBytes', v: F.num(s.fits ? D.stegoPlanSample.shortfallBytes : s.shortfallBytes), mono: true },
          { k: 'notes[]', v: D.stegoPlanSample.notes.length ? D.stegoPlanSample.notes.join(' / ') : '（空）', mono: true }
        ]));
        box.appendChild(mono('容量账本示例行：file_len=' + F.num(1258291) + ' -> shard0=' + F.num(621990) +
          ' + shard1=' + F.num(622034) + ' + shard2=' + F.num(14267) + ' = ' + F.num(1258291)));
        box.appendChild(cap('逐片 shardBytes 之和 = file_len；末片不足按实际写入，不补齐、不填充占位。'));
        if (!s.fits) {
          box.appendChild(UI.alertbar({
            tone: 'danger', icon: 'ban', title: '容量不足：如实拒绝',
            text: '需要 ' + F.num(s.imagesNeeded) + ' 张图，已选 ' + F.num(s.imagesGiven) + '；不截断、不降质，请继续选图。'
          }));
        }
        if (s.errors.length) {
          box.appendChild(UI.alertbar({ tone: 'danger', icon: 'alert', title: '存在不支持的图片形态', text: s.errors.map(function (e) { return e.name + ' -> ' + e.error; }).join('；') + '（阻止提交）' }));
        }
        return box;
      }

      /* 5 结果分支：fits=false / 有图 error -> 阻止提交 */
      if (w.step === 4) {
        var s2 = planSummary(w);
        box.appendChild(strong('5 结果分支'));
        if (!s2.fits) {
          box.appendChild(UI.alertbar({
            tone: 'danger', icon: 'ban', title: '需要 ' + F.num(s2.imagesNeeded) + ' 张图，已选 ' + F.num(s2.imagesGiven),
            text: '缺口 ' + F.bytes(s2.shortfallBytes) + '；提交已被阻止。'
          }));
          box.appendChild(h('div', { class: 'row gap-2 wrap' }, [
            mbtn('查看所需图数', { variant: 'danger', flex: '1 1 140px', title: '查看 imagesNeeded / imagesGiven / shortfallBytes', onClick: function () { openNotFitsSheet(s2, paint); } }),
            mbtn('继续选图', { variant: 'primary', flex: '1 1 120px', title: '返回载体选择继续选图', onClick: function () { w.step = 1; paint(); } })
          ]));
        } else if (s2.errors.length) {
          box.appendChild(UI.alertbar({ tone: 'danger', icon: 'alert', title: '不支持的图片形态阻止提交', text: s2.errors.map(function (e) { return e.name + ' -> ' + e.error; }).join('；') }));
          box.appendChild(h('div', { class: 'row gap-2 wrap' }, [
            mbtn('查看不支持的图', { variant: 'danger', flex: '1 1 140px', title: '逐条查看 error', onClick: function () { openBadImagesSheet(s2.errors, paint); } }),
            mbtn('移除不支持的图片', { flex: '1 1 140px', title: '从已选载体中移除 error 项', onClick: function () {
              w.carriers = w.carriers.filter(function (k) { return k !== 'bad'; });
              w.badAdded = false;
              paint();
            } })
          ]));
        } else {
          box.appendChild(UI.alertbar({ tone: 'info', icon: 'check', title: '容量与形态检查通过', text: '下一步开始嵌入：写入过程可取消，取消时如实提示已生成的图数。' }));
        }
        return box;
      }

      /* 6 执行与进度 */
      if (w.step === 5) {
        var total = D.stegoPlanSample.perImage.length;
        box.appendChild(strong(w.done ? '嵌入完成' : '正在按 shard_index 顺序写入载体…'));
        box.appendChild(UI.bar(total ? (w.produced / total) : 0, w.done ? 'success' : 'accent'));
        box.appendChild(cap('已生成 ' + F.num(w.produced) + ' / ' + F.num(total) + ' 张 · 当前：' +
          ((D.stegoPlanSample.perImage[Math.min(w.produced, total - 1)] || {}).path || '—')));
        if (!w.done) box.appendChild(mbtn('取消写入', { variant: 'danger', title: '取消写入（返回码 ' + F.num(12) + '；如实提示已生成的图数）', onClick: function () { cancelEmbedRun(paint); } }));
        box.appendChild(cap('取消不可撤销已写出的载体：界面只如实报告，不假装回滚。'));
        return box;
      }

      /* 7 完成 */
      box.appendChild(strong(w.cancelled ? '嵌入已取消' : '嵌入完成'));
      box.appendChild(UI.alertbar({
        tone: w.cancelled ? 'warn' : 'info', icon: w.cancelled ? 'alert' : 'check',
        title: w.cancelled ? ('已取消 · 已生成的图数 ' + F.num(w.produced)) : ('已生成 ' + F.num(w.produced) + ' 张载体图片'),
        text: w.cancelled ? errText(12) + '；已生成的载体不会被回滚，也不会删除。'
          : '每次嵌入均写 vault-audit（security 类别）；审计失败不阻断业务但发 ERROR_DIAG。'
      }));
      D.stegoPlanSample.perImage.slice(0, Math.max(1, w.produced)).forEach(function (p) {
        box.appendChild(MUI.mkv([
          { k: '生成的图片', v: p.path },
          { k: '逐图占用 shardBytes', v: F.bytes(p.shardBytes), mono: true },
          { k: 'usable', v: F.num(p.usable), mono: true }
        ]));
      });
      box.appendChild(cap('产物详情（flags / shard_index / shard_crc32 / file_hdr）见页面「隐写产物详情」区。'));
      return box;
    }

    paint();
    sheet = MUI.sheet({
      title: '隐写嵌入向导', sub: 'vault_core_stego_plan -> vault_core_stego_embed_multi', size: 'tall',
      body: hostEl, onClose: stopEmbedRun,
      footer: function (close) { return [mbtn('关闭向导', { title: '关闭向导（已生成的载体保留）', onClick: function () { close(); } })]; }
    });
  }

  function openNotFitsSheet(s, onContinue) {
    MUI.sheet({
      title: '需要 ' + F.num(s.imagesNeeded) + ' 张图，已选 ' + F.num(s.imagesGiven),
      sub: '提交已被阻止', size: 'tall',
      body: col([
        MUI.mkv([
          { k: 'imagesNeeded', v: F.num(s.imagesNeeded), mono: true },
          { k: 'imagesGiven', v: F.num(s.imagesGiven), mono: true },
          { k: 'shortfallBytes', v: F.num(s.shortfallBytes) + '（' + F.bytes(s.shortfallBytes) + '）', mono: true }
        ]),
        UI.alertbar({ tone: 'danger', icon: 'ban', title: '提交已被阻止', text: '不截断、不降质、不更换编码：请继续选图后重新估算。' })
      ], 3),
      footer: function (close) {
        return [
          mbtn('继续选图', { variant: 'primary', title: '返回载体选择继续选图', onClick: function () { close(); if (embedWiz) embedWiz.step = 1; if (onContinue) onContinue(); } }),
          mbtn('留在当前步骤', { onClick: close })
        ];
      }
    });
  }
  function openBadImagesSheet(errors, onFix) {
    MUI.sheet({
      title: '存在不支持的图片', sub: '逐条列出并阻止提交', size: 'tall',
      body: col([
        UI.alertbar({ tone: 'danger', icon: 'alert', title: '不支持的图片形态', text: '灰度 / 调色板 / 16 位 PNG 不支持嵌入；逐条列出并阻止提交。' }),
        MUI.mlist(errors.map(function (e) {
          return MUI.mrow({ icon: 'ban', danger: true, chevron: false, title: e.name, sub: e.error });
        }))
      ], 3),
      footer: function (close) {
        return [
          mbtn('移除不支持的图片', { variant: 'primary', title: '从已选载体中移除 error 项', onClick: function () {
            close();
            if (embedWiz) { embedWiz.carriers = embedWiz.carriers.filter(function (k) { return k !== 'bad'; }); embedWiz.badAdded = false; }
            if (onFix) onFix();
          } }),
          mbtn('关闭', { onClick: close })
        ];
      }
    });
  }
  function runEmbed(paint) {
    var total = D.stegoPlanSample.perImage.length;
    stopEmbedRun();
    embedWiz.produced = 0; embedWiz.cancelled = false; embedWiz.done = false;
    embedTimer = setInterval(function () {
      embedWiz.produced += 1;
      if (embedWiz.produced >= total) { embedWiz.produced = total; embedWiz.done = true; stopEmbedRun(); embedWiz.step = 6; }
      paint();
    }, 420);
  }
  function cancelEmbedRun(paint) {
    stopEmbedRun();
    embedWiz.cancelled = true; embedWiz.done = false; embedWiz.step = 6;
    paint();
    UI.toast({
      tone: 'warn', title: '嵌入已取消',
      msg: '已生成的图数 ' + F.num(embedWiz.produced) + ' · ' + errText(12) + '（已生成的载体保留，不回滚）', duration: 4200
    });
  }

  /* ---------------- 提取向导（4 步，顺序无关） ---------------- */
  function openExtractWizard() {
    if (!S.get('stegoEnabled')) { openStegoDisabledSheet(); return; }
    extractWiz = { step: 0, picks: [], target: '/外部路径/还原输出/', name: '还原-输出.png', removedNotice: '', missing: [] };
    var hostEl = h('div', { class: 'col gap-4' });
    var sheet = null;
    function sortedPicks() { return extractWiz.picks.slice().sort(function (a, b) { return a.shardIndex - b.shardIndex; }); }
    function computeMissing() {
      var have = {};
      extractWiz.picks.forEach(function (p) { have[p.shardIndex] = true; });
      var missing = [];
      for (var i = 0; i < D.stegoPaths.length; i++) if (!have[i]) missing.push(i);
      return missing;
    }
    function paint() { hostEl.innerHTML = ''; hostEl.appendChild(stepBody()); hostEl.appendChild(nav()); }

    function nav() {
      return col([
        MUI.steps({ current: extractWiz.step, items: EXTRACT_STEPS.map(function (l) { return { label: l }; }) }),
        mbtn(extractWiz.step === 1 ? '开始提取' : '下一步', {
          variant: 'primary', disabled: extractWiz.step >= EXTRACT_STEPS.length - 1,
          onClick: function () {
            if (extractWiz.step === 0 && !extractWiz.picks.length) { UI.toast({ tone: 'warn', title: '请先选择图片', msg: '至少选择 ' + F.num(1) + ' 张载体图片' }); return; }
            extractWiz.step = Math.min(EXTRACT_STEPS.length - 1, extractWiz.step + 1);
            if (extractWiz.step === 2) extractWiz.missing = computeMissing();
            paint();
          }
        }),
        mbtn('上一步', { disabled: extractWiz.step === 0, onClick: function () { extractWiz.step = Math.max(0, extractWiz.step - 1); paint(); } }),
        mbtn('关闭向导', { onClick: function () { if (sheet) sheet.close(); } })
      ], 2);
    }

    function stepBody() {
      var w = extractWiz, box = col([], 3);

      /* 1 选图（顺序无关；显示按 shard_index 排序后的顺序） */
      if (w.step === 0) {
        box.appendChild(cap('1 选图：顺序无关。嵌入时的选择顺序不代表提取需要保持的顺序 —— 外壳按 shard_index 排序后再拼接。'));
        D.stegoPaths.forEach(function (p) {
          var on = w.picks.some(function (x) { return x.shardIndex === p.shardIndex; });
          box.appendChild(a11y(h('button', {
            class: 'mrow', type: 'button', style: { borderRadius: '10px', border: '1px solid var(--c-border)' },
            onclick: function () {
              var i = -1;
              w.picks.forEach(function (x, k) { if (x.shardIndex === p.shardIndex) i = k; });
              if (i >= 0) w.picks.splice(i, 1); else w.picks.push(p);
              w.removedNotice = '';
              paint();
            }
          }, [
            h('span', { class: 'mr-ico', html: VS.icon('image', 22) }),
            h('div', { class: 'mr-main' }, [
              h('div', { class: 'mr-title', text: p.carrier }),
              h('div', { class: 'mr-sub', text: 'shard_index ' + F.num(p.shardIndex) + ' / ' + F.num(p.shardTotal) + ' · file_len ' + F.num(p.fileLen) })
            ]),
            on ? h('span', { html: VS.icon('check-circle', 18) }) : null
          ]), '选择载体 ' + p.carrier + (on ? '（已选）' : '')));
        });
        box.appendChild(h('div', { class: 'row gap-2 wrap' }, [
          mbtn('移除第 2 张', { variant: 'danger', flex: '1 1 140px', title: '演示：移除第 2 张（shard_index = 1），制造缺片', onClick: function () {
            w.picks = w.picks.filter(function (x) { return x.shardIndex !== 1; });
            w.removedNotice = '已移除第 ' + F.num(2) + ' 张（shard_index = ' + F.num(1) + '）';
            paint();
          } }),
          cap('已选 ' + F.num(w.picks.length) + ' 张')
        ]));
        if (w.picks.length) {
          box.appendChild(MUI.mkv([{ k: '按 shard_index 排序后的顺序', v: sortedPicks().map(function (p) { return F.num(p.shardIndex); }).join(' -> '), mono: true }]));
        }
        if (w.removedNotice) box.appendChild(cap(w.removedNotice));
        return box;
      }

      /* 2 目标路径 */
      if (w.step === 1) {
        var targetInput = UI.input({ value: w.target, onInput: function () { w.target = targetInput.value; } });
        var nameInput = UI.input({ value: w.name, onInput: function () { w.name = nameInput.value; } });
        box.appendChild(UI.field({ label: '还原目标路径', control: a11y(targetInput, '还原目标路径'), hint: 'Dart 只传路径：载荷拼接与 AEAD 全在 vault-core，外壳侧不见明文。' }));
        box.appendChild(UI.field({ label: '还原文件名', control: a11y(nameInput, '还原文件名'), hint: '载荷内 name 为空时使用该名称；不提取得任何路径信息。' }));
        box.appendChild(MUI.mkv([
          { k: 'shard_total', v: F.num(D.stegoPaths.length), mono: true },
          { k: '已选', v: F.num(w.picks.length), mono: true },
          { k: '缺片 missing[]', v: computeMissing().length ? computeMissing().map(F.num).join(', ') : '（无）', mono: true }
        ]));
        return box;
      }

      /* 3 执行 */
      if (w.step === 2) {
        box.appendChild(strong('3 执行'));
        box.appendChild(UI.bar(1, 'accent'));
        box.appendChild(cap('乱序重组：按 shard_index 排序后拼接；完成。'));
        return box;
      }

      /* 4 结果：成功 / 失败（缺片绝不静默补零或跳过） */
      box.appendChild(strong('4 结果'));
      var missing = (w.missing && w.missing.length) ? w.missing : computeMissing();
      if (missing.length) {
        box.appendChild(UI.alertbar({ tone: 'danger', icon: 'alert', title: 'badImages · missing[]', text: '缺片 missing = [' + missing.map(F.num).join(', ') + ']' }));
        box.appendChild(UI.alertbar({
          tone: 'danger', icon: 'ban', title: '绝不静默补零或跳过',
          text: '缺片一律失败：错误码 ' + F.num(6) + ' + missing[]。界面不补零、不跳过、不产出部分文件，也不把「大部分片已到」当成成功。'
        }));
        box.appendChild(MUI.mkv([
          { k: 'shard_total', v: F.num(D.stegoPaths.length), mono: true },
          { k: '已收到', v: F.num(D.stegoPaths.length - missing.length), mono: true },
          { k: 'missing[]', v: '[' + missing.map(F.num).join(', ') + ']', mono: true }
        ]));
        return box;
      }
      box.appendChild(UI.alertbar({ tone: 'info', icon: 'check', title: '提取成功', text: '乱序重组完成；逐片 shard_crc32 已校验。' }));
      box.appendChild(MUI.mkv([
        { k: 'name', v: D.stegoPlanSample.name },
        { k: 'size', v: F.bytes(D.stegoPlanSample.fileLen) + '（' + F.num(D.stegoPlanSample.fileLen) + ' B）', mono: true },
        { k: 'shards', v: F.num(D.stegoPaths.length), mono: true },
        { k: '目标路径', v: w.target + w.name }
      ]));
      if (demo.stegoLegacy) {
        box.appendChild(UI.alertbar({ tone: 'warn', icon: 'info', title: '该载荷为 V1.0 兼容格式', text: '兼容读：可按旧格式解析，字段语义与新版本一致。' }));
      } else if (demo.stegoNewer) {
        box.appendChild(UI.alertbar({ tone: 'danger', icon: 'ban', title: '当前客户端无法读取更高版本载荷', text: 'payload_ver 高于本客户端支持上限：如实拒绝，不做尽力解析、不猜测字段。' }));
      }
      return box;
    }

    paint();
    sheet = MUI.sheet({
      title: '隐写提取向导', sub: 'vault_core_stego_extract_multi（乱序重组 / 缺片报 missing[]）',
      size: 'tall', body: hostEl,
      footer: function (close) { return [mbtn('关闭向导', { onClick: function () { close(); } })]; }
    });
  }

  /* ---------------- 单图模式：超容量 -> 码 6，绝不截断 ---------------- */
  function openSingleImage(kind) {
    if (!S.get('stegoEnabled')) { openStegoDisabledSheet(); return; }
    var first = D.stegoPlanSample.perImage[0] || { path: '—', capacity: 0, usable: 0 };
    var body = col([], 3);
    if (kind === 'capacity') {
      body.appendChild(MUI.mkv([
        { k: '载体', v: first.path },
        { k: 'capacity', v: F.num(first.capacity) + ' B', mono: true },
        { k: 'usable（fillRatio ' + F.num(0.8) + '）', v: F.num(first.usable) + ' B', mono: true },
        { k: '来源接口', v: 'vault_core_stego_capacity（负值即错误码）', mono: true }
      ]));
      body.appendChild(cap('容量估算仅供 UI 预估：真值仍以 vault_core_stego_plan 为准。'));
    } else if (kind === 'embed') {
      var over = a11y(UI.input({ type: 'number', value: first.capacity + 1 }), '拟嵌入载荷字节数（默认超出容量以演示错误码 ' + F.num(6) + '）');
      body.appendChild(UI.field({
        label: '载荷字节数（默认 ' + F.num(first.capacity + 1) + '，超出单图容量）', control: over,
        hint: '超容量 -> 错误码 ' + F.num(6) + ' + 「payload exceeds image capacity」，绝不截断。'
      }));
      body.appendChild(mbtn('执行单图嵌入', { variant: 'danger', title: '执行单图嵌入（超容量报错误码 ' + F.num(6) + '）', onClick: function () {
        var n = parseInt(over.value, 10) || 0;
        if (n > first.capacity) {
          UI.toast({
            tone: 'danger', title: '错误码 ' + F.num(6) + ' · payload exceeds image capacity',
            msg: '载荷 ' + F.num(n) + ' B 超出单图 capacity ' + F.num(first.capacity) + ' B；如实拒绝，绝不截断。', duration: 6000
          });
        } else {
          UI.toast({ tone: 'success', title: '嵌入完成', msg: '单图模式 flags bit1 置位；已写审计（security 类别）' });
        }
      } }));
    } else {
      body.appendChild(MUI.mkv([
        { k: '载体', v: first.path, mono: true },
        { k: '模式', v: '单图（flags bit1 = 1，shard_total 应为 ' + F.num(1) + '）', mono: true },
        { k: '来源接口', v: 'vault_core_stego_extract', mono: true }
      ]));
      body.appendChild(h('div', { class: 'row gap-2 wrap' }, [
        mbtn('执行单图提取', { variant: 'primary', flex: '1 1 140px', title: '执行单图提取', onClick: function () { UI.toast({ tone: 'success', title: '单图提取成功', msg: '已按 flags bit1 走单图路径' }); } }),
        mbtn('演示：损坏载荷头', { flex: '1 1 140px', title: '演示载荷头损坏时的失败分支', onClick: function () { UI.toast({ tone: 'danger', title: '错误码 ' + F.num(6) + ' · 载荷头损坏', msg: '如实拒绝，不做尽力解析' }); } })
      ]));
    }
    MUI.sheet({
      title: kind === 'capacity' ? '单图容量估算' : (kind === 'embed' ? '单图嵌入' : '单图提取'),
      sub: 'vault_core_stego_' + (kind === 'capacity' ? 'capacity' : kind),
      size: 'tall', body: body,
      footer: function (close) { return [mbtn('关闭', { onClick: close })]; }
    });
  }

  /* ---------------- 批量区：逐项状态，不聚合 ---------------- */
  function openBatch(kind) {
    if (!S.get('stegoEnabled')) { openStegoDisabledSheet(); return; }
    var items = D.vault.entries.slice(0, 5).map(function (e, i) {
      var fail = i === 2;
      return {
        name: e.name, ok: !fail, code: fail ? (kind === 'embed' ? 7 : 6) : 0,
        note: fail ? (kind === 'embed' ? 'last_error = fits=false（需要更多载体）' : 'missing[] = [' + F.num(1) + ']') : '完成'
      };
    });
    var okCount = items.filter(function (x) { return x.ok; }).length;
    MUI.sheet({
      title: kind === 'embed' ? '批量嵌入（顺序调用）' : '批量提取（顺序调用）',
      sub: '逐项状态 · 不聚合为一个总状态', size: 'tall',
      body: col([
        UI.alertbar({
          tone: 'info', icon: 'info', title: '外壳侧顺序调用',
          text: '批量 = 对每个条目顺序调用单次接口；下列为逐项结果，UI 不把它折叠成一个「批量完成」。单项失败不影响其余项，也不回滚已成功的产物。'
        }),
        cap('成功 ' + F.num(okCount) + ' 项 · 失败 ' + F.num(items.length - okCount) + ' 项（逐项计数，非总状态）'),
        MUI.mlist(items.map(function (it) {
          return MUI.mrow({
            icon: it.ok ? 'check-circle' : 'x-circle', danger: !it.ok, chevron: false, title: it.name,
            sub: it.ok ? it.note : ('错误码 ' + F.num(it.code) + ' · ' + ((VS.ERR[it.code] || {}).title || '失败') + ' · ' + it.note),
            trail: [it.ok ? UI.badge({ text: '成功', tone: 'success' }) : UI.badge({ text: '失败', tone: 'danger', title: it.note })]
          });
        }))
      ], 3),
      footer: function (close) { return [mbtn('关闭', { onClick: close })]; }
    });
  }

  /* ======================================================================
   * 5. 全屏强告警（/alarm）· 阻塞式 · 语义化动作
   * ==================================================================== */
  var ALARM_SOURCES = {
    audit_verify: {
      key: 'audit_verify', tone: 'danger', icon: 'shieldoff',
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
      key: 'need_recovery', tone: 'danger', icon: 'recover',
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
  function openAlarmFullscreen(ctx, src, force) {
    var now = Date.now(), last = alarmSeen[src.key];
    if (!force && last && now - last < ALARM_IDEMPOTENT_MS) {
      UI.toast({ tone: 'info', title: '同源事件已幂等拦截', msg: src.key + '：距上次弹出 ' + F.num(Math.round((now - last) / 60000)) + ' 分钟，不再重复弹出' });
      return null;
    }
    alarmSeen[src.key] = now;
    if (alarmActive) { alarmActive.close(); alarmActive = null; }

    var content = h('div', { class: 'col gap-3', style: { width: '100%', alignItems: 'center' } }, [
      h('span', { style: { color: 'var(--c-danger)' }, html: VS.icon(src.icon, 52) }),
      h('div', { class: 't-h1', text: src.conclusion })
    ]);
    src.reasons.slice(0, 3).forEach(function (r) { content.appendChild(h('div', { class: 't-body', text: r })); });
    content.appendChild(h('div', { class: 't-caption t-mono', text: '数据来源：' + src.source }));
    content.appendChild(h('div', { class: 't-caption', text: '同源事件 ' + F.num(5) + ' 分钟内幂等 · 无「点任意处关闭」' }));

    var primary = mbtn(src.primary.label, {
      variant: 'danger', style: { minHeight: '52px', fontSize: '16px' }, title: src.primary.label,
      onClick: function () {
        if (alarmActive) { alarmActive.close(); alarmActive = null; }
        if (src.primary.action && VS.actions[src.primary.action]) { VS.actions[src.primary.action](ctx); return; }
        if (src.primary.route) go(ctx, src.primary.route, src.primary.params);
      }
    });
    var secondary = mbtn(src.secondary.label, {
      style: { minHeight: '48px' }, title: src.secondary.label,
      onClick: function () {
        if (alarmActive) { alarmActive.close(); alarmActive = null; }
        if (src.secondary.action && VS.actions[src.secondary.action]) { VS.actions[src.secondary.action](ctx); return; }
        if (src.secondary.route) go(ctx, src.secondary.route);
        if (src.secondary.close) UI.toast({ tone: 'info', title: '已确认', msg: '告警已记录；未做任何修复动作' });
      }
    });
    content.appendChild(h('div', { class: 'col gap-2', style: { width: '100%', marginTop: '8px' } }, [primary, secondary]));

    alarmActive = MUI.mfullscreen({ tone: 'danger', content: content, dismissible: false });
    return alarmActive;
  }

  var alarmPage = function (ctx) {
    ctx = ctx || { params: {}, go: null };
    rebuild = null;
    var srcKey = (ctx.params && (ctx.params.src || ctx.params.source)) || alarmSrc;
    var src = ALARM_SOURCES[srcKey] || ALARM_SOURCES.audit_verify;
    alarmSrc = src.key;
    if (!alarmActive) setTimeout(function () { openAlarmFullscreen(ctx, src, false); }, 0);

    return MUI.page({
      title: '全屏强告警',
      sub: '/alarm · 阻塞式 · 语义化动作 · 同源事件 ' + F.num(5) + ' 分钟幂等',
      back: true, tab: false,
      actions: [{ icon: 'danger', label: '再次触发告警', onClick: function () { openAlarmFullscreen(ctx, src, true); } }],
      body: MUI.screen([
        UI.alertbar({
          tone: 'danger', icon: 'danger', title: '阻塞式全屏告警（/alarm）',
          text: 'danger 级事件以全屏阻塞方式呈现：无关闭按钮、Esc 无效、导航栈不可返回；必须选择一个有语义的动作才能离开。禁止「点任意处关闭」。'
        }),
        MUI.mcard({
          title: '当前来源', sub: 'src = ' + src.key,
          body: MUI.mkv([
            { k: '结论', v: src.conclusion },
            { k: '数据来源', v: src.source, mono: true },
            { k: '主要动作', v: src.primary.label, mono: true },
            { k: '次要动作', v: src.secondary.label, mono: true },
            { k: '幂等窗口', v: F.num(5) + ' 分钟（同源事件）', mono: true }
          ])
        }),
        MUI.mcard({
          title: '演示：两种来源', sub: '审计链中断 / 启动恢复未完成（码 ' + F.num(11) + '）· 长按条目切换来源',
          body: col(Object.keys(ALARM_SOURCES).map(function (k) {
            var s = ALARM_SOURCES[k];
            return MUI.mrow({
              icon: s.icon, chevron: false,
              title: k === 'audit_verify' ? '审计链中断' : ('启动恢复未完成（码 ' + F.num(11) + '）'),
              sub: s.conclusion + ' · ' + s.source,
              trail: k === src.key ? [UI.badge({ text: '当前', tone: 'accent' })] : null,
              onLongPress: function () { openAlarmFullscreen(ctx, s, true); }
            });
          }), 2)
        }),
        MUI.mcard({
          title: '同源事件 ' + F.num(5) + ' 分钟幂等',
          body: col([
            cap('同一来源事件在 ' + F.num(5) + ' 分钟内只弹一次；重复到达的事件被幂等拦截并记录，不重复打扰用户。'),
            mbtn('强制再次弹出（忽略幂等）', { title: '强制再次弹出该来源的全屏告警', onClick: function () { openAlarmFullscreen(ctx, src, true); } }),
            mbtn('重置幂等记录', { title: '清空幂等记录（演示）', onClick: function () { alarmSeen = {}; UI.toast({ tone: 'info', title: '已重置', msg: '幂等记录已清空（演示）' }); } })
          ], 3)
        })
      ]),
      onBack: function () { if (VS.nav) VS.nav.back(); }
    });
  };
  VS.pages['alarm'] = alarmPage;

  /* ======================================================================
   * 6. 演示控制台（移动端 sheet）
   * ==================================================================== */
  function openDemoSheet() {
    var m = MUI.sheet({
      title: '演示控制台', sub: '切换状态与能力位以验收呈现', size: 'tall',
      body: col([
        MUI.mcard({
          title: '会话与租约', body: col([
            demoToggle('只读降级（码 ' + F.num(9) + '）', '另一实例持有写租约 -> 写操作禁用 + reason', S.get('readOnly'), function (v) {
              S.set({ readOnly: v, readonlyReason: v ? '另一实例持有写租约（演示）' : '' }, true); m.close(); attr();
            }),
            demoToggle('维护态（码 ' + F.num(10) + '）', '业务写入冻结；擦除 / 销毁仍允许', inMaintenance(), function (v) {
              S.set({ maintenance: v ? 'rotation' : null }, true);
              S.set({ session: v ? 'Maintenance' : 'Active' }, true);
              m.close(); attr();
            })
          ], 3)
        }),
        MUI.mcard({
          title: '审计 / 擦除 / 时钟', body: col([
            demoToggle('审计链校验失败', 'danger + brokenAt 段边界 + 取证引导（不自动修复）', demo.auditBroken, function (v) { demo.auditBroken = v; m.close(); attr(); }),
            demoToggle('审计连续写入失败', '灰态「审计不可用」，不显示条数、不视为通过', demo.auditDown, function (v) { demo.auditDown = v; m.close(); attr(); }),
            demoToggle('物理删除失败', '待清理条目显示「介质擦除待完成」，不得显示「已擦除」', demo.physicalDeleteFailed, function (v) { demo.physicalDeleteFailed = v; m.close(); attr(); }),
            demoToggle('去掉平台安全删除位', '验证「安全擦除」字样必须消失（method_bits 无平台位）', demo.maskPlatformBit, function (v) { demo.maskPlatformBit = v; m.close(); attr(); }),
            demoToggle('系统时钟异常', '显示「检测到系统时钟异常（超前 > ' + F.num(24) + 'h）」需显式确认', demo.clockSkew, function (v) { demo.clockSkew = v; m.close(); attr(); })
          ], 3)
        })
      ], 3),
      footer: function (close) {
        return [
          mbtn('触发全屏强告警（/alarm）', { icon: 'danger', variant: 'danger', title: '触发全屏强告警', onClick: function () { close(); go({ params: {}, go: null }, 'alarm', { src: 'audit_verify' }); } }),
          mbtn('关闭', { onClick: function () { close(); } })
        ];
      }
    });
    return m;
  }

  /* ======================================================================
   * 7. 动作注册（外壳 / 通知 / 命令面板可调用）
   * ==================================================================== */
  VS.actions['security.scanIntegrity'] = function (ctx) { openFullScan(ctx || { params: {}, go: null }); };
  VS.actions['security.verifyAudit'] = function (ctx) { openAuditVerify(ctx || { params: {}, go: null }); };
  VS.actions['security.exportAudit'] = function (ctx) { exportAuditBundle(ctx || { params: {}, go: null }); };
  VS.actions['security.demoPanel'] = function () { openDemoSheet(); };
  VS.actions['security.destroy'] = function (ctx) {
    S.set({ secTab: 'destroy' }, true);
    go(ctx || { params: {}, go: null }, 'security', { tab: 'destroy' });
  };
  VS.actions['alarm.open'] = function (ctx) {
    ctx = ctx || { params: {}, go: null };
    var key = (ctx.params && (ctx.params.src || ctx.params.source)) || alarmSrc;
    openAlarmFullscreen(ctx, ALARM_SOURCES[key] || ALARM_SOURCES.audit_verify, true);
  };
  VS.actions['stego.enable'] = function () { if (!S.get('stegoEnabled')) confirmEnableStego(); };
  VS.actions['stego.disable'] = function () { if (S.get('stegoEnabled')) confirmDisableStego(); };
  VS.actions['stego.embed'] = function (ctx) {
    ctx = ctx || { params: {}, go: null };
    if (!S.get('stegoEnabled')) { openStegoDisabledSheet(); return; }
    go(ctx, 'stego');
    openEmbedWizard(ctx);
  };
  VS.actions['stego.extract'] = function (ctx) {
    ctx = ctx || { params: {}, go: null };
    if (!S.get('stegoEnabled')) { openStegoDisabledSheet(); return; }
    go(ctx, 'stego');
    openExtractWizard();
  };
  VS.actions['stego.capacity'] = function () { openSingleImage('capacity'); };

})(window);
