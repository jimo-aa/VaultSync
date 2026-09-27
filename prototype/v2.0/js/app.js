/* ============================================================================
 * VaultSync V2.0 原型 — 应用外壳 / 路由 / 锁屏 / 通知 / 命令面板 / 演示控制台
 * ==========================================================================*/
(function (global) {
  'use strict';

  var VS = global.VS;
  var h = VS.util.h, UI = VS.ui, F = VS.fmt, D = VS.data, S = VS.store;
  var U = VS.util;

  VS.pages = VS.pages || {};
  VS.actions = VS.actions || {};

  var root = document.getElementById('root');
  var currentPage = null;
  var unlockTimer = null;

  /* ======================================================================
   * 0. 导航模型（docs/v2.0/03 §2.1）
   * ==================================================================== */
  var NAV = [
    { section: '空间', items: [
      { id: 'vault',   label: '保险箱', icon: 'vault' },
      { id: 'devices', label: '设备',   icon: 'devices' },
      { id: 'sync',    label: '同步',   icon: 'sync', badge: function () {
          var q = S.get('queue').filter(function (t) { return t.state === 'running' || t.state === 'queued'; }).length;
          return q ? { text: String(q), tone: '' } : null; } }
    ]},
    { section: '控制', items: [
      { id: 'security', label: '安全中心', icon: 'security', badge: function () {
          var n = S.get('unread'); return n ? { text: String(n), tone: 'danger' } : null; } },
      { id: 'stego',    label: '隐写术',   icon: 'stego', requires: 'stego' },
      { id: 'settings', label: '设置',     icon: 'settings' }
    ]},
    { section: '运维', items: [
      { id: 'queue',    label: '队列',     icon: 'queue' },
      { id: 'license',  label: '授权与订阅', icon: 'award' },
      { id: 'diagnostics', label: '诊断',  icon: 'gauge' },
      { id: 'about',    label: '关于',     icon: 'info' }
    ]}
  ];

  var ROUTE_TITLES = {
    vault: '保险箱', devices: '设备', sync: '同步', queue: '传输队列',
    security: '安全中心', stego: '隐写术', settings: '设置',
    'license': '授权与订阅', 'recover': '恢复向导',
    'maintenance-rotate': 'MK 轮换', 'maintenance-migrate': '格式迁移',
    'about': '关于', 'diagnostics': '诊断', 'alarm': '安全告警'
  };

  /* ======================================================================
   * 1. 断点档位（§2.2）
   * ==================================================================== */
  function computeBp() {
    var w = window.innerWidth;
    if (w < 720) return 'phone';
    if (w < 1280) return 'tablet';
    if (w > 1820) return 'wide';
    return 'desktop';
  }
  function syncBp() {
    var bp = computeBp();
    if (S.get('bp') !== bp) {
      document.documentElement.dataset.bp = bp;
      S.set({ bp: bp }, true);
    }
  }

  /* ======================================================================
   * 2. 主题 / 语言
   * ==================================================================== */
  function applyTheme() {
    document.documentElement.dataset.theme = S.get('theme') === 'system'
      ? (window.matchMedia && window.matchMedia('(prefers-color-scheme: light)').matches ? 'light' : 'dark')
      : S.get('theme');
    F.setLocale(S.get('locale'));
  }

  /* ======================================================================
   * 3. 锁屏（§4.1）
   * ==================================================================== */
  var DEMO_PW = '1234';
  var DEMO_DUMMY = '88888888';

  function strengthOf(pw) {
    var score = 0;
    if (pw.length >= 8) score++;
    if (pw.length >= 12) score++;
    if (/[a-z]/.test(pw) && /[A-Z]/.test(pw)) score++;
    if (/\d/.test(pw)) score++;
    if (/[^A-Za-z0-9]/.test(pw)) score++;
    var levels = [
      { pct: 0.16, label: '很弱', hint: '易被离线爆破' },
      { pct: 0.34, label: '弱',   hint: '建议增加长度与字符种类' },
      { pct: 0.55, label: '一般', hint: '可通过（Argon2id 派生 KEK）' },
      { pct: 0.78, label: '强',   hint: '通过（Argon2id 64 MiB · t=3 · p=2）' },
      { pct: 1.00, label: '很强', hint: '通过（Argon2id 64 MiB · t=3 · p=2）' }
    ];
    return levels[U.clamp(score, 0, 4)];
  }

  function renderLock() {
    var dialState = 'idle';
    var lockEl = h('div', { class: 'lock', role: 'main', 'aria-label': '保险箱解锁' });
    lockEl.appendChild(h('div', { class: 'lock-grid' }));

    /* --- 转盘锁（签名元素） --- */
    var dial = h('div', { class: 'dial', dataset: { state: dialState }, 'aria-hidden': 'true' });
    dial.innerHTML = '<svg viewBox="0 0 160 160">' +
      '<circle class="dial-ring-a" cx="80" cy="80" r="72" fill="none" stroke="var(--c-border)" stroke-width="1.5" stroke-dasharray="3 7"/>' +
      '<circle class="dial-ring-b" cx="80" cy="80" r="60" fill="none" stroke="var(--c-accent-line)" stroke-width="1.5" stroke-dasharray="26 12"/>' +
      '<circle cx="80" cy="80" r="46" fill="none" stroke="var(--c-border)" stroke-width="1"/>' +
      '<circle class="dial-core" cx="80" cy="80" r="9" fill="var(--c-text-2)"/>' +
      '<path d="M80 34v10M80 116v10M34 80h10M116 80h10" stroke="var(--c-text-2)" stroke-width="1.5" stroke-linecap="round"/>' +
      '</svg>';

    /* --- 输入区 --- */
    var input = UI.input({ type: 'password', placeholder: '主密码', id: 'lock-pw', size: 'lg' });
    input.classList.add('pw-input', 'input-mono');
    var strengthBar = UI.bar(0);
    var strengthText = h('div', { class: 'strength-text', text: '输入后显示强度评估' });

    var unlockBtn = UI.btn({ label: '解锁', variant: 'accent', onClick: doUnlock, attrs: { 'aria-label': '解锁保险箱' } });
    var bioBtn = UI.btn({ icon: 'fingerprint', label: '生物识别', onClick: function () {
      if (!S.get('bioBound')) { showErr(5); return; }
      UI.toast({ tone: 'info', title: 'Windows Hello', msg: '已通过平台安全区校验（原型模拟）' });
      setTimeout(function () { grant('Unlocked'); }, 400);
    }});
    /* 能力位 CAP_BIO 未置位 → 生物识别入口整体隐藏（不报错、不置灰） */
    if (!S.get('biometricAvailable') || S.get('caps').CAP_BIO === false) bioBtn.classList.add('hidden');
    var manageBtn = UI.btn({ label: '绑定 / 管理', onClick: function () { bioManageModal(); } });

    var errBar = h('div', { class: 'col gap-2', style: { width: '100%' } });

    input.addEventListener('input', function () {
      var st = strengthOf(input.value);
      strengthText.textContent = input.value ? (st.label + ' · ' + st.hint) : '输入后显示强度评估';
      strengthBar.firstChild.style.width = (input.value ? Math.round(st.pct * 100) : 0) + '%';
    });
    input.addEventListener('keydown', function (e) { if (e.key === 'Enter') doUnlock(); });

    function showErr(code, extra) {
      var e = VS.ERR[code];
      errBar.innerHTML = '';
      errBar.appendChild(UI.alertbar({
        tone: e.tone === 'stale' ? 'info' : e.tone,
        title: '错误码 ' + code + ' · ' + e.title,
        text: extra || e.hint
      }));
      dial.dataset.state = 'err';
      setTimeout(function () { dial.dataset.state = 'idle'; }, 600);
    }

    function doUnlock() {
      var now = Date.now();
      if (now < S.get('cooldownUntil')) {
        showErr(2, '冷却中，请等待倒计时结束');
        return;
      }
      var pw = input.value;
      if (!pw) { input.focus(); return; }
      if (pw === DEMO_PW) {
        dial.dataset.state = 'ok';
        UI.toast({ tone: 'success', title: '解锁成功', msg: 'MK 已解封，进入保险箱（Mock）' });
        setTimeout(function () { grant('Unlocked'); }, 320);
      } else if (pw === DEMO_DUMMY) {
        dial.dataset.state = 'ok';
        UI.toast({ tone: 'info', title: '已进入', msg: '伪空间：与真空间信息架构逐一对应' });
        setTimeout(function () { grant('Dummy'); }, 320);
      } else {
        var n = S.get('unlockAttempts') + 1;
        S.set({ unlockAttempts: n }, true);
        if (n >= 3) {
          S.set({ cooldownUntil: now + 30000 }, true);
          showErr(2, '连续失败 3 次，已触发暴力破解保护（30s 延时）');
          UI.toast({ tone: 'warn', title: '暴力破解保护已触发', msg: '冷却 30 秒后重试（错误码 2）' });
        } else {
          showErr(1, '第 ' + n + ' 次失败，再失败 ' + (3 - n) + ' 次将触发冷却');
          UI.toast({ tone: 'danger', title: '主密码错误', msg: '错误码 1 · 第 ' + n + ' 次' });
        }
      }
    }

    lockEl.appendChild(h('div', { class: 'lock-card' }, [
      dial,
      h('div', { class: 'col gap-1', style: { alignItems: 'center' } }, [
        h('div', { class: 'lock-brand', text: 'VaultSync' }),
        h('div', { class: 'lock-tagline', text: 'YOUR DATA · EVERYWHERE · ENCRYPTED' })
      ]),
      errBar,
      h('div', { class: 'pw-row' }, [input, unlockBtn]),
      h('div', { class: 'strength' }, [strengthBar, strengthText]),
      h('div', { class: 'lock-actions' }, [bioBtn, manageBtn]),
      h('div', { class: 'lock-links' }, [
        h('button', { type: 'button', text: '忘记主密码？', onclick: forgotModal }),
        S.get('recovery') === null ? null : h('button', {
          type: 'button', style: { color: 'var(--c-danger)' },
          text: '⚠ 启动恢复未完成 → 进入恢复向导', onclick: function () { enterShell(); go('recover'); }
        })
      ])
    ]));

    lockEl.appendChild(h('div', { class: 'lock-foot' }, [
      h('span', { text: '演示口令：1234 = 真实保险箱 · 88888888 = 伪空间' }),
      h('span', { text: '纯静态原型 · 全部为 Mock 数据' })
    ]));
    return lockEl;
  }

  function bioManageModal() {
    var bound = S.get('bioBound');
    var m = UI.modal({
      title: '生物识别', sub: 'Windows Hello / 平台安全区', size: 'sm',
      body: [
        UI.errorBox(4, { text: '能力位 CAP_BIO 未置位时，本入口整体隐藏（不报错、不置灰）' }),
        h('div', { class: 'mt-3' }, [UI.kv([
          { k: '绑定状态', v: bound ? '已绑定' : '未绑定 · 可绑定' },
          { k: '平台', v: 'Windows Hello（平台安全区）' },
          { k: '用途', v: '作为第二把 KEK 解封 MK；MK 本身永不变更' },
          { k: '失败语义', v: '码 5 = 安全区失效 → 引导重绑；码 4 = 安全存储不可用 → 隐藏入口' }
        ])])
      ],
      footer: [
        UI.btn({ label: '关闭', onClick: function () { m.close(); } }),
        UI.btn({ label: bound ? '重新绑定' : '立即绑定', variant: 'primary', onClick: function () {
          S.set({ bioBound: !bound }, true); m.close();
          UI.toast({ tone: 'success', title: bound ? '已重新绑定' : '绑定成功', msg: '原型模拟：未真正写入平台安全区' });
        }})
      ]
    });
  }

  function forgotModal() {
    var m = UI.modal({
      title: '忘记主密码', size: 'sm', tone: 'maintenance',
      body: [
        h('p', { class: 't-body', text: 'MK 由 CSPRNG 生成且永不变更；主密码只是解封 MK 的 KEK 之一。没有主密码或生物识别，任何人都无法解封 MK —— 这是设计红线，不存在后门。' }),
        h('div', { class: 'mt-3' }, [UI.alertbar({
          tone: 'warn', title: '可选路径',
          text: '① 用已绑定的生物识别解封；② 用另一台已配对设备导出恢复票据（需双方指纹核验）；③ 若两把 KEK 全部丢失，只能执行安全重置 —— 数据不可恢复。'
        })])
      ],
      footer: [UI.btn({ label: '我知道了', variant: 'primary', onClick: function () { m.close(); } })]
    });
  }

  /* ======================================================================
   * 4. 进入主壳层
   * ==================================================================== */
  function grant(mode) {
    S.mutate(function (st) {
      st.lock = mode;
      st.unlockAttempts = 0;
      st.cooldownUntil = 0;
      st.session = 'Active';
      st.readOnly = false;
      st.maintenance = null;
    }, true);
    if (mode === 'Dummy') {
      S.mutate(function (st) { st.route = 'vault'; st.routeParams = {}; }, true);
    } else {
      S.mutate(function (st) { st.route = 'vault'; st.routeParams = {}; }, true);
    }
    enterShell();
  }

  function enterShell() {
    root.innerHTML = '';
    root.appendChild(renderShell());
    renderRoute();
    S.emit({ type: 'shell' });
  }

  /* ======================================================================
   * 5. 外壳
   * ==================================================================== */
  function sidebarEl() {
    var collapsed = S.get('sidebarCollapsed');
    var aside = h('aside', { class: 'sidebar' });
    aside.appendChild(h('div', { class: 'sidebar-brand' }, [
      h('span', { class: 'brand-mark', html: VS.icon('vault', 16) }),
      h('span', { class: 'brand-name', text: 'VaultSync' })
    ]));
    var nav = h('nav', { class: 'sidebar-nav', 'aria-label': '主导航' });
    NAV.forEach(function (group) {
      nav.appendChild(h('div', { class: 'nav-section-title', text: group.section }));
      group.items.forEach(function (item) {
        if (item.requires === 'stego' && !S.get('stegoEnabled')) return; /* 默认隐藏，不置灰 */
        if (item.id === 'license' && !S.cap('CAP_LICENSE')) return;
        var badge = item.badge ? item.badge() : null;
        var el = h('button', {
          class: 'nav-item', type: 'button',
          'aria-current': S.get('route') === item.id ? 'page' : null,
          onclick: function () { go(item.id); }
        }, [
          h('span', { class: 'nav-ico', html: VS.icon(item.icon, 18) }),
          h('span', { class: 'nav-label', text: item.label }),
          badge ? h('span', { class: 'nav-count', dataset: { tone: badge.tone }, text: badge.text }) : null
        ]);
        nav.appendChild(el);
      });
    });
    aside.appendChild(nav);

    var sessionState = S.get('readOnly') ? 'RO' : (S.get('maintenance') ? 'Maintenance' : S.get('session'));
    aside.appendChild(h('div', { class: 'sidebar-foot' }, [
      h('div', { class: 'session-badge', dataset: { state: sessionState } }, [
        h('span', { class: S.get('readOnly') || S.get('maintenance') ? 'status-dot warn' : 'status-dot online' }),
        h('span', { class: 'session-badge-text', text:
          S.get('readOnly') ? '会话：只读（租约被占）'
          : S.get('maintenance') ? '会话：维护中 · 写冻结'
          : '会话：Active · 可写' })
      ]),
      h('div', { class: 'session-fp', title: '设备指纹尾 8 位', text: '指纹 ' + D.devices[0].fingerprint.slice(-23) }),
      h('div', { class: 'row gap-1' }, [
        UI.iconBtn({ icon: collapsed ? 'chevron-right' : 'chevron-left', size: 'sm', label: collapsed ? '展开侧栏' : '收起侧栏',
          onClick: function () { S.mutate(function (st) { st.sidebarCollapsed = !st.sidebarCollapsed; }, true); enterShell(); } }),
        UI.iconBtn({ icon: 'sparkle', size: 'sm', label: '演示控制台', onClick: openDemoConsole }),
        UI.iconBtn({ icon: 'help', size: 'sm', label: '键盘快捷键', onClick: shortcutsModal })
      ])
    ]));
    return aside;
  }

  function topbarEl() {
    var route = S.get('route');
    var searchInput;
    var searchBox = UI.searchBox({
      placeholder: '搜索文件 / 标签 / 全文…  (Ctrl+K)',
      onKeyDown: function (e) { if (e.key === 'Enter' && e.target.value.trim()) { openSearch(e.target.value.trim()); } }
    });
    searchBox.classList.add('topbar-search');
    return h('header', { class: 'topbar' }, [
      h('div', { class: 'topbar-title', text: ROUTE_TITLES[route] || 'VaultSync' }),
      h('span', { class: 'topbar-spacer' }),
      searchBox,
      h('div', { class: 'topbar-actions' }, [
        UI.btn({ icon: 'refresh', label: '立即同步', size: 'sm', onClick: function () { VS.actions['syncNow'] ? VS.actions['syncNow']({}) : go('sync'); } }),
        notificationBell(),
        UI.btn({ icon: 'lock', label: '立即锁定', size: 'sm', variant: 'primary', onClick: lockNow }),
        S.get('lock') === 'Dummy' ? UI.badge({ text: '伪空间', tone: 'stale' }) : null
      ]),
      S.get('syncState') === 'Syncing' ? h('div', { class: 'topbar-progress' }, [h('i')]) : null
    ]);
  }

  function notificationBell() {
    var panel = null;
    var bell = h('div', { class: 'bell' });
    var btn = UI.iconBtn({ icon: 'bell', label: '通知中心', onClick: function () {
      if (panel) { panel.remove(); panel = null; return; }
      panel = renderNotifPanel();
      bell.appendChild(panel);
      setTimeout(function () {
        document.addEventListener('mousedown', function onDoc(e) {
          if (panel && !panel.contains(e.target) && !btn.contains(e.target)) { panel.remove(); panel = null; }
          document.removeEventListener('mousedown', onDoc, true);
        }, true);
      }, 0);
      S.mutate(function (st) { st.unread = 0; st.notifications.forEach(function (n) { n.read = true; }); }, true);
      refreshSidebarBadge();
    }});
    bell.appendChild(btn);
    var unread = S.get('unread');
    if (unread) bell.appendChild(h('span', { class: 'bell-dot', text: unread > 9 ? '9+' : String(unread) }));
    return bell;
  }

  function renderNotifPanel() {
    var panel = h('div', { class: 'notif-panel', role: 'dialog', 'aria-label': '通知中心' });
    var list = S.get('notifications');
    panel.appendChild(h('div', { class: 'card-head' }, [
      h('div', { class: 'grow' }, [h('div', { class: 't-strong', text: '通知中心' }), h('div', { class: 't-caption', text: '三级分发：info / warn / danger（danger 不可关闭）' })]),
      UI.btn({ label: '全部已读', size: 'sm', onClick: function () {
        S.mutate(function (st) { st.unread = 0; st.notifications.forEach(function (n) { n.read = true; }); });
        panel.innerHTML = ''; panel.appendChild(renderNotifPanel().firstChild); refreshSidebarBadge();
      }})
    ]));
    if (!list.length) panel.appendChild(UI.empty({ icon: 'bell-off', title: '暂无通知' }));
    list.forEach(function (n) {
      panel.appendChild(h('div', {
        class: 'notif-item', dataset: { tone: n.tone, read: String(n.read) },
        onclick: function () { if (n.route) go(n.route); panel.remove(); }
      }, [
        h('span', { class: 'ni-ico', html: VS.icon(n.tone === 'danger' ? 'danger' : n.tone === 'warn' ? 'alert' : 'info', 18) }),
        h('div', { class: 'grow' }, [
          h('div', { class: 't-strong', text: n.title }),
          h('div', { class: 't-caption', text: n.msg }),
          h('div', { class: 'ni-src', text: '来源 ' + n.source + ' · ' + F.relative(n.tsMs) })
        ]),
        n.tone === 'danger' ? UI.badge({ text: '强告警', tone: 'danger' }) : null
      ]));
    });
    return panel;
  }

  function refreshSidebarBadge() {
    var nav = document.querySelector('.sidebar-nav');
    if (!nav) return;
    var fresh = sidebarEl().querySelector('.sidebar-nav');
    if (fresh) nav.replaceWith(fresh);
  }

  function bannersEl() {
    var wrap = h('div', { class: 'banners' });
    /* 只读降级（码 9）——不弹错误框（§3.2） */
    if (S.get('readOnly')) {
      wrap.appendChild(UI.maintBanner({
        tone: 'stale', icon: 'lock',
        title: '只读 · 保险箱被另一进程占用（租约被占）',
        sub: S.get('readonlyReason') || '另一实例持有写租约，本窗口已切换为只读。写入类操作全部禁用，导出到外部路径与仅读操作仍可用。',
        actions: [
          UI.btn({ label: '可用功能清单', size: 'sm', onClick: readOnlyModal }),
          UI.btn({ label: '只读打开', size: 'sm', variant: 'primary', onClick: function () { UI.toast({ tone: 'info', title: '已确认只读模式' }); } }),
          UI.btn({ label: '退出', size: 'sm', onClick: function () { UI.toast({ tone: 'warn', title: '请关闭另一实例后重试' }); } })
        ]
      }));
    }
    /* 维护态（码 10） */
    if (S.get('maintenance')) {
      var isRot = S.get('maintenance') === 'rotation';
      var isMig = S.get('maintenance') === 'migrate';
      var rot = D.rotationState, mig = D.migrationState;
      wrap.appendChild(UI.maintBanner({
        icon: isMig ? 'migrate' : 'refresh',
        title: isRot ? '只读 · MK 轮换进行中 ' + F.pct(rot.progress.done / rot.progress.total)
                     : isMig ? '只读 · 迁移 V2→V3 校验阶段 ' + F.pct(mig.doneFiles / mig.totalFiles)
                     : '只读 · 维护中',
        sub: '业务写入已冻结（导入 / 编辑 / 改名 / 打标签 / 删除 / 擦除 / 分享 / 同步 / 轮换 / 迁移发起）。导出到外部路径仍允许。',
        progress: isRot ? rot.progress.done / rot.progress.total : (isMig ? mig.doneFiles / mig.totalFiles : null),
        actions: [
          UI.btn({ label: '查看进度', size: 'sm', variant: 'primary', onClick: function () { go(isMig ? 'maintenance-migrate' : 'maintenance-rotate'); } }),
          UI.btn({ label: '暂停', size: 'sm', disabled: isMig, title: isMig ? '迁移不支持暂停，只支持取消' : '',
            onClick: function () { UI.toast({ tone: 'info', title: '已请求暂停', msg: '轮换可随时关闭应用，下次启动自动继续' }); } })
        ]
      }));
    }
    /* 恢复中（码 11） */
    if (S.get('session') === 'NeedsRecovery' || S.get('recoveryPending')) {
      wrap.appendChild(UI.alertbar({
        tone: 'danger', icon: 'danger',
        title: '需要恢复 · 无法给出确定视图',
        text: '启动恢复未完成。未确认前不会写入任何数据。',
        actions: [UI.btn({ label: '前往恢复向导', size: 'sm', variant: 'danger', onClick: function () { go('recover'); } })]
      }));
    }
    /* 事件溢出（EVENT_OVERFLOW 23） */
    if (S.get('eventOverflow')) {
      wrap.appendChild(UI.alertbar({
        tone: 'warn', icon: 'alert',
        title: '事件流溢出',
        text: 'EVENT_OVERFLOW(23)：已丢弃 ' + (S.get('eventOverflowDropped') || 12) + ' 帧，队列页已按快照重建（seq 缺口不允许静默沿用旧值）。',
        onClose: function () { S.set({ eventOverflow: false }); enterShell(); }
      }));
    }
    return wrap;
  }

  function readOnlyModal() {
    var m = UI.modal({
      title: '当前只读 · 可用功能清单', size: 'md', tone: 'maintenance',
      body: [
        UI.alertbar({ tone: 'info', title: '这不是错误', text: '租约被占（码 9）是正常的多实例保护，按只读降级处理，不弹出错误框。' }),
        h('div', { class: 'grid-2 mt-4' }, [
          UI.card({ title: '仍可用', body: h('ul', { class: 'col gap-2' }, VS.derive.readOnlyAllowed.map(function (t) {
            return h('li', { class: 'row gap-2' }, [UI.icon('check-circle', 14), h('span', { text: t })]); })) }),
          UI.card({ title: '已禁用', body: h('ul', { class: 'col gap-2' }, VS.derive.readOnlyDenied.map(function (t) {
            return h('li', { class: 'row gap-2' }, [UI.icon('ban', 14), h('span', { text: t })]); })) })
        ]),
        h('div', { class: 'mt-3' }, [UI.muted('注：索引压实（低优先级写）被静默跳过且 UI 不提示，避免噪声。')])
      ],
      footer: [UI.btn({ label: '我知道了', variant: 'primary', onClick: function () { m.close(); } })]
    });
  }

  /* ======================================================================
   * 6. 路由
   * ==================================================================== */
  function go(route, params) {
    S.mutate(function (st) { st.route = route; st.routeParams = params || {}; }, true);
    if (location.hash !== '#/' + route) {
      history.replaceState(null, '', '#/' + route);
    }
    renderRoute();
  }
  VS.go = go;

  function renderRoute() {
    var route = S.get('route');
    var host = document.querySelector('.vs-page');
    if (!host) { enterShell(); return; }
    /* /alarm 为阻塞式全屏，不走普通页面容器 */
    if (route === 'alarm' && VS.pages['alarm']) {
      currentPage = VS.pages['alarm']({ params: S.get('routeParams'), go: go });
      return;
    }
    host.innerHTML = '';
    host.scrollTop = 0;
    var fn = VS.pages[route];
    var node;
    if (fn) {
      try { node = fn({ params: S.get('routeParams'), go: go }); }
      catch (e) {
        console.error('[VS] page render failed: ' + route, e);
        node = h('div', { class: 'page' }, [UI.errorBox(8, { text: '页面渲染异常：' + e.message })]);
      }
    } else {
      node = h('div', { class: 'page' }, [UI.empty({
        icon: 'help', title: '页面尚未实现',
        desc: '路由「' + route + '」没有注册渲染函数。请检查对应 views-*.js 是否已加载。'
      })]);
    }
    currentPage = node;
    host.appendChild(node);
    /* 同步顶栏标题与侧栏高亮 */
    var t = document.querySelector('.topbar-title');
    if (t) t.textContent = ROUTE_TITLES[route] || 'VaultSync';
    document.querySelectorAll('.nav-item').forEach(function (el) {
      var label = el.querySelector('.nav-label');
      var id = Object.keys(ROUTE_TITLES).find(function (k) { return ROUTE_TITLES[k] === (label ? label.textContent : ''); });
      el.setAttribute('aria-current', id === route ? 'page' : '');
    });
    VS.store.emit({ type: 'route', route: route });
  }
  VS.renderRoute = renderRoute;

  function renderShell() {
    applyTheme();
    var app = h('div', {
      class: 'app', dataset: { collapsed: String(!!S.get('sidebarCollapsed')) }
    }, [
      sidebarEl(),
      h('main', { class: 'main' }, [
        topbarEl(),
        bannersEl(),
        h('div', { class: 'vs-page', role: 'main' })
      ])
    ]);
    return app;
  }

  /* ======================================================================
   * 7. 命令面板（Ctrl+K）
   * ==================================================================== */
  function openPalette() {
    var overlay = h('div', { class: 'palette-overlay' });
    var input = UI.input({ placeholder: '输入命令或页面名…', id: 'palette-input' });
    var list = h('div', { class: 'palette-list' });
    var active = 0, filtered = D.commands.slice();

    function run(cmd) {
      overlay.remove();
      if (cmd.route) { go(cmd.route); return; }
      if (cmd.action === 'lock') { lockNow(); return; }
      if (cmd.action === 'theme') { S.mutate(function (st) { st.theme = st.theme === 'dark' ? 'light' : 'dark'; }, true); applyTheme(); UI.toast({ tone: 'info', title: '主题已切换' }); return; }
      if (cmd.action === 'import' && VS.actions['import']) { VS.actions['import'](); return; }
      if (cmd.action === 'newFolder' && VS.actions['newFolder']) { VS.actions['newFolder'](); return; }
      if (cmd.action === 'search') { go('vault'); setTimeout(function () { openSearch(''); }, 80); return; }
      if (cmd.action === 'syncNow') { startSync(); return; }
      if (cmd.action === 'bio') { bioManageModal(); return; }
      if (cmd.action === 'demo') { openDemoConsole(); return; }
      UI.toast({ tone: 'info', title: cmd.label, msg: '原型未接线该动作' });
    }
    function renderList() {
      list.innerHTML = '';
      if (!filtered.length) { list.appendChild(UI.empty({ icon: 'search', title: '没有匹配的命令' })); return; }
      var groups = {};
      filtered.forEach(function (c) { (groups[c.group] = groups[c.group] || []).push(c); });
      Object.keys(groups).forEach(function (g) {
        list.appendChild(h('div', { class: 'palette-group', text: g }));
        groups[g].forEach(function (c) {
          var idx = filtered.indexOf(c);
          list.appendChild(h('div', {
            class: 'palette-item', dataset: { active: String(idx === active) },
            onclick: function () { run(c); },
            onmouseenter: function () { active = idx; renderList(); }
          }, [
            h('span', { class: 'pi-ico', html: VS.icon(c.icon, 16) }),
            h('span', { class: 'pi-label', text: c.label }),
            c.shortcut ? h('span', { class: 'pi-kbd', text: c.shortcut }) : null
          ]));
        });
      });
    }
    input.addEventListener('input', function () {
      var q = input.value.toLowerCase();
      filtered = D.commands.filter(function (c) { return c.label.toLowerCase().indexOf(q) >= 0; });
      active = 0; renderList();
    });
    input.addEventListener('keydown', function (e) {
      if (e.key === 'ArrowDown') { e.preventDefault(); active = Math.min(active + 1, filtered.length - 1); renderList(); }
      else if (e.key === 'ArrowUp') { e.preventDefault(); active = Math.max(active - 1, 0); renderList(); }
      else if (e.key === 'Enter') { e.preventDefault(); if (filtered[active]) run(filtered[active]); }
      else if (e.key === 'Escape') { e.preventDefault(); overlay.remove(); }
    });
    overlay.addEventListener('mousedown', function (e) { if (e.target === overlay) overlay.remove(); });
    overlay.appendChild(h('div', { class: 'palette', role: 'dialog', 'aria-label': '命令面板' }, [
      h('div', { class: 'palette-input' }, [input]),
      list,
      h('div', { class: 'palette-foot' }, [
        h('span', { text: '↑↓ 选择' }), h('span', { text: 'Enter 执行' }), h('span', { text: 'Esc 关闭' })
      ])
    ]));
    document.body.appendChild(overlay);
    renderList();
    setTimeout(function () { input.focus(); }, 20);
  }

  /* ======================================================================
   * 8. 快捷键
   * ==================================================================== */
  function shortcutsModal() {
    var rows = [
      { k: 'Ctrl + K', d: '打开命令面板 / 全局检索' },
      { k: 'Ctrl + L', d: '立即锁定' },
      { k: 'Ctrl + A', d: '全选当前文件夹条目' },
      { k: 'Ctrl + Shift + N', d: '新建文件夹' },
      { k: 'F2', d: '重命名选中项' },
      { k: 'Delete', d: '删除选中项（进入确认弹窗）' },
      { k: 'Esc', d: '关闭弹层 / 清除选中 / 退出全屏告警（danger 告警 Esc 无效）' },
      { k: 'Space', d: '队列页暂停 / 继续' },
      { k: 'R', d: '队列页重试失败项' },
      { k: '← →', d: '锁屏 / 向导步骤切换' }
    ];
    var m = UI.modal({
      title: '键盘快捷键', size: 'sm',
      body: [UI.table({ columns: [
        { key: 'k', label: '按键', render: function (r) { return h('span', { class: 't-mono', text: r.k }); } },
        { key: 'd', label: '动作' }
      ], rows: rows })],
      footer: [UI.btn({ label: '关闭', variant: 'primary', onClick: function () { m.close(); } })]
    });
  }

  /* ======================================================================
   * 9. 演示控制台（切换状态以验收全部状态呈现）
   * ==================================================================== */
  function openDemoConsole() {
    function sw(label, sub, checked, onChange) {
      return UI.switchCtl({ label: label, sub: sub, checked: checked, onChange: function (e) { onChange(e.target.checked); } });
    }
    var capsList = h('div', { class: 'col gap-2' }, Object.keys(VS.CAP).map(function (k) {
      return UI.switchCtl({
        label: k, sub: VS.CAP[k].name + ' · bit ' + VS.CAP[k].bit,
        checked: S.get('caps')[k] !== false,
        onChange: function (e) {
          S.mutate(function (st) { st.caps[k] = e.target.checked; }, true);
          UI.toast({ tone: 'info', title: k + (e.target.checked ? ' 已置位' : ' 未置位'),
            msg: e.target.checked ? '按真值档渲染' : '按「隐藏或置灰」处理，不假装成功' });
          enterShell(); renderRoute();
        }
      });
    }));

    var m = UI.modal({
      title: '演示控制台', sub: '切换引擎状态与能力位，验收全部状态呈现（不改任何真实数据）', size: 'lg',
      body: [
        h('div', { class: 'grid-2' }, [
          UI.card({ title: '会话与租约', body: h('div', { class: 'col gap-3' }, [
            sw('只读降级（码 9）', '另一实例持有写租约', S.get('readOnly'), function (v) { S.set({ readOnly: v, readonlyReason: v ? '另一实例持有写租约（演示）' : '' }, true); refresh(); }),
            sw('维护态（码 10）', '轮换 / 迁移进行中，业务写入冻结', !!S.get('maintenance'), function (v) { S.set({ maintenance: v ? 'rotation' : null }, true); if (v) S.set({ session: 'Maintenance' }, true); else S.set({ session: 'Active' }, true); refresh(); }),
            sw('需要恢复（码 11）', '启动恢复未完成', S.get('session') === 'NeedsRecovery', function (v) { S.set({ session: v ? 'NeedsRecovery' : 'Active', recoveryPending: v }, true); refresh(); }),
            sw('锁屏显示恢复入口', '启动恢复未完成时锁屏出现入口', S.get('recovery') !== null, function (v) { S.set({ recovery: v ? D.recoveryReport : null }, true); UI.toast({ tone: 'info', title: v ? '锁屏已显示恢复入口' : '已隐藏' }); }),
            sw('事件流溢出（23）', 'seq 缺口 → 按快照重建', !!S.get('eventOverflow'), function (v) { S.set({ eventOverflow: v, eventOverflowDropped: 12 }, true); refresh(); }),
            sw('同步中', '顶栏 2px 金色进度条', S.get('syncState') === 'Syncing', function (v) { S.set({ syncState: v ? 'Syncing' : 'Idle' }, true); refresh(); })
          ]) }),
          UI.card({ title: '能力位（capability_bits）', sub: '未置位 → 隐藏或置灰，不得降级为假装成功', body: capsList })
        ]),
        h('div', { class: 'mt-4' }, [
          UI.card({ title: '状态维度速览（docs/v2.0/03 §3.2）', body: h('div', { class: 'grid-3' }, [
            statMini('锁状态', S.get('lock')),
            statMini('会话', S.get('session')),
            statMini('同步', S.get('syncState')),
            statMini('队列', S.get('queue').filter(function (t) { return t.state === 'running'; }).length ? 'Running' : 'Empty'),
            statMini('轮换', S.get('maintenance') === 'rotation' ? D.rotationState.phase : 'None'),
            statMini('迁移', S.get('maintenance') === 'migrate' ? D.migrationState.phase : 'None')
          ]) })
        ]),
        h('div', { class: 'mt-4' }, [
          UI.alertbar({ tone: 'info', title: '演示用入口', text: '这些开关只影响前端 Mock 状态；真实实现中这些状态全部由引擎事件驱动，前端不得自行推断。' })
        ])
      ],
      footer: [
        UI.btn({ label: '触发全屏强告警（/alarm 演示）', onClick: function () { m.close(); go('alarm'); } }),
        UI.btn({ label: '关闭', variant: 'primary', onClick: function () { m.close(); } })
      ]
    });

    function refresh() { m.close(); enterShell(); renderRoute(); openDemoConsole(); }
    function statMini(label, value) {
      return h('div', { class: 'stat-card panel-alt', style: { borderRadius: 'var(--r-sm)' } }, [
        h('div', { class: 'stat-label', text: label }),
        h('div', { class: 't-mono', text: String(value) })
      ]);
    }
  }

  /* ======================================================================
   * 10. 全局动作
   * ==================================================================== */
  function lockNow() {
    S.mutate(function (st) { st.lock = 'Locked'; st.session = 'Active'; st.readOnly = false; st.maintenance = null; }, true);
    UI.closeAll();
    root.innerHTML = '';
    root.appendChild(renderLock());
    UI.toast({ tone: 'info', title: '已锁定', msg: '会话已结束，MK 已从内存清除（Mock）' });
  }
  VS.actions['lock'] = lockNow;

  function startSync() {
    if (VS.derive.denyWrite()) {
      var code = VS.derive.denyWrite();
      UI.toast({ tone: code === 9 ? 'warn' : 'info', title: '无法发起同步', msg: '错误码 ' + code + ' · ' + VS.ERR[code].title });
      return;
    }
    S.set({ syncState: 'Syncing' }, true);
    enterShell();
    UI.toast({ tone: 'info', title: '已发起同步', msg: '事件 TASK_QUEUED(6) → TASK_PROGRESS(7)' });
    setTimeout(function () {
      S.set({ syncState: 'Idle' }, true);
      var t = document.querySelector('.topbar-progress');
      if (t) t.remove();
      UI.toast({ tone: 'success', title: '同步完成', msg: '结论由快照 vault_core_task_list 确认，不假设事件不丢' });
    }, 6000);
  }
  /* 注意：views-sync.js 已注册带门禁的 syncNow（CAP_TASKS + 写冻结提示）；
   * 这里只作为兜底，绝不覆盖已注册的版本（app.js 最后加载）。 */
  if (!VS.actions['syncNow']) VS.actions['syncNow'] = startSync;

  function openSearch(q) {
    VS.actions['openSearch'] ? VS.actions['openSearch'](q) : UI.toast({ tone: 'info', title: '检索', msg: 'vault 页未注册检索动作' });
  }

  /* ======================================================================
   * 11. 启动
   * ==================================================================== */
  function boot() {
    applyTheme();
    syncBp();

    /* 初始状态：锁屏 + 一条恢复提示（用于演示 §4.1 的恢复入口） */
    S.set({ recovery: null, unread: D.notifications.filter(function (n) { return !n.read; }).length }, true);

    root.appendChild(renderLock());

    window.addEventListener('resize', U.debounce(function () {
      var before = S.get('bp');
      syncBp();
      if (before !== S.get('bp') && S.get('lock') === 'Unlocked') enterShell();
    }, 160));

    document.addEventListener('keydown', function (e) {
      var mod = e.ctrlKey || e.metaKey;
      if (mod && e.key.toLowerCase() === 'k') { e.preventDefault(); S.get('lock') === 'Locked' ? null : openPalette(); return; }
      if (mod && e.key.toLowerCase() === 'l') { e.preventDefault(); if (S.get('lock') !== 'Locked') lockNow(); return; }
      if (e.key === 'Escape') {
        if (UI.layerCount()) { UI.closeTop(); return; }
        if (document.querySelector('.palette-overlay')) { document.querySelector('.palette-overlay').remove(); return; }
        S.mutate(function (st) { st.selection = []; }, true);
        if (S.get('lock') === 'Unlocked') renderRoute();
      }
    });

    /* 演示：首次进入 6 秒后模拟一条 warn 级事件 */
    setTimeout(function () {
      if (S.get('lock') !== 'Unlocked') return;
      S.mutate(function (st) {
        st.notifications.unshift({ id: 'n-live', tone: 'warn', title: '链路已降级为中继',
          msg: 'MacBook-Air：打洞超时（hole_punch_timeout）', tsMs: Date.now(), read: false, source: 'PATH_DEGRADED(13)', route: 'devices' });
        st.unread++;
      });
      UI.toast({ tone: 'warn', title: '链路已降级为中继', msg: 'MacBook-Air：打洞超时 · 已自动回退，不静默', duration: 6000 });
    }, 6000);

    /* hash 路由回填 */
    var h0 = location.hash.replace(/^#\/?/, '');
    if (h0 && VS.pages[h0]) { /* 直接进入某页仍需先解锁 */ }
  }

  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', boot);
  else boot();

  /* 暴露给视图模块的工具 */
  VS.app = { enterShell: enterShell, renderRoute: renderRoute, renderLock: renderLock, openPalette: openPalette, lockNow: lockNow };

})(window);
