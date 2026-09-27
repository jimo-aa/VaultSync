/* ============================================================================
 * VaultSync V2.0 原型 · 移动端系统层视图（注册于 VS.pages / VS.actions）
 *   settings + 12 个设置子页 / license / recover /
 *   maintenance-rotate / maintenance-migrate / diagnostics / about
 *
 * 语料依据：docs/v2.0/03 §2.2 §4.3（手机窄屏主壳）、04 §1.2 与 §六（各端展示差异矩阵）、
 *           05-0X、06 D9、07、08、11 §二（诚实纪律）、12（授权与订阅）
 * 形态纪律（移动端固有，不因能力位改变）：
 *   · 二级 / 三级界面一律底部面板：MUI.sheet / MUI.actionSheet / MUI.confirmSheet；
 *     本文件**不出现 UI.modal / UI.confirm / UI.confirmPhrase**（桌面端居中弹窗）。
 *   · 设置 = 分组列表（mgroupTitle + mlist + mrow），每项 chevron 进全屏子页；
 *     列表项长按 → MUI.actionSheet。
 *   · 触控目标 ≥ 44×44；页边距走 --sp-4（由 .screen 提供）。
 * 诚实纪律：本文件出现的每个读数都要能回答「读的是哪个接口」；答不出来就
 *   标 tierMark('design')，架构上没有的就整块不渲染（tierMark('hidden') 登记结论）。
 * ==========================================================================*/
(function (global) {
  'use strict';

  var VS = global.VS;
  var h = VS.util.h, UI = VS.ui, MUI = VS.mui, F = VS.fmt, D = VS.data, S = VS.store, U = VS.util;

  VS.pages = VS.pages || {};
  VS.actions = VS.actions || {};

  var NOW = D.NOW, MIN = D.MIN, DAY = D.DAY;

  /* ======================================================================
   * 0. 局部原语（VS.mui 里没有、或需要移动端化包装的）
   * ==================================================================== */

  /** 路由跳转：优先 ctx.go，其次 VS.nav.go，最后写 store */
  function go(ctx, route, params) {
    if (ctx && typeof ctx.go === 'function') { ctx.go(route, params); return; }
    if (VS.nav && typeof VS.nav.go === 'function') { VS.nav.go(route, params); return; }
    S.set({ route: route, routeParams: params || {} });
  }

  /** 写操作判据唯一来源：VS.derive.denyWrite() → 9 只读 / 10 维护 / 5 未解锁 / null 允许 */
  function denyReason() {
    var c = VS.derive.denyWrite();
    if (c === 9) return '只读降级：另一实例持有写租约（错误码 9）';
    if (c === 10) return '维护态：业务写入已冻结（错误码 10）';
    if (c === 5) return '保险箱未解锁（错误码 5）';
    return null;
  }
  function frozen() { return denyReason() !== null; }
  function frozenNote() {
    var r = denyReason();
    return r ? MUI.limitedNote('写操作当前不可用 · ' + r) : null;
  }
  function isMaintenance() {
    return !!S.get('maintenance') || S.get('session') === 'Maintenance';
  }

  /** 整宽按钮（触控 ≥48px） */
  function mbtn(label, o) {
    o = o || {};
    var tip = o.title || label;
    return h('button', {
      class: 'mbtn-block', type: 'button',
      dataset: { variant: o.variant || 'primary' },
      disabled: !!o.disabled,
      title: tip, 'aria-label': tip,
      onclick: o.disabled ? null : (o.onClick || null)
    }, [o.icon ? h('span', { html: VS.icon(o.icon, 18) }) : null, h('span', { text: label })]);
  }

  /** 受写冻结保护的整宽按钮：冻结 → 禁用 + reason，而不是假装成功 */
  function guardBtn(o) {
    o = o || {};
    var r = denyReason();
    if (r && !o.ignoreFrozen) {
      return mbtn(o.label, { variant: o.variant, icon: o.icon, disabled: true, title: o.label + ' · ' + r });
    }
    return mbtn(o.label, { variant: o.variant, icon: o.icon, title: o.title || o.label, onClick: o.onClick });
  }

  /** 数字：0 / 空 / 缺失 → 灰（绝不把「没有」显示成 0，也不显示绿灯） */
  function numOrDash(v) {
    if (v === null || v === undefined || v === '' || v === 0) return '—';
    return F.num(v);
  }
  function designMark(title) {
    var m = UI.tierMark('design', '设计保证');
    m.title = title || '本项在原型数据源中没有对应字段，按设计保证标注';
    return m;
  }
  function hiddenMark(title) {
    var m = UI.tierMark('hidden', '不渲染');
    m.title = title || '后端无此接口，本端不渲染该行';
    return m;
  }

  /** 取节点内的全部 button（优先 querySelectorAll；原型外壳里 mseg/select 的子节点也是 button） */
  function allButtons(el) {
    var out = [];
    if (!el) return out;
    if (el.querySelectorAll) {
      Array.prototype.forEach.call(el.querySelectorAll('button'), function (b) { out.push(b); });
    }
    if (!out.length && el.children) {
      Array.prototype.forEach.call(el.children, function (c) {
        if (c && c.tagName === 'BUTTON') out.push(c);
      });
    }
    return out;
  }

  /** 分段控件：触控目标抬到 44px（mobile.css 的 .mseg 默认 34px） */
  function seg(o) {
    var el = MUI.mseg(o);
    allButtons(el).forEach(function (b) {
      b.style.minHeight = '44px';
      b.title = b.textContent || '';
      b.setAttribute('aria-label', b.textContent || '');
    });
    return el;
  }

  /** 可导航行（chevron）：补齐 title / aria-label */
  function navRow(o) {
    var r = MUI.mrow(o);
    var tip = o.title || o.label || '';
    r.title = tip;
    r.setAttribute('aria-label', o.ariaLabel || tip);
    return r;
  }

  /** 只读信息行（不可点） */
  function infoRow(o) {
    o = o || {};
    var tip = o.title || o.sub || o.label;
    return h('div', { class: 'mrow', style: { cursor: 'default' }, title: tip }, [
      o.icon ? h('span', { class: 'mr-ico', html: VS.icon(o.icon, 22) }) : null,
      h('div', { class: 'mr-main' }, [
        h('div', { class: 'mr-title' }, [h('span', { class: 't-truncate', text: o.label }), o.mark || null]),
        o.sub ? h('div', { class: 'mr-sub', text: o.sub }) : null
      ]),
      h('span', { class: 'mr-trail' }, [
        o.badge || null,
        o.node || null,
        o.value !== undefined && o.value !== null && o.value !== ''
          ? h('span', { class: o.mono ? 't-mono' : 't-caption', text: o.value })
          : null
      ])
    ]);
  }

  /** 开关行：左侧说明，右侧开关（不可用 → disabled + reason，绝不静默失败） */
  function switchRow(o) {
    o = o || {};
    var ctl = UI.switchCtl({
      checked: !!o.checked, disabled: !!o.disabled,
      onChange: o.disabled ? null : o.onChange
    });
    var tip = o.disabled && o.reason ? ('不可用：' + o.reason) : (o.title || o.label);
    ctl.title = tip;
    if (ctl.input) { ctl.input.title = tip; ctl.input.setAttribute('aria-label', o.label); }
    return h('div', {
      class: 'mrow', style: { cursor: 'default' },
      dataset: { disabled: o.disabled ? 'true' : '' },
      'aria-disabled': o.disabled ? 'true' : null,
      title: tip
    }, [
      h('div', { class: 'mr-main' }, [
        h('div', { class: 'mr-title' }, [h('span', { class: 't-truncate', text: o.label }), o.mark || null]),
        o.sub ? h('div', { class: 'mr-sub', text: o.sub }) : null,
        o.disabled && o.reason ? h('div', { class: 'mr-sub', style: { color: 'var(--c-stale)' }, text: o.reason }) : null
      ]),
      h('span', { class: 'mr-trail' }, [ctl])
    ]);
  }

  /** 标签在上、控件在下的一行（分段 / 输入这类宽控件） */
  function ctlBlock(o) {
    o = o || {};
    return h('div', {
      class: 'mrow',
      style: { cursor: 'default', flexDirection: 'column', alignItems: 'stretch', gap: '8px' }
    }, [
      h('div', { class: 'mr-main' }, [
        h('div', { class: 'mr-title' }, [h('span', { text: o.label }), o.mark || null]),
        o.sub ? h('div', { class: 'mr-sub', text: o.sub }) : null
      ]),
      o.node
    ]);
  }

  /** 移动端维护横幅（自带实现：app.js 会重定义 MUI.maintBanner 为 3 参版本，不依赖它） */
  function maintBanner(o) {
    o = o || {};
    return h('div', { class: 'maint-banner', dataset: { tone: o.tone || 'maintenance' }, role: 'status' }, [
      h('span', { class: 'mb-ico', html: VS.icon(o.icon || (o.tone === 'stale' ? 'lock' : 'refresh'), 18) }),
      h('div', { class: 'mb-text' }, [
        h('div', { class: 'mb-title', text: o.title || '' }),
        o.sub ? h('div', { class: 'mb-sub', text: o.sub }) : null,
        o.progress !== undefined && o.progress !== null
          ? h('div', { style: { marginTop: '6px' } }, [MUI.bar(o.progress, 'maintenance', 'sm')])
          : null
      ])
    ]);
  }

  /** 分组：标题 + 列表（+ 可选脚注） */
  function group(title, rows, note) {
    var out = [MUI.mgroupTitle(title), MUI.mlist(rows.filter(Boolean))];
    if (note) out.push(MUI.mgroupNote(note));
    return out;
  }

  /** 系统动效偏好：浏览器媒体查询真实读数（不是 mock） */
  function reducedMotion() {
    try { return !!(global.matchMedia && global.matchMedia('(prefers-reduced-motion: reduce)').matches); }
    catch (e) { return false; }
  }

  /** 移动端本端外壳约束（docs/v2.0/04 §1.2 与 §六）：不因能力位开关而改变 */
  var MOBILE_FACTS = {
    bgSync: '移动端后台同步为尽力而为：系统可能在省电模式暂停',
    bgSyncSub: '前台优先 + 系统调度式刷新（Android WorkManager / iOS BGAppRefreshTask），间隔由 OS 决定；被系统拒绝时不报错，下次前台继续。',
    inbound: '仅前台监听；后台不保持 P2P 入站连接',
    noSplit: '本端无分栏视图：视图切换器只显示可用项（不置灰），因此外观设置里没有分栏选项。',
    screenshot: 'FLAG_SECURE（Android）/ iOS 遮罩；来源：docs/v2.0/04 §六 差异矩阵',
    managed: 'CAP_MOBILE_MANAGED · 受管外壳：无独立守护进程，后台刷新由系统调度决定是否执行'
  };

  /* ---------------------------------------------------------------------
   * 0.1 页面本地状态
   *     仅在「data.js 确实没有该字段」时启用，且在使用点标注来源。
   * ------------------------------------------------------------------- */
  var L = {
    /* 解锁与落锁 */
    idleLock: !!(D.settings && D.settings.lockPolicy && D.settings.lockPolicy.idle),
    idleMinutes: Math.round((D.settings && D.settings.autoLockIdleMs ? D.settings.autoLockIdleMs : 15 * MIN) / MIN),
    awayLock: true,
    /* 「离开应用落锁 N 秒」在 data.js 中无对应字段 → 本地默认 30 秒（5–300） */
    awaySeconds: 30,
    lockOnScreenSaver: !!(D.settings && D.settings.lockOnScreenSaver),
    lockOnSleep: !!(D.settings && D.settings.lockOnSleep),
    lockOnUsbRemoval: !!(D.settings && D.settings.lockOnUsbRemoval),
    /* 生物识别：CAP_BIO 在 core.js 中已登记（bit 12）；错误码 4 / 5 由原型注入 */
    bioBound: true,
    bioCode: null,
    /* 伪装入口 */
    disguiseEnabled: !!(D.settings && D.settings.disguiseEnabled),
    /* 通知矩阵：移动端为「应用内 + 系统推送 + 邮件」三渠道（不是「系统通知」） */
    notify: {
      info: { app: !!(D.settings && D.settings.notifications.info), push: true, mail: false },
      warn: { app: !!(D.settings && D.settings.notifications.warn), push: true, mail: true },
      danger: { app: true, push: true, mail: true }
    },
    dndEnabled: !!(D.settings && D.settings.dndEnabled),
    dndStart: (D.settings && D.settings.dndStart) || '23:00',
    dndEnd: (D.settings && D.settings.dndEnd) || '07:00',
    /* 外观 */
    theme: (D.settings && D.settings.theme) || 'system',
    locale: (D.settings && D.settings.locale) || 'zh-CN',
    density: 'comfortable',
    /* 隐私 */
    screenshotProtect: !!(D.settings && D.settings.screenshotProtect),
    thumbnails: !!(D.settings && D.settings.thumbnailsEnabled),
    telemetry: !!(D.settings && D.settings.telemetry),      /* 默认关闭 */
    /* 从属密钥多选 */
    keysSel: {},
    /* 库格式版本（引擎头部读数；data.migrationState.srcVer） */
    formatVersion: D.migrationState.srcVer,
    /* 轮换页原型注入（无接口可读的两种态） */
    rotHasOldMk: true,
    rotAutoResume: 0,
    rotConflict: false,
    /* 恢复向导 */
    recStep: 0, recPath: 'rollback', recConfirmed: false, recAttempts: 0, recProgress: 0,
    /* 迁移向导 */
    migRunning: false, migBackupDays: 7,
    /* 授权：中继凭据兑换状态机 */
    relayExchange: (D.licenseState && D.licenseState.exchangeState) || 'idle'
  };

  /* ======================================================================
   * 从属密钥表（key_id 1–8；9–255 为保留区间 → 整块不渲染）
   * 语料：docs/v2.0/07（从属密钥层，独立包装于 VSVB v3 头部）
   * ==================================================================== */
  var SUB_KEYS = [
    { id: 1, use: '索引密钥（Index）', gen: NOW - 121 * DAY, wrap: 'wrapped' },
    { id: 2, use: '搜索密钥（Search）', gen: NOW - 121 * DAY, wrap: 'wrapped' },
    { id: 3, use: '分享密钥（Share）', gen: NOW - 96 * DAY, wrap: 'wrapped' },
    { id: 4, use: '订单信封密钥（Order）', gen: NOW - 96 * DAY, wrap: 'wrapped' },
    { id: 5, use: '隐写载荷密钥（Stego）', gen: NOW - 62 * DAY, wrap: 'wrapped' },
    { id: 6, use: '链键（AuditChain）', gen: NOW - 121 * DAY, wrap: 'wrapped' },
    { id: 7, use: '审计导出密钥（Export）', gen: NOW - 45 * DAY, wrap: 'rotation_pending' },
    { id: 8, use: '发现指纹盐（DiscoverySalt）', gen: NOW - 30 * DAY, wrap: 'wrapped' }
  ];
  /* 选中 {1,2,6,7} 时必须处于维护态，否则禁用并给 reason */
  var SUB_KEYS_NEED_MAINT = [1, 2, 6, 7];
  var SUB_WRAP_LABEL = {
    wrapped: { label: '已包装', tone: 'success' },
    rotation_pending: { label: '待重包装', tone: 'warning' }
  };

  /* ======================================================================
   * A. settings — Tab 根页（分组列表）
   * ==================================================================== */
  var APP_VERSION = '2.0.0';               /* 原型本地常量：无对应接口读数 */
  var APP_BUILD = '2026.09.15-prototype';  /* 原型本地常量：无对应接口读数 */

  function syncSummary() {
    var p = D.syncPolicy;
    var mode = p.mode === 'auto' ? '自动' : (p.mode === 'manual' ? '手动' : '定时');
    return mode + ' · 窗口 ' + (p.timeWindowEnabled ? (p.windowStart + '–' + p.windowEnd) : '不限') +
      ' · 并发 ' + F.num(p.maxConcurrent);
  }
  /** sync-policy 路由不存在时兜底到 sync 页 */
  function goSyncPolicy(ctx) {
    if (VS.pages['sync-policy']) { go(ctx, 'sync-policy'); return; }
    go(ctx, 'sync');
    if (!VS.pages['sync']) {
      UI.toast({ tone: 'info', title: '同步策略', msg: '本端未注册 sync-policy / sync 路由' });
    } else {
      UI.toast({ tone: 'info', title: '同步策略', msg: '已跳到「同步」页（本端未注册 sync-policy 路由）' });
    }
  }

  var LICENSE_STATE = {
    active: { label: '生效中', tone: 'success' },
    grace: { label: '宽限期', tone: 'warning' },
    expired: { label: '已到期', tone: 'danger' },
    revoked: { label: '已撤销', tone: 'danger' },
    offline_grace: { label: '离线宽限', tone: 'warning' }
  };

  /** 导出诊断包（脱敏）：移动端一律底部面板 */
  function exportDiagSheet() {
    MUI.sheet({
      title: '导出诊断包（脱敏）',
      sub: '只含计数与哈希前缀',
      body: [
        MUI.mkv([
          { k: '包含', v: '计数 · 错误码汇总 · 哈希前缀（前 6 位）· 版本与构建号' },
          { k: '不含', v: '文件路径原文 · 文件名原文 · 查询词 · 密钥材料 · 明文' },
          { k: '默认上报', v: '关闭（需显式导出）' }
        ]),
        h('div', { class: 'mt-3' }, [MUI.muted('诊断包用于取证与排障；导出本身会写一条 system 类审计（审计写入失败不阻断业务）。')])
      ],
      footer: function (close) {
        return [
          mbtn('导出到所选路径', {
            variant: 'primary',
            onClick: function () { close(); UI.toast({ tone: 'success', title: '诊断包已导出', msg: '原型演示：未写出真实文件' }); }
          }),
          mbtn('取消', { variant: 'ghost', onClick: close })
        ];
      }
    });
  }

  VS.pages['settings'] = function (ctx) {
    var kids = [];
    var a = (D.settings && D.settings.advanced) || {};
    var ls = D.licenseState, lmeta = LICENSE_STATE[ls.state] || { label: ls.state, tone: '' };
    var inRotation = S.get('maintenance') === 'rotation';

    /* ---- 1. 账户与安全 ---- */
    kids = kids.concat(group('账户与安全', [
      navRow({
        icon: 'lock', label: '解锁与落锁', sub: '空闲超时 ' + F.num(L.idleMinutes) + ' 分钟 · 离开应用 ' + F.num(L.awaySeconds) + ' 秒',
        onClick: function () { go(ctx, 'settings-lock'); }
      }),
      /* CAP_BIO 未置位 → 入口整块不渲染（不是置灰） */
      S.cap('CAP_BIO') ? navRow({
        icon: 'fingerprint', label: '生物识别',
        sub: L.bioCode === 4 ? '安全存储不可用（码 4）→ 本区块隐藏' : '生物识别为主路径；不可用时该区块隐藏而非置灰',
        onClick: function () { go(ctx, 'settings-bio'); }
      }) : null,
      navRow({ icon: 'key', label: '主密码', sub: '零重加密：MK 永不变更，仅重封 KEK', onClick: function () { go(ctx, 'settings-password'); } }),
      navRow({ icon: 'eye-off', label: '伪装入口', sub: '与主密码共用同一输入控件，冷却计数共用', onClick: function () { go(ctx, 'settings-disguise'); } }),
      navRow({ icon: 'hash', label: '从属密钥管理', sub: '8 条（key_id 1–8）；9–255 保留区间不渲染', onClick: function () { go(ctx, 'settings-keys'); } }),
      navRow({
        icon: 'refresh', label: 'MK 轮换',
        sub: inRotation ? '轮换进行中（维护态）' : '四阶段：头部标记 → 数据换钥 → 传播 → 完成',
        onClick: function () { go(ctx, 'settings-rotate'); }
      }),
      /* 已是 v3 → 该行不渲染（不写「已是最新」） */
      L.formatVersion < 3 ? navRow({
        icon: 'migrate', label: '格式与迁移',
        sub: 'VSVB v' + F.num(L.formatVersion) + ' → v' + F.num(D.migrationState.dstVer),
        onClick: function () { go(ctx, 'settings-migrate'); }
      }) : null
    ]));

    /* ---- 2. 同步 ---- */
    kids.push(MUI.mgroupTitle('同步'));
    var syncRows = [
      navRow({ icon: 'sync', label: '同步策略', sub: syncSummary(), onClick: function () { goSyncPolicy(ctx); } })
    ];
    if (S.cap('CAP_SELECTIVE_SYNC')) {
      syncRows.push(navRow({
        icon: 'filter', label: '选择性同步',
        sub: '按文件夹包含 / 排除；同给会被原子拒绝（错误码 7）',
        onClick: function () { goSyncPolicy(ctx); }
      }));
    }
    kids.push(MUI.mlist(syncRows));
    /* 后台同步：只读说明行（固定文案，不得改写） */
    kids.push(MUI.mlist([
      infoRow({
        icon: 'clock', label: '后台同步说明', value: '只读',
        sub: MOBILE_FACTS.bgSync, title: MOBILE_FACTS.bgSync
      }),
      infoRow({
        icon: 'wifi', label: '持续监听（P2P 入站）', value: '仅前台',
        sub: MOBILE_FACTS.inbound, title: MOBILE_FACTS.inbound
      }),
      S.cap('CAP_MOBILE_MANAGED') ? infoRow({
        icon: 'cpu', label: '受管后台', value: 'CAP_MOBILE_MANAGED',
        sub: MOBILE_FACTS.managed, mark: UI.tierMark('real')
      }) : null
    ].filter(Boolean)));
    kids.push(MUI.limitedNote(MOBILE_FACTS.bgSync + '。' + MOBILE_FACTS.bgSyncSub));

    /* ---- 3. 通知 ---- */
    kids.push(MUI.mgroupTitle('通知'));
    kids.push(MUI.mlist([
      navRow({
        icon: 'bell', label: '通知渠道矩阵',
        sub: notifyMatrixSummary(),
        onClick: function () { go(ctx, 'settings-notifications'); }
      }),
      switchRow({
        label: '不打扰时段',
        sub: L.dndStart + ' – ' + L.dndEnd + '（仅抑制非危险级；危险级照常投递）',
        checked: L.dndEnabled,
        onChange: function (e) { L.dndEnabled = e.target.checked; }
      })
    ]));

    /* ---- 4. 外观与语言 ---- */
    kids.push(MUI.mgroupTitle('外观与语言'));
    kids.push(MUI.mlist([
      ctlBlock({
        label: '主题', sub: '跟随系统 / 亮 / 暗',
        node: seg({
          value: L.theme,
          items: [
            { value: 'system', label: '跟随系统', icon: 'monitor' },
            { value: 'light', label: '亮', icon: 'sun' },
            { value: 'dark', label: '暗', icon: 'moon' }
          ],
          onChange: function (v) {
            L.theme = v;
            if (v === 'light' || v === 'dark') { S.set({ theme: v }); document.documentElement.dataset.theme = v; }
            UI.toast({ tone: 'info', title: '主题已切换', msg: v === 'system' ? '跟随系统（原型演示）' : v });
          }
        })
      }),
      navRow({ icon: 'languages', label: '语言', sub: L.locale === 'en' ? 'English' : '简体中文', onClick: function () { go(ctx, 'settings-appearance'); } }),
      navRow({
        icon: 'loader', label: '动效偏好',
        sub: reducedMotion() ? '已开启 prefers-reduced-motion → 入场动画关闭' : '未开启 prefers-reduced-motion',
        onClick: function () { go(ctx, 'settings-appearance'); }
      }),
      navRow({ icon: 'list', label: '密度', sub: densityLabel(L.density), onClick: function () { go(ctx, 'settings-appearance'); } })
    ]));
    kids.push(MUI.mgroupNote(MOBILE_FACTS.noSplit));

    /* ---- 5. 隐私与安全 ---- */
    kids.push(MUI.mgroupTitle('隐私与安全'));
    kids.push(MUI.mlist([
      infoRow({
        icon: 'shield', label: '截屏保护（本端能力）', value: '支持',
        badge: UI.badge({ text: '本端', tone: 'success' }),
        sub: MOBILE_FACTS.screenshot, title: MOBILE_FACTS.screenshot
      }),
      switchRow({
        label: '缩略图缓存',
        sub: 'LRU 上限比桌面更小；落锁即清，不落盘（CAP_THUMBNAIL ' + (S.cap('CAP_THUMBNAIL') ? '已置位' : '未置位') + '）',
        checked: L.thumbnails,
        disabled: !S.cap('CAP_THUMBNAIL'),
        reason: 'CAP_THUMBNAIL 未置位：本端不生成缩略图',
        onChange: function (e) { L.thumbnails = e.target.checked; }
      }),
      switchRow({
        label: '脱敏诊断上报',
        sub: L.telemetry ? '已开启：仅上报计数与哈希前缀' : '默认关闭；开启后仅上报计数与哈希前缀',
        checked: L.telemetry,
        onChange: function (e) { L.telemetry = e.target.checked; }
      }),
      navRow({ icon: 'settings', label: '隐私与安全（口径详情）', sub: '脱敏口径 · 缓存放行 · 截屏保护来源', onClick: function () { go(ctx, 'settings-privacy'); } })
    ]));

    /* ---- 6. 高级 ---- */
    kids.push(MUI.mgroupTitle('高级'));
    kids.push(MUI.mlist([
      navRow({ icon: 'refresh', label: '更新通道', sub: (D.settings && D.settings.updateChannel) || 'stable', onClick: function () { go(ctx, 'settings-update'); } }),
      navRow({ icon: 'cpu', label: '诊断', sub: 'vault_core_diagnostics 快照 · ERROR_DIAG · 事件流', onClick: function () { go(ctx, 'diagnostics'); } }),
      navRow({
        icon: 'gauge', label: '工作 Isolate 与渲染节流',
        sub: '只读：Isolate ' + (a.workIsolate ? '开启' : '关闭') + ' · 节流 ' + F.num(a.renderThrottleMs) + ' ms',
        onClick: function () { go(ctx, 'settings-advanced'); }
      })
    ]));

    /* ---- 7. 关于与授权 ---- */
    kids.push(MUI.mgroupTitle('关于与授权'));
    kids.push(MUI.mlist([
      navRow({ icon: 'info', label: '关于', sub: '版本 / 构建 / 证据入口四层 / 能力三档清单', onClick: function () { go(ctx, 'about'); } }),
      navRow({ icon: 'book', label: '开源许可', sub: '逐组件许可文本随发行包提供', onClick: function () { go(ctx, 'about'); } }),
      navRow({
        icon: 'award', label: '授权与订阅',
        sub: String(ls.tier).toUpperCase() + ' · ' + lmeta.label + ' · 席位 ' + F.num(ls.seatsUsed) + '/' + F.num(ls.seats),
        onClick: function () { go(ctx, 'license'); }
      })
    ]));

    /* ---- 8. 底部：版本号 + 减配声明 ---- */
    kids.push(h('div', { class: 'mgroup-note', style: { paddingTop: '8px' } }, [
      h('div', { class: 't-mono', text: 'VaultSync ' + APP_VERSION + ' · 构建 ' + APP_BUILD }),
      h('div', { text: '本端为移动端（<720 档）：减配项已如实标注' })
    ]));

    return MUI.page({
      title: '设置',
      sub: null,
      back: false,
      actions: [{
        icon: 'sparkle', label: '演示控制台',
        onClick: function () { if (VS.actions['mobile.demo']) VS.actions['mobile.demo'](); }
      }],
      overflow: [
        { label: '立即锁定', icon: 'lock', sub: '锁定保险箱并清空会话', onClick: function () { if (VS.actions['lock']) VS.actions['lock'](); } },
        { label: '检查更新', icon: 'refresh', sub: '通道：' + ((D.settings && D.settings.updateChannel) || 'stable'), onClick: function () { checkUpdate(); } },
        { label: '导出诊断包（脱敏）', icon: 'export', sub: '只含计数与哈希前缀', onClick: exportDiagSheet },
        {
          label: '发起 MK 轮换', icon: 'key', danger: true,
          disabled: frozen() || inRotation,
          reason: inRotation ? '轮换已在进行中（错误码 10）' : (denyReason() || ''),
          onClick: function () { go(ctx, 'settings-rotate'); }
        },
        { label: '关于', icon: 'info', sub: '版本与证据入口', onClick: function () { go(ctx, 'about'); } }
      ],
      body: MUI.screen(kids),
      tab: true,
      fab: null
    });
  };

  function densityLabel(d) {
    return d === 'compact' ? '紧凑' : (d === 'loose' ? '宽松' : '标准');
  }
  function checkUpdate() {
    UI.toast({ tone: 'info', title: '正在检查更新', msg: '通道：' + ((D.settings && D.settings.updateChannel) || 'stable') + ' · 清单必须验签' });
  }

  /* ======================================================================
   * B. 设置子页（tab:false + back:true 的全屏路由）
   * ==================================================================== */

  /* ---- B1. settings-lock ---- */
  VS.pages['settings-lock'] = function (ctx) {
    var idleSeg = seg({
      value: L.idleMinutes,
      items: [5, 15, 30, 60].map(function (m) { return { value: m, label: F.num(m) + ' 分钟' }; }),
      onChange: function (v) { L.idleMinutes = v; }
    });

    var awayInput = UI.input({ type: 'number', value: String(L.awaySeconds) });
    awayInput.min = '5'; awayInput.max = '300'; awayInput.step = '5';
    awayInput.style.width = '110px';
    awayInput.style.minHeight = '44px';
    awayInput.title = '离开应用后落锁的秒数（最小 5，最大 300，默认 30）';
    awayInput.setAttribute('aria-label', awayInput.title);
    awayInput.addEventListener('change', function () {
      var v = parseInt(awayInput.value, 10);
      if (isNaN(v)) v = 30;
      L.awaySeconds = U.clamp(v, 5, 300);
      awayInput.value = String(L.awaySeconds);
      UI.toast({ tone: 'info', title: '离开应用落锁', msg: '已设为 ' + F.num(L.awaySeconds) + ' 秒' });
    });

    var kids = [];
    kids = kids.concat(group('锁定策略', [
      switchRow({
        label: '① 空闲超时自动落锁', checked: L.idleLock,
        sub: '无输入达到所选时长后自动落锁（默认开启 · 15 分钟）',
        onChange: function (e) { L.idleLock = e.target.checked; }
      }),
      ctlBlock({ label: '空闲超时时长', sub: '5 / 15 / 30 / 60 分钟，默认 15 分钟' + (L.idleLock ? '' : '（开关关闭时不生效）'), node: idleSeg }),
      switchRow({
        label: '② 离开应用后落锁', checked: L.awayLock,
        sub: '应用失去前台焦点后开始计时',
        onChange: function (e) { L.awayLock = e.target.checked; }
      }),
      ctlBlock({
        label: '离开应用落锁时长', sub: '最小 5 秒 · 最大 300 秒 · 默认 30 秒',
        node: h('div', { class: 'row gap-2' }, [
          awayInput,
          h('span', { class: 't-caption', text: '秒' }),
          L.awayLock ? null : h('span', { class: 't-caption t-muted', text: '（开关关闭时不生效）' })
        ])
      }),
      switchRow({
        label: '③ 锁屏时同步落锁', checked: L.lockOnScreenSaver,
        sub: '系统锁屏事件触发落锁',
        onChange: function (e) { L.lockOnScreenSaver = e.target.checked; }
      }),
      switchRow({
        label: '④ 睡眠时同步落锁', checked: L.lockOnSleep,
        sub: '设备进入睡眠 / 待机时落锁',
        onChange: function (e) { L.lockOnSleep = e.target.checked; }
      })
    ]));

    kids = kids.concat(group('介质与远端', [
      switchRow({
        label: '移除外部介质后落锁', checked: L.lockOnUsbRemoval,
        sub: '保险箱位于可移动介质时生效',
        onChange: function (e) { L.lockOnUsbRemoval = e.target.checked; }
      }),
      infoRow({
        icon: 'pin', label: '远程锁定', value: '只读说明',
        badge: UI.badge({ text: '服务端指令', tone: 'info' }),
        sub: '由已配对设备广播的签名指令触发（kind=2 锁定指令）。本端不提供本地开关，也不存在本地绕过路径。'
      })
    ]));

    /* 异地检测：架构上无此接口 → 该行不渲染，仅登记结论 */
    kids.push(h('div', { class: 'mgroup-note' }, [
      h('div', { class: 'row gap-2' }, [
        h('span', { text: '异地检测（异地登录检测）：' }),
        hiddenMark('本端不采集位置、不上报登录地 → 该行不渲染（此处仅登记结论）')
      ]),
      h('div', { text: '本端没有可渲染的开关：架构上不存在该接口，因此既不置灰也不显示「关闭」。' })
    ]));
    kids.push(frozenNote());

    return MUI.page({
      title: '解锁与落锁', back: true, tab: false,
      body: MUI.screen(kids)
    });
  };

  /* ---- B2. settings-bio ---- */
  VS.pages['settings-bio'] = function (ctx) {
    /* CAP_BIO 未置位 / 错误码 4 → 整块隐藏（渲染空屏，不报错、不置灰、不给降级路径） */
    if (!S.cap('CAP_BIO') || L.bioCode === 4) {
      return MUI.page({ title: '生物识别', back: true, tab: false, body: MUI.screen([]) });
    }

    var kids = [];
    kids.push(MUI.mcard({
      title: '绑定状态',
      sub: '凭据存放于平台安全存储；MK 只在引擎内解封，展示层不接触',
      body: MUI.mlist([
        infoRow({
          label: '当前绑定', value: L.bioBound ? '已绑定' : '未绑定',
          badge: L.bioBound ? UI.badge({ text: '已绑定', tone: 'success' }) : UI.badge({ text: '未绑定', tone: '' }),
          mark: UI.tierMark('real')
        }),
        infoRow({ label: '来源接口', value: 'CAP_BIO', mono: true, sub: 'capability_bits bit ' + F.num(VS.CAP.CAP_BIO.bit) + '；绑定结果由 vault_core_bio_bind 返回' }),
        infoRow({ label: '失败语义', sub: '码 5（未绑定 / 安全区失效）→ 引导重绑；码 4（安全存储不可用）→ 本区块隐藏' })
      ], { flush: true })
    }));

    if (L.bioCode === 5) {
      kids.push(UI.errorBox(5, {
        text: '安全区失效，已绑定的生物识别凭据不可用。请重新绑定。',
        actions: [UI.btn({ label: '重新绑定', variant: 'primary', icon: 'fingerprint', title: '重新绑定生物识别', onClick: openBioRebind })]
      }));
    }
    kids.push(frozenNote());

    var acts = [];
    if (!L.bioBound) {
      acts.push(guardBtn({
        label: '绑定生物识别', icon: 'fingerprint',
        onClick: function () { L.bioBound = true; L.bioCode = null; UI.toast({ tone: 'success', title: '已绑定', msg: '原型演示：绑定流程已受理' }); VS.render(); }
      }));
    } else {
      acts.push(guardBtn({
        label: '重新绑定', icon: 'refresh',
        onClick: openBioRebind
      }));
      acts.push(guardBtn({
        label: '解绑生物识别', icon: 'ban', variant: 'danger',
        onClick: function () {
          L.bioBound = false; L.bioCode = null;
          UI.toast({ tone: 'warn', title: '已解绑', msg: '可用主密码继续解锁' });
          VS.render();
        }
      }));
    }
    acts.push(mbtn('原型注入：安全区失效（码 5）', {
      variant: 'ghost', icon: 'alert',
      onClick: function () { L.bioCode = 5; VS.render(); }
    }));
    acts.push(mbtn('原型注入：安全存储不可用（码 4）', {
      variant: 'ghost', icon: 'alert',
      onClick: function () { L.bioCode = 4; VS.render(); }
    }));
    kids.push(h('div', { class: 'col gap-2' }, acts));

    function openBioRebind() {
      if (frozen()) { UI.toast({ tone: 'warn', title: '重新绑定不可用', msg: denyReason() }); return; }
      MUI.sheet({
        title: '重新绑定生物识别',
        sub: '先吊销平台安全存储中的旧凭据，再写入新凭据',
        body: [
          h('div', { class: 't-body', text: '重新绑定不改变 MK，因此不需要重新加密任何数据；旧凭据被吊销后立即失效。' }),
          h('div', { class: 'mt-3' }, [MUI.mkv([
            { k: '失败：码 4', v: '安全存储不可用 → 本区块隐藏' },
            { k: '失败：码 5', v: '安全区失效 → 回到本面板重试' }
          ])])
        ],
        footer: function (close) {
          return [
            mbtn('确认重绑', {
              variant: 'primary',
              onClick: function () { close(); L.bioCode = null; L.bioBound = true; UI.toast({ tone: 'success', title: '重新绑定已完成', msg: '原型演示' }); VS.render(); }
            }),
            mbtn('取消', { variant: 'ghost', onClick: close })
          ];
        }
      });
    }

    return MUI.page({ title: '生物识别', back: true, tab: false, body: MUI.screen(kids) });
  };

  /* ---- B3. settings-password ---- */
  VS.pages['settings-password'] = function (ctx) {
    var kids = [
      MUI.mcard({
        title: '零重加密',
        sub: 'MK 永不变更，仅重封 KEK',
        body: h('div', { class: 'col gap-2' }, [
          h('div', { class: 't-body', text: 'MK 由 CSPRNG 生成且永不变更；主密码只是解封 MK 的两把 KEK 之一。改密码 = 用新 KEK 重封同一份 MK，不触发任何文件重加密。' }),
          MUI.muted('因此改密是常数级操作：与库大小无关，也不会让任何已同步设备重新拉取数据。')
        ])
      }),
      MUI.mlist([
        infoRow({ label: '上次修改', value: F.dateLong(NOW - 84 * DAY), mark: designMark('原型本地时间线，非引擎读数') }),
        infoRow({ label: '失败冷却', sub: '连续失败后按引擎返回值给出等待时间（错误码 1 / 2），真伪密码共用同一计数器' }),
        infoRow({ label: '生物识别', value: S.cap('CAP_BIO') ? (L.bioBound ? '已绑定' : '未绑定') : '能力未置位', sub: '生物识别是另一把 KEK，与主密码相互独立' })
      ]),
      frozenNote(),
      h('div', { class: 'col gap-2' }, [
        guardBtn({ label: '修改主密码', icon: 'key', onClick: openChangeMasterPassword })
      ])
    ];

    function openChangeMasterPassword() {
      if (frozen()) { UI.toast({ tone: 'warn', title: '修改主密码不可用', msg: denyReason() }); return; }
      var cur = UI.input({ type: 'password', placeholder: '当前主密码', size: 'lg' });
      var nw = UI.input({ type: 'password', placeholder: '新主密码', size: 'lg' });
      var cf = UI.input({ type: 'password', placeholder: '再次输入新主密码', size: 'lg' });
      [cur, nw, cf].forEach(function (i) { i.setAttribute('aria-label', i.placeholder); i.title = i.placeholder; });
      var bar = MUI.bar(0, 'accent', 'sm');
      var strength = h('div', { class: 't-caption', text: '强度：未输入' });

      /* 强度估计为本地启发式，不是引擎输入（诚实标注） */
      function score(v) {
        if (!v) return 0;
        var s = Math.min(1, v.length / 20);
        if (/[a-z]/.test(v) && /[A-Z]/.test(v)) s += 0.15;
        if (/\d/.test(v)) s += 0.1;
        if (/[^\w]/.test(v)) s += 0.15;
        return U.clamp(s, 0, 1);
      }
      nw.addEventListener('input', function () {
        var p = score(nw.value);
        bar.firstChild.style.width = (Math.round(p * 1000) / 10) + '%';
        strength.textContent = '强度：' + (nw.value ? F.pct(p) : '未输入');
      });
      function sync() {
        if (okBtn) okBtn.disabled = !(cur.value && nw.value && nw.value === cf.value && score(nw.value) >= 0.35);
      }
      var okBtn = null;
      [cur, nw, cf].forEach(function (i) { i.addEventListener('input', sync); });

      MUI.sheet({
        title: '修改主密码',
        sub: '零重加密：MK 永不变更，仅重封 KEK',
        size: 'tall',
        body: [
          UI.field({ label: '当前主密码', req: true, control: cur }),
          UI.field({ label: '新主密码', req: true, control: nw, hint: '建议 12 位以上，混用大小写、数字与符号' }),
          h('div', { class: 'col gap-2' }, [bar, strength]),
          UI.field({ label: '确认新主密码', req: true, control: cf }),
          MUI.limitedNote('强度条为本地启发式估计，不是引擎读数；引擎只返回成功或错误码（1 密码错误 / 2 冷却）。')
        ],
        footer: function (close) {
          okBtn = mbtn('确认修改', {
            variant: 'primary', disabled: true,
            onClick: function () { close(); UI.toast({ tone: 'success', title: '主密码已修改', msg: '原型演示：未发生任何重加密' }); }
          });
          sync();
          return [okBtn, mbtn('取消', { variant: 'ghost', onClick: close })];
        }
      });
    }

    return MUI.page({ title: '主密码', back: true, tab: false, body: MUI.screen(kids) });
  };

  /* ---- B4. settings-disguise ---- */
  VS.pages['settings-disguise'] = function (ctx) {
    var kids = [
      MUI.mlist([
        switchRow({
          label: '启用伪装入口', checked: L.disguiseEnabled,
          sub: '输入伪装密码进入伪空间；伪空间与真空间使用同一套界面',
          onChange: function (e) { L.disguiseEnabled = e.target.checked; }
        })
      ]),
      UI.alertbar({
        tone: 'warn', icon: 'alert', title: '共用同一输入控件',
        text: '伪装密码与主密码共用同一输入控件，UI 不做真伪区分，冷却计数共用。'
      }),
      MUI.mlist([
        infoRow({ label: '真伪不可区分', sub: '锁屏只有一个密码输入框：不存在「伪装密码」专用入口，也不做任何视觉或时序上的区分。' }),
        infoRow({ label: '冷却计数共用', sub: '真 / 伪密码失败共用同一冷却窗口（错误码 1 / 2），因此从冷却行为无法反推输入的是哪一种密码。' }),
        infoRow({ label: '伪装态提示', value: '无', sub: '进入伪空间后不显示任何「当前为伪装态」的标记，避免被旁观者识别。' })
      ]),
      MUI.muted('伪装入口是抗胁迫设计，不是权限分级：伪空间内的数据与真空间相互独立。')
    ];
    return MUI.page({ title: '伪装入口', back: true, tab: false, body: MUI.screen(kids) });
  };

  /* ---- B5. settings-keys ---- */
  VS.pages['settings-keys'] = function (ctx) {
    var maint = isMaintenance();
    var kids = [];

    var rows = SUB_KEYS.map(function (k) {
      var w = SUB_WRAP_LABEL[k.wrap] || { label: '未知', tone: '' };
      var needMaint = SUB_KEYS_NEED_MAINT.indexOf(k.id) >= 0;
      return navRow({
        label: 'key_id ' + F.num(k.id) + ' · ' + k.use,
        sub: '生成 ' + F.dateLong(k.gen) + '（' + F.relative(k.gen) + '）' + (needMaint ? ' · 维护态专用' : ''),
        badges: UI.badge({ text: w.label, tone: w.tone }),
        trail: h('span', { class: 't-caption', text: '长按操作' }),
        title: 'key_id ' + k.id + ' · ' + k.use,
        ariaLabel: 'key_id ' + k.id + ' · ' + k.use,
        onClick: function () { openKeyActions(k, needMaint, maint); },
        onLongPress: function () { openKeyActions(k, needMaint, maint); }
      });
    });
    kids.push(MUI.mlist(rows));
    kids.push(MUI.mgroupNote('key_id 9–255 为保留区间：本端不渲染对应行（无接口、无包装记录）。'));
    kids.push(h('div', { class: 'mgroup-note row gap-2' }, [
      h('span', { text: '保留区间：' }), hiddenMark('9–255 保留 → 不渲染')
    ]));
    kids.push(MUI.muted('从属密钥独立包装于 VSVB v3 头部：不再从 MK 直接派生，MK 轮换后可单独重包装。'));
    kids.push(frozenNote());
    kids.push(h('div', { class: 'col gap-2' }, [
      guardBtn({ label: '轮换从属密钥', icon: 'refresh', onClick: openSubKeyRotateSheet })
    ]));

    function openKeyActions(k, needMaint, isMaint) {
      var blocked = needMaint && !isMaint;
      MUI.actionSheet({
        title: 'key_id ' + F.num(k.id),
        sub: k.use,
        items: [
          {
            label: '轮换此密钥', icon: 'refresh',
            disabled: blocked || frozen(),
            reason: blocked ? 'key_id ' + k.id + ' 的轮换会与业务写入竞争，必须先进入维护态（错误码 10）' : denyReason(),
            onClick: function () { UI.toast({ tone: 'info', title: '已受理轮换', msg: 'key_id ' + F.num(k.id) + '（原型演示）' }); }
          },
          { label: '复制 key_id', icon: 'copy', onClick: function () { UI.copy('key_id=' + k.id); } },
          { label: '查看包装状态', icon: 'info', sub: (SUB_WRAP_LABEL[k.wrap] || {}).label, onClick: function () {
            MUI.sheet({
              title: 'key_id ' + F.num(k.id) + ' 包装状态',
              sub: k.use,
              body: MUI.mkv([
                { k: '包装', v: (SUB_WRAP_LABEL[k.wrap] || {}).label },
                { k: '生成时间', v: F.dateLong(k.gen) },
                { k: '距今', v: F.relative(k.gen) },
                { k: '维护态专用', v: needMaint ? '是（1 / 2 / 6 / 7）' : '否' }
              ])
            });
          } }
        ]
      });
    }

    function openSubKeyRotateSheet() {
      var isMaint = isMaintenance();
      var sel = {};
      var boxes = [];
      var wrap = h('div', { class: 'col gap-3' });

      SUB_KEYS.forEach(function (k) {
        var needMaint = SUB_KEYS_NEED_MAINT.indexOf(k.id) >= 0;
        var disabled = needMaint && !isMaint;
        var reason = disabled ? ('key_id ' + k.id + ' 的轮换必须处于维护态（选中会与业务写入竞争）') : null;
        var cb = UI.checkbox({
          label: 'key_id ' + F.num(k.id) + ' · ' + k.use,
          sub: reason || (needMaint ? '维护态专用密钥' : '可随时轮换'),
          checked: false, disabled: disabled,
          onChange: function (e) { sel[k.id] = e.target.checked; sync(); }
        });
        cb.title = reason || ('选中以轮换 key_id ' + k.id);
        if (cb.input) { cb.input.title = cb.title; cb.input.setAttribute('aria-label', cb.title); }
        boxes.push(cb);
        wrap.appendChild(cb);
      });
      if (!isMaint) {
        wrap.appendChild(MUI.limitedNote('当前非维护态：key_id 1 / 2 / 6 / 7（索引 / 搜索 / 链键 / 审计导出）禁用，其余 4 条可随时轮换。'));
      }

      var startBtn = null;
      function sync() {
        if (!startBtn) return;
        var s = Object.keys(sel).filter(function (k) { return sel[k]; });
        var bad = s.some(function (k) { return SUB_KEYS_NEED_MAINT.indexOf(parseInt(k, 10)) >= 0; }) && !isMaint;
        startBtn.disabled = !s.length || bad || frozen();
      }

      MUI.sheet({
        title: '轮换从属密钥',
        sub: '可多选；维护态专用密钥在非维护态下禁用',
        size: 'tall',
        body: wrap,
        footer: function (close) {
          startBtn = mbtn('开始轮换', {
            variant: 'primary', disabled: true,
            title: '轮换选中的从属密钥',
            onClick: function () {
              var n = Object.keys(sel).filter(function (k) { return sel[k]; }).length;
              close();
              UI.toast({ tone: 'info', title: '已受理从属密钥轮换', msg: '原型演示：' + F.num(n) + ' 条密钥' });
            }
          });
          sync();
          return [startBtn, mbtn('取消', { variant: 'ghost', onClick: close })];
        }
      });
    }

    return MUI.page({ title: '从属密钥管理', back: true, tab: false, body: MUI.screen(kids) });
  };

  /* ---- B6. settings-rotate ---- */
  VS.pages['settings-rotate'] = function (ctx) {
    var inRotation = S.get('maintenance') === 'rotation';
    var kids = [];

    kids.push(MUI.mcard({
      title: '当前状态',
      sub: 'MK 是全库根密钥；轮换只换 MK 并重包装从属密钥，不重加密文件体',
      body: MUI.mlist([
        infoRow({
          label: '轮换状态', value: inRotation ? '轮换进行中' : '空闲',
          badge: inRotation ? UI.badge({ text: '轮换进行中', tone: 'maintenance' }) : UI.badge({ text: '空闲', tone: '' })
        }),
        infoRow({ label: '最近一次轮换', value: F.dateLong(NOW - 180 * DAY), mark: designMark('原型本地时间线') }),
        infoRow({ label: '四阶段', sub: '① 头部标记 → ② 数据换钥 → ③ 传播 → ④ 完成' }),
        infoRow({ label: '中断行为', sub: '可随时关闭应用，下次启动由恢复日志自动续做；自动续做上限 3 次，第 4 次须显式确认。' })
      ], { flush: true })
    }));

    if (inRotation) {
      kids.push(maintBanner({
        tone: 'maintenance', icon: 'refresh',
        title: 'MK 轮换进行中',
        sub: '维护态：业务写入冻结（错误码 10），只读与导出仍可用'
      }));
    } else {
      kids.push(MUI.muted('「发起轮换」需要主密码 + 确认短语；确认后立即进入维护态，业务写入冻结。'));
    }
    kids.push(frozenNote());

    kids.push(h('div', { class: 'col gap-2' }, [
      guardBtn({
        label: '发起轮换', icon: 'key', variant: 'danger',
        ignoreFrozen: inRotation,
        onClick: function () { openStartRotation(ctx); }
      }),
      mbtn('查看进度', { variant: 'ghost', icon: 'loader', onClick: function () { go(ctx, 'maintenance-rotate'); } })
    ]));

    return MUI.page({
      title: 'MK 轮换', back: true, tab: false,
      body: MUI.screen(kids)
    });
  };

  function openStartRotation(ctx) {
    if (isMaintenance()) {
      UI.toast({ tone: 'warn', title: '已在维护态', msg: '同一时刻只允许一个维护任务（错误码 10）' });
      return;
    }
    var pw = UI.input({ type: 'password', placeholder: '主密码', size: 'lg' });
    pw.title = '主密码'; pw.setAttribute('aria-label', '主密码');
    MUI.confirmSheet({
      title: '发起 MK 轮换',
      sub: '确认后立即进入维护态，业务写入冻结',
      tone: 'danger',
      body: [
        UI.errorBox(10, { text: '确认后本会话进入维护态：导入 / 编辑 / 删除 / 同步全部冻结，数据仍可读。' }),
        h('div', { class: 'mt-3' }, [UI.field({ label: '主密码', req: true, control: pw, hint: '身份校验：与确认短语缺一不可' })])
      ],
      phrase: 'ROTATE-MK',
      hint: '轮换期间可随时关闭应用；下次启动自动继续（自动续做上限 3 次，第 4 次须显式确认）。',
      confirmLabel: '进入维护态并开始',
      onConfirm: function () {
        if (!pw.value) { UI.toast({ tone: 'warn', title: '缺少主密码', msg: '请输入主密码后再发起轮换' }); return; }
        S.set({ maintenance: 'rotation', session: 'Maintenance' });
        UI.toast({ tone: 'maintenance', title: '已进入维护态', msg: 'MK 轮换已开始（原型演示）' });
        go(ctx, 'maintenance-rotate');
      }
    });
  }

  /* ---- B7. settings-migrate ---- */
  VS.pages['settings-migrate'] = function (ctx) {
    /* 已是 v3 → 整块不渲染（不写「已是最新」） */
    if (L.formatVersion >= 3) {
      return MUI.page({ title: '格式与迁移', back: true, tab: false, body: MUI.screen([]) });
    }
    var ms = D.migrationState;
    var allOk = (ms.precheck || []).every(function (p) { return p.ok; });
    var kids = [
      MUI.mlist([
        infoRow({ label: '当前库格式版本', value: 'VSVB v' + F.num(L.formatVersion), sub: '头部版本由引擎报告（vault_core_abi_info / 头部读取）' }),
        infoRow({ label: '可迁移目标版本', value: 'VSVB v' + F.num(ms.dstVer), sub: '迁移会把从属密钥移入独立包装区' }),
        infoRow({ label: '迁移前置检查', sub: '独占写租约 · 非维护态 · 磁盘余量 ≥ 库大小 × 1.2' }),
        infoRow({ label: '不通过时', value: '不进入维护态', sub: '引擎返回 9（租约被占）/ 10（维护态）/ 3（磁盘不可写），且不进入维护态' })
      ]),
      MUI.limitedNote('迁移期间进入维护态：业务写入冻结（错误码 10），数据仍可读；取消会停在当前原子单元（码 12 + 末帧），库部分新部分旧但完全可用。'),
      frozenNote(),
      h('div', { class: 'col gap-2' }, [
        allOk
          ? guardBtn({
              label: '开始迁移', icon: 'migrate',
              onClick: function () { go(ctx, 'maintenance-migrate'); }
            })
          : mbtn('开始迁移', {
              disabled: true, icon: 'migrate',
              title: '前置检查未通过：会返回 9 / 10 / 3 且不进入维护态'
            })
      ])
    ];
    if (!allOk) {
      kids.push(UI.alertbar({
        tone: 'danger', icon: 'danger', title: '前置检查未通过',
        text: '不满足条件时「开始迁移」保持禁用：引擎会返回 9（租约被占）/ 10（维护态）/ 3（磁盘不可写），并且不会进入维护态。'
      }));
    }
    return MUI.page({ title: '格式与迁移', back: true, tab: false, body: MUI.screen(kids) });
  };

  /* ---- B8. settings-appearance ---- */
  VS.pages['settings-appearance'] = function (ctx) {
    var kids = [
      MUI.mlist([
        ctlBlock({
          label: '主题', sub: '跟随系统 / 亮 / 暗',
          node: seg({
            value: L.theme,
            items: [
              { value: 'system', label: '跟随系统', icon: 'monitor' },
              { value: 'light', label: '亮', icon: 'sun' },
              { value: 'dark', label: '暗', icon: 'moon' }
            ],
            onChange: function (v) {
              L.theme = v;
              if (v === 'light' || v === 'dark') { S.set({ theme: v }); document.documentElement.dataset.theme = v; }
              UI.toast({ tone: 'info', title: '主题已切换', msg: v === 'system' ? '跟随系统（原型演示）' : v });
            }
          })
        }),
        ctlBlock({
          label: '语言', sub: '简体中文 / English',
          node: seg({
            value: L.locale,
            items: [
              { value: 'zh-CN', label: '简体中文' },
              { value: 'en', label: 'English' }
            ],
            onChange: function (v) {
              L.locale = v;
              S.set({ locale: v });
              F.setLocale(v === 'en' ? 'en' : 'zh-CN');
              UI.toast({ tone: 'info', title: '语言已切换', msg: v === 'en' ? 'English' : '简体中文' });
            }
          })
        }),
        ctlBlock({
          label: '密度', sub: '紧凑 / 标准 / 宽松（原型本地状态）',
          node: seg({
            value: L.density,
            items: [
              { value: 'compact', label: '紧凑' },
              { value: 'comfortable', label: '标准' },
              { value: 'loose', label: '宽松' }
            ],
            onChange: function (v) { L.density = v; }
          })
        })
      ]),
      MUI.mlist([
        infoRow({
          icon: 'loader', label: '动效偏好',
          value: reducedMotion() ? '已开启' : '未开启',
          badge: reducedMotion() ? UI.badge({ text: 'prefers-reduced-motion', tone: 'info' }) : null,
          mark: UI.tierMark('real'),
          sub: '浏览器媒体查询真实读数：为真时全部入场动画关闭。本端不提供覆盖开关。'
        })
      ]),
      MUI.mgroupNote(MOBILE_FACTS.noSplit)
    ];
    return MUI.page({ title: '外观与语言', back: true, tab: false, body: MUI.screen(kids) });
  };

  /* ---- B9. settings-notifications ---- */
  var M_CHANNELS = [
    { key: 'app', label: '应用内', cap: null, note: '通知中心，始终可用' },
    { key: 'push', label: '系统推送', cap: 'CAP_NOTIFICATIONS', note: '经自有中继下发（受管）' },
    { key: 'mail', label: '邮件', cap: null, note: '退化为每日摘要邮件（可选）' }
  ];
  var M_LEVELS = [
    { key: 'info', label: '信息（info）', desc: '同步完成、配对成功等' },
    { key: 'warn', label: '警告（warn）', desc: '链路降级、重试、配额接近上限' },
    { key: 'danger', label: '危险（danger）', desc: '需要恢复、启动恢复未完成、撤销' }
  ];

  function visibleChannels() {
    return M_CHANNELS.filter(function (c) { return !c.cap || S.cap(c.cap); });
  }
  function notifyMatrixSummary() {
    var cols = visibleChannels().length;
    return F.num(M_LEVELS.length) + ' 级别 × ' + F.num(cols) + ' 渠道；危险级不可关闭';
  }

  VS.pages['settings-notifications'] = function (ctx) {
    var cols = visibleChannels();
    var hidden = M_CHANNELS.filter(function (c) { return c.cap && !S.cap(c.cap); });
    var kids = [];

    M_LEVELS.forEach(function (lv) {
      var isDanger = lv.key === 'danger';
      var rows = cols.map(function (c) {
        return switchRow({
          label: c.label,
          sub: isDanger ? '危险级通知不可关闭（安全基线）' : c.note,
          checked: !!L.notify[lv.key][c.key],
          disabled: isDanger,
          reason: isDanger ? '危险级通知是安全语义，不可关闭' : null,
          onChange: function (e) { L.notify[lv.key][c.key] = e.target.checked; }
        });
      });
      kids.push(MUI.mcard({ title: lv.label, sub: lv.desc, body: MUI.mlist(rows, { flush: true }) }));
    });

    kids.push(MUI.muted('危险级通知的可达性是安全基线，因此该行开关一律禁用并写明原因；渠道能力位未置位时整列不渲染（不是置灰）。'));
    if (hidden.length) {
      kids.push(h('div', { class: 'row gap-2 wrap' }, [
        hiddenMark('capability_bits 未置位 → 该渠道整列不渲染'),
        h('span', { class: 't-caption', text: '未渲染的渠道：' + hidden.map(function (c) { return c.label; }).join('、') })
      ]));
    }
    kids.push(MUI.mgroupNote('本端通知渠道是「应用内 + 系统推送（+ 可选邮件）」，不是通用的「系统通知」表述。'));

    var start = UI.input({ type: 'time', value: L.dndStart });
    var end = UI.input({ type: 'time', value: L.dndEnd });
    [start, end].forEach(function (i) { i.style.minHeight = '44px'; i.title = '不打扰时段'; i.setAttribute('aria-label', '不打扰时段'); });
    start.disabled = !L.dndEnabled; end.disabled = !L.dndEnabled;
    start.addEventListener('change', function () { L.dndStart = start.value || L.dndStart; });
    end.addEventListener('change', function () { L.dndEnd = end.value || L.dndEnd; });

    kids.push(MUI.mcard({
      title: '不打扰时段',
      sub: '仅抑制非危险级；危险级仍会投递',
      body: h('div', { class: 'col gap-3' }, [
        switchRow({
          label: '启用不打扰时段', checked: L.dndEnabled,
          onChange: function (e) {
            L.dndEnabled = e.target.checked;
            start.disabled = !L.dndEnabled; end.disabled = !L.dndEnabled;
          }
        }),
        h('div', { class: 'row gap-3' }, [
          start, h('span', { class: 't-caption', text: '至' }), end
        ]),
        MUI.muted('时段内系统推送静默入通知中心；危险级不受时段抑制。')
      ])
    }));

    return MUI.page({ title: '通知', back: true, tab: false, body: MUI.screen(kids) });
  };

  /* ---- B10. settings-privacy ---- */
  VS.pages['settings-privacy'] = function (ctx) {
    var kids = [
      MUI.mlist([
        infoRow({
          icon: 'shield', label: '截屏保护（本端能力）', value: '支持',
          badge: UI.badge({ text: '本端', tone: 'success' }),
          mark: UI.tierMark('real'),
          sub: MOBILE_FACTS.screenshot,
          title: MOBILE_FACTS.screenshot
        }),
        infoRow({
          label: '能力边界', sub: 'FLAG_SECURE / iOS 遮罩只能阻止系统级截屏与录制；不保证第三方注入型工具或物理拍摄一定失败。',
          badge: UI.badge({ text: '不夸大', tone: 'warning' })
        })
      ]),
      MUI.mlist([
        switchRow({
          label: '缩略图缓存',
          sub: '按需生成；LRU 上限比桌面更小，且落锁即清（不落盘、不进审计以外的地方）',
          checked: L.thumbnails,
          disabled: !S.cap('CAP_THUMBNAIL'),
          reason: 'CAP_THUMBNAIL 未置位：本端不生成缩略图',
          onChange: function (e) { L.thumbnails = e.target.checked; }
        }),
        switchRow({
          label: '脱敏诊断上报',
          sub: '默认关闭；开启后仅上报计数与哈希前缀',
          checked: L.telemetry,
          onChange: function (e) { L.telemetry = e.target.checked; }
        }),
        infoRow({
          label: '脱敏口径',
          sub: '诊断包只含计数、错误码汇总与哈希前缀（如 sha256 前 6 位）；不含目标路径原文、文件名原文、查询词与密钥材料。'
        }),
        infoRow({
          label: '本端减配', sub: '移动端缩略图缓存上限比桌面小（docs/v2.0/04 §六），且不提供分栏视图；这两项是外壳固有约束，不因能力位改变。',
          mark: hiddenMark('分栏视图：本端不渲染该选项')
        })
      ])
    ];
    return MUI.page({ title: '隐私与安全', back: true, tab: false, body: MUI.screen(kids) });
  };

  /* ---- B11. settings-update ---- */
  VS.pages['settings-update'] = function (ctx) {
    var ch = (D.settings && D.settings.updateChannel) || 'stable';
    var sel = MUI.select({
      value: ch,
      options: [
        { value: 'stable', label: 'stable', sub: '仅正式版本，默认' },
        { value: 'beta', label: 'beta', sub: '候选版本，含未完成收口的变更' },
        { value: 'nightly', label: 'nightly', sub: '每日构建，仅供验证' }
      ],
      onChange: function (v) {
        if (D.settings) D.settings.updateChannel = v;
        UI.toast({ tone: 'info', title: '更新通道已切换', msg: v });
      }
    });
    sel.title = '更新通道';
    var selBtn = allButtons(sel)[0];
    if (selBtn) {
      selBtn.style.minHeight = '44px';
      selBtn.title = '更新通道';
      selBtn.setAttribute('aria-label', '更新通道');
    }

    var kids = [
      MUI.mlist([ctlBlock({ label: '更新通道', sub: 'stable / beta / nightly', node: sel })]),
      MUI.muted('无论通道为何，更新清单都必须通过签名校验；校验失败时更新整体拒绝（不做部分安装）。'),
      h('div', { class: 'col gap-2' }, [
        mbtn('检查更新', { icon: 'refresh', onClick: checkUpdate })
      ])
    ];
    return MUI.page({ title: '更新通道', back: true, tab: false, body: MUI.screen(kids) });
  };

  /* ---- B12. settings-advanced ---- */
  VS.pages['settings-advanced'] = function (ctx) {
    var a = (D.settings && D.settings.advanced) || {};
    var kids = [
      MUI.mlist([
        infoRow({
          icon: 'cpu', label: '工作 Isolate', value: a.workIsolate ? '开启' : '关闭',
          sub: '同步 / 校验 / 打包放在独立 Isolate，避免阻塞 UI 线程'
        }),
        infoRow({ icon: 'gauge', label: '渲染节流', value: F.num(a.renderThrottleMs) + ' ms', sub: '只读展示项，不提供本地开关' }),
        infoRow({ icon: 'queue', label: '事件环形缓冲', value: F.num(a.eventRingBuffer) + ' 条', sub: '只读展示项；溢出丢失计数见诊断页 EVENT_OVERFLOW' })
      ]),
      MUI.muted('以上三项均为只读展示：本端不提供本地开关，读数来源为引擎的参数回读接口。'),
      h('div', { class: 'col gap-2' }, [
        mbtn('打开诊断', { icon: 'cpu', onClick: function () { go(ctx, 'diagnostics'); } }),
        mbtn('导出诊断包（脱敏）', { variant: 'ghost', icon: 'export', onClick: exportDiagSheet })
      ])
    ];
    return MUI.page({ title: '高级', back: true, tab: false, body: MUI.screen(kids) });
  };

  /* 通知矩阵摘要需要在 settings 之前可用 */
  /* ======================================================================
   * C. license — 授权与订阅
   * 本项目**没有**商业订单 / 下单 / 支付 / 结算 / 退款 / 发票 / 试用期；
   * 本页不虚构购物车、支付方式、订单列表、发票或试用倒计时。
   * ==================================================================== */
  var RELAY_TOKEN_STATE = {
    valid: { label: '有效', tone: 'success' },
    expiring: { label: '即将到期', tone: 'warning' },
    expired: { label: '已到期', tone: 'danger' },
    revoked: { label: '已撤销', tone: 'danger' }
  };
  var ENVELOPE_STATUS = {
    delivered: { label: '已送达', tone: 'success' },
    acked: { label: '已确认', tone: 'success' },
    in_flight: { label: '传输中', tone: 'info' }
  };
  var EXCHANGE_STATE = {
    idle: '空闲', requesting: '请求中', awaiting: '等待中继响应',
    granted: '已授予', failed: '失败', expired: '已过期'
  };
  /* 非商业冲突：技术状态而非商业限制 */
  var TECH_CODES = [9, 10, 11, 12, 13, 6, 3];
  var FEAT_KEYS = [
    'feat_multi_device', 'feat_collab', 'feat_relay_quota', 'feat_managed_ops',
    'feat_burn_share', 'feat_stego', 'feat_audit_export', 'feat_sso'
  ];
  var FEAT_LABEL = {
    feat_multi_device: '多设备配对', feat_collab: '协作空间', feat_relay_quota: '中继配额',
    feat_managed_ops: '托管运维', feat_burn_share: '阅后即焚分享', feat_stego: '隐写载荷',
    feat_audit_export: '审计链加密导出', feat_sso: 'SSO 接入'
  };

  function maskToken(t) {
    t = String(t || '');
    if (t.length <= 12) return t;
    return t.slice(0, 6) + '••••••' + t.slice(t.length - 4);
  }

  VS.pages['license'] = function (ctx) {
    var ls = D.licenseState;
    var lmeta = LICENSE_STATE[ls.state] || { label: ls.state, tone: '' };
    var tokenMeta = RELAY_TOKEN_STATE[ls.relayTokenState];
    var kids = [];

    /* ① 当前授权状态卡 */
    kids.push(MUI.mcard({
      title: '当前授权状态', sub: '档位 / 状态 / 到期 / 席位 / 离线宽限',
      body: h('div', { class: 'col gap-3' }, [
        MUI.mkv([
          { k: '档位', v: String(ls.tier).toUpperCase() },
          { k: '状态', node: UI.badge({ text: lmeta.label, tone: lmeta.tone }) },
          { k: '激活时间', v: F.dateLong(ls.activatedMs) },
          { k: '到期时间', v: F.dateLong(ls.expiresMs) },
          { k: '席位', v: F.num(ls.seatsUsed) + ' / ' + F.num(ls.seats) },
          { k: '离线宽限剩余', v: F.num(ls.offlineDaysLeft) + ' 天' }
        ]),
        tokenMeta
          ? UI.alertbar({
              tone: tokenMeta.tone === 'success' ? 'info' : tokenMeta.tone, icon: 'info',
              title: '中继凭据状态：' + tokenMeta.label,
              text: '凭据轮换不影响同步：到期后自动回落 Free 档，本地加密与 P2P 直连同步不被阻断。'
            })
          : UI.alertbar({
              tone: 'stale', icon: 'ban', title: '中继凭据状态不可读',
              text: '接口原值为「' + String(ls.relayTokenState) + '」，不是 valid / expiring / expired / revoked 之一。本端不猜测，按未知显示。'
            })
      ])
    }));

    /* ② 套餐对比：纵向滚动的卡片列表（不是横向 4 列） */
    var planCards = D.plans.map(function (p) {
      var isCurrent = p.id === ls.tier;
      var feats = h('ul', { class: 'plan-feats' }, (p.feats || []).map(function (f) {
        return h('li', { dataset: { off: f.on ? 'false' : 'true' } }, [UI.icon(f.on ? 'check' : 'x', 13), h('span', { text: f.t })]);
      }));
      return h('div', {
        class: 'plan-card',
        dataset: { featured: p.featured ? 'true' : 'false', current: isCurrent ? 'true' : 'false' }
      }, [
        p.featured ? h('span', { class: 'plan-tag', text: '推荐' }) : null,
        h('div', { class: 't-h1', text: p.name }),
        h('div', { class: 'plan-price' }, [
          h('span', { text: p.priceLabel }),
          h('span', { class: 'plan-per', text: ' ' + p.per })
        ]),
        /* 价格是可配置占位，取自 D.plans[i].priceLabel；本文件不硬编码任何金额 */
        MUI.muted('价格字段来自配置占位：' + p.priceLabel + ' / ' + p.per),
        feats,
        isCurrent
          ? mbtn('当前档位', { disabled: true, title: '当前已处于该档位' })
          : mbtn('选择此档', {
              variant: p.featured ? 'primary' : 'ghost',
              title: '激活 ' + p.name + ' 许可证',
              onClick: function () { openLicenseActivation(p); }
            })
      ]);
    });
    kids.push(MUI.mcard({
      title: '套餐对比', sub: '可纵向滚动 · 价格为可配置占位，本页不硬编码任何金额',
      body: h('div', { class: 'col gap-3' }, planCards)
    }));

    /* ③ 中继凭据 */
    var daysLeft = Math.round((ls.relayTokenExpiresMs - NOW) / DAY);
    kids.push(MUI.mcard({
      title: '中继凭据',
      sub: 'token 12 个月 · 到期前 30 天提示 · 到期自动回落 Free 档且不阻断同步',
      body: h('div', { class: 'col gap-3' }, [
        MUI.mkv([
          { k: 'Token', node: h('div', { class: 'row gap-2' }, [
            h('span', { class: 't-mono', text: maskToken(ls.relayToken), title: '脱敏显示' }),
            UI.iconBtn({ icon: 'copy', size: 'sm', label: '复制 Token', onClick: function () { UI.copy(ls.relayToken); } })
          ]) },
          { k: '状态', node: tokenMeta ? UI.badge({ text: tokenMeta.label, tone: tokenMeta.tone })
            : UI.badge({ text: '未登记：' + String(ls.relayTokenState), tone: '', title: '接口原值不是 valid / expiring / expired / revoked' }) },
          { k: '到期时间', v: F.dateLong(ls.relayTokenExpiresMs) },
          { k: '剩余', v: F.num(daysLeft) + ' 天' },
          { k: '配额', v: F.bytes(ls.relayQuotaUsedBytes) + ' / ' + F.bytes(ls.relayQuotaTotalBytes) },
          { k: '兑换状态', v: EXCHANGE_STATE[L.relayExchange] || String(L.relayExchange) }
        ]),
        MUI.bar(F.pctOf(ls.relayQuotaUsedBytes, ls.relayQuotaTotalBytes), 'accent', 'sm'),
        MUI.muted('中继只转发密文，不落盘、不解密。凭据到期后本端自动回落 Free 档配额；本地加密与 P2P 直连同步不受影响。'),
        h('div', { class: 'col gap-2' }, [
          guardBtn({
            label: '轮换 Token', icon: 'refresh',
            onClick: function () { openRotateTokenSheet(); }
          }),
          mbtn('激活 / 更换许可证', { variant: 'ghost', icon: 'award', onClick: function () { openLicenseActivation(null); } })
        ])
      ])
    }));

    /* ④ 跨设备指令信封（不是商业订单） */
    var envs = D.orderEnvelopes || [];
    var envBody = [];
    envBody.push(UI.alertbar({
      tone: 'warn', icon: 'alert', title: '这些是跨设备指令信封，不是商业订单',
      text: 'kind 只有三种：1 销毁指令 / 2 锁定指令 / 3 轮换传播。本项目没有下单、支付、结算、退款、发票与试用期，因此不存在商业订单列表。'
    }));
    envBody.push(envs.length ? MUI.mlist(envs.map(function (e) {
      var st = ENVELOPE_STATUS[e.status] || { label: e.status, tone: '' };
      var expired = e.expiresMs < NOW;
      return infoRow({
        label: 'kind ' + F.num(e.kind) + ' · ' + e.kindLabel,
        sub: (e.targets || []).join('、') + ' · ' + e.id,
        badge: UI.badge({ text: st.label, tone: st.tone }),
        value: expired ? '已失效' : F.dateShort(e.expiresMs)
      });
    })) : MUI.empty({ icon: 'queue', title: '暂无可信指令信封', desc: '没有待投递或已确认的跨设备指令。' }));
    kids.push(MUI.mcard({ title: '跨设备指令信封', sub: 'D.orderEnvelopes · 与商业订单无关', body: h('div', { class: 'col gap-3' }, envBody) }));

    /* ⑤ Free-forever */
    var free = D.plans[0];
    var freeFeats = (free.feats || []).filter(function (f) { return f.on; });
    kids.push(MUI.mcard({
      title: 'Free-forever', sub: 'Free 档永久免费（取自 D.plans[0].feats）',
      body: h('div', { class: 'col gap-3' }, [
        h('ul', { class: 'plan-feats' }, freeFeats.map(function (f) {
          return h('li', { dataset: { off: 'false' } }, [UI.icon('check', 13), h('span', { text: f.t })]);
        })),
        UI.alertbar({
          tone: 'info', icon: 'info', title: '不设试用期',
          text: 'Free 档能力不随使用时长衰减，也不会因为未升级而在本地加密 / 同步 / 密钥管理上降级；不存在试用倒计时。'
        })
      ])
    }));

    /* ⑥ 8 个 feat_* 能力门：用徽标表示开 / 关，不用置灰按钮 */
    var feats = ls.features || {};
    kids.push(MUI.mcard({
      title: '能力门（feat_*）', sub: '开关用徽标表达，不使用置灰按钮',
      body: MUI.mlist(FEAT_KEYS.map(function (k) {
        var on = !!feats[k];
        return infoRow({
          label: FEAT_LABEL[k] || k,
          sub: k,
          badge: UI.badge({ text: on ? '已开启' : '未开启', tone: on ? 'success' : '' }),
          value: on ? '当前档位可用' : '升级后可用'
        });
      }))
    }));

    /* ⑦ 降级提示 */
    kids.push(MUI.mcard({
      title: '降级提示', sub: '档位限制 vs 非商业冲突：两类原因必须分开表达',
      body: h('div', { class: 'col gap-3' }, [
        MUI.mlist([
          infoRow({
            label: '档位限制导致的能力缺失', value: '升级后可用',
            badge: UI.badge({ text: '商业档位', tone: 'accent' }),
            sub: '显示为「升级后可用」；不做功能降级埋点，也不在本地能力上打折。'
          })
        ]),
        h('div', { class: 'col gap-2' }, [
          h('div', { class: 't-strong', text: '非商业冲突（技术状态）' }),
          h('div', { class: 't-caption', text: '以下错误码是引擎 / 中继的技术状态，不是商业限制，不得展示为「需要升级」：' }),
          h('div', { class: 'row gap-2 wrap' }, TECH_CODES.map(function (c) {
            var tone = VS.tone(c) === 'danger' ? 'danger' : (VS.tone(c) === 'stale' ? 'stale' : 'info');
            return UI.badge({ text: F.num(c) + ' · ' + VS.errTitle(c), tone: tone, title: (VS.ERR[c] || {}).hint || '' });
          }))
        ])
      ])
    }));

    function openRotateTokenSheet() {
      MUI.sheet({
        title: '轮换中继 Token', sub: '旧 Token 立即失效；本地直连同步不受影响',
        body: [
          h('div', { class: 't-body', text: '轮换后旧 Token 立即失效；正在进行的中继传输会在下一个会话重新协商。' }),
          h('div', { class: 'mt-3' }, [MUI.muted('本地 P2P 直连同步不需要凭据，因此轮换期间同步不会被阻断。')])
        ],
        footer: function (close) {
          return [
            guardBtn({
              label: '立即轮换', variant: 'danger', icon: 'refresh',
              onClick: function () { close(); UI.toast({ tone: 'info', title: 'Token 已轮换', msg: '原型演示' }); }
            }),
            mbtn('取消', { variant: 'ghost', onClick: close })
          ];
        }
      });
    }

    return MUI.page({
      title: '授权与订阅',
      sub: String(ls.tier).toUpperCase() + ' · ' + lmeta.label,
      back: true, tab: false,
      actions: [{ icon: 'award', label: '激活许可证', onClick: function () { openLicenseActivation(null); } }],
      body: MUI.screen(kids)
    });
  };

  /** 许可证激活：MUI.sheet 六步（D.licenseSteps）；步骤切换即重建面板（底部面板不做 in-place 局部替换） */
  function openLicenseActivation(plan) {
    var keyInput = UI.input({ mono: true, placeholder: 'VS-XXXX-XXXX-XXXX-XXXX', size: 'lg' });
    keyInput.title = '许可证密钥'; keyInput.setAttribute('aria-label', '许可证密钥');

    function open(step) {
      var host = h('div', { class: 'col gap-3' });

      function renderBody() {
        host.innerHTML = '';
        host.appendChild(MUI.steps({
          items: D.licenseSteps.map(function (label, i) {
            return { label: label, state: i < step ? 'done' : (i === step ? 'active' : 'todo') };
          })
        }));
        var ls = D.licenseState;
        if (step === 0) {
          host.appendChild(UI.field({ label: '许可证密钥', req: true, control: keyInput, hint: '等宽输入框：密钥按原样粘贴，不做自动大小写转换' }));
          host.appendChild(MUI.muted('密钥在本地校验签名与有效期，不联网下单、不涉及任何支付流程。'));
        } else if (step === 1) {
          host.appendChild(MUI.bar(1, 'accent', 'sm'));
          host.appendChild(MUI.mkv([
            { k: '签名校验', v: '已通过（原型演示）' },
            { k: '有效期', v: '校验中' },
            { k: '签发者', v: 'VaultSync 授权服务' }
          ]));
        } else if (step === 2) {
          host.appendChild(MUI.mkv([
            { k: '设备指纹', v: D.fingerprint(), mono: true },
            { k: '绑定方式', v: '写入授权记录，随许可证一同保存' },
            { k: '解绑', v: '显式解绑后可把席位让给其他设备' }
          ]));
        } else if (step === 3) {
          host.appendChild(MUI.mkv([
            { k: '写入位置', v: '本机授权记录（不写保险箱数据区）' },
            { k: '审计', v: 'license.activate' }
          ]));
        } else if (step === 4) {
          host.appendChild(MUI.mkv([
            { k: '配额已用', v: F.bytes(ls.relayQuotaUsedBytes) },
            { k: '配额总量', v: F.bytes(ls.relayQuotaTotalBytes) },
            { k: '用量占比', v: F.pct(F.pctOf(ls.relayQuotaUsedBytes, ls.relayQuotaTotalBytes)) },
            { k: '兑换状态', v: EXCHANGE_STATE[L.relayExchange] || String(L.relayExchange) }
          ]));
          host.appendChild(UI.alertbar({
            tone: 'info', icon: 'info', title: '中继凭据兑换状态机',
            text: 'idle → requesting → awaiting → granted；失败分支为 failed / expired。任一分支都不阻断本地加密与 P2P 同步。'
          }));
        } else {
          host.appendChild(UI.alertbar({
            tone: 'info', icon: 'check-circle', title: '激活流程完成',
            text: '档位写入授权记录；中继凭据与配额随即生效。不设试用期，不做功能降级埋点。'
          }));
        }
      }
      renderBody();

      var last = step >= D.licenseSteps.length - 1;
      MUI.sheet({
        title: '激活许可证' + (plan ? '：' + plan.name : ''),
        sub: '六步流程 · 不做任何支付 / 结算 / 发票',
        size: 'tall',
        body: host,
        footer: function (close) {
          var next = mbtn(last ? '完成' : '下一步', {
            variant: 'primary',
            disabled: step === 0 && !keyInput.value,
            title: last ? '完成激活' : '下一步',
            onClick: function () {
              if (last) { close(); UI.toast({ tone: 'success', title: '激活完成', msg: '原型演示' }); return; }
              var nextStep = step + 1;
              if (nextStep === 1) L.relayExchange = 'requesting';
              if (nextStep === 4) L.relayExchange = 'granted';
              close();
              open(nextStep);
            }
          });
          keyInput.addEventListener('input', function () { next.disabled = step === 0 && !keyInput.value; });
          return [
            next,
            step > 0 ? mbtn('上一步', { variant: 'ghost', onClick: function () { close(); open(step - 1); } }) : null,
            mbtn('关闭', { variant: 'ghost', onClick: close })
          ].filter(Boolean);
        }
      });
    }
    open(0);
  }

  /* ======================================================================
   * D. recover — 启动恢复向导（D.recoveryReport）
   * 硬约束：未确认前不得写入任何数据；无法给出确定视图时不静默猜测。
   * ==================================================================== */
  var REC_PATHS = [
    { value: 'rollback', title: '自动回滚', desc: '丢弃未提交的段尾部，回到最近一次已提交快照（引擎建议：' + D.recoveryReport.suggested + '）' },
    { value: 'replay', title: '重放已提交变更', desc: '按位置日志重放已提交但未落盘的变更；不丢弃数据，耗时更长' },
    { value: 'forensic', title: '导出诊断包（取证）', desc: '只导出计数与哈希前缀，供人工分析；不修改任何库文件' },
    { value: 'continue', title: '显式确认继续', desc: '以当前视图继续使用；未提交变更保持未提交状态，风险自担' }
  ];
  var MIG_FILE_STATUS = {
    done: { label: '完成', tone: 'success' },
    failed: { label: '失败', tone: 'danger' },
    skipped: { label: '跳过', tone: 'warning' },
    pending: { label: '待处理', tone: '' }
  };

  function recDetailRows() {
    var wrap = h('div', { class: 'col' });
    D.recoveryReport.detail.forEach(function (d) {
      wrap.appendChild(infoRow({
        label: d.label, sub: d.value,
        badge: d.ok ? UI.badge({ text: '正常', tone: 'success' }) : UI.badge({ text: '异常', tone: 'danger' })
      }));
    });
    return wrap;
  }

  VS.pages['recover'] = function (ctx) {
    var rp = D.recoveryReport;
    var kids = [];

    var stepsEl = MUI.steps({
      items: [
        { label: '诊断结论', state: L.recStep > 0 ? 'done' : 'active' },
        { label: '选择路径', state: L.recStep > 1 ? 'done' : (L.recStep === 1 ? 'active' : 'todo') },
        { label: '执行', state: L.recStep > 2 ? 'done' : (L.recStep === 2 ? 'active' : 'todo') },
        { label: '完成或失败', state: L.recStep === 3 ? 'done' : 'todo' }
      ]
    });

    var cl = rp.detail.filter(function (d) { return d.key === 'crash_loop'; })[0];
    if (cl && !cl.ok) {
      kids.push(UI.alertbar({
        tone: 'danger', icon: 'danger', title: '崩溃循环保护已触发',
        text: '同一启动周期内多次失去租约并重启。为避免反复写入造成二次损坏，本端不自动选择路径，也不会在未确认的情况下写入。请先保持现场（不要删除任何文件），导出诊断包后再选择路径。'
      }));
    }

    kids.push(MUI.mcard({
      title: '恢复向导', sub: '四步：诊断 → 选择路径 → 执行 → 完成或失败',
      body: h('div', { class: 'col gap-3' }, [
        stepsEl,
        rp.uncommittedChanges
          ? UI.alertbar({
              tone: 'warn', icon: 'alert', title: '存在未提交变更',
              text: '未提交变更 ' + F.num(rp.uncommittedChanges) + ' 处；段状态 ' + rp.segmentStatus + '，租约状态 ' + rp.leaseStatus + '。'
            })
          : UI.alertbar({ tone: 'info', icon: 'info', title: '无未提交变更', text: '可直接打开保险箱。' })
      ])
    }));

    /* ① 诊断结论 */
    kids.push(MUI.mcard({
      title: '① 诊断结论', sub: '结论来自引擎恢复报告；本端不猜测、不补全',
      body: h('div', { class: 'col gap-3' }, [
        UI.alertbar({
          tone: 'danger', icon: 'alert', title: '无法给出确定视图',
          text: '位置日志与租约不一致时，本端无法确定「哪一份数据是完整的」。因此向导不会静默选择路径，必须由你确认后才可能发生写入。'
        }),
        recDetailRows(),
        MUI.mkv([
          { k: '审计链头', v: rp.auditHead, mono: true },
          { k: '建议路径', v: rp.suggested === 'rollback' ? '自动回滚' : '重放已提交变更' },
          { k: '快照状态', v: rp.snapshotStatus },
          { k: '租约状态', v: rp.leaseStatus, mono: true }
        ]),
        h('div', { class: 'col gap-2' }, [
          mbtn('导出诊断包（脱敏）', { icon: 'export', onClick: exportDiagSheet }),
          mbtn('只读方式打开', {
            variant: 'ghost', icon: 'eye',
            title: '以只读方式打开保险箱（不写入任何数据）',
            onClick: function () {
              S.set({ readOnly: true, readonlyReason: '恢复未完成，按只读降级打开（错误码 11）' });
              UI.toast({ tone: 'info', title: '已按只读方式打开', msg: '写入操作全部禁用' });
              go(ctx, 'vault');
            }
          })
        ])
      ])
    }));

    /* ② 选择路径 */
    var pick = MUI.radioCards({
      value: L.recPath, options: REC_PATHS,
      onChange: function (v) { L.recPath = v; if (L.recStep < 1) { L.recStep = 1; } }
    });
    pick.setAttribute('aria-label', '选择恢复路径');
    kids.push(MUI.mcard({
      title: '② 选择路径', sub: '四条路径的写入语义不同：只有前两条会写入数据',
      body: h('div', { class: 'col gap-3' }, [
        pick,
        MUI.muted('「导出诊断包（取证）」与「只读方式打开」不写入任何库文件；「自动回滚」与「重放已提交变更」会写入，且必须显式确认。')
      ])
    }));

    /* ③ 执行 */
    var execBody = [];
    if (!L.recConfirmed) {
      execBody.push(UI.alertbar({
        tone: 'stale', icon: 'lock', title: '未确认，未写入',
        text: '在执行前本向导不会修改任何库文件。点击「确认并执行」后才可能发生写入。'
      }));
    } else {
      var isWrite = L.recPath === 'rollback' || L.recPath === 'replay';
      execBody.push(UI.alertbar({
        tone: isWrite ? 'warn' : 'info', icon: isWrite ? 'alert' : 'info',
        title: isWrite ? '已确认：将写入数据' : '已确认：不写入数据',
        text: '路径：' + ((REC_PATHS.filter(function (p) { return p.value === L.recPath; })[0] || {}).title || '—')
      }));
      execBody.push(MUI.bar(L.recProgress, 'maintenance', 'lg'));
      execBody.push(MUI.mkv([
        { k: '进度', v: F.pct(L.recProgress) },
        { k: '回滚重试', v: F.num(L.recAttempts) + ' / 2' },
        { k: '当前路径', v: (REC_PATHS.filter(function (p) { return p.value === L.recPath; })[0] || {}).title }
      ]));
      if (L.recAttempts >= 2) {
        execBody.push(UI.alertbar({
          tone: 'danger', icon: 'danger', title: '回滚重试已达上限（2 次）',
          text: '继续重试可能扩大损坏。改为「引导保留现场」：停止一切写入，冻结现场并导出诊断包，交由人工取证。'
        }));
      }
    }
    kids.push(MUI.mcard({
      title: '③ 执行', sub: '回滚重试上限 2 次；超限改为引导保留现场',
      body: h('div', { class: 'col gap-3' }, execBody)
    }));
    kids.push(h('div', { class: 'col gap-2' }, [
      mbtn('确认并执行', {
        variant: 'danger', icon: 'check',
        disabled: L.recConfirmed,
        title: '确认恢复路径并开始执行（确认后可能写入）',
        onClick: function () {
          L.recConfirmed = true; L.recStep = 2; L.recProgress = 0.35;
          UI.toast({ tone: 'warn', title: '已确认，开始执行', msg: '路径：' + L.recPath });
          VS.render();
        }
      }),
      mbtn('模拟一次失败并重试', {
        variant: 'ghost', icon: 'rotate-ccw',
        onClick: function () {
          L.recAttempts = Math.min(2, L.recAttempts + 1);
          L.recProgress = Math.max(0, L.recProgress - 0.1);
          UI.toast({ tone: 'warn', title: '回滚重试', msg: '已重试 ' + F.num(L.recAttempts) + ' / 2 次' });
          VS.render();
        }
      })
    ]));

    /* ④ 完成或失败 */
    kids.push(MUI.mcard({
      title: '④ 完成或失败',
      body: h('div', { class: 'col gap-3' }, [
        L.recAttempts >= 2
          ? UI.alertbar({ tone: 'danger', icon: 'danger', title: '恢复失败：已保留现场', text: '未完成恢复。现场未被清理，诊断包可用于人工取证。' })
          : UI.alertbar({ tone: 'info', icon: 'info', title: '等待执行', text: '确认并执行后在此显示完成或失败结论。' }),
        MUI.mkv([
          { k: '完成态', v: L.recConfirmed ? (L.recAttempts >= 2 ? '失败（保留现场）' : '可继续') : '未开始' },
          { k: '写入次数', v: L.recConfirmed ? F.num(1) : '—' }
        ]),
        mbtn('完成并打开保险箱', {
          icon: 'vault',
          disabled: !L.recConfirmed || L.recAttempts >= 2,
          onClick: function () {
            S.set({ readOnly: false, readonlyReason: '', maintenance: null, session: 'Active' });
            UI.toast({ tone: 'success', title: '恢复完成', msg: '原型演示' });
            go(ctx, 'vault');
          }
        })
      ])
    }));

    return MUI.page({
      title: '恢复向导',
      sub: '未确认前不得写入；无法给出确定视图时如实说明',
      back: true, tab: false,
      actions: [{ icon: 'export', label: '导出诊断包（脱敏）', onClick: exportDiagSheet }],
      body: MUI.screen(kids)
    });
  };

  /* ======================================================================
   * E. maintenance-rotate — MK 轮换进度页
   * ==================================================================== */
  var ROT_PHASES = [
    { key: 'mark', label: '① 头部标记' },
    { key: 'rekey', label: '② 数据换钥' },
    { key: 'propagate', label: '③ 传播' },
    { key: 'done', label: '④ 完成' }
  ];
  var PROP_STATE = {
    confirmed: { label: '已确认', tone: 'success' },
    /* 硬约束：对端离线必须显示「待补推」，不得显示完成 */
    offline: { label: '待补推', tone: 'warning' },
    pending: { label: '传播中', tone: 'info' }
  };
  function rotPhaseIndex(phase) {
    for (var i = 0; i < ROT_PHASES.length; i++) if (ROT_PHASES[i].key === phase) return i;
    return 0;
  }
  function rotStateN(phase) {
    if (phase === 'propagate') return 2;
    if (phase === 'done') return 3;
    return 1;
  }

  VS.pages['maintenance-rotate'] = function (ctx) {
    var rs = D.rotationState;
    var idx = rotPhaseIndex(rs.phase);
    var pct = F.pctOf(rs.progress.done, rs.progress.total);
    var n = rotStateN(rs.phase);
    var headline = 'MK 轮换进行中 ' + F.pct(pct) + '%，可随时关闭应用，下次启动自动继续';
    var kids = [];

    if (L.rotConflict) {
      kids.push(UI.alertbar({
        tone: 'danger', icon: 'danger', title: '检测到并发轮换冲突',
        text: '另一实例已发起轮换，本端的轮换已作废。需要重新发起轮换。'
      }));
    }

    kids.push(MUI.mcard({
      title: '轮换进度',
      sub: '轮换 ID ' + rs.rotationId + ' · 开始于 ' + F.dateLong(rs.startedMs),
      body: h('div', { class: 'col gap-3' }, [
        maintBanner({ tone: 'maintenance', icon: 'refresh', title: headline, sub: '维护态：业务写入冻结（错误码 10），只读与导出仍可用', progress: pct }),
        MUI.steps({
          items: ROT_PHASES.map(function (p, i) {
            return { label: p.label, state: i < idx ? 'done' : (i === idx ? 'active' : 'todo') };
          })
        }),
        rs.interrupted ? UI.alertbar({ tone: 'warn', icon: 'alert', title: '上次启动被中断', text: '由恢复日志自动续做。' }) : null
      ])
    }));

    /* ② 数据换钥：1 284 / 3 040（文件 1 190 · 文件夹 94） */
    kids.push(MUI.mcard({
      title: '② 数据换钥',
      body: h('div', { class: 'col gap-2' }, [
        MUI.bar(pct, 'maintenance', 'lg'),
        h('div', { class: 'row-between' }, [
          h('div', { class: 't-num', text: F.num(rs.progress.done) + ' / ' + F.num(rs.progress.total) }),
          h('div', { class: 't-caption', text: '文件 ' + F.num(rs.filesDone) + ' · 文件夹 ' + F.num(rs.foldersDone) })
        ]),
        MUI.muted('换钥只重包装 FSKey 与从属密钥，不重加密文件体；因此可在任意原子单元处续做。')
      ])
    }));

    /* ③ 传播：对端离线 → 待补推 */
    var prop = rs.propagation || [];
    kids.push(MUI.mcard({
      title: '③ 传播', sub: '对端离线时显示「待补推」，补推在下次上线后自动完成',
      body: h('div', { class: 'col gap-3' }, [
        prop.length ? MUI.mlist(prop.map(function (p) {
          var meta = PROP_STATE[p.state] || { label: p.state, tone: '' };
          var note = p.state === 'offline' ? '离线（待下次上线补推）' : p.note;
          return infoRow({
            icon: 'devices', label: p.device, sub: note,
            badge: UI.badge({ text: meta.label, tone: meta.tone }),
            value: p.state === 'confirmed' ? '已确认' : ''
          });
        })) : MUI.empty({ icon: 'devices', title: '暂无传播记录', desc: '尚未产生任何设备传播条目。' }),
        MUI.muted('传播完成不等于全部设备已确认：只有收到对端 ack 才标记为已确认。')
      ])
    }));

    /* 自动续做策略 */
    kids.push(MUI.mcard({
      title: '自动续做', sub: '自动续做上限 3 次，第 4 次须显式确认',
      body: h('div', { class: 'col gap-3' }, [
        MUI.mkv([
          { k: '已自动续做', v: F.num(L.rotAutoResume) + ' 次' },
          { k: '上限', v: '3 次' },
          { k: '超过上限', v: L.rotAutoResume >= 3 ? '需显式确认' : '尚未达到' }
        ]),
        L.rotAutoResume >= 3 ? UI.alertbar({
          tone: 'warn', icon: 'alert', title: '已达自动续做上限',
          text: '下次续做需要你显式确认（第 4 次）。反复失败时应先查看传播状态与诊断包。'
        }) : null,
        mbtn('继续续做', {
          icon: 'play',
          onClick: function () {
            if (L.rotAutoResume >= 3) { openAutoResumeConfirm(); return; }
            L.rotAutoResume += 1;
            UI.toast({ tone: 'info', title: '已续做', msg: '第 ' + F.num(L.rotAutoResume) + ' 次' });
            VS.render();
          }
        })
      ])
    }));

    /* 处理选项：state=1 给回滚选项；state=2 只完成传播 */
    var canRollback = (n === 1 && L.rotHasOldMk);
    var rollbackReason = canRollback
      ? '用旧 MK 回滚到轮换前状态'
      : (n !== 1
          ? '不可用：仅 state=1（头部标记 / 数据换钥阶段）可回滚，当前为传播或完成阶段'
          : '不可用：本端未持有旧 MK，无法解密旧头部');
    kids.push(MUI.mcard({
      title: '处理选项', sub: 'state=1 给回滚选项；state=2 只能完成传播',
      body: h('div', { class: 'col gap-3' }, [
        MUI.mkv([
          { k: '当前阶段', v: (ROT_PHASES[idx] || {}).label },
          { k: '状态档位', v: 'state=' + F.num(n) },
          { k: '旧 MK', v: L.rotHasOldMk ? '本端持有' : '已销毁' }
        ]),
        MUI.muted('「用旧 MK 回滚」仅在 state=1 且本端确实持有旧 MK 时可用，其余情况禁用并给出原因。'),
        h('div', { class: 'col gap-2' }, [
          mbtn('暂停', { variant: 'ghost', icon: 'pause', onClick: function () {
            UI.toast({ tone: 'info', title: '轮换已暂停', msg: '下次启动可按自动续做策略继续' });
          } }),
          mbtn('放弃', { variant: 'danger', icon: 'cancel', onClick: openGiveUpRotation }),
          mbtn('用旧 MK 回滚', {
            variant: 'danger', icon: 'rotate-ccw',
            disabled: !canRollback, title: rollbackReason,
            onClick: function () {
              MUI.confirmSheet({
                title: '用旧 MK 回滚', tone: 'danger',
                sub: '还原头部与从属密钥包装区',
                body: [UI.errorBox(10, { text: '回滚会还原头部与从属密钥包装区；已传播到对端的新头部需要重新传播。' })],
                phrase: 'ROLLBACK',
                confirmLabel: '执行回滚',
                onConfirm: function () { UI.toast({ tone: 'warn', title: '已用旧 MK 回滚', msg: '原型演示' }); }
              });
            }
          })
        ])
      ])
    }));

    /* 原型状态注入：两项在 data.js 中没有可读字段 */
    kids.push(MUI.mcard({
      title: '原型状态注入', sub: '无接口可读的状态，显式标注为原型注入',
      body: h('div', { class: 'col gap-2' }, [
        switchRow({
          label: '本端持有旧 MK', checked: L.rotHasOldMk, sub: '原型注入：真实值由引擎头部状态决定',
          onChange: function (e) { L.rotHasOldMk = e.target.checked; }
        }),
        switchRow({
          label: '模拟并发轮换冲突', checked: L.rotConflict, sub: '原型注入：另一实例同时发起轮换',
          onChange: function (e) { L.rotConflict = e.target.checked; }
        }),
        MUI.muted('以上两项在原型数据源中没有对应字段，仅用于演示状态；刷新页面即恢复默认。')
      ])
    }));

    function openAutoResumeConfirm() {
      MUI.confirmSheet({
        title: '自动续做已达上限（3 次）',
        sub: '第 4 次续做必须显式确认', tone: 'danger',
        body: [
          MUI.mkv([
            { k: '已自动续做', v: F.num(L.rotAutoResume) + ' 次' },
            { k: '上限', v: '3 次' },
            { k: '轮换 ID', v: D.rotationState.rotationId, mono: true }
          ]),
          h('div', { class: 'mt-3' }, [MUI.muted('反复自动续做通常意味着轮换过程本身在重复失败。确认后本次续做会再尝试一次，并重新计数。')])
        ],
        phrase: 'RESUME',
        confirmLabel: '我确认继续第 4 次',
        onConfirm: function () {
          L.rotAutoResume = 1;
          UI.toast({ tone: 'warn', title: '已确认续做', msg: '计数已重置为 1' });
          VS.render();
        }
      });
    }

    function openGiveUpRotation() {
      var nn = rotStateN(D.rotationState.phase);
      MUI.actionSheet({
        title: '放弃 / 暂停轮换',
        sub: nn === 1 ? 'state=1（头部标记 / 数据换钥）：可回滚' : 'state=2（传播）：只能完成传播',
        items: [
          {
            label: '用旧 MK 回滚', icon: 'rotate-ccw',
            sub: nn === 1 ? '回到轮换前的头部与包装状态' : '当前阶段不可回滚',
            disabled: !(nn === 1 && L.rotHasOldMk),
            reason: nn !== 1 ? '传播阶段旧 MK 已无法解出正确视图（state=2）' : '本端未持有旧 MK',
            onClick: function () {
              MUI.confirmSheet({
                title: '用旧 MK 回滚', tone: 'danger',
                body: [UI.errorBox(10, { text: '回滚会重写头部与从属密钥包装区。' })],
                phrase: 'ROLLBACK',
                confirmLabel: '执行回滚',
                onConfirm: function () {
                  S.set({ maintenance: null, session: 'Active' });
                  UI.toast({ tone: 'warn', title: '已用旧 MK 回滚', msg: '轮换已中止（原型演示）' });
                  VS.render();
                }
              });
            }
          },
          {
            label: '只完成传播', icon: 'play',
            sub: nn === 2 ? '把当前 MK 传播给全部设备后结束' : '跳到传播阶段并结束',
            onClick: function () {
              S.set({ maintenance: null, session: 'Active' });
              UI.toast({ tone: 'info', title: '只完成传播', msg: '不提供回滚（原型演示）' });
              VS.render();
            }
          },
          {
            label: '暂停并保留现状', icon: 'pause',
            sub: '下次启动按自动续做策略继续',
            onClick: function () { UI.toast({ tone: 'info', title: '轮换已暂停', msg: '保留现状，可续做' }); }
          }
        ]
      });
    }

    return MUI.page({
      title: 'MK 轮换', sub: headline,
      back: true, tab: false,
      actions: [{ icon: 'export', label: '导出诊断包（脱敏）', onClick: exportDiagSheet }],
      onBack: function () {
        UI.toast({ tone: 'info', title: '轮换在后台继续', msg: '维护态未改变；可随时关闭应用' });
        if (VS.nav) VS.nav.back();
      },
      body: MUI.screen(kids)
    });
  };

  /* ======================================================================
   * F. maintenance-migrate — 格式迁移向导
   * ==================================================================== */
  var MIG_STEPS = [
    { label: '① 前置检查' },
    { label: '② M1 容器' },
    { label: '③ M2 索引' },
    { label: '④ M3 审计' },
    { label: '⑤ M4 头部抬升' }
  ];
  function migPhaseIndex(phase) {
    if (phase === 'scanning') return 1;
    if (phase === 'converting') return 2;
    if (phase === 'verifying') return 4;
    if (phase === 'done') return 4;
    return 0;
  }
  function migFileRow(f) {
    var st = MIG_FILE_STATUS[f.status] || { label: f.status, tone: '' };
    return h('div', { class: 'mrow', style: { cursor: 'default', alignItems: 'flex-start' }, title: f.name + ' · ' + st.label }, [
      h('div', { class: 'mr-main' }, [
        h('div', { class: 'mr-title' }, [
          h('span', { class: 't-truncate', text: f.name }),
          UI.badge({ text: st.label, tone: st.tone })
        ]),
        h('div', { class: 'mr-sub t-mono', text: f.fileId }),
        h('div', { class: 'mr-sub', text: 'v' + F.num(f.srcVer) + ' → v' + F.num(f.dstVer) + ' · sha256 ' + String(f.sha256).slice(0, 12) }),
        h('div', { class: 'mr-sub', text: F.dateLong(f.tsMs) + ' · ' + F.relative(f.tsMs) }),
        f.note ? h('div', { class: 'mr-sub', style: { color: f.status === 'failed' ? 'var(--c-danger)' : 'var(--c-text-2)' }, text: f.note }) : null
      ])
    ]);
  }

  VS.pages['maintenance-migrate'] = function (ctx) {
    var ms = D.migrationState;
    var idx = migPhaseIndex(ms.phase);
    var pct = F.pctOf(ms.doneFiles, ms.totalFiles);
    var allOk = (ms.precheck || []).every(function (p) { return p.ok; });
    var files = ms.files || [];
    var failed = files.filter(function (f) { return f.status === 'failed'; }).length;
    var kids = [];

    kids.push(MUI.mcard({
      title: '迁移概览',
      sub: 'VSVB v' + F.num(ms.srcVer) + ' → v' + F.num(ms.dstVer) + ' · 当前阶段 ' + ms.phase,
      body: h('div', { class: 'col gap-3' }, [
        MUI.steps({
          items: MIG_STEPS.map(function (p, i) {
            var st = ms.phase === 'done' ? 'done' : (i < idx ? 'done' : (i === idx ? 'active' : 'todo'));
            return { label: p.label, state: st };
          })
        }),
        MUI.bar(pct, 'maintenance', 'lg'),
        h('div', { class: 'row-between' }, [
          h('div', { class: 't-num', text: F.num(ms.doneFiles) + ' / ' + F.num(ms.totalFiles) }),
          h('div', { class: 't-caption', text: '当前文件 ' + ms.currentFileId })
        ]),
        MUI.muted('总进度只是汇总；真实依据是下面的逐文件列表。取消时停在当前原子单元（码 12 + 末帧），库部分新部分旧但完全可用。')
      ])
    }));

    /* 前置检查清单 */
    kids.push(MUI.mcard({
      title: '前置检查', sub: '独占写租约 · 非维护态 · 磁盘余量 ≥ 库大小 × 1.2',
      body: h('div', { class: 'col gap-3' }, [
        MUI.mlist((ms.precheck || []).map(function (p) {
          return infoRow({
            label: p.label, sub: p.note,
            badge: p.ok ? UI.badge({ text: '通过', tone: 'success' }) : UI.badge({ text: '未通过', tone: 'danger' })
          });
        }), { flush: true }),
        allOk
          ? UI.alertbar({ tone: 'info', icon: 'check-circle', title: '前置检查全部通过', text: '可以开始迁移。迁移期间进入维护态，业务写入冻结（错误码 10）。' })
          : UI.alertbar({
              tone: 'danger', icon: 'danger', title: '前置检查未通过',
              text: '不满足条件时「开始迁移」禁用：引擎会返回 9（租约被占）/ 10（维护态）/ 3（磁盘不可写），并且不会进入维护态。'
            })
      ])
    }));

    /* 逐文件进度：必须逐行显示，不能只给总进度 */
    kids.push(MUI.mcard({
      title: '逐文件进度',
      sub: '共 ' + F.num(files.length) + ' 条 · 完成 ' + F.num(files.filter(function (f) { return f.status === 'done'; }).length) + ' · 失败 ' + F.num(failed),
      body: h('div', { class: 'col gap-3' }, [
        files.length ? MUI.mlist(files.map(migFileRow), { flush: true })
                     : MUI.empty({ icon: 'list', title: '暂无迁移文件记录', desc: '尚未产生逐文件处理记录。' }),
        UI.alertbar({
          tone: failed ? 'warn' : 'info', icon: failed ? 'alert' : 'info',
          title: failed ? '存在单文件失败' : '无单文件失败',
          text: '单文件失败只影响该行：前后 SHA-256 不一致 → 已删除新文件，旧副本仍在，其余文件继续处理，不会导致整库失败。'
        }),
        MUI.muted('逐行显示，不合并为单一总进度；每行的新旧版本、摘要前缀、时间与备注均可单独核对。')
      ])
    }));

    /* 失败语义 */
    kids.push(MUI.mcard({
      title: '失败语义', sub: '磁盘不足与单文件失败的处置不同',
      body: MUI.mlist([
        infoRow({ label: '磁盘不足（码 3）', sub: '保留 .mig.tmp 供直接重试；不产生半成品（不会留下部分写入的目标文件）。', badge: UI.badge({ text: '可重试', tone: 'warning' }) }),
        infoRow({ label: '单文件校验失败', sub: '前后 SHA-256 不一致 → 已删除新文件，旧副本仍在；该行标记 failed，其余继续。', badge: UI.badge({ text: '不整库失败', tone: 'info' }) }),
        infoRow({ label: '会话取消（码 12）', sub: '停在当前原子单元末帧；库部分新部分旧但完全可用，可断点续做。', badge: UI.badge({ text: '已取消', tone: 'info' }) })
      ], { flush: true })
    }));

    /* 操作 */
    kids.push(h('div', { class: 'col gap-2' }, [
      allOk
        ? guardBtn({
            label: L.migRunning ? '迁移进行中' : '开始迁移', icon: 'migrate',
            ignoreFrozen: false,
            onClick: function () {
              L.migRunning = true;
              S.set({ maintenance: 'migrate', session: 'Maintenance' });
              UI.toast({ tone: 'maintenance', title: '迁移已开始', msg: '原型演示：进入维护态' });
              VS.render();
            }
          })
        : mbtn('开始迁移', { disabled: true, icon: 'migrate', title: '前置检查未通过：会返回 9 / 10 / 3 且不进入维护态' }),
      mbtn('从断点续做', {
        variant: 'ghost', icon: 'play',
        disabled: !allOk,
        title: allOk ? '从上次原子单元边界续做' : '前置检查未通过：续做同样会返回 9 / 10 / 3',
        onClick: function () {
          L.migRunning = true;
          UI.toast({ tone: 'info', title: '已从断点续做', msg: '原型演示' });
          VS.render();
        }
      }),
      mbtn('取消迁移', {
        variant: 'danger', icon: 'cancel',
        onClick: function () {
          MUI.confirmSheet({
            title: '取消迁移', tone: 'warn',
            sub: '停在当前原子单元（码 12 + 末帧）',
            body: [
              UI.errorBox(12, { text: '取消会停在当前原子单元（错误码 12 + 末帧标记）。取消后库是「部分新、部分旧」的状态，但完全可用。' }),
              h('div', { class: 'mt-3' }, [MUI.mkv([
                { k: '当前阶段', v: D.migrationState.phase },
                { k: '已完成文件', v: F.num(D.migrationState.doneFiles) + ' / ' + F.num(D.migrationState.totalFiles) },
                { k: '当前文件', v: D.migrationState.currentFileId, mono: true }
              ])])
            ],
            confirmLabel: '停在当前原子单元',
            onConfirm: function () {
              L.migRunning = false;
              S.set({ maintenance: null, session: 'Active' });
              UI.toast({ tone: 'info', title: '已取消迁移', msg: '停在当前原子单元的末帧标记处（原型演示）' });
              VS.render();
            }
          });
        }
      })
    ]));

    /* 完成与备份 */
    kids.push(MUI.mcard({
      title: '完成与备份', sub: '备份 vault.vsb.pre-v3 保留 7 天；M4 后不回滚',
      body: h('div', { class: 'col gap-3' }, [
        MUI.mlist([
          infoRow({ label: '备份文件', value: 'vault.vsb.pre-v3', mono: true }),
          infoRow({ label: '备份保留期', value: F.num(L.migBackupDays) + ' 天' })
        ], { flush: true }),
        UI.alertbar({
          tone: 'danger', icon: 'danger', title: 'M4 后不回滚',
          text: '头部抬升（M4）完成后，旧格式的读取路径不再维护，因此不提供回滚到 v' + F.num(ms.srcVer) + ' 的自动路径。备份仅用于人工取证与导出。'
        }),
        mbtn('立即删除备份', {
          variant: 'danger', icon: 'trash',
          onClick: function () {
            MUI.confirmSheet({
              title: '立即删除备份', tone: 'danger', sub: '删除后不可恢复',
              body: [UI.errorBox(6, { text: '删除后无法回退到 v' + F.num(ms.srcVer) + ' 视图；如需保留取证材料请先导出。' })],
              phrase: 'DELETE-BACKUP',
              confirmLabel: '删除备份',
              onConfirm: function () {
                L.migBackupDays = 0;
                UI.toast({ tone: 'warn', title: '备份已删除', msg: '原型演示' });
                VS.render();
              }
            });
          }
        })
      ])
    }));

    return MUI.page({
      title: '格式迁移', sub: 'VSVB v' + F.num(ms.srcVer) + ' → v' + F.num(ms.dstVer),
      back: true, tab: false,
      actions: [{ icon: 'export', label: '导出诊断包（脱敏）', onClick: exportDiagSheet }],
      body: MUI.screen(kids)
    });
  };

  /* ======================================================================
   * G. diagnostics / about
   * ==================================================================== */
  function errorDiagSummary() {
    /* 本原型中唯一可读的错误码来源：脱敏审计 detail.result 的计数 */
    var counts = {};
    (D.auditLog || []).forEach(function (a) {
      var r = a.detail && a.detail.result;
      if (r === null || r === undefined || r === 0) return;
      if (!counts[r]) counts[r] = { code: r, count: 0, lastMs: a.tsMs };
      counts[r].count += 1;
      if (a.tsMs > counts[r].lastMs) counts[r].lastMs = a.tsMs;
    });
    return Object.keys(counts).map(function (k) { return counts[k]; }).sort(function (x, y) { return y.count - x.count; });
  }

  VS.pages['diagnostics'] = function (ctx) {
    var errs = errorDiagSummary();
    var a = (D.settings && D.settings.advanced) || {};
    var snapshotText = [
      '{',
      '  "source": "vault_core_diagnostics()",',
      '  "mock": true,',
      '  "compactionPending": null,        // 原型数据源没有该字段',
      '  "segmentStatus": "' + D.recoveryReport.segmentStatus + '",',
      '  "snapshotStatus": "' + D.recoveryReport.snapshotStatus + '",',
      '  "leaseStatus": "' + D.recoveryReport.leaseStatus + '",',
      '  "uncommittedChanges": ' + D.recoveryReport.uncommittedChanges + ',',
      '  "auditHeadPrefix": "' + String(D.recoveryReport.auditHead).slice(0, 12) + '…",',
      '  "auditVerified": ' + (D.auditVerified ? 'true' : 'false') + ',',
      '  "eventRingBuffer": ' + a.eventRingBuffer + ',',
      '  "eventStreamSeq": null,           // 原型数据源没有独立的事件流 seq',
      '  "eventOverflowLost": null,        // 原型数据源没有溢出丢失计数',
      '  "renderThrottleMs": ' + a.renderThrottleMs,
      '}'
    ].join('\n');

    var kids = [
      MUI.mcard({
        title: '诊断快照', sub: 'vault_core_diagnostics()（原型：本地假数据快照，非真实读数）',
        body: h('div', { class: 'col gap-3' }, [
          MUI.mkv([
            { k: 'compactionPending', node: h('div', { class: 'row gap-2' }, [
              h('span', { class: 't-muted', text: '—' }),
              designMark('原型数据源没有该字段，按未知显示（不显示 0）')
            ]) },
            { k: '恢复结论', v: '未提交变更 ' + F.num(D.recoveryReport.uncommittedChanges) + ' 处 · 段 ' + D.recoveryReport.segmentStatus + ' · 租约 ' + D.recoveryReport.leaseStatus },
            { k: '审计链头', v: String(D.recoveryReport.auditHead).slice(0, 16) + '…', mono: true },
            { k: '审计链校验', node: D.auditVerified ? UI.badge({ text: '通过', tone: 'success' }) : UI.badge({ text: '未通过', tone: 'danger' }) },
            { k: '事件环形缓冲', v: F.num(a.eventRingBuffer) + ' 条' },
            { k: '渲染节流', v: F.num(a.renderThrottleMs) + ' ms' }
          ]),
          MUI.codeBlock(snapshotText),
          MUI.muted('快照为原型本地假数据；真实实现中每个字段都应能指回某个导出函数的返回结构。')
        ])
      }),
      MUI.mcard({
        title: 'ERROR_DIAG 汇总', sub: '按错误码聚合（来源：脱敏审计 detail.result 计数）',
        body: errs.length
          ? MUI.mlist(errs.map(function (e) {
              var tone = VS.tone(e.code) === 'danger' ? 'danger' : (VS.tone(e.code) === 'stale' ? 'stale' : 'info');
              return infoRow({
                label: F.num(e.code) + ' · ' + VS.errTitle(e.code),
                sub: F.dateLong(e.lastMs) + ' · ' + F.relative(e.lastMs),
                badge: UI.badge({ text: '×' + F.num(e.count), tone: tone, title: (VS.ERR[e.code] || {}).hint || '' })
              });
            }))
          : MUI.empty({ icon: 'list', title: '无错误码记录', desc: '审计日志中没有非零 result；按「无数据一律灰」原则不显示 0，也不显示绿灯。' })
      }),
      MUI.mcard({
        title: '事件流', sub: 'seq 与环形缓冲溢出计数',
        body: h('div', { class: 'col gap-3' }, [
          MUI.mkv([
            { k: '审计链 seq（最近）', v: numOrDash((D.auditLog[0] || {}).seq), mono: true },
            { k: '事件流 seq', node: h('div', { class: 'row gap-2' }, [
              h('span', { class: 't-muted', text: '—' }),
              designMark('原型数据源没有独立的事件流 seq')
            ]) },
            { k: 'EVENT_OVERFLOW 丢失', node: h('div', { class: 'row gap-2' }, [
              h('span', { class: 't-muted', text: '—' }),
              designMark('原型数据源没有溢出丢失计数')
            ]) },
            { k: '环形缓冲容量', v: F.num(a.eventRingBuffer) + ' 条' }
          ]),
          UI.alertbar({
            tone: 'stale', icon: 'ban', title: '诚实纪律',
            text: '凡在本端答不出「读的是哪个接口」的读数，一律按未知显示并标注设计保证，不显示 0 冒充正常，也不整块编造。'
          })
        ])
      }),
      MUI.mcard({
        title: '恢复结论', sub: '与恢复向导同源（D.recoveryReport）',
        body: h('div', { class: 'col gap-3' }, [
          recDetailRows(),
          mbtn('打开恢复向导', { variant: 'ghost', icon: 'recover', onClick: function () { go(ctx, 'recover'); } })
        ])
      })
    ];

    return MUI.page({
      title: '诊断', sub: 'vault_core_diagnostics · ERROR_DIAG · 事件流',
      back: true, tab: false,
      actions: [{ icon: 'export', label: '导出诊断包（脱敏）', onClick: exportDiagSheet }],
      body: MUI.screen(kids)
    });
  };

  var OSS_LICENSES = [
    { name: 'Flutter', license: 'BSD-3-Clause', use: '客户端 UI 框架（五端共用一套 Token）' },
    { name: 'Rust 标准库与 crates.io 依赖', license: 'MIT / Apache-2.0（逐 crate 见 LICENSES）', use: '核心引擎与 FFI 门面' },
    { name: 'snow', license: 'Apache-2.0 / MIT', use: 'Noise 信道（设备配对）' },
    { name: 'ed25519-dalek', license: 'BSD-3-Clause', use: '签名与清单验签' },
    { name: 'argon2', license: 'MIT / Apache-2.0', use: 'Argon2id KDF（KEK 派生）' },
    { name: 'aes-gcm', license: 'MIT / Apache-2.0', use: 'AES-256-GCM AEAD' }
  ];

  /** 能力三档清单：能力 / 档位 / 来源接口 —— 答不出接口就标 design 或整块不渲染 */
  function capabilityRows() {
    var rows = Object.keys(VS.CAP).map(function (k) {
      var meta = VS.CAP[k];
      return { name: meta.name, key: k, tier: S.capTier(k), src: 'capability_bits bit ' + F.num(meta.bit) };
    });
    rows.push({ name: '后台同步（移动受管）', key: 'CAP_MOBILE_MANAGED', tier: S.capTier('CAP_MOBILE_MANAGED'), src: '如实声明：尽力而为，无独立守护进程（docs/v2.0/04 §1.2）' });
    rows.push({ name: 'P2P 入站监听', key: '—', tier: 'design', src: '本端外壳约束：仅前台监听（docs/v2.0/04 §六）' });
    rows.push({ name: '分栏视图', key: '—', tier: 'hidden', src: '手机档架构上不提供 → 整块不渲染' });
    rows.push({ name: '异地检测', key: '—', tier: 'hidden', src: '架构上无此接口 → 该行不渲染' });
    rows.push({ name: '内存哨兵', key: '—', tier: 'design', src: (D.securityChecks.filter(function (c) { return c.key === 'sentinel'; })[0] || {}).src || '（无导出）' });
    rows.push({ name: '应用版本 / 构建号', key: '—', tier: 'design', src: '原型本地常量，无接口读数' });
    return rows;
  }

  VS.pages['about'] = function (ctx) {
    var kids = [
      MUI.mcard({
        title: 'VaultSync', sub: '端到端加密的多设备文件同步系统',
        body: MUI.mkv([
          { k: '产品名', v: 'VaultSync' },
          { k: '版本', node: h('div', { class: 'row gap-2' }, [h('span', { class: 't-mono', text: APP_VERSION }), designMark('原型本地常量，无接口读数')]) },
          { k: '构建号', node: h('div', { class: 'row gap-2' }, [h('span', { class: 't-mono', text: APP_BUILD }), designMark('原型本地常量，无接口读数')]) },
          { k: '本端形态', v: '移动端（<720 档）· 受管外壳' },
          { k: '开源许可', v: '见下方开源许可列表' },
          { k: '设计基线', v: 'docs/v2.0（现行）/ docs/v1.0（归档）' }
        ])
      }),
      MUI.mcard({
        title: '差异化定位', sub: '端到端加密 + 自有中继 + 隐写 + 审计链 + 多端一致',
        body: h('ul', { class: 'plan-feats' }, [
          '端到端加密：明文与密钥只存在于本地核心引擎，中继仅转发密文',
          '自有中继：可自建、可私有部署；中继不落盘、不解密',
          '隐写术：载荷为 AEAD 密文，载体只搬位不做密码学；不可检测性无安全保证',
          '审计链：链式哈希日志、导出加密；审计写入失败不阻断业务',
          '多端一致：MK 不变 + per-file FSKey + CDC 分块，局部修改不触发全文件重加密'
        ].map(function (t) {
          return h('li', { dataset: { off: 'false' } }, [UI.icon('check', 13), h('span', { text: t })]);
        }))
      }),
      MUI.mcard({
        title: '开源许可',
        body: h('div', { class: 'col gap-2' }, OSS_LICENSES.map(function (l) {
          return infoRow({ label: l.name, sub: l.use, value: l.license });
        }))
      }),
      MUI.mcard({
        title: '证据入口', sub: '四层：可核实来源 / 未核实项目名 / 未覆盖 / 免责',
        body: h('div', { class: 'col gap-2' }, [
          infoRow({ label: '① 可核实来源', value: '可核实', badge: UI.badge({ text: '可核实', tone: 'success' }), sub: '仓库内设计文档、任务面板、FFI 导出清单与冒烟测试断言；每条对外声称都应能指到具体文件与接口名。' }),
          infoRow({ label: '② 未核实项目名不得作为竞品证据', value: '不作证据', badge: UI.badge({ text: '不作证据', tone: 'warning' }), sub: '未经核实的外部项目名、版本号与测试结论一律不作为对比证据；本端不展示任何未核实的竞品结论。' }),
          infoRow({ label: '③ 未覆盖', value: '标注 design', badge: UI.badge({ text: '标注 design', tone: 'stale' }), sub: '尚无运行时接口的能力按「设计保证」标注；例如内存哨兵与异地检测在本端没有可读接口。' }),
          infoRow({ label: '④ 免责', value: '非承诺', sub: '原型为纯静态 Mock：全部数据为前端假数据，不含真实加密逻辑，不构成安全性承诺。' })
        ])
      }),
      MUI.mcard({
        title: '能力三档清单', sub: '逐项：能力 / 档位 / 来源接口 —— 答不出接口即标注或不渲染',
        body: h('div', { class: 'col gap-3' }, [
          MUI.mlist(capabilityRows().map(function (r) {
            return infoRow({
              label: r.name,
              sub: r.key + ' · ' + r.src,
              node: UI.tierMark(r.tier)
            });
          }), { flush: true }),
          MUI.muted('三档含义：真值 = 有运行时接口读数；设计保证 = 契约存在但无导出，如实标注；不渲染 = 架构上无此接口，整块不出现。')
        ])
      }),
      MUI.mcard({
        title: '致谢',
        body: h('div', { class: 'col gap-2' }, [
          h('div', { class: 't-body', text: '感谢 Flutter、Rust 生态与各开源依赖的作者；完整许可文本随发行包提供。' }),
          MUI.muted('原型语料来自 docs/v2.0 各模块设计文档；术语以 docs/README.md 的统一术语表为准。')
        ])
      }),
      h('div', { class: 'col gap-2' }, [
        mbtn('打开诊断', { icon: 'cpu', onClick: function () { go(ctx, 'diagnostics'); } }),
        mbtn('返回设置', { variant: 'ghost', icon: 'settings', onClick: function () { go(ctx, 'settings'); } })
      ])
    ];

    return MUI.page({
      title: '关于', sub: '版本 / 许可 / 差异化定位 / 证据入口 / 能力三档清单',
      back: true, tab: false,
      body: MUI.screen(kids)
    });
  };

  /* ======================================================================
   * H. 注册动作（供演示控制台 / 其它页面调用；不覆盖 app.js 的 mobile.*）
   * ==================================================================== */
  VS.actions['mobile.openSettings'] = function () { go(null, 'settings'); };
  VS.actions['mobile.openLicense'] = function () { go(null, 'license'); };
  VS.actions['mobile.openAbout'] = function () { go(null, 'about'); };
  VS.actions['mobile.openDiagnostics'] = function () { go(null, 'diagnostics'); };
  VS.actions['mobile.openRecover'] = function () { go(null, 'recover'); };
  VS.actions['mobile.exportDiagnostics'] = function () { exportDiagSheet(); };
  VS.actions['mobile.checkUpdate'] = function () { checkUpdate(); };

})(window);
