/* ============================================================================
 * VaultSync V2.0 原型 — 系统层视图（注册于 VS.pages）
 *   settings / recover / maintenance-rotate / maintenance-migrate
 *   license / about / diagnostics
 *
 * 语料依据：docs/v2.0/03（前端设计）、04（展示形式）、05-0X、07、08、12、13
 * 依赖：core.js（VS.util / VS.fmt / VS.store / VS.derive / VS.CAP / VS.ERR）、
 *       ui.js（VS.ui.*）、data.js（VS.data.*）
 *
 * 诚实纪律（docs/v2.0/11 §二）：本文件出现的每个读数都要能回答「读的是哪个接口」。
 *   · 有原型数据支撑的 → 直接引用 D.*，不写死。
 *   · 原型数据源没有的字段   → 隐藏（不渲染）或用 UI.tierMark('design') 显式标注，
 *                            绝不假装成真值、绝不显示 0 冒充「正常」。
 * ==========================================================================*/
(function (global) {
  'use strict';

  var VS = global.VS;
  var h = VS.util.h, UI = VS.ui, F = VS.fmt, D = VS.data, S = VS.store;

  VS.pages = VS.pages || {};
  VS.actions = VS.actions || {};

  var NOW = D.NOW, MIN = D.MIN, DAY = D.DAY;

  /* ======================================================================
   * 0. 局部原语（VS.ui 里没有的，只在本文件内实现）
   * ==================================================================== */

  /** 写操作被冻结时给出的人类可读 reason；null = 允许写入 */
  function denyReason() {
    var c = VS.derive.denyWrite();
    if (c === 9) return '只读降级：另一实例持有写租约（错误码 9）';
    if (c === 10) return '维护态：业务写入已冻结（错误码 10）';
    if (c === 5) return '保险箱未解锁（错误码 5）';
    return null;
  }
  function frozen() { return denyReason() !== null; }

  /** 受写冻结保护的按钮：冻结则置灰 + reason，而不是假装成功 */
  function guardBtn(o) {
    var r = denyReason();
    if (r && !o.ignoreFrozen) {
      return UI.btn({
        label: o.label, icon: o.icon, variant: o.variant, size: o.size,
        disabled: true, title: (o.title || o.label) + ' · ' + r
      });
    }
    return UI.btn({
      label: o.label, icon: o.icon, variant: o.variant, size: o.size,
      title: o.title || o.label, onClick: o.onClick
    });
  }

  /** 写冻结提示条（有 reason 才渲染，没有就整块不渲染） */
  function frozenBar() {
    var r = denyReason();
    if (!r) return null;
    return UI.alertbar({ tone: 'stale', icon: 'ban', title: '写操作当前不可用', text: r });
  }

  /** 页面骨架：page > page-head + page-body */
  function pageView(o) {
    return h('div', { class: 'page' }, [
      h('div', { class: 'page-head' }, [
        h('div', { class: 'page-titles' }, [
          h('h1', { class: 't-h1', text: o.title }),
          o.sub ? h('div', { class: 't-caption', text: o.sub }) : null
        ]),
        h('div', { class: 'page-actions' }, o.actions || [])
      ]),
      h('div', { class: 'page-body' }, o.body || [])
    ]);
  }

  /** 一张分组卡片（带 id，供左侧锚点导航滚动） */
  function section(id, title, sub, body, actions) {
    var card = UI.card({ title: title, sub: sub, body: body, actions: actions });
    card.id = 'sec-' + id;
    card.classList.add('sec-anchor');
    return card;
  }

  /** 只读信息行（.list-row，但不显示手型光标） */
  function infoRow(o) {
    return h('div', { class: 'list-row', style: { cursor: 'default' }, title: o.title || '' }, [
      h('div', { class: 'lr-main' }, [
        h('div', { class: 'lr-title' }, [h('span', { text: o.label }), o.mark || null]),
        o.sub ? h('div', { class: 'lr-sub', text: o.sub }) : null
      ]),
      h('div', { class: 'lr-actions' }, [
        o.badge || null,
        o.node || (o.value !== undefined && o.value !== null && o.value !== ''
          ? h('span', { class: o.mono ? 't-mono t-muted' : 't-caption', text: o.value })
          : null)
      ])
    ]);
  }

  /** 开关行：左侧说明，右侧开关 */
  function switchRow(o) {
    var ctl = UI.switchCtl({
      checked: !!o.checked,
      disabled: !!o.disabled,
      onChange: o.disabled ? null : o.onChange
    });
    var tip = o.reason || o.label;
    ctl.title = tip;
    if (ctl.input) ctl.input.title = tip;
    if (ctl.input && o.ariaLabel) ctl.input.setAttribute('aria-label', o.ariaLabel);
    return h('div', { class: 'list-row', style: { cursor: 'default' } }, [
      h('div', { class: 'lr-main' }, [
        h('div', { class: 'lr-title' }, [h('span', { text: o.label }), o.mark || null]),
        o.sub ? h('div', { class: 'lr-sub', text: o.sub }) : null
      ]),
      h('div', { class: 'lr-actions' }, [o.badge || null, ctl])
    ]);
  }

  /** 紧凑行（label + 右侧控件），用于表单密度高的分组 */
  function ctlRow(label, node, opts) {
    opts = opts || {};
    return h('div', { class: 'row-between', style: { padding: '6px 0' } }, [
      h('div', { class: 'grow' }, [
        h('div', { class: 't-strong', text: label }),
        opts.sub ? h('div', { class: 't-caption', text: opts.sub }) : null
      ]),
      h('div', { class: 'row gap-2' }, [node])
    ]);
  }

  /** 定位导航：优先 VS.go，其次 router，最后写 store（三种都存在时取第一个可用的） */
  function navigate(route, params) {
    if (typeof VS.go === 'function') { VS.go(route, params); return; }
    if (VS.router && typeof VS.router.go === 'function') { VS.router.go(route, params); return; }
    S.set({ route: route, routeParams: params || {} });
  }
  function goCtx(ctx, route, params) {
    if (ctx && typeof ctx.go === 'function') { ctx.go(route, params); return; }
    navigate(route, params);
  }

  /** 数字：0 / 空 → 灰（绝不把「没有」显示成 0） */
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

  /* ---------------------------------------------------------------------
   * 0.1 本页面的原型本地状态
   *     仅在「data.js 里确实没有该字段」时使用，并且在使用点标注来源。
   * ------------------------------------------------------------------- */
  var L = {
    /* 解锁与落锁：data.settings 只有布尔与 autoLockIdleMs */
    idleLock: !!(D.settings && D.settings.lockPolicy && D.settings.lockPolicy.idle),
    idleMinutes: Math.round((D.settings && D.settings.autoLockIdleMs ? D.settings.autoLockIdleMs : 15 * MIN) / MIN),
    /* 「离开应用落锁 N 秒」在 data.js 中无对应字段 → 本地默认 30s（5–300） */
    awayLock: true, awaySeconds: 30,
    lockOnScreenSaver: !!(D.settings && D.settings.lockOnScreenSaver),
    lockOnSleep: !!(D.settings && D.settings.lockOnSleep),
    lockOnUsbRemoval: !!(D.settings && D.settings.lockOnUsbRemoval),
    /* 生物识别：CAP_BIO 未登记在 VS.CAP 中（见本文件末尾自查结论） */
    bioBound: true, bioLastCode: null,
    /* 伪装入口 */
    disguiseEnabled: !!(D.settings && D.settings.disguiseEnabled),
    /* 通知矩阵：data.settings.notifications 只有 info/warn/danger/systemNotify/sound */
    matrix: {
      info:   { app: !!(D.settings && D.settings.notifications.info),   sys: !!(D.settings && D.settings.notifications.systemNotify), push: true,  mail: false },
      warn:   { app: !!(D.settings && D.settings.notifications.warn),   sys: !!(D.settings && D.settings.notifications.systemNotify), push: true,  mail: true },
      danger: { app: !!(D.settings && D.settings.notifications.danger), sys: true, push: true, mail: true }
    },
    dndEnabled: !!(D.settings && D.settings.dndEnabled),
    dndStart: (D.settings && D.settings.dndStart) || '23:00',
    dndEnd: (D.settings && D.settings.dndEnd) || '07:00',
    /* 外观 */
    density: 'comfortable',
    /* 隐私 */
    screenshotProtect: !!(D.settings && D.settings.screenshotProtect),
    thumbnailsEnabled: !!(D.settings && D.settings.thumbnailsEnabled),
    telemetry: !!(D.settings && D.settings.telemetry),           /* 默认关闭 */
    /* 高级 */
    workIsolate: !!(D.settings && D.settings.advanced.workIsolate),
    /* 从属密钥轮换弹窗的选中集合 */
    subKeySel: {},
    /* 维护态（与 VS.store 同源，供从属密钥多选判定使用） */
    formatVersion: D.migrationState.srcVer,
    /* 轮换页：可放弃/回滚的状态档位（1 = mark|rekey，2 = propagate，3 = done） */
    rotHasOldMk: true,
    rotAutoResumeCount: 3,
    rotConflict: false,
    /* 恢复向导 */
    recStep: 0, recPath: 'rollback', recConfirmed: false, recAttempts: 0, recProgress: 0,
    /* 迁移向导 */
    migRunning: false, migStep: 0,
    /* 授权：中继凭据兑换状态机 */
    relayExchange: (D.licenseState && D.licenseState.exchangeState) || 'idle'
  };

  function isMaintenance() {
    return !!S.get('maintenance') || S.get('session') === 'Maintenance';
  }

  /* ======================================================================
   * A. settings — 设置页
   * ==================================================================== */

  var SETTINGS_SECTIONS = [
    { id: 'lock', label: '解锁与落锁' },
    { id: 'bio', label: '生物识别' },
    { id: 'mpw', label: '主密码' },
    { id: 'disguise', label: '伪装入口' },
    { id: 'subkeys', label: '从属密钥管理' },
    { id: 'mk', label: 'MK 轮换' },
    { id: 'format', label: '格式与迁移' },
    { id: 'sync', label: '同步策略' },
    { id: 'notify', label: '通知渠道矩阵' },
    { id: 'appearance', label: '外观与语言' },
    { id: 'update', label: '更新通道' },
    { id: 'privacy', label: '隐私与安全' },
    { id: 'advanced', label: '高级 / 诊断' },
    { id: 'about', label: '关于' },
    { id: 'license', label: '授权入口' }
  ];

  /* 从属密钥表（key_id 1–8；9–255 保留 → 整块不渲染） */
  var SUB_KEYS = [
    { id: 1, use: '索引密钥（Index）',       gen: NOW - 121 * DAY, wrap: 'wrapped' },
    { id: 2, use: '搜索密钥（Search）',      gen: NOW - 121 * DAY, wrap: 'wrapped' },
    { id: 3, use: '分享密钥（Share）',       gen: NOW - 96 * DAY,  wrap: 'wrapped' },
    { id: 4, use: '订单信封密钥（Order）',   gen: NOW - 96 * DAY,  wrap: 'wrapped' },
    { id: 5, use: '隐写载荷密钥（Stego）',   gen: NOW - 62 * DAY,  wrap: 'wrapped' },
    { id: 6, use: '链键（AuditChain）',      gen: NOW - 121 * DAY, wrap: 'wrapped' },
    { id: 7, use: '审计导出密钥（Export）',  gen: NOW - 45 * DAY,  wrap: 'rotation_pending' },
    { id: 8, use: '发现指纹盐（DiscoverySalt）', gen: NOW - 30 * DAY, wrap: 'wrapped' }
  ];
  /* 选中 {1,2,6,7} 时必须处于维护态，否则禁用并给 reason */
  var SUB_KEYS_NEED_MAINT = [1, 2, 6, 7];
  var SUB_WRAP_LABEL = {
    wrapped: { label: '已包装', tone: 'success' },
    rotation_pending: { label: '待重包装', tone: 'warning' }
  };

  function sectionLock() {
    var idleSelect = UI.select({
      value: L.idleMinutes,
      disabled: !L.idleLock,
      options: [
        { value: 5, label: '5 分钟' }, { value: 15, label: '15 分钟' },
        { value: 30, label: '30 分钟' }, { value: 60, label: '60 分钟' }
      ],
      onChange: function (v) { L.idleMinutes = v; }
    });
    idleSelect.title = '空闲超时时长（默认 15 分钟）';

    var awayInput = UI.input({ type: 'number', value: String(L.awaySeconds) });
    awayInput.min = '5'; awayInput.max = '300'; awayInput.step = '5';
    awayInput.style.width = '96px';
    awayInput.title = '离开应用后落锁的秒数（最小 5，最大 300）';
    awayInput.addEventListener('change', function () {
      L.awaySeconds = VS.util.clamp(parseInt(awayInput.value, 10) || 30, 5, 300);
      awayInput.value = String(L.awaySeconds);
      UI.toast({ tone: 'info', title: '离开应用落锁', msg: '已设为 ' + F.num(L.awaySeconds) + ' 秒' });
    });

    var awaySelect = UI.select({
      value: L.awaySeconds, disabled: !L.awayLock,
      options: [5, 10, 30, 60, 120, 300].map(function (s) { return { value: s, label: F.num(s) + ' 秒' }; }),
      onChange: function (v) { L.awaySeconds = v; awayInput.value = String(v); }
    });
    awaySelect.title = '离开应用落锁秒数（5–300）';

    return section('lock', '解锁与落锁', '超时 / 离开 / 系统事件触发的自动锁定；远端锁定为服务端指令，本端不设开关', [
      h('div', { class: 'col', style: { gap: '0' } }, [
        switchRow({
          label: '空闲超时自动落锁', checked: L.idleLock, ariaLabel: '空闲超时自动落锁',
          sub: '无输入达到所选时长后自动落锁',
          onChange: function (e) { L.idleLock = e.target.checked; idleSelect.dataset.disabled = L.idleLock ? 'false' : 'true'; }
        }),
        ctlRow('空闲超时时长', idleSelect, { sub: '可选 5 / 15 / 30 / 60 分钟，默认 15 分钟' }),
        switchRow({
          label: '离开应用后落锁', checked: L.awayLock, ariaLabel: '离开应用后落锁',
          sub: '应用失去前台焦点后计时',
          onChange: function (e) { L.awayLock = e.target.checked; }
        }),
        ctlRow('离开应用落锁时长', h('div', { class: 'row gap-2' }, [awaySelect, awayInput]), {
          sub: '最小 5 秒 · 最大 300 秒 · 默认 30 秒'
        }),
        switchRow({
          label: '锁屏时同步落锁', checked: L.lockOnScreenSaver, ariaLabel: '锁屏时同步落锁',
          onChange: function (e) { L.lockOnScreenSaver = e.target.checked; }
        }),
        switchRow({
          label: '睡眠时同步落锁', checked: L.lockOnSleep, ariaLabel: '睡眠时同步落锁',
          onChange: function (e) { L.lockOnSleep = e.target.checked; }
        }),
        infoRow({
          label: '远程锁定',
          sub: '由已配对设备广播的签名指令触发（kind=2 锁定指令）。本端不提供本地开关，也不存在本地绕过路径。',
          badge: UI.badge({ text: '服务端指令', tone: 'info' }),
          node: h('span', { class: 't-caption t-muted', text: '只读说明' })
        }),
        switchRow({
          label: '移除外部介质后落锁', checked: L.lockOnUsbRemoval, ariaLabel: '移除外部介质后落锁',
          sub: '保险箱位于可移动介质时生效',
          onChange: function (e) { L.lockOnUsbRemoval = e.target.checked; }
        }),
        infoRow({
          label: '异地检测（异地登录检测）',
          sub: '架构上不存在该接口：本端不采集位置、不上报登录地，因此没有可渲染的开关。',
          mark: hiddenMark('后端无此接口 → 该行不渲染（此处仅登记结论）')
        })
      ])
    ]);
  }

  function sectionBio() {
    /* CAP_BIO 未置位 → 入口隐藏（不报错、不置灰） */
    if (!S.cap('CAP_BIO')) return null;
    var tier = S.capTier('CAP_BIO');
    var code = L.bioLastCode;

    var body = [];
    body.push(infoRow({
      label: '绑定状态',
      sub: '凭据存放于平台安全存储；MK 只在引擎内解封，展示层不接触。',
      badge: L.bioBound ? UI.badge({ text: '已绑定', tone: 'success' })
                        : UI.badge({ text: '未绑定', tone: '' }),
      mark: tier === 'real' ? UI.tierMark('real') : designMark('CAP_BIO 未登记在 VS.CAP，按设计保证标注')
    }));

    /* 错误码 5（安全区失效）→ 引导重绑；错误码 4（安全存储不可用）→ 入口隐藏 */
    if (code === 5) {
      body.push(UI.errorBox(5, {
        text: '安全区失效，已绑定的生物识别凭据不可用。请重新绑定。',
        actions: [UI.btn({ label: '重新绑定', variant: 'primary', icon: 'fingerprint', title: '重新绑定生物识别', onClick: openBioRebind })]
      }));
    } else if (code === 4) {
      /* 硬约束：错误码 4 → 隐藏入口，不报错、不置灰 */
      return section('bio', '生物识别', null, [
        UI.empty({
          icon: 'fingerprint',
          title: '本机不可用生物识别',
          desc: '平台安全存储不可用（错误码 4）。按能力隐藏入口，不提供降级路径。'
        })
      ]);
    } else {
      body.push(UI.muted('错误码 5（安全区失效）会在此给出重绑引导；错误码 4（安全存储不可用）按能力隐藏入口，置灰不出现。'));
    }

    var actions = [];
    if (!L.bioBound) {
      actions.push(guardBtn({ label: '绑定', icon: 'fingerprint', variant: 'primary', title: '绑定生物识别', onClick: function () { L.bioBound = true; UI.toast({ tone: 'success', title: '已绑定', msg: '原型演示：绑定流程已受理' }); } }));
    } else {
      actions.push(guardBtn({ label: '解绑', icon: 'ban', variant: 'danger', title: '解绑生物识别（危险）', onClick: function () { L.bioBound = false; UI.toast({ tone: 'warn', title: '已解绑', msg: '可用主密码继续解锁' }); } }));
      actions.push(guardBtn({ label: '重新绑定', icon: 'refresh', title: '重新绑定', onClick: openBioRebind }));
    }

    function openBioRebind() {
      if (frozen()) { UI.toast({ tone: 'warn', title: '重新绑定不可用', msg: denyReason() }); return; }
      UI.confirm({
        title: '重新绑定生物识别',
        sub: '旧凭据将被吊销后重建',
        tone: 'warn',
        body: h('div', { class: 'col gap-2' }, [
          h('div', { class: 't-body', text: '重新绑定会先吊销平台安全存储中的旧凭据，再写入新凭据。MK 不变更，因此不需要重新加密任何数据。' }),
          UI.muted('失败语义：安全存储不可用（码 4）→ 入口隐藏；安全区失效（码 5）→ 回到本弹窗重新绑定。')
        ]),
        confirmLabel: '确认重绑',
        onConfirm: function () { L.bioLastCode = null; UI.toast({ tone: 'success', title: '重新绑定已完成', msg: '原型演示' }); }
      });
    }

    return section('bio', '生物识别', '平台安全存储中的 KEK 之一；解封 MK 的 wrapped 副本', body, actions);
  }

  function sectionMasterPassword() {
    return section('mpw', '主密码', '修改主密码只重封 KEK，MK 与全部数据密文保持不变', [
      infoRow({
        label: '零重加密',
        sub: 'MK 由 CSPRNG 生成且永不变更；主密码只是解封 MK 的两把 KEK 之一。改密码 = 用新 KEK 重封同一份 MK，不触发任何文件重加密。'
      }),
      infoRow({ label: '上次修改', value: F.dateLong(NOW - 84 * DAY), mono: false }),
      infoRow({ label: '失败冷却', sub: '连续失败后按引擎返回值给出等待时间（错误码 1 / 2）' })
    ], [UI.btn({
      label: '修改主密码', icon: 'key', variant: 'primary', title: '修改主密码',
      onClick: openChangeMasterPassword
    })]);
  }

  function openChangeMasterPassword() {
    if (frozen()) { UI.toast({ tone: 'warn', title: '修改主密码不可用', msg: denyReason() }); return; }
    var cur = UI.input({ type: 'password', placeholder: '当前主密码' });
    var nw = UI.input({ type: 'password', placeholder: '新主密码' });
    var cf = UI.input({ type: 'password', placeholder: '再次输入新主密码' });
    var bar = UI.bar(0, 'accent', 'sm');
    var strength = h('div', { class: 't-caption', text: '强度：未输入' });

    /* 强度条：只做长度与字符集的经验估计（原型本地启发式，非引擎输入） */
    function score(v) {
      if (!v) return 0;
      var s = Math.min(1, v.length / 20);
      if (/[a-z]/.test(v) && /[A-Z]/.test(v)) s += 0.15;
      if (/\d/.test(v)) s += 0.1;
      if (/[^\w]/.test(v)) s += 0.15;
      return Math.max(0, Math.min(1, s));
    }
    nw.addEventListener('input', function () {
      var p = score(nw.value);
      bar.firstChild.style.width = (Math.round(p * 1000) / 10) + '%';
      strength.textContent = '强度：' + (nw.value ? F.pct(p) : '未输入');
    });

    var m = UI.modal({
      title: '修改主密码',
      sub: '零重加密：MK 永不变更，仅重封 KEK',
      size: 'md',
      body: h('div', { class: 'col gap-3' }, [
        UI.field({ label: '当前主密码', req: true, control: cur }),
        UI.field({ label: '新主密码', req: true, control: nw, hint: '建议 12 位以上，混用大小写、数字与符号' }),
        h('div', { class: 'col gap-1' }, [bar, strength]),
        UI.field({ label: '确认新主密码', req: true, control: cf }),
        UI.alertbar({
          tone: 'info', icon: 'info', title: '说明',
          text: 'MK 永不变更，仅重封 KEK；改密不触发任何文件重加密，也不会重建会话密钥。'
        })
      ]),
      autofocus: false,
      footer: function () {
        return [
          UI.btn({ label: '取消', title: '取消修改', onClick: function () { m.close(); } }),
          UI.btn({
            label: '确认修改', variant: 'primary', title: '确认修改主密码',
            disabled: true,
            onClick: function () {
              m.close();
              UI.toast({ tone: 'success', title: '主密码已修改', msg: '原型演示：未发生任何重加密' });
            }
          })
        ];
      }
    });
    setTimeout(function () {
      var confirmBtn = m.el.querySelector('.modal-foot .btn-primary');
      function sync() {
        var ok = cur.value && nw.value && nw.value === cf.value && score(nw.value) >= 0.35;
        if (confirmBtn) confirmBtn.disabled = !ok;
      }
      [cur, nw, cf].forEach(function (i) { i.addEventListener('input', sync); });
      cur.focus();
    }, 40);
  }

  function sectionDisguise() {
    return section('disguise', '伪装入口', '伪空间与真空间使用同一套控件，展示层不做真伪区分', [
      switchRow({
        label: '启用伪装入口', checked: L.disguiseEnabled, ariaLabel: '启用伪装入口',
        sub: '输入伪装密码进入伪空间；伪装密码与主密码共用同一输入控件',
        onChange: function (e) { L.disguiseEnabled = e.target.checked; }
      }),
      UI.alertbar({
        tone: 'warn', icon: 'alert', title: '共用输入控件',
        text: '伪装密码与主密码共用同一输入控件，UI 不做真伪区分；失败冷却计数也共用同一计数器。'
      }),
      infoRow({ label: '冷却计数', sub: '真 / 伪密码失败共用同一冷却窗口（错误码 1 / 2），因此从冷却行为无法反推输入的是哪一种密码。' })
    ]);
  }

  function sectionSubKeys() {
    var rows = SUB_KEYS.map(function (k) {
      var w = SUB_WRAP_LABEL[k.wrap] || { label: '未知', tone: '' };
      return h('tr', {}, [
        h('td', { class: 'num t-num', text: F.num(k.id) }),
        h('td', { text: k.use }),
        h('td', { class: 't-caption', text: F.dateLong(k.gen) }),
        h('td', { class: 't-caption', text: F.relative(k.gen) }),
        h('td', {}, [UI.badge({ text: w.label, tone: w.tone })])
      ]);
    });

    /* 自定义渲染以保证数字 / 时间一律走 F.*，不手写拼接 */
    var body = [h('div', { class: 'table-wrap' }, [
      h('table', { class: 'data' }, [
        h('thead', {}, [h('tr', {}, [
          h('th', { style: { width: '72px' } }, 'key_id'),
          h('th', {}, '用途'),
          h('th', {}, '生成时间'),
          h('th', {}, '距今'),
          h('th', { style: { width: '120px' } }, '包装状态')
        ])]),
        h('tbody', {}, rows)
      ])
    ])];
    body.push(UI.muted('key_id 9–255 为保留区间，本端不渲染对应行（无接口、无包装记录）。'));
    body.push(UI.muted('独立包装于 VSVB v3 头部：从属密钥不再从 MK 直接派生，MK 轮换后可单独重包装。'));

    return section('subkeys', '从属密钥管理', '8 条已定义的从属密钥（索引 / 搜索 / 分享 / 订单信封 / 隐写载荷 / 链键 / 审计导出 / 发现指纹盐）', body, [
      guardBtn({ label: '轮换从属密钥', icon: 'refresh', title: '轮换从属密钥', onClick: openSubKeyRotate })
    ]);
  }

  function openSubKeyRotate() {
    L.subKeySel = {};
    var maint = isMaintenance();
    var boxes = [];
    var wrap = h('div', { class: 'col gap-1' });

    SUB_KEYS.forEach(function (k) {
      var needMaint = SUB_KEYS_NEED_MAINT.indexOf(k.id) >= 0;
      var disabled = needMaint && !maint;
      var reason = disabled ? 'key_id ' + k.id + ' 的轮换必须处于维护态（否则业务写入会与重包装竞争）' : null;
      var cb = UI.checkbox({
        label: 'key_id ' + k.id + ' · ' + k.use,
        sub: reason || (needMaint ? '属于维护态专用密钥' : null),
        checked: false,
        disabled: disabled,
        onChange: function (e) { L.subKeySel[k.id] = e.target.checked; }
      });
      cb.title = reason || ('选中以轮换 key_id ' + k.id);
      if (cb.querySelector('input')) cb.querySelector('input').title = cb.title;
      boxes.push(cb);
      wrap.appendChild(cb);
    });

    if (!maint) {
      wrap.appendChild(UI.alertbar({
        tone: 'stale', icon: 'ban', title: '当前非维护态',
        text: 'key_id 1 / 2 / 6 / 7（索引、搜索、链键、审计导出）的轮换会与业务写入竞争，必须先进入维护态；其余 4 条可随时轮换。'
      }));
    }

    var m = UI.modal({
      title: '轮换从属密钥',
      sub: '可多选；维护态专用密钥在非维护态下禁用',
      size: 'lg',
      body: wrap,
      autofocus: false,
      footer: function () {
        var sel = Object.keys(L.subKeySel).filter(function (k) { return L.subKeySel[k]; });
        var blocked = sel.some(function (k) { return SUB_KEYS_NEED_MAINT.indexOf(parseInt(k, 10)) >= 0; }) && !maint;
        var btn = UI.btn({
          label: '开始轮换', variant: 'primary', title: blocked ? '选中项包含维护态专用密钥，当前非维护态' : '开始轮换选中的从属密钥',
          disabled: !sel.length || blocked || frozen(),
          onClick: function () {
            m.close();
            UI.toast({ tone: 'info', title: '已受理从属密钥轮换', msg: '原型演示：' + F.num(sel.length) + ' 条密钥' });
          }
        });
        boxes.forEach(function (b) {
          if (b.querySelector('input')) b.querySelector('input').addEventListener('change', function () {
            var s = Object.keys(L.subKeySel).filter(function (k) { return L.subKeySel[k]; });
            var bad = s.some(function (k) { return SUB_KEYS_NEED_MAINT.indexOf(parseInt(k, 10)) >= 0; }) && !maint;
            btn.disabled = !s.length || bad || frozen();
          });
        });
        return [
          UI.btn({ label: '取消', title: '取消', onClick: function () { m.close(); } }),
          btn
        ];
      }
    });
  }

  function sectionMkRotation() {
    var maintenance = S.get('maintenance');
    var inRotation = maintenance === 'rotation';
    var body = [
      infoRow({
        label: '当前状态',
        sub: 'MK 是全库根密钥；轮换只换 MK 并重包装从属密钥，不重加密文件体（文件由 per-file FSKey 保护，FSKey 由 MK 包装）。',
        badge: inRotation ? UI.badge({ text: '轮换进行中', tone: 'maintenance' }) : UI.badge({ text: '空闲', tone: '' })
      }),
      infoRow({ label: '最近一次轮换', value: F.dateLong(NOW - 180 * DAY) }),
      infoRow({ label: '进行中轮换', sub: '进入维护态期间，业务写入冻结（错误码 10）；只读仍可用。' })
    ];
    if (inRotation) {
      body.push(UI.maintBanner({
        tone: 'maintenance', title: 'MK 轮换进行中', sub: '可随时关闭应用，下次启动自动继续',
        actions: [UI.btn({ label: '查看进度', icon: 'loader', title: '查看轮换进度', onClick: function () { navigate('maintenance-rotate'); } })]
      }));
    } else {
      body.push(UI.muted('「发起轮换」需要主密码 + 确认短语；确认后立即进入维护态。'));
    }

    return section('mk', 'MK 轮换', '全库根密钥轮换；四阶段（头部标记 → 数据换钥 → 传播 → 完成）', body, [
      guardBtn({
        label: '发起轮换', icon: 'key', variant: 'danger', title: '发起 MK 轮换（进入维护态）',
        ignoreFrozen: inRotation,
        onClick: openStartRotation
      }),
      UI.btn({ label: '查看进度', icon: 'loader', title: '前往轮换进度页', onClick: function () { navigate('maintenance-rotate'); } })
    ]);
  }

  function openStartRotation() {
    if (isMaintenance()) {
      UI.toast({ tone: 'warn', title: '已在维护态', msg: '同一时刻只允许一个维护任务（错误码 10）' });
      return;
    }
    var pw = UI.input({ type: 'password', placeholder: '输入主密码以确认身份' });
    var phrase = 'ROTATE-MK';
    var input = UI.input({ mono: true, placeholder: phrase });
    var m = UI.modal({
      title: '发起 MK 轮换',
      sub: '确认后立即进入维护态，业务写入冻结',
      tone: 'danger',
      size: 'md',
      body: h('div', { class: 'col gap-3' }, [
        UI.errorBox(10, { text: '确认后本会话进入维护态：导入 / 编辑 / 删除 / 同步全部冻结，数据仍可读。' }),
        UI.field({ label: '主密码', req: true, control: pw }),
        UI.field({ label: '请输入确认短语「' + phrase + '」', req: true, control: input }),
        UI.muted('轮换期间可随时关闭应用；下次启动由恢复日志自动续做（自动续做上限 3 次，第 4 次须显式确认）。')
      ]),
      autofocus: false,
      footer: function () {
        var btn = UI.btn({
          label: '进入维护态并开始', variant: 'danger', disabled: true,
          title: '需要主密码与确认短语同时正确',
          onClick: function () {
            m.close();
            S.set({ maintenance: 'rotation', session: 'Maintenance' });
            UI.toast({ tone: 'maintenance', title: '已进入维护态', msg: 'MK 轮换已开始（原型演示）' });
            navigate('maintenance-rotate');
          }
        });
        function sync() { btn.disabled = !(pw.value && input.value.trim() === phrase); }
        pw.addEventListener('input', sync); input.addEventListener('input', sync);
        return [UI.btn({ label: '取消', title: '取消', onClick: function () { m.close(); } }), btn];
      }
    });
  }

  function sectionFormat() {
    /* 已是 v3 → 整块不渲染（不写「已是最新」） */
    if (L.formatVersion >= 3) return null;
    return section('format', '格式与迁移', '容器头部版本与迁移入口', [
      infoRow({
        label: '当前库格式版本',
        value: 'VSVB v' + F.num(L.formatVersion),
        sub: '库头部版本由引擎报告；迁移到 v3 会把从属密钥移入独立包装区。'
      }),
      infoRow({ label: '可迁移目标版本', value: 'VSVB v' + F.num(D.migrationState.dstVer) }),
      infoRow({ label: '迁移前置检查', sub: '独占写租约 · 非维护态 · 磁盘余量 ≥ 库大小 × 1.2；不通过时返回 9 / 10 / 3 且不进入维护态。' })
    ], [
      guardBtn({
        label: '开始迁移', icon: 'migrate', variant: 'primary', title: '前往格式迁移向导',
        onClick: function () { navigate('maintenance-migrate'); }
      })
    ]);
  }

  function sectionSyncPolicy() {
    var p = D.syncPolicy;
    return section('sync', '同步策略', '策略编辑在「同步」页的弹窗中完成，这里只给摘要', [
      UI.kv([
        { k: '同步模式', v: p.mode === 'auto' ? '自动' : (p.mode === 'manual' ? '手动' : '定时') },
        { k: '时间窗', v: p.timeWindowEnabled ? (p.windowStart + ' – ' + p.windowEnd) : '不限' },
        { k: '下行限速', v: p.bandwidthDownKbps ? F.num(p.bandwidthDownKbps) + ' Kbps' : '不限' },
        { k: '上行限速', v: p.bandwidthUpKbps ? F.num(p.bandwidthUpKbps) + ' Kbps' : '不限' },
        { k: '最大并发', v: F.num(p.maxConcurrent) },
        { k: '冲突策略', v: p.conflictStrategy === 'keep_both' ? '保留双份' : p.conflictStrategy },
        { k: '中继回退', v: p.relayEnabled ? '已启用' : '已关闭' },
        { k: '填充档位', v: p.paddingLevel ? F.num(p.paddingLevel) + ' / 4' : '关闭' },
        { k: '排除规则', v: F.num((p.excludePatterns || []).length) + ' 条' }
      ])
    ], [
      UI.btn({
        label: '编辑', icon: 'edit', title: '在「同步」页编辑策略',
        onClick: function () { navigate('sync'); }
      })
    ]);
  }

  /* 通知矩阵：3 级别 × 4 渠道；danger 行不可关闭；CAP_NOTIFICATIONS 未置位 → 该渠道列不渲染 */
  var NOTIFY_LEVELS = [
    { key: 'info', label: '信息（info）', desc: '同步完成、配对成功等' },
    { key: 'warn', label: '警告（warn）', desc: '链路降级、重试、配额接近上限' },
    { key: 'danger', label: '危险（danger）', desc: '需要恢复、启动恢复未完成、撤销' }
  ];
  var NOTIFY_CHANNELS = [
    { key: 'app', label: '应用内', cap: null, note: '通知中心，始终可用' },
    { key: 'sys', label: '系统通知', cap: 'CAP_NOTIFICATIONS', note: '操作系统级横幅 / 通知中心' },
    { key: 'push', label: '移动推送', cap: 'CAP_NOTIFICATIONS', note: '经自有中继下发到已配对移动端' },
    { key: 'mail', label: '邮件', cap: null, note: '退化为每日摘要邮件' }
  ];

  function sectionNotify() {
    var cols = NOTIFY_CHANNELS.filter(function (c) { return !c.cap || S.cap(c.cap); });
    var hiddenCols = NOTIFY_CHANNELS.filter(function (c) { return c.cap && !S.cap(c.cap); });

    var thead = h('thead', {}, [h('tr', {}, [h('th', {}, '级别')].concat(cols.map(function (c) {
      return h('th', { title: c.note }, c.label);
    })))]);

    var tbody = h('tbody', {}, NOTIFY_LEVELS.map(function (lv) {
      var isDanger = lv.key === 'danger';
      var cells = cols.map(function (c) {
        var checked = !!L.matrix[lv.key][c.key];
        /* danger 行硬约束：不可关闭 */
        var disabled = isDanger;
        var ctl = UI.switchCtl({
          checked: checked, disabled: disabled,
          onChange: function (e) { L.matrix[lv.key][c.key] = e.target.checked; }
        });
        ctl.title = disabled ? '危险级通知不可关闭（安全基线）' : (lv.label + ' · ' + c.label);
        if (ctl.input) ctl.input.title = ctl.title;
        if (ctl.input) ctl.input.setAttribute('aria-label', lv.label + ' · ' + c.label);
        return h('td', {}, [h('div', { class: 'row gap-2' }, [ctl,
          isDanger ? h('span', { class: 't-caption', text: '不可关闭' }) : null
        ])]);
      });
      return h('tr', {}, [h('td', {}, [
        h('div', { class: 't-strong', text: lv.label }),
        h('div', { class: 't-caption', text: lv.desc })
      ])].concat(cells));
    }));

    var body = [h('div', { class: 'table-wrap' }, [h('table', { class: 'data' }, [thead, tbody])])];
    body.push(UI.muted('危险级通知的可达性是安全基线，因此该行开关一律禁用；渠道未置位时整列不渲染（不是置灰）。'));
    if (hiddenCols.length) {
      body.push(h('div', { class: 'row gap-2' }, [
        hiddenMark('capability_bits 未置位 → 该渠道列不渲染'),
        h('span', { class: 't-caption', text: '未渲染的渠道列：' + hiddenCols.map(function (c) { return c.label; }).join('、') })
      ]));
    }

    var dndSwitch = UI.switchCtl({
      checked: L.dndEnabled,
      onChange: function (e) { L.dndEnabled = e.target.checked; start.disabled = !e.target.checked; end.disabled = !e.target.checked; }
    });
    dndSwitch.title = '不打扰时段';
    if (dndSwitch.input) dndSwitch.input.title = '不打扰时段';

    var start = UI.input({ type: 'time', value: L.dndStart });
    var end = UI.input({ type: 'time', value: L.dndEnd });
    start.title = '不打扰开始时间'; end.title = '不打扰结束时间';
    start.disabled = !L.dndEnabled; end.disabled = !L.dndEnabled;
    start.style.maxWidth = '120px'; end.style.maxWidth = '120px';

    body.push(h('div', { class: 'col gap-2', style: { marginTop: '4px' } }, [
      h('div', { class: 'row gap-3' }, [
        dndSwitch,
        start, h('span', { class: 't-caption', text: '至' }), end
      ]),
      UI.muted('不打扰时段内：系统通知与移动推送静默入通知中心；危险级通知仍会投递（不可关闭的渠道不受时段抑制）。')
    ]));

    return section('notify', '通知渠道矩阵', '三级 × 四渠道；渠道能力位未置位时整列不渲染', body);
  }

  function sectionAppearance() {
    var themeSeg = UI.seg({
      value: (D.settings && D.settings.theme) || S.get('theme'),
      items: [
        { value: 'system', label: '跟随系统' },
        { value: 'light', label: '亮' },
        { value: 'dark', label: '暗' }
      ],
      onChange: function (v) {
        if (v === 'light' || v === 'dark') { S.set({ theme: v }); document.documentElement.dataset.theme = v; }
        UI.toast({ tone: 'info', title: '主题已切换', msg: v === 'system' ? '跟随系统（原型演示）' : v });
      }
    });
    themeSeg.title = '主题：跟随系统 / 亮 / 暗';

    var langSel = UI.select({
      value: (D.settings && D.settings.locale) || 'zh-CN',
      options: [{ value: 'zh-CN', label: '简体中文' }, { value: 'en', label: 'English' }],
      onChange: function (v) { S.set({ locale: v }); F.setLocale(v === 'en' ? 'en' : 'zh-CN'); UI.toast({ tone: 'info', title: '语言已切换', msg: v }); }
    });
    langSel.title = '界面语言';

    var densitySeg = UI.seg({
      value: L.density,
      items: [
        { value: 'compact', label: '紧凑' },
        { value: 'comfortable', label: '标准' },
        { value: 'loose', label: '宽松' }
      ],
      onChange: function (v) { L.density = v; }
    });
    densitySeg.title = '信息密度（原型本地状态）';

    /* prefers-reduced-motion 是浏览器真实读数，不是 mock */
    var reduced = false;
    try { reduced = !!(global.matchMedia && global.matchMedia('(prefers-reduced-motion: reduce)').matches); } catch (e) { reduced = false; }

    return section('appearance', '外观与语言', '主题 / 语言 / 动效偏好 / 密度', [
      ctlRow('主题', themeSeg, { sub: '默认跟随系统' }),
      ctlRow('语言', langSel, { sub: '简体中文 / English' }),
      infoRow({
        label: '减弱动效（prefers-reduced-motion）',
        sub: '浏览器媒体查询真实读数：为真时全部入场动画关闭。本端不提供覆盖开关。',
        badge: reduced ? UI.badge({ text: '已开启', tone: 'info' }) : UI.badge({ text: '未开启', tone: '' }),
        mark: UI.tierMark('real')
      }),
      ctlRow('信息密度', densitySeg, { sub: '紧凑 / 标准 / 宽松（原型本地状态）' })
    ]);
  }

  function sectionUpdate() {
    var ch = (D.settings && D.settings.updateChannel) || 'stable';
    var sel = UI.select({
      value: ch,
      options: [
        { value: 'stable', label: 'stable', sub: '仅正式版本，默认' },
        { value: 'beta', label: 'beta', sub: '候选版本，含未完成收口的变更' },
        { value: 'nightly', label: 'nightly', sub: '每日构建，仅供验证' }
      ],
      onChange: function (v) { UI.toast({ tone: 'info', title: '更新通道已切换', msg: v }); }
    });
    sel.title = '更新通道';

    return section('update', '更新通道', '清单验签在引擎侧完成；本端只选择通道', [
      ctlRow('更新通道', sel, { sub: 'stable / beta / nightly' }),
      UI.muted('无论通道为何，更新清单都必须通过签名校验；校验失败时更新整体拒绝（不做部分安装）。')
    ], [
      UI.btn({
        label: '检查更新', icon: 'refresh', title: '立即检查更新',
        onClick: function () { UI.toast({ tone: 'info', title: '正在检查更新', msg: '原型演示：当前通道 ' + ch }); }
      })
    ]);
  }

  function sectionPrivacy() {
    var thumbNote = h('span', { class: 't-caption', text: '缩略图缓存受 LRU 上限约束，且落锁即清（不落盘、不写审计以外的地方）。' });
    var tb = UI.switchCtl({ checked: L.thumbnailsEnabled, onChange: function (e) { L.thumbnailsEnabled = e.target.checked; } });
    tb.title = '缩略图缓存';
    if (tb.input) tb.input.title = '缩略图缓存';

    return section('privacy', '隐私与安全', '截屏保护 / 缩略图 / 诊断上报', [
      switchRow({
        label: '进程级截屏保护', checked: L.screenshotProtect, ariaLabel: '进程级截屏保护',
        sub: '对窗口设置系统级截屏抑制；不保证第三方注入型工具一定失败',
        onChange: function (e) { L.screenshotProtect = e.target.checked; }
      }),
      h('div', { class: 'list-row', style: { cursor: 'default' } }, [
        h('div', { class: 'lr-main' }, [
          h('div', { class: 'lr-title' }, [h('span', { text: '缩略图缓存' })]),
          thumbNote
        ]),
        h('div', { class: 'lr-actions' }, [tb])
      ]),
      switchRow({
        label: '脱敏诊断上报', checked: L.telemetry, ariaLabel: '脱敏诊断上报',
        sub: '默认关闭。开启后仅上报计数与哈希前缀，不含路径、文件名、查询词与密钥材料',
        onChange: function (e) { L.telemetry = e.target.checked; }
      }),
      infoRow({
        label: '脱敏口径',
        sub: '诊断包只含计数与哈希前缀（如 sha256 前 6 位）；不含目标路径原文、文件名原文、查询词。'
      })
    ]);
  }

  function sectionAdvanced(ctx) {
    var a = (D.settings && D.settings.advanced) || {};
    var wi = UI.switchCtl({ checked: L.workIsolate, onChange: function (e) { L.workIsolate = e.target.checked; } });
    wi.title = '工作 Isolate';
    if (wi.input) wi.input.title = '工作 Isolate';

    return section('advanced', '高级 / 诊断', '诊断快照与后台执行参数', [
      h('div', { class: 'list-row', style: { cursor: 'default' } }, [
        h('div', { class: 'lr-main' }, [
          h('div', { class: 'lr-title' }, [h('span', { text: '工作 Isolate' })]),
          h('div', { class: 'lr-sub', text: '把同步 / 校验 / 打包放到独立 Isolate，避免阻塞 UI 线程' })
        ]),
        h('div', { class: 'lr-actions' }, [wi])
      ]),
      infoRow({
        label: '渲染节流',
        value: F.num(a.renderThrottleMs) + ' ms',
        sub: '只读展示项，不提供本地开关'
      }),
      infoRow({
        label: '事件环形缓冲',
        value: F.num(a.eventRingBuffer) + ' 条',
        sub: '只读展示项；溢出丢失计数见诊断页 EVENT_OVERFLOW'
      })
    ], [
      UI.btn({
        label: '打开诊断', icon: 'cpu', variant: 'primary', title: '打开诊断页',
        onClick: function () { goCtx(ctx, 'diagnostics'); }
      })
    ]);
  }

  function sectionAboutSettings(ctx) {
    return section('about', '关于', '版本、构建与开源许可', [
      infoRow({ label: '产品', value: 'VaultSync' }),
      infoRow({ label: '版本', value: APP_VERSION, mark: designMark('原型本地常量：无对应接口读数') }),
      infoRow({ label: '构建号', value: APP_BUILD, mark: designMark('原型本地常量：无对应接口读数') }),
      infoRow({ label: '许可', value: '开源许可见「关于」页' })
    ], [
      UI.btn({ label: '查看开源许可', icon: 'book', title: '前往关于页查看开源许可', onClick: function () { goCtx(ctx, 'about'); } })
    ]);
  }

  function sectionLicenseEntry(ctx) {
    var ls = D.licenseState;
    var meta = LICENSE_STATE[ls.state] || { label: ls.state, tone: '' };
    return section('license', '授权入口', '档位、席位与中继凭据', [
      infoRow({
        label: '当前档位',
        sub: '授权与订阅在独立页面管理；本页不重复商业信息',
        badge: UI.badge({ text: String(ls.tier).toUpperCase(), tone: 'accent' }),
        node: UI.badge({ text: meta.label, tone: meta.tone })
      }),
      infoRow({ label: '席位', value: F.num(ls.seatsUsed) + ' / ' + F.num(ls.seats) }),
      infoRow({ label: '到期时间', value: F.dateLong(ls.expiresMs) })
    ], [
      UI.btn({ label: '管理授权', icon: 'award', variant: 'primary', title: '前往授权与订阅页', onClick: function () { goCtx(ctx, 'license'); } })
    ]);
  }

  /* 侧栏：锚点导航（滚到对应分组卡片） */
  function settingsNav(activeId, onPick) {
    var nav = h('nav', { class: 'settings-nav', 'aria-label': '设置分组导航' });
    SETTINGS_SECTIONS.forEach(function (s) {
      nav.appendChild(h('button', {
        class: 'list-row', type: 'button',
        dataset: { selected: s.id === activeId ? 'true' : 'false' },
        title: s.label,
        'aria-label': s.label,
        onclick: function () { onPick(s.id); }
      }, [
        h('span', { class: 'grow t-truncate', text: s.label })
      ]));
    });
    return nav;
  }

  VS.pages['settings'] = function (ctx) {
    var active = 'lock';
    var sectionsHost = h('div', { class: 'col gap-4' });
    var refs = {};

    var builders = {
      lock: sectionLock,
      bio: sectionBio,
      mpw: sectionMasterPassword,
      disguise: sectionDisguise,
      subkeys: sectionSubKeys,
      mk: sectionMkRotation,
      format: sectionFormat,
      sync: sectionSyncPolicy,
      notify: sectionNotify,
      appearance: sectionAppearance,
      update: sectionUpdate,
      privacy: sectionPrivacy,
      advanced: function () { return sectionAdvanced(ctx); },
      about: function () { return sectionAboutSettings(ctx); },
      license: function () { return sectionLicenseEntry(ctx); }
    };

    SETTINGS_SECTIONS.forEach(function (s) {
      var node = builders[s.id] ? builders[s.id]() : null;
      if (!node) return;   /* 整块不渲染的情形（v3 已是最新、CAP 未置位等） */
      refs[s.id] = node;
      sectionsHost.appendChild(node);
    });

    var nav = settingsNav(active, function (id) {
      active = id;
      Array.prototype.forEach.call(nav.children, function (b, i) {
        b.dataset.selected = SETTINGS_SECTIONS[i].id === id ? 'true' : 'false';
      });
      var target = refs[id];
      if (!target) return;
      try { target.scrollIntoView({ behavior: 'smooth', block: 'start' }); }
      catch (e) { target.scrollIntoView(); }
    });

    return pageView({
      title: '设置',
      sub: '解锁与落锁 · 生物识别 · 密钥 · 通知 · 外观 · 隐私 · 诊断 · 授权',
      actions: [
        frozen() ? UI.badge({ text: '写操作已冻结', tone: 'stale', title: denyReason() }) : null,
        UI.btn({ label: '立即锁定', icon: 'lock', title: '立即锁定保险箱', onClick: function () { S.set({ lock: 'Locked', session: 'Expired' }); UI.toast({ tone: 'info', title: '已锁定', msg: '原型演示' }); } })
      ],
      body: [frozenBar(), h('div', { class: 'two-col' }, [nav, sectionsHost])]
    });
  };

  /* ======================================================================
   * B. recover — 启动恢复向导（D.recoveryReport）
   *     硬约束：未确认前不得写入任何数据
   * ==================================================================== */

  var REC_PATHS = [
    { value: 'rollback', title: '自动回滚', desc: '丢弃未提交的段尾部，回到最近一次已提交快照（推荐：' + D.recoveryReport.suggested + '）' },
    { value: 'replay', title: '重放已提交变更', desc: '按位置日志重放已提交但未落盘的变更；不丢弃数据，耗时更长' },
    { value: 'forensic', title: '导出诊断包（取证）', desc: '只导出计数与哈希前缀，供人工分析；不修改任何库文件' },
    { value: 'continue', title: '显式确认继续', desc: '以当前视图继续使用；未提交变更保持未提交状态，风险自担' }
  ];

  function recDetailRows() {
    var wrap = h('div', { class: 'col gap-1' });
    D.recoveryReport.detail.forEach(function (d) {
      wrap.appendChild(infoRow({
        label: d.label,
        sub: d.value,
        badge: d.ok ? UI.badge({ text: '正常', tone: 'success' }) : UI.badge({ text: '异常', tone: 'danger' })
      }));
    });
    return wrap;
  }

  function exportDiagPackageModal() {
    return UI.confirm({
      title: '导出诊断包（脱敏）',
      sub: '只含计数与哈希前缀',
      tone: 'warn',
      body: h('div', { class: 'col gap-2' }, [
        UI.kv([
          { k: '包含', v: '计数、错误码汇总、哈希前缀（前 6 位）、版本与构建号' },
          { k: '不含', v: '文件路径原文、文件名原文、查询词、密钥材料、明文' },
          { k: '默认上报', v: '关闭（需显式导出）' }
        ]),
        UI.muted('诊断包用于取证与排障；导出本身会写一条 license/settings 之外的 system 类审计（审计失败不阻断业务）。')
      ]),
      confirmLabel: '导出到所选路径',
      confirmVariant: 'primary',
      onConfirm: function () { UI.toast({ tone: 'success', title: '诊断包已导出', msg: '原型演示：未写出真实文件' }); }
    });
  }

  function recStepLabels() {
    return [
      { label: '诊断结论', state: L.recStep > 0 ? 'done' : 'active' },
      { label: '选择路径', state: L.recStep > 1 ? 'done' : (L.recStep === 1 ? 'active' : 'todo') },
      { label: '执行', state: L.recStep > 2 ? 'done' : (L.recStep === 2 ? 'active' : 'todo') },
      { label: '完成或失败', state: L.recStep === 3 ? 'done' : 'todo' }
    ];
  }

  function buildRecover(ctx) {
    var body = [];
    var rp = D.recoveryReport;

    /* 步骤条就地刷新（原型没有路由级重渲染，交互后需要手动同步） */
    var stepsEl = UI.steps({ items: recStepLabels() });
    function syncSteps() {
      var fresh = UI.steps({ items: recStepLabels() });
      var kids = Array.prototype.slice.call(fresh.children || []);
      stepsEl.innerHTML = '';
      kids.forEach(function (k) { stepsEl.appendChild(k); });
    }

    /* ---- crash_loop 触发态 ---- */
    var cl = rp.detail.filter(function (d) { return d.key === 'crash_loop'; })[0];
    var crashLoop = !!(cl && !cl.ok);
    if (crashLoop) {
      body.push(UI.alertbar({
        tone: 'danger', icon: 'danger', title: '检测到崩溃循环保护已触发',
        text: '同一启动周期内多次失去租约并重启。为避免反复写入造成二次损坏，本端不自动选择路径，也不会在未确认的情况下写入。请先保持现场（不要删除任何文件），导出诊断包后再选择路径。'
      }));
    } else if (cl) {
      body.push(UI.alertbar({
        tone: 'info', icon: 'info', title: '崩溃循环保护',
        text: cl.value + '。若该保护触发，本向导会停止自动续做并要求显式确认。'
      }));
    }

    body.push(UI.card({
      title: '恢复向导',
      sub: '四步：诊断 → 选择路径 → 执行 → 完成或失败',
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

    /* ---- ① 诊断结论 ---- */
    body.push(UI.card({
      title: '① 诊断结论',
      sub: '结论来自引擎恢复报告；本端不猜测、不补全',
      body: h('div', { class: 'col gap-3' }, [
        UI.alertbar({
          tone: 'danger', icon: 'alert', title: '无法给出确定视图',
          text: '位置日志与租约不一致时，本端无法确定「哪一份数据是完整的」。因此向导不会静默选择路径，必须由你确认后才可能发生写入。'
        }),
        recDetailRows(),
        UI.kv([
          { k: '审计链头', v: rp.auditHead, mono: true },
          { k: '建议路径', v: rp.suggested === 'rollback' ? '自动回滚' : '重放已提交变更' },
          { k: '快照状态', v: rp.snapshotStatus },
          { k: '租约状态', v: rp.leaseStatus, mono: true }
        ]),
        h('div', { class: 'row gap-2 wrap' }, [
          UI.btn({ label: '导出诊断包（脱敏）', icon: 'export', title: '导出脱敏诊断包（只含计数与哈希前缀）', onClick: exportDiagPackageModal }),
          UI.btn({
            label: '只读方式打开', icon: 'eye', title: '以只读方式打开保险箱（不写入任何数据）',
            onClick: function () {
              S.set({ readOnly: true, readonlyReason: '恢复未完成，按只读降级打开（错误码 11）', route: 'vault' });
              UI.toast({ tone: 'info', title: '已按只读方式打开', msg: '写入操作全部禁用' });
            }
          })
        ])
      ])
    }));

    /* ---- ② 选择路径 ---- */
    var pick = UI.radioCards({
      value: L.recPath, options: REC_PATHS,
      onChange: function (v) { L.recPath = v; if (L.recStep < 1) { L.recStep = 1; syncSteps(); } }
    });
    pick.setAttribute('aria-label', '选择恢复路径');
    body.push(UI.card({
      title: '② 选择路径',
      sub: '四条路径的行为差异是明确的：只有前两条会写入数据',
      body: h('div', { class: 'col gap-3' }, [
        pick,
        UI.muted('「导出诊断包（取证）」与「只读方式打开」不写入任何库文件；「自动回滚」与「重放已提交变更」会写入，且必须显式确认。')
      ])
    }));

    /* ---- ③ 执行 ---- */
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
        text: '路径：' + (REC_PATHS.filter(function (p) { return p.value === L.recPath; })[0] || {}).title
      }));
      execBody.push(UI.bar(L.recProgress, 'maintenance', 'lg'));
      execBody.push(UI.kv([
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
    body.push(UI.card({
      title: '③ 执行',
      sub: '回滚重试上限 2 次；超限改为引导保留现场',
      body: h('div', { class: 'col gap-3' }, execBody),
      actions: [
        UI.btn({
          label: '确认并执行', icon: 'check', variant: 'danger',
          title: '确认恢复路径并开始执行（确认后可能写入）',
          disabled: L.recConfirmed,
          onClick: function () {
            L.recConfirmed = true;
            L.recStep = 2;
            L.recProgress = 0.35;
            syncSteps();
            UI.toast({ tone: 'warn', title: '已确认，开始执行', msg: '路径：' + L.recPath });
          }
        }),
        UI.btn({
          label: '模拟一次失败并重试', icon: 'rotate-ccw', title: '原型演示：累加回滚重试次数',
          onClick: function () {
            L.recAttempts = Math.min(2, L.recAttempts + 1);
            L.recProgress = Math.max(0, L.recProgress - 0.1);
            UI.toast({ tone: 'warn', title: '回滚重试', msg: '已重试 ' + F.num(L.recAttempts) + ' / 2 次' });
          }
        })
      ]
    }));

    /* ---- ④ 完成或失败 ---- */
    var finish = L.recAttempts >= 2
      ? UI.alertbar({ tone: 'danger', icon: 'danger', title: '恢复失败：已保留现场', text: '未完成恢复。现场未被清理，诊断包可用于人工取证。' })
      : UI.alertbar({ tone: 'info', icon: 'info', title: '等待执行', text: '确认并执行后在此显示完成或失败结论。' });
    body.push(UI.card({
      title: '④ 完成或失败',
      body: h('div', { class: 'col gap-3' }, [
        finish,
        UI.kv([
          { k: '完成态', v: L.recConfirmed ? (L.recAttempts >= 2 ? '失败（保留现场）' : '可继续') : '未开始' },
          { k: '写入次数', v: L.recConfirmed ? F.num(1) : '—' }
        ])
      ]),
      actions: [
        UI.btn({
          label: '完成并打开保险箱', icon: 'vault', variant: 'primary',
          title: '完成恢复并打开保险箱',
          disabled: !L.recConfirmed || L.recAttempts >= 2,
          onClick: function () {
            S.set({ readOnly: false, readonlyReason: '', maintenance: null, session: 'Active' });
            UI.toast({ tone: 'success', title: '恢复完成', msg: '原型演示' });
            goCtx(ctx, 'vault');
          }
        })
      ]
    }));

    return pageView({
      title: '恢复向导',
      sub: '启动恢复：未确认前不得写入；无法给出确定视图时如实说明',
      actions: [
        UI.btn({ label: '导出诊断包（脱敏）', icon: 'export', title: '导出脱敏诊断包', onClick: exportDiagPackageModal }),
        UI.btn({
          label: '只读方式打开', icon: 'eye', title: '只读打开（不写入）',
          onClick: function () {
            S.set({ readOnly: true, readonlyReason: '恢复未完成，按只读降级打开（错误码 11）' });
            UI.toast({ tone: 'info', title: '已按只读方式打开' });
          }
        })
      ],
      body: body
    });
  }

  VS.pages['recover'] = function (ctx) { return buildRecover(ctx); };

  /* ======================================================================
   * C. maintenance-rotate — MK 轮换进度页（D.rotationState）
   * ==================================================================== */

  var ROT_PHASES = [
    { key: 'mark', label: '头部标记' },
    { key: 'rekey', label: '数据换钥' },
    { key: 'propagate', label: '传播' },
    { key: 'done', label: '完成' }
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
  /* 放弃/回滚的状态档位：1 = mark|rekey（可回滚），2 = propagate（只能完成传播），3 = done */
  function rotStateN(phase) {
    if (phase === 'propagate') return 2;
    if (phase === 'done') return 3;
    return 1;
  }

  function rotPropagationTable() {
    var rows = D.rotationState.propagation || [];
    if (!rows.length) return UI.empty({ icon: 'devices', title: '暂无传播记录', desc: '尚未产生任何设备传播条目。' });
    return h('div', { class: 'table-wrap' }, [
      h('table', { class: 'data' }, [
        h('thead', {}, [h('tr', {}, [
          h('th', {}, '设备'), h('th', {}, '状态'), h('th', {}, '说明')
        ])]),
        h('tbody', {}, rows.map(function (p) {
          var meta = PROP_STATE[p.state] || { label: p.state, tone: '' };
          /* 离线设备的说明也强制改写为「待补推」，避免任何「完成」暗示 */
          var note = p.state === 'offline' ? '离线（待下次上线补推）' : p.note;
          return h('tr', {}, [
            h('td', { text: p.device }),
            h('td', {}, [UI.badge({ text: meta.label, tone: meta.tone })]),
            h('td', { class: 't-caption', text: note })
          ]);
        }))
      ])
    ]);
  }

  function openGiveUpRotation() {
    var phase = D.rotationState.phase;
    var n = rotStateN(phase);
    var canRollback = n === 1 && L.rotHasOldMk;
    var rollbackReason = n !== 1
      ? '当前处于传播阶段（state=2），头部标记已覆盖旧 MK，无法用旧 MK 回滚；只能完成传播'
      : (!L.rotHasOldMk ? '本端未持有旧 MK：无法解密旧头部，回滚不可用' : null);

    var pick = UI.radioCards({
      value: n === 1 ? 'rollback' : 'finish',
      options: [
        { value: 'rollback', title: '用旧 MK 回滚', desc: canRollback ? '回到轮换前的头部与包装状态' : (rollbackReason || '不可用') },
        { value: 'finish', title: '只完成传播', desc: '不回滚，把当前 MK 传播给全部设备后结束' },
        { value: 'pause', title: '暂停并保留现状', desc: '暂停轮换；下次启动按自动续做策略继续' }
      ]
    });
    pick.setAttribute('aria-label', '放弃轮换的处理方式');

    var m = UI.modal({
      title: '放弃 / 暂停轮换',
      sub: n === 1 ? 'state=1（头部标记 / 数据换钥）：可回滚' : 'state=2（传播）：只能完成传播',
      tone: 'danger',
      size: 'lg',
      body: h('div', { class: 'col gap-3' }, [
        UI.errorBox(10, {
          text: n === 1
            ? '当前处于换钥阶段，旧 MK 与旧头部仍在，允许回滚。回滚是唯一会把已换钥的从属密钥还原的操作。'
            : '当前处于传播阶段：新头部已生效，旧 MK 无法再解出正确视图，因此不提供回滚。'
        }),
        pick,
        UI.muted('无论选择哪一项，本端都不会删除库文件；「用旧 MK 回滚」会重写头部与从属密钥包装区。')
      ]),
      footer: function () {
        return [
          UI.btn({ label: '取消', title: '取消', onClick: function () { m.close(); } }),
          UI.btn({
            label: '确认', variant: 'danger', title: '确认放弃 / 暂停',
            onClick: function () {
              m.close();
              S.set({ maintenance: null, session: 'Active' });
              UI.toast({ tone: 'warn', title: '轮换已中止', msg: '原型演示：状态已回到空闲' });
            }
          })
        ];
      }
    });
  }

  function openAutoResumeConfirm() {
    return UI.confirm({
      title: '自动续做已达上限（3 次）',
      sub: '第 4 次续做必须显式确认',
      tone: 'danger',
      body: h('div', { class: 'col gap-2' }, [
        UI.kv([
          { k: '已自动续做', v: F.num(L.rotAutoResumeCount) + ' 次' },
          { k: '上限', v: '3 次' },
          { k: '轮换 ID', v: D.rotationState.rotationId, mono: true }
        ]),
        UI.muted('反复自动续做通常意味着轮换过程本身在重复失败。继续之前请确认你已查看进度与传播状态；确认后本次续做会再尝试一次，并重新计数。')
      ]),
      confirmLabel: '我确认继续第 4 次',
      confirmVariant: 'danger',
      onConfirm: function () {
        L.rotAutoResumeCount = 1;
        UI.toast({ tone: 'warn', title: '已确认续做', msg: '计数已重置为 1' });
      }
    });
  }

  function buildRotate(ctx) {
    var rs = D.rotationState;
    var idx = rotPhaseIndex(rs.phase);
    var pct = F.pctOf(rs.progress.done, rs.progress.total);
    var n = rotStateN(rs.phase);

    /* 必现文案 */
    var headline = 'MK 轮换进行中 ' + F.pct(pct) + '%，可随时关闭应用，下次启动自动继续';

    var steps = UI.steps({
      items: ROT_PHASES.map(function (p, i) {
        return { label: p.label, state: i < idx ? 'done' : (i === idx ? 'active' : 'todo') };
      })
    });

    var body = [];

    /* 并发轮换冲突态 */
    if (L.rotConflict) {
      body.push(UI.alertbar({
        tone: 'danger', icon: 'danger', title: '检测到并发轮换冲突',
        text: '另一实例已发起轮换，本端的轮换已作废。需要重新发起轮换。'
      }));
    }

    body.push(UI.card({
      title: '轮换进度',
      sub: '轮换 ID ' + rs.rotationId + ' · 开始于 ' + F.dateLong(rs.startedMs) + '（' + F.relative(rs.startedMs) + '）',
      body: h('div', { class: 'col gap-3' }, [
        UI.maintBanner({
          tone: 'maintenance', icon: 'refresh', title: headline,
          sub: '维护态期间业务写入冻结（错误码 10），只读与导出仍可用',
          progress: pct
        }),
        steps,
        rs.interrupted
          ? UI.alertbar({ tone: 'warn', icon: 'alert', title: '上次启动被中断', text: '由恢复日志自动续做。' })
          : null
      ])
    }));

    /* ② 数据换钥的进度明细 */
    body.push(UI.card({
      title: '② 数据换钥',
      body: h('div', { class: 'col gap-2' }, [
        UI.bar(pct, 'maintenance', 'lg'),
        h('div', { class: 'row-between' }, [
          h('div', { class: 't-num', text: F.num(rs.progress.done) + ' / ' + F.num(rs.progress.total) }),
          h('div', { class: 't-caption', text: '文件 ' + F.num(rs.filesDone) + ' · 文件夹 ' + F.num(rs.foldersDone) })
        ]),
        UI.muted('换钥只重包装 FSKey 与从属密钥，不重加密文件体；因此中断后可在任意原子单元处续做。')
      ])
    }));

    /* ③ 传播 */
    body.push(UI.card({
      title: '③ 传播',
      sub: '对端离线时显示「待补推」，补推在下次上线后自动完成',
      body: h('div', { class: 'col gap-3' }, [
        rotPropagationTable(),
        UI.muted('传播完成不等于全部设备已确认：只有收到对端 ack 才标记为已确认。')
      ])
    }));

    /* 自动续做策略 */
    body.push(UI.card({
      title: '自动续做',
      sub: '自动续做上限 3 次，第 4 次须显式确认',
      body: h('div', { class: 'col gap-3' }, [
        UI.kv([
          { k: '已自动续做', v: F.num(L.rotAutoResumeCount) + ' 次' },
          { k: '上限', v: '3 次' },
          { k: '超过上限', v: L.rotAutoResumeCount >= 3 ? '需显式确认' : '尚未达到' }
        ]),
        L.rotAutoResumeCount >= 3
          ? UI.alertbar({
              tone: 'warn', icon: 'alert', title: '已达自动续做上限',
              text: '下次续做需要你显式确认（第 4 次）。反复失败时应先查看传播状态与诊断包。'
            })
          : null
      ]),
      actions: [
        UI.btn({
          label: '继续续做', icon: 'play', title: '继续自动续做',
          onClick: function () {
            if (L.rotAutoResumeCount >= 3) openAutoResumeConfirm();
            else { L.rotAutoResumeCount += 1; UI.toast({ tone: 'info', title: '已续做', msg: '第 ' + F.num(L.rotAutoResumeCount) + ' 次' }); }
          }
        })
      ]
    }));

    /* 「用旧 MK 回滚」仅当 state=1 且本端持旧 MK；否则禁用 + reason */
    var canRollback = (n === 1 && L.rotHasOldMk);
    var rollbackReason = canRollback
      ? '用旧 MK 回滚到轮换前状态'
      : (n !== 1
          ? '不可用：仅 state=1（头部标记 / 数据换钥阶段）可回滚，当前为传播或完成阶段'
          : '不可用：本端未持有旧 MK，无法解密旧头部');

    var actions = [
      UI.btn({
        label: '暂停', icon: 'pause', title: '暂停轮换（保留现状，可续做）',
        onClick: function () {
          UI.toast({ tone: 'info', title: '轮换已暂停', msg: '下次启动可按自动续做策略继续' });
        }
      }),
      UI.btn({
        label: '放弃', icon: 'cancel', variant: 'danger', title: '放弃轮换（进入处理选项）',
        onClick: openGiveUpRotation
      }),
      UI.btn({
        label: '用旧 MK 回滚', icon: 'rotate-ccw', variant: 'danger',
        disabled: !canRollback, title: rollbackReason,
        onClick: function () {
          UI.confirmPhrase({
            title: '用旧 MK 回滚', tone: 'danger', phrase: 'ROLLBACK',
            body: UI.errorBox(10, { text: '回滚会还原头部与从属密钥包装区；已传播到对端的新头部需要重新传播。' }),
            confirmLabel: '执行回滚',
            onConfirm: function () { UI.toast({ tone: 'warn', title: '已用旧 MK 回滚', msg: '原型演示' }); }
          });
        }
      })
    ];

    body.push(UI.card({
      title: '处理选项',
      sub: 'state=1 给出回滚选项；state=2 只能完成传播',
      body: h('div', { class: 'col gap-2' }, [
        UI.kv([
          { k: '当前阶段', v: (ROT_PHASES[idx] || {}).label },
          { k: '状态档位', v: 'state=' + F.num(n) },
          { k: '旧 MK', v: L.rotHasOldMk ? '本端持有' : '已销毁' }
        ]),
        UI.muted('「用旧 MK 回滚」仅在 state=1 且本端确实持有旧 MK 时可用，其余情况禁用并给出原因。')
      ]),
      actions: actions
    }));

    /* 原型状态注入（仅用于演示并发冲突与旧 MK 丢失两种不可由 mock 数据表达的态） */
    var inject = h('div', { class: 'col gap-2' }, [
      h('div', { class: 'row gap-3 wrap' }, [
        (function () {
          var sw = UI.switchCtl({ checked: L.rotConflict, onChange: function (e) { L.rotConflict = e.target.checked; } });
          sw.title = '原型演示：模拟并发轮换冲突';
          if (sw.input) sw.input.title = sw.title;
          return h('div', { class: 'row gap-2' }, [sw, h('span', { class: 't-caption', text: '模拟并发轮换冲突（原型注入）' })]);
        })(),
        (function () {
          var sw2 = UI.switchCtl({ checked: L.rotHasOldMk, onChange: function (e) { L.rotHasOldMk = e.target.checked; } });
          sw2.title = '原型演示：本端是否持有旧 MK';
          if (sw2.input) sw2.input.title = sw2.title;
          return h('div', { class: 'row gap-2' }, [sw2, h('span', { class: 't-caption', text: '本端持有旧 MK（原型注入）' })]);
        })()
      ]),
      UI.muted('以上两项在原型数据源中没有对应字段，仅用于演示状态；刷新页面即恢复默认。')
    ]);
    body.push(UI.card({ title: '原型状态注入', sub: '无接口可读的状态，显式标注为原型注入', body: inject }));

    return pageView({
      title: 'MK 轮换进度',
      sub: headline,
      actions: [
        UI.badge({ text: '维护态', tone: 'maintenance', title: '维护态：业务写入冻结' }),
        UI.btn({ label: '导出诊断包（脱敏）', icon: 'export', title: '导出脱敏诊断包', onClick: exportDiagPackageModal }),
        UI.btn({ label: '返回设置', icon: 'settings', title: '返回设置页', onClick: function () { goCtx(ctx, 'settings'); } })
      ],
      body: body
    });
  }

  VS.pages['maintenance-rotate'] = function (ctx) { return buildRotate(ctx); };

  /* ======================================================================
   * D. maintenance-migrate — 格式迁移向导（D.migrationState）
   * ==================================================================== */

  var MIG_STEPS = [
    { key: 'pre', label: 'M4 前置' },
    { key: 'container', label: 'M1 容器' },
    { key: 'index', label: 'M2 索引' },
    { key: 'audit', label: 'M3 审计' },
    { key: 'head', label: 'M4 头部抬升' },
    { key: 'cleanup', label: '清理' }
  ];
  var MIG_FILE_STATUS = {
    done: { label: '完成', tone: 'success' },
    failed: { label: '失败', tone: 'danger' },
    skipped: { label: '跳过', tone: 'warning' },
    pending: { label: '待处理', tone: '' }
  };

  function migPhaseIndex(phase) {
    if (phase === 'scanning') return 1;
    if (phase === 'converting') return 2;
    if (phase === 'verifying') return 4;
    if (phase === 'done') return 5;
    return 0;
  }

  function migPrecheckCard() {
    var list = D.migrationState.precheck || [];
    var allOk = list.every(function (p) { return p.ok; });
    var rows = list.map(function (p) {
      return infoRow({
        label: p.label,
        sub: p.note,
        badge: p.ok ? UI.badge({ text: '通过', tone: 'success' }) : UI.badge({ text: '未通过', tone: 'danger' })
      });
    });

    var body = [h('div', { class: 'col', style: { gap: '0' } }, rows)];
    body.push(allOk
      ? UI.alertbar({ tone: 'info', icon: 'check-circle', title: '前置检查全部通过', text: '可以开始迁移。迁移期间进入维护态，业务写入冻结（错误码 10）。' })
      : UI.alertbar({
          tone: 'danger', icon: 'danger', title: '前置检查未通过',
          text: '不满足条件时「开始迁移」禁用：引擎会返回 9（租约被占）/ 10（维护态）/ 3（磁盘不可写），并且不会进入维护态。'
        }));

    return UI.card({
      title: '前置检查',
      sub: '独占写租约 · 非维护态 · 磁盘余量 ≥ 库大小 × 1.2',
      body: body,
      actions: [
        /* 前置检查不通过 → 「开始」必须禁用，并说明会返回 9 / 10 / 3 且不进入维护态 */
        allOk
          ? guardBtn({
              label: '开始迁移', icon: 'migrate', variant: 'primary',
              title: '开始格式迁移（进入维护态）',
              onClick: function () {
                L.migRunning = true;
                UI.toast({ tone: 'maintenance', title: '迁移已开始', msg: '原型演示：进入维护态' });
              }
            })
          : UI.btn({
              label: '开始迁移', icon: 'migrate', variant: 'primary', disabled: true,
              title: '前置检查未通过：会返回 9（租约被占）/ 10（维护态）/ 3（磁盘不可写），且不进入维护态'
            }),
        UI.btn({
          label: '取消', icon: 'cancel', title: '取消迁移',
          onClick: function () {
            UI.confirm({
              title: '取消迁移', tone: 'warn',
              body: h('div', { class: 'col gap-2' }, [
                UI.errorBox(12, { text: '取消会停在当前原子单元（错误码 12 + 末帧标记）。取消后库是「部分新、部分旧」的状态，但完全可用。' }),
                UI.kv([
                  { k: '当前阶段', v: D.migrationState.phase },
                  { k: '已完成文件', v: F.num(D.migrationState.doneFiles) + ' / ' + F.num(D.migrationState.totalFiles) },
                  { k: '当前文件', v: D.migrationState.currentFileId, mono: true }
                ])
              ]),
              confirmLabel: '停在当前原子单元',
              onCancel: function () {},
              onConfirm: function () {
                L.migRunning = false;
                UI.toast({ tone: 'info', title: '已取消迁移', msg: '停在第 12 号原子单元的末帧标记处（原型演示）' });
              }
            });
          }
        }),
        UI.btn({
          label: '从断点续做', icon: 'play', title: '从上次原子单元边界续做',
          onClick: function () { L.migRunning = true; UI.toast({ tone: 'info', title: '已从断点续做', msg: '原型演示' }); }
        })
      ]
    });
  }

  function migFilesCard() {
    var files = D.migrationState.files || [];
    if (!files.length) return UI.card({ title: '逐文件进度', body: UI.empty({ icon: 'list', title: '暂无迁移文件记录' }) });

    var thead = h('thead', {}, [h('tr', {}, [
      h('th', {}, '文件'),
      h('th', { style: { width: '110px' } }, 'src → dst'),
      h('th', { style: { width: '120px' } }, 'sha256 前缀'),
      h('th', { style: { width: '96px' } }, '状态'),
      h('th', { style: { width: '170px' } }, '时间'),
      h('th', {}, '备注')
    ])]);

    var tbody = h('tbody', {}, files.map(function (f) {
      var st = MIG_FILE_STATUS[f.status] || { label: f.status, tone: '' };
      return h('tr', {}, [
        h('td', {}, [
          h('div', { class: 't-strong t-truncate', text: f.name }),
          h('div', { class: 't-caption t-mono', text: f.fileId })
        ]),
        h('td', { class: 't-num', text: F.num(f.srcVer) + ' → ' + F.num(f.dstVer) }),
        h('td', { class: 't-mono', text: String(f.sha256).slice(0, 12) }),
        h('td', {}, [UI.badge({ text: st.label, tone: st.tone })]),
        h('td', { class: 't-caption' }, [
          h('div', { text: F.dateLong(f.tsMs) }),
          h('div', { text: F.relative(f.tsMs) })
        ]),
        h('td', { class: 't-caption', text: f.note || '—' })
      ]);
    }));

    var failed = files.filter(function (f) { return f.status === 'failed'; }).length;
    var body = [h('div', { class: 'table-wrap', style: { maxHeight: '420px' } }, [h('table', { class: 'data' }, [thead, tbody])])];
    body.push(UI.alertbar({
      tone: failed ? 'warn' : 'info', icon: failed ? 'alert' : 'info',
      title: failed ? '存在单文件失败' : '无单文件失败',
      text: '单文件失败只影响该行：前后 SHA-256 不一致 → 已删除新文件，旧副本仍在，其余文件继续处理，不会导致整库失败。'
    }));
    body.push(UI.muted('逐行显示，不合并为单一总进度；每行的新旧版本、摘要前缀、时间与备注均可单独核对。'));

    return UI.card({
      title: '逐文件进度',
      sub: '共 ' + F.num(files.length) + ' 条 · 完成 ' + F.num(files.filter(function (f) { return f.status === 'done'; }).length) +
           ' · 失败 ' + F.num(failed),
      body: body
    });
  }

  function buildMigrate(ctx) {
    var ms = D.migrationState;
    var idx = migPhaseIndex(ms.phase);
    var pct = F.pctOf(ms.doneFiles, ms.totalFiles);
    var allOk = (ms.precheck || []).every(function (p) { return p.ok; });

    var body = [];
    body.push(UI.card({
      title: '迁移概览',
      sub: 'VSVB v' + F.num(ms.srcVer) + ' → v' + F.num(ms.dstVer) + ' · 当前阶段 ' + ms.phase,
      body: h('div', { class: 'col gap-3' }, [
        UI.steps({
          items: MIG_STEPS.map(function (p, i) { return { label: p.label, state: i < idx ? 'done' : (i === idx ? 'active' : 'todo') }; })
        }),
        UI.bar(pct, 'maintenance', 'lg'),
        h('div', { class: 'row-between' }, [
          h('div', { class: 't-num', text: F.num(ms.doneFiles) + ' / ' + F.num(ms.totalFiles) }),
          h('div', { class: 't-caption', text: '当前文件 ' + ms.currentFileId })
        ]),
        UI.muted('总进度只是汇总；真实依据是下面的逐文件表。取消时停在当前原子单元（码 12 + 末帧），库部分新部分旧但完全可用。')
      ])
    }));

    body.push(migPrecheckCard());
    body.push(migFilesCard());

    /* 磁盘不足 / 半成品语义 */
    body.push(UI.card({
      title: '失败语义',
      sub: '磁盘不足与单文件失败的处置不同',
      body: h('div', { class: 'col gap-2' }, [
        infoRow({
          label: '磁盘不足（码 3）',
          sub: '保留 .mig.tmp 供直接重试；不产生半成品（不会留下部分写入的目标文件）。',
          badge: UI.badge({ text: '可重试', tone: 'warning' })
        }),
        infoRow({
          label: '单文件校验失败',
          sub: '前后 SHA-256 不一致 → 已删除新文件，旧副本仍在；该行标记 failed，其余文件继续。',
          badge: UI.badge({ text: '不整库失败', tone: 'info' })
        }),
        infoRow({
          label: '会话取消（码 12）',
          sub: '停在当前原子单元末帧；库为「部分新、部分旧」但完全可用，可断点续做。',
          badge: UI.badge({ text: '已取消', tone: 'info' })
        })
      ])
    }));

    /* 完成与备份策略 */
    body.push(UI.card({
      title: '完成与备份',
      sub: '备份 vault.vsb.pre-v3 保留 7 天；M4 后不回滚',
      body: h('div', { class: 'col gap-2' }, [
        infoRow({ label: '备份文件', value: 'vault.vsb.pre-v3', mono: true }),
        infoRow({ label: '备份保留期', value: '7 天' }),
        UI.alertbar({
          tone: 'danger', icon: 'danger', title: 'M4 后不回滚',
          text: '头部抬升（M4）完成后，旧格式的读取路径不再维护，因此不提供回滚到 v' + F.num(ms.srcVer) + ' 的自动路径。备份仅用于人工取证与导出。'
        }),
        UI.muted('「立即删除备份」需要显式确认；删除后不可恢复。')
      ]),
      actions: [
        UI.btn({
          label: '立即删除备份', icon: 'trash', variant: 'danger', title: '立即删除 vault.vsb.pre-v3 备份',
          onClick: function () {
            UI.confirmPhrase({
              title: '立即删除备份', tone: 'danger', phrase: 'DELETE-BACKUP',
              body: UI.errorBox(6, { text: '删除后无法回退到 v' + F.num(ms.srcVer) + ' 视图；如需保留取证材料请先导出。' }),
              confirmLabel: '删除备份',
              onConfirm: function () { UI.toast({ tone: 'warn', title: '备份已删除', msg: '原型演示' }); }
            });
          }
        })
      ]
    }));

    return pageView({
      title: '格式迁移向导',
      sub: 'VSVB v' + F.num(ms.srcVer) + ' → v' + F.num(ms.dstVer) + ' · 逐文件可核对 · 不进入维护态的前置条件已列明',
      actions: [
        allOk ? UI.badge({ text: '前置检查通过', tone: 'success' })
              : UI.badge({ text: '前置检查未通过', tone: 'danger' }),
        UI.btn({ label: '导出诊断包（脱敏）', icon: 'export', title: '导出脱敏诊断包', onClick: exportDiagPackageModal }),
        UI.btn({ label: '返回设置', icon: 'settings', title: '返回设置页', onClick: function () { goCtx(ctx, 'settings'); } })
      ],
      body: body
    });
  }

  VS.pages['maintenance-migrate'] = function (ctx) { return buildMigrate(ctx); };

  /* ======================================================================
   * E. license — 授权与订阅（D.licenseState / D.plans / D.orderEnvelopes）
   *     本页不存在商业订单 / 下单 / 支付 / 结算 / 退款 / 发票 / 试用期
   * ==================================================================== */

  var LICENSE_STATE = {
    active: { label: '生效中', tone: 'success' },
    grace: { label: '宽限期', tone: 'warning' },
    expired: { label: '已到期', tone: 'danger' },
    revoked: { label: '已撤销', tone: 'danger' },
    offline_grace: { label: '离线宽限', tone: 'warning' }
  };
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

  /* 能力门演示：8 个 feat_* */
  var FEAT_KEYS = [
    'feat_multi_device', 'feat_collab', 'feat_relay_quota', 'feat_managed_ops',
    'feat_burn_share', 'feat_stego', 'feat_audit_export', 'feat_sso'
  ];
  var FEAT_LABEL = {
    feat_multi_device: '多设备配对',
    feat_collab: '协作空间',
    feat_relay_quota: '中继配额',
    feat_managed_ops: '托管运维',
    feat_burn_share: '阅后即焚分享',
    feat_stego: '隐写载荷',
    feat_audit_export: '审计链加密导出',
    feat_sso: 'SSO 接入'
  };

  function maskToken(t) {
    t = String(t || '');
    if (t.length <= 12) return t;
    return t.slice(0, 6) + '••••••' + t.slice(t.length - 4);
  }

  function licenseStateCard() {
    var ls = D.licenseState;
    var meta = LICENSE_STATE[ls.state] || { label: ls.state, tone: '' };
    var tokenMeta = RELAY_TOKEN_STATE[ls.relayTokenState];
    return UI.card({
      title: '当前授权状态',
      sub: '档位 / 状态 / 到期 / 席位 / 离线宽限',
      body: h('div', { class: 'col gap-3' }, [
        UI.kv([
          { k: '档位', v: String(ls.tier).toUpperCase() },
          { k: '状态', node: UI.badge({ text: meta.label, tone: meta.tone }) },
          { k: '激活时间', v: F.dateLong(ls.activatedMs) },
          { k: '到期时间', v: F.dateLong(ls.expiresMs) },
          { k: '席位', v: F.num(ls.seatsUsed) + ' / ' + F.num(ls.seats) },
          { k: '离线宽限剩余', v: F.num(ls.offlineDaysLeft) + ' 天' }
        ]),
        tokenMeta
          ? UI.alertbar({ tone: tokenMeta.tone === 'success' ? 'info' : tokenMeta.tone, icon: 'info', title: '中继凭据状态：' + tokenMeta.label, text: '凭据轮换不影响同步：到期后自动回落 Free 档，同步不被阻断。' })
          : UI.alertbar({
              tone: 'stale', icon: 'ban', title: '中继凭据状态不可读',
              text: '接口返回的原值为「' + String(ls.relayTokenState) + '」，不是 valid / expiring / expired / revoked 之一。本端不猜测状态，按未知显示。'
            })
      ])
    });
  }

  function planGrid() {
    var cards = D.plans.map(function (p) {
      var isCurrent = p.id === D.licenseState.tier;
      var feats = h('ul', { class: 'plan-feats' }, (p.feats || []).map(function (f) {
        return h('li', { dataset: { off: f.on ? 'false' : 'true' } }, [
          UI.icon(f.on ? 'check' : 'x', 13),
          h('span', { text: f.t })
        ]);
      }));
      return h('div', {
        class: 'plan-card',
        dataset: { featured: p.featured ? 'true' : 'false', current: isCurrent ? 'true' : 'false' }
      }, [
        p.featured ? h('span', { class: 'plan-tag', text: '推荐' }) : null,
        h('div', { class: 't-h2', text: p.name }),
        h('div', { class: 'plan-price' }, [
          h('span', { text: p.priceLabel }),
          h('span', { class: 'plan-per', text: ' ' + p.per })
        ]),
        /* 价格是可配置占位，来自 D.plans[i].priceLabel，本文件不硬编码任何价格 */
        UI.muted('价格字段来自配置占位：' + p.priceLabel + ' / ' + p.per),
        feats,
        isCurrent
          ? UI.btn({ label: '当前档位', disabled: true, title: '当前已处于该档位', block: true })
          : UI.btn({
              label: '选择此档', variant: p.featured ? 'primary' : undefined, block: true,
              title: '激活 ' + p.name + ' 许可证',
              onClick: function () { openLicenseActivation(p); }
            })
      ]);
    });
    return h('div', { class: 'plan-grid' }, cards);
  }

  function openLicenseActivation(plan) {
    var step = 0;
    var keyInput = UI.input({ mono: true, placeholder: 'VS-XXXX-XXXX-XXXX-XXXX' });
    var host = h('div', { class: 'col gap-3' });

    function stepsView() {
      return UI.steps({
        items: D.licenseSteps.map(function (label, i) {
          return { label: label, state: i < step ? 'done' : (i === step ? 'active' : 'todo') };
        })
      });
    }

    function renderBody() {
      host.innerHTML = '';
      host.appendChild(stepsView());
      var ls = D.licenseState;

      if (step === 0) {
        host.appendChild(UI.field({ label: '许可证密钥', req: true, control: keyInput, hint: '等宽输入框：密钥按原样粘贴，不做自动大小写转换' }));
        host.appendChild(UI.muted('密钥在本地校验签名与有效期，不联网下单、不涉及任何支付流程。'));
      } else if (step === 1) {
        host.appendChild(UI.bar(1, 'accent', 'sm'));
        host.appendChild(UI.kv([
          { k: '签名校验', v: '已通过（原型演示）' },
          { k: '有效期', v: '校验中' },
          { k: '签发者', v: 'VaultSync 授权服务' }
        ]));
      } else if (step === 2) {
        host.appendChild(UI.kv([
          { k: '设备指纹', v: D.fingerprint(), mono: true },
          { k: '绑定方式', v: '写入授权记录，随许可证一同保存' },
          { k: '解绑', v: '在此页面显式解绑后可将席位让给其他设备' }
        ]));
      } else if (step === 3) {
        host.appendChild(UI.kv([
          { k: '写入位置', v: '本机授权记录（不写保险箱数据区）' },
          { k: '审计', v: 'license.activate' }
        ]));
      } else if (step === 4) {
        host.appendChild(UI.kv([
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
          text: '档位已写入授权记录；中继凭据与配额随即生效。不设试用期，不做功能降级埋点。'
        }));
      }
    }
    renderBody();

    var m = UI.modal({
      title: '激活许可证' + (plan ? '：' + plan.name : ''),
      sub: '六步流程 · 不做任何支付 / 结算 / 发票',
      size: 'lg',
      body: host,
      autofocus: false,
      footer: function () {
        return [
          UI.btn({ label: '关闭', title: '关闭', onClick: function () { m.close(); } }),
          step > 0 ? UI.btn({
            label: '上一步', title: '上一步',
            onClick: function () { step -= 1; renderBody(); }
          }) : null,
          UI.btn({
            label: step >= D.licenseSteps.length - 1 ? '完成' : '下一步',
            variant: 'primary',
            title: step >= D.licenseSteps.length - 1 ? '完成激活' : '下一步',
            disabled: step === 0 && !keyInput.value,
            onClick: function () {
              if (step >= D.licenseSteps.length - 1) { m.close(); UI.toast({ tone: 'success', title: '激活完成', msg: '原型演示' }); return; }
              step += 1;
              if (step === 1) L.relayExchange = 'requesting';
              if (step === 4) L.relayExchange = 'granted';
              renderBody();
            }
          })
        ];
      }
    });
    keyInput.addEventListener('input', function () {
      var btn = m.el.querySelector('.modal-foot .btn-primary');
      if (btn && step === 0) btn.disabled = !keyInput.value;
    });
  }

  function relayCard() {
    var ls = D.licenseState;
    var tokenMeta = RELAY_TOKEN_STATE[ls.relayTokenState];
    var daysLeft = F.num(Math.round((ls.relayTokenExpiresMs - NOW) / DAY));
    return UI.card({
      title: '中继凭据',
      sub: 'token 有效期 12 个月 · 到期前 30 天提示 · 到期自动回落 Free 档且不阻断同步',
      body: h('div', { class: 'col gap-3' }, [
        UI.kv([
          { k: 'Token', node: h('div', { class: 'row gap-2' }, [
            h('span', { class: 't-mono', text: maskToken(ls.relayToken), title: '脱敏显示' }),
            UI.iconBtn({
              icon: 'copy', size: 'sm', label: '复制 Token',
              onClick: function () { UI.copy(ls.relayToken); }
            })
          ]) },
          { k: '状态', node: tokenMeta ? UI.badge({ text: tokenMeta.label, tone: tokenMeta.tone })
                                       : UI.badge({ text: '未登记：' + String(ls.relayTokenState), tone: '', title: '接口原值不是 valid / expiring / expired / revoked' }) },
          { k: '到期时间', v: F.dateLong(ls.relayTokenExpiresMs) },
          { k: '剩余', v: daysLeft + ' 天' },
          { k: '配额', v: F.bytes(ls.relayQuotaUsedBytes) + ' / ' + F.bytes(ls.relayQuotaTotalBytes) },
          { k: '兑换状态', v: EXCHANGE_STATE[L.relayExchange] || String(L.relayExchange) }
        ]),
        UI.bar(F.pctOf(ls.relayQuotaUsedBytes, ls.relayQuotaTotalBytes), 'accent', 'sm'),
        UI.muted('中继只转发密文，不落盘、不解密。凭据到期后：本端自动回落 Free 档的中继配额，本地加密与 P2P 直连同步不受影响。')
      ]),
      actions: [
        guardBtn({
          label: '轮换 Token', icon: 'refresh', title: '轮换中继 Token',
          onClick: function () {
            UI.confirm({
              title: '轮换中继 Token', tone: 'warn',
              body: h('div', { class: 'col gap-2' }, [
                h('div', { class: 't-body', text: '轮换后旧 Token 立即失效；正在进行的中继传输会在下一个会话重新协商。' }),
                UI.muted('本地直连同步不需要凭据，因此轮换期间同步不会被阻断。')
              ]),
              confirmLabel: '立即轮换',
              onConfirm: function () { UI.toast({ tone: 'info', title: 'Token 已轮换', msg: '原型演示' }); }
            });
          }
        })
      ]
    });
  }

  function envelopeCard() {
    var rows = D.orderEnvelopes || [];
    var body = [];
    body.push(UI.alertbar({
      tone: 'warn', icon: 'alert', title: '这些是跨设备指令信封，不是商业订单',
      text: 'kind 只有三种：1 销毁指令 / 2 锁定指令 / 3 轮换传播。本项目没有下单、支付、结算、退款、发票与试用期，因此不存在商业订单列表。'
    }));
    if (!rows.length) {
      body.push(UI.empty({ icon: 'queue', title: '暂无指令信封', desc: '没有待投递或已确认的跨设备指令。' }));
    } else {
      body.push(h('div', { class: 'table-wrap' }, [
        h('table', { class: 'data' }, [
          h('thead', {}, [h('tr', {}, [
            h('th', {}, 'ID'), h('th', { style: { width: '130px' } }, 'kind'),
            h('th', {}, '目标设备'), h('th', { style: { width: '110px' } }, '状态'),
            h('th', { style: { width: '180px' } }, '创建时间'), h('th', { style: { width: '180px' } }, '失效时间')
          ])]),
          h('tbody', {}, rows.map(function (e) {
            var st = ENVELOPE_STATUS[e.status] || { label: e.status, tone: '' };
            var expired = e.expiresMs < NOW;
            return h('tr', {}, [
              h('td', { class: 't-mono', text: e.id }),
              h('td', { text: F.num(e.kind) + ' · ' + e.kindLabel }),
              h('td', { class: 't-caption', text: (e.targets || []).join('、') || '—' }),
              h('td', {}, [UI.badge({ text: st.label, tone: st.tone })]),
              h('td', { class: 't-caption', text: F.dateLong(e.createdMs) }),
              h('td', { class: 't-caption' }, [
                h('span', { text: F.dateLong(e.expiresMs) }),
                expired ? UI.badge({ text: '已失效', tone: 'stale' }) : null
              ])
            ]);
          }))
        ])
      ]));
    }
    return UI.card({ title: '跨设备指令信封', sub: 'D.orderEnvelopes · 与商业订单无关', body: body });
  }

  function freeForeverCard() {
    var free = D.plans[0];
    var feats = (free.feats || []).filter(function (f) { return f.on; });
    return UI.card({
      title: 'Free-forever',
      sub: 'Free 档永久免费包含的能力（取自 D.plans[0].feats）',
      body: h('div', { class: 'col gap-2' }, [
        h('ul', { class: 'plan-feats' }, feats.map(function (f) {
          return h('li', { dataset: { off: 'false' } }, [UI.icon('check', 13), h('span', { text: f.t })]);
        })),
        UI.alertbar({
          tone: 'info', icon: 'info', title: '不设试用期，不做功能降级埋点',
          text: 'Free 档能力不会随使用时长衰减，也不会因为未升级而在本地加密 / 同步 / 密钥管理上降级。'
        })
      ])
    });
  }

  function featGateCard() {
    var feats = D.licenseState.features || {};
    var rows = FEAT_KEYS.map(function (k) {
      var on = !!feats[k];
      return h('tr', {}, [
        h('td', { class: 't-mono', text: k }),
        h('td', { text: FEAT_LABEL[k] || '—' }),
        h('td', {}, [UI.badge({ text: on ? '已开启' : '未开启', tone: on ? 'success' : '' })]),
        h('td', { class: 't-caption', text: on ? '当前档位可用' : '升级后可用' })
      ]);
    });
    return UI.card({
      title: '能力门（feat_*）',
      sub: '开关状态用徽标表达，不使用置灰按钮',
      body: h('div', { class: 'col gap-3' }, [
        h('div', { class: 'table-wrap' }, [
          h('table', { class: 'data' }, [
            h('thead', {}, [h('tr', {}, [
              h('th', {}, '能力键'), h('th', {}, '能力'), h('th', { style: { width: '110px' } }, '当前档位'), h('th', {}, '说明')
            ])]),
            h('tbody', {}, rows)
          ])
        ]),
        UI.muted('能力门只影响档位特性；本地加密、保险箱读写与 P2P 直连同步不属于能力门范围。')
      ])
    });
  }

  function degradationCard() {
    return UI.card({
      title: '降级提示',
      sub: '档位限制 vs 非商业冲突：两类原因必须分开表达',
      body: h('div', { class: 'col gap-2' }, [
        infoRow({
          label: '档位限制导致的能力缺失',
          sub: '显示为「升级后可用」，不做功能降级埋点，也不在本地能力上打折。',
          badge: UI.badge({ text: '升级后可用', tone: 'accent' })
        }),
        h('div', { class: 'col gap-1' }, [
          h('div', { class: 't-strong', text: '非商业冲突（技术状态）' }),
          h('div', { class: 't-caption', text: '以下错误码是引擎 / 中继的技术状态，不是商业限制，不得展示为「需要升级」：' }),
          h('div', { class: 'row gap-2 wrap' }, TECH_CODES.map(function (c) {
            return UI.badge({ text: F.num(c) + ' · ' + VS.errTitle(c), tone: VS.tone(c) === 'danger' ? 'danger' : (VS.tone(c) === 'stale' ? 'stale' : 'info'), title: (VS.ERR[c] || {}).hint || '' });
          }))
        ])
      ])
    });
  }

  function buildLicense(ctx) {
    var body = [
      licenseStateCard(),
      UI.card({ title: '套餐对比', sub: '价格为可配置占位；本页不硬编码任何金额', body: planGrid() }),
      relayCard(),
      envelopeCard(),
      freeForeverCard(),
      featGateCard(),
      degradationCard()
    ];
    return pageView({
      title: '授权与订阅',
      sub: '档位 / 席位 / 中继凭据 / 跨设备指令信封 · 无商业订单、无支付、无试用期',
      actions: [
        UI.btn({
          label: '激活许可证', icon: 'award', variant: 'primary', title: '打开许可证激活流程',
          onClick: function () { openLicenseActivation(null); }
        }),
        UI.btn({ label: '返回设置', icon: 'settings', title: '返回设置页', onClick: function () { goCtx(ctx, 'settings'); } })
      ],
      body: body
    });
  }

  VS.pages['license'] = function (ctx) { return buildLicense(ctx); };

  /* ======================================================================
   * F. about / diagnostics
   * ==================================================================== */

  var APP_VERSION = '2.0.0';              /* 原型本地常量：无对应接口读数 */
  var APP_BUILD = '2026.09.15-prototype'; /* 原型本地常量：无对应接口读数 */

  var OSS_LICENSES = [
    { name: 'Flutter', license: 'BSD-3-Clause', use: '桌面客户端 UI 框架' },
    { name: 'Rust 标准库与 crates.io 依赖', license: 'MIT / Apache-2.0（逐 crate 见 LICENSES）', use: '核心引擎与 FFI 门面' },
    { name: 'snow', license: 'Apache-2.0 / MIT', use: 'Noise 信道（设备配对）' },
    { name: 'ed25519-dalek', license: 'BSD-3-Clause', use: '签名与清单验签' },
    { name: 'argon2', license: 'MIT / Apache-2.0', use: 'Argon2id KDF' },
    { name: 'aes-gcm', license: 'MIT / Apache-2.0', use: 'AES-256-GCM AEAD' }
  ];

  /* 能力三档清单：能力 / 档位 / 来源接口 —— 答不出接口就标 design 或不渲染 */
  function capabilityRows() {
    var rows = Object.keys(VS.CAP).map(function (k) {
      var meta = VS.CAP[k];
      var tier = S.capTier(k);
      return { name: meta.name, key: k, tier: tier, src: 'capability_bits bit ' + F.num(meta.bit) };
    });
    /* 非能力位、但需要在同一张表里如实登记的项目 */
    rows.push({ name: '生物识别', key: 'CAP_BIO（未登记）', tier: 'design', src: '未在 VS.CAP 中登记 → 无可判定接口' });
    rows.push({ name: '异地检测', key: '—', tier: 'hidden', src: '架构上无此接口 → 不渲染' });
    rows.push({ name: '内存哨兵', key: '—', tier: 'design', src: (D.securityChecks.filter(function (c) { return c.key === 'sentinel'; })[0] || {}).src || '（无导出）' });
    rows.push({ name: '恢复日志', key: 'CAP_RECOVERY', tier: S.capTier('CAP_RECOVERY'), src: 'ENGINE_READY(1).recoveryReport' });
    rows.push({ name: '应用版本', key: '—', tier: 'design', src: '原型本地常量，无接口读数' });
    return rows;
  }

  function buildAbout(ctx) {
    var rows = capabilityRows().map(function (r) {
      return h('tr', {}, [
        h('td', { text: r.name }),
        h('td', { class: 't-mono t-caption', text: r.key }),
        h('td', {}, [UI.tierMark(r.tier)]),
        h('td', { class: 't-caption', text: r.src })
      ]);
    });

    return pageView({
      title: '关于',
      sub: '版本 / 许可 / 差异化定位 / 证据入口 / 能力三档清单',
      actions: [
        UI.btn({ label: '打开诊断', icon: 'cpu', title: '打开诊断页', onClick: function () { goCtx(ctx, 'diagnostics'); } }),
        UI.btn({ label: '返回设置', icon: 'settings', title: '返回设置页', onClick: function () { goCtx(ctx, 'settings'); } })
      ],
      body: [
        UI.card({
          title: 'VaultSync',
          sub: '端到端加密的多设备文件同步系统',
          body: UI.kv([
            { k: '产品名', v: 'VaultSync' },
            { k: '版本', node: h('div', { class: 'row gap-2' }, [h('span', { class: 't-mono', text: APP_VERSION }), designMark('原型本地常量')]) },
            { k: '构建号', node: h('div', { class: 'row gap-2' }, [h('span', { class: 't-mono', text: APP_BUILD }), designMark('原型本地常量')]) },
            { k: '许可', v: '见下方开源许可列表' },
            { k: '设计基线', v: 'docs/v2.0（现行）/ docs/v1.0（归档）' }
          ])
        }),
        UI.card({
          title: '差异化定位',
          sub: '端到端加密 + 自有中继 + 隐写 + 审计链 + 多端一致',
          body: h('ul', { class: 'plan-feats' }, [
            { t: '端到端加密：明文与密钥只存在于本地核心引擎，中继仅转发密文' },
            { t: '自有中继：可自建、可私有部署，中继不落盘、不解密' },
            { t: '隐写术：载荷为 AEAD 密文，载体只搬位不做密码学；不可检测性无安全保证' },
            { t: '审计链：链式哈希日志，导出加密；审计写入失败不阻断业务' },
            { t: '多端一致：MK 不变、per-file FSKey、CDC 分块使局部修改不触发全文件重加密' }
          ].map(function (x) {
            return h('li', { dataset: { off: 'false' } }, [UI.icon('check', 13), h('span', { text: x.t })]);
          }))
        }),
        UI.card({
          title: '开源许可',
          body: h('div', { class: 'table-wrap' }, [
            h('table', { class: 'data' }, [
              h('thead', {}, [h('tr', {}, [h('th', {}, '组件'), h('th', {}, '许可'), h('th', {}, '用途')])]),
              h('tbody', {}, OSS_LICENSES.map(function (l) {
                return h('tr', {}, [
                  h('td', { text: l.name }),
                  h('td', { class: 't-caption', text: l.license }),
                  h('td', { class: 't-caption', text: l.use })
                ]);
              }))
            ])
          ])
        }),
        UI.card({
          title: '证据入口',
          sub: '四层：可核实来源 / 未核实项目名 / 未覆盖 / 免责',
          body: h('div', { class: 'col gap-2' }, [
            infoRow({ label: '① 可核实来源', sub: '仓库内的设计文档、任务面板、FFI 导出清单与冒烟测试断言；每条对外声称都应能指到具体文件与接口名。', badge: UI.badge({ text: '可核实', tone: 'success' }) }),
            infoRow({ label: '② 未核实项目名不得作为竞品证据', sub: '未经核实的外部项目名、版本号与测试结论一律不作为对比证据使用；本端不展示任何未核实的竞品结论。', badge: UI.badge({ text: '不作为证据', tone: 'warning' }) }),
            infoRow({ label: '③ 未覆盖', sub: '尚无运行时接口的能力按「设计保证」标注；例如内存哨兵与异地检测在本端没有可读接口。', badge: UI.badge({ text: '标注 design', tone: 'stale' }) }),
            infoRow({ label: '④ 免责', sub: '原型为纯静态 Mock：全部数据为前端假数据，不含真实加密逻辑，不构成安全性承诺。', badge: UI.badge({ text: '非承诺', tone: '' }) })
          ])
        }),
        UI.card({
          title: '能力三档清单',
          sub: '逐项：能力 / 档位 / 来源接口 —— 答不出接口即标注或不渲染',
          body: h('div', { class: 'col gap-2' }, [
            h('div', { class: 'table-wrap', style: { maxHeight: '460px' } }, [
              h('table', { class: 'data' }, [
                h('thead', {}, [h('tr', {}, [
                  h('th', {}, '能力'), h('th', {}, '能力位 / 键'), h('th', { style: { width: '150px' } }, '档位'), h('th', {}, '来源接口')
                ])]),
                h('tbody', {}, rows)
              ])
            ]),
            UI.muted('三档含义：真值 = 有运行时接口读数；设计保证 = 契约存在但无导出，如实标注；不渲染 = 架构上无此接口，整块不出现。')
          ])
        }),
        UI.card({
          title: '致谢',
          body: h('div', { class: 'col gap-2' }, [
            h('div', { class: 't-body', text: '感谢 Flutter、Rust 生态与各开源依赖的作者；完整许可文本随发行包提供。' }),
            UI.muted('原型语料来自 docs/v2.0 各模块设计文档；术语以 docs/README.md 的统一术语表为准。')
          ])
        })
      ]
    });
  }

  VS.pages['about'] = function (ctx) { return buildAbout(ctx); };

  /* ---- 诊断：快照 / ERROR_DIAG 汇总 / 事件流 ---- */

  function errorDiagSummary() {
    /* 从审计日志的脱敏 detail.result 汇总：这是本原型中唯一可读的错误码来源 */
    var counts = {};
    (D.auditLog || []).forEach(function (a) {
      var r = a.detail && a.detail.result;
      if (r === null || r === undefined || r === 0) return;
      if (!counts[r]) counts[r] = { code: r, count: 0, lastMs: a.tsMs };
      counts[r].count += 1;
      if (a.tsMs > counts[r].lastMs) counts[r].lastMs = a.tsMs;
    });
    return Object.keys(counts).map(function (k) { return counts[k]; }).sort(function (a, b) { return b.count - a.count; });
  }

  function diagnosticSnapshot() {
    var src = 'vault_core_diagnostics()（原型：本地假数据快照，非真实读数）';
    return {
      src: src,
      compactionPending: null,       /* 原型数据源无此字段 → 显示为未知 */
      recovery: D.recoveryReport,
      eventSeq: (D.auditLog[0] || {}).seq,   /* 审计链序，非事件流 seq */
      overflowLost: null             /* 原型数据源无此字段 → 显示为未知 */
    };
  }

  function buildDiagnostics(ctx) {
    var snap = diagnosticSnapshot();
    var errs = errorDiagSummary();

    var snapshotText = [
      '{',
      '  "source": "vault_core_diagnostics()",',
      '  "mock": true,',
      '  "compactionPending": null,        // 原型数据源无此字段',
      '  "segmentStatus": "' + D.recoveryReport.segmentStatus + '",',
      '  "snapshotStatus": "' + D.recoveryReport.snapshotStatus + '",',
      '  "leaseStatus": "' + D.recoveryReport.leaseStatus + '",',
      '  "uncommittedChanges": ' + D.recoveryReport.uncommittedChanges + ',',
      '  "auditHeadPrefix": "' + String(D.recoveryReport.auditHead).slice(0, 12) + '…",',
      '  "auditVerified": ' + (D.auditVerified ? 'true' : 'false') + ',',
      '  "eventRingBuffer": ' + D.settings.advanced.eventRingBuffer + ',',
      '  "renderThrottleMs": ' + D.settings.advanced.renderThrottleMs,
      '}'
    ].join('\n');

    return pageView({
      title: '诊断',
      sub: 'vault_core_diagnostics 快照 · 恢复结论 · ERROR_DIAG 汇总 · 事件流',
      actions: [
        UI.btn({ label: '导出诊断包（脱敏）', icon: 'export', variant: 'primary', title: '导出脱敏诊断包（只含计数与哈希前缀）', onClick: exportDiagPackageModal }),
        UI.btn({ label: '返回设置', icon: 'settings', title: '返回设置页', onClick: function () { goCtx(ctx, 'settings'); } })
      ],
      body: [
        UI.card({
          title: '诊断快照',
          sub: snap.src,
          body: h('div', { class: 'col gap-3' }, [
            UI.kv([
              { k: 'compactionPending', node: h('div', { class: 'row gap-2' }, [
                h('span', { class: 't-muted', text: '—' }), designMark('原型数据源没有该字段，按未知显示（不显示 0）')
              ]) },
              { k: '恢复结论', v: '未提交变更 ' + F.num(snap.recovery.uncommittedChanges) + ' 处 · 段 ' + snap.recovery.segmentStatus + ' · 租约 ' + snap.recovery.leaseStatus },
              { k: '审计链头', v: String(snap.recovery.auditHead).slice(0, 16) + '…', mono: true },
              { k: '审计链校验', node: D.auditVerified ? UI.badge({ text: '通过', tone: 'success' }) : UI.badge({ text: '未通过', tone: 'danger' }) },
              { k: '事件环形缓冲', v: F.num(D.settings.advanced.eventRingBuffer) + ' 条' },
              { k: '渲染节流', v: F.num(D.settings.advanced.renderThrottleMs) + ' ms' }
            ]),
            UI.codeBlock(snapshotText),
            UI.muted('快照为原型本地假数据；真实实现中每个字段都应能指回某个导出函数的返回结构。')
          ])
        }),
        UI.card({
          title: 'ERROR_DIAG 汇总',
          sub: '按错误码聚合（来源：脱敏审计 detail.result 的计数）',
          body: errs.length
            ? h('div', { class: 'table-wrap' }, [
                h('table', { class: 'data' }, [
                  h('thead', {}, [h('tr', {}, [
                    h('th', { style: { width: '90px' } }, '错误码'), h('th', {}, '含义'),
                    h('th', { style: { width: '110px' } }, '计数'), h('th', { style: { width: '200px' } }, '最近一次')
                  ])]),
                  h('tbody', {}, errs.map(function (e) {
                    var tone = VS.tone(e.code) === 'danger' ? 'danger' : (VS.tone(e.code) === 'stale' ? 'stale' : 'info');
                    return h('tr', {}, [
                      h('td', { class: 'num t-num', text: F.num(e.code) }),
                      h('td', {}, [UI.badge({ text: VS.errTitle(e.code), tone: tone, title: (VS.ERR[e.code] || {}).hint || '' })]),
                      h('td', { class: 'num', text: F.num(e.count) }),
                      h('td', { class: 't-caption', text: F.dateLong(e.lastMs) + ' · ' + F.relative(e.lastMs) })
                    ]);
                  }))
                ])
              ])
            : UI.empty({ icon: 'list', title: '无错误码记录', desc: '审计日志中没有非零 result；按「无数据一律灰」原则不显示 0，也不显示绿灯。' })
        }),
        UI.card({
          title: '事件流',
          sub: 'seq 与环形缓冲溢出计数',
          body: h('div', { class: 'col gap-3' }, [
            UI.kv([
              { k: '审计链 seq（最近）', v: snap.eventSeq ? F.num(snap.eventSeq) : '—', mono: true },
              { k: '事件流 seq', node: h('div', { class: 'row gap-2' }, [h('span', { class: 't-muted', text: '—' }), designMark('原型数据源没有独立的事件流 seq')]) },
              { k: 'EVENT_OVERFLOW 丢失', node: h('div', { class: 'row gap-2' }, [h('span', { class: 't-muted', text: '—' }), designMark('原型数据源没有溢出丢失计数')]) },
              { k: '环形缓冲容量', v: F.num(D.settings.advanced.eventRingBuffer) + ' 条' }
            ]),
            UI.alertbar({
              tone: 'stale', icon: 'ban', title: '诚实纪律',
              text: '凡在本端答不出「读的是哪个接口」的读数，一律按未知显示并标注设计保证，不显示 0 冒充正常，也不整块编造。'
            })
          ])
        }),
        UI.card({
          title: '恢复结论',
          sub: '与恢复向导同源（D.recoveryReport）',
          body: h('div', { class: 'col gap-2' }, recDetailRows()),
          actions: [
            UI.btn({ label: '打开恢复向导', icon: 'recover', title: '前往恢复向导', onClick: function () { goCtx(ctx, 'recover'); } })
          ]
        })
      ]
    });
  }

  VS.pages['diagnostics'] = function (ctx) { return buildDiagnostics(ctx); };

  /* ======================================================================
   * G. 注册动作（供命令面板 / 菜单调用）
   * ==================================================================== */
  VS.actions['system.openSettings'] = function () { navigate('settings'); };
  VS.actions['system.openDiagnostics'] = function () { navigate('diagnostics'); };
  VS.actions['system.openAbout'] = function () { navigate('about'); };
  VS.actions['system.manageLicense'] = function () { navigate('license'); };
  VS.actions['system.openRecover'] = function () { navigate('recover'); };
  VS.actions['system.exportDiagnostics'] = function () { exportDiagPackageModal(); };
  VS.actions['system.checkUpdate'] = function () {
    UI.toast({ tone: 'info', title: '正在检查更新', msg: '通道：' + ((D.settings && D.settings.updateChannel) || 'stable') });
  };

})(window);
