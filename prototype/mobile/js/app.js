/* ============================================================================
 * VaultSync V2.0 原型 · 移动端外壳与导航（<720 档）
 * 依据 docs/v2.0/03 §4.3 手机窄屏主壳：单行顶栏 · 总览条 · 底部 Tab（5 项）
 *      · Tab 切换不重建页面状态 · 滚动位置按 Tab 记忆 · 详情为全屏路由
 * 与桌面端共用 ../v2.0/js/{core,ui,data}.js（一套内核、受管外壳）
 * ==========================================================================*/
(function (global) {
  'use strict';

  var VS = global.VS;
  var h = VS.util.h, UI = VS.ui, MUI = VS.mui, F = VS.fmt, D = VS.data, S = VS.store;

  VS.pages = VS.pages || {};
  VS.actions = VS.actions || {};

  var root = null;
  var screenEl = null;

  /* ======================================================================
   * 1. 导航：每个 Tab 一条独立栈（Tab 切换不重建页面状态）
   * ==================================================================== */
  var TABS = ['vault', 'sync', 'devices', 'security', 'settings'];
  var TAB_META = {
    vault:    { label: '保险箱',   icon: 'vault' },
    sync:     { label: '同步',     icon: 'sync' },
    devices:  { label: '设备',     icon: 'devices' },
    security: { label: '安全中心', icon: 'security' },
    settings: { label: '设置',     icon: 'settings' }
  };

  var nav = VS.nav = {
    TABS: TABS,
    isTab: function (r) { return TABS.indexOf(r) >= 0; },
    stacks: {},
    activeTab: 'vault',
    demo: false
  };
  TABS.forEach(function (t) { nav.stacks[t] = [{ route: t, params: {}, scrollTop: 0 }]; });

  function top() { return nav.stacks[nav.activeTab][nav.stacks[nav.activeTab].length - 1]; }

  nav.go = function (route, params) {
    if (nav.isTab(route)) {
      nav.switchTab(route);
      return;
    }
    var stack = nav.stacks[nav.activeTab];
    var cur = stack[stack.length - 1];
    if (cur && screenEl) cur.scrollTop = screenEl.scrollTop;
    stack.push({ route: route, params: params || {}, scrollTop: 0 });
    render();
  };

  nav.replace = function (route, params) {
    var stack = nav.stacks[nav.activeTab];
    stack[stack.length - 1] = { route: route, params: params || {}, scrollTop: 0 };
    render();
  };

  nav.back = function () {
    if (MUI.back()) return;
    var stack = nav.stacks[nav.activeTab];
    if (stack.length > 1) { stack.pop(); render(); }
  };

  nav.switchTab = function (tab) {
    if (!nav.stacks[tab]) return;
    if (screenEl && nav.stacks[nav.activeTab]) {
      var cur = top();
      if (cur) cur.scrollTop = screenEl.scrollTop;
    }
    nav.activeTab = tab;
    render();
  };

  nav.reset = function (tab) {
    nav.stacks[tab] = [{ route: tab, params: {}, scrollTop: 0 }];
  };

  /* ======================================================================
   * 2. 渲染
   * ==================================================================== */
  function render() {
    var entry = top();
    var factory = VS.pages[entry.route];
    var page;
    if (factory) {
      try {
        page = factory({ params: entry.params, go: nav.go, nav: nav });
      } catch (e) {
        console.error('[VS] 页面渲染失败: ' + entry.route, e);
        page = MUI.page({
          title: '页面渲染异常',
          body: MUI.screen([MUI.errorBox(8, { text: e.message })]),
          tab: true
        });
      }
    } else {
      page = MUI.page({
        title: '页面尚未实现',
        body: MUI.screen([MUI.empty({
          icon: 'help', title: '路由未注册',
          desc: '路由「' + entry.route + '」没有渲染函数，请检查对应 views-*.js 是否已加载。'
        })]),
        tab: true
      });
    }
    paint(page, entry);
  }

  function paint(page, entry) {
    var host = document.querySelector('.phone-screen') || root;
    host.innerHTML = '';

    var shell = h('div', { class: 'shell' });
    shell.appendChild(MUI.statusbar());

    shell.appendChild(MUI.appbar({
      title: page.title,
      sub: page.sub,
      back: page.back || !nav.isTab(entry.route),
      onBack: page.onBack || function () { nav.back(); },
      actions: page.actions,
      overflow: page.overflow
    }));

    /* 维护态 / 只读降级横幅（§1.2 状态透明） */
    var banners = bannersEl();
    if (banners) shell.appendChild(banners);

    /* 总览条：仅同步相关页提供，无事件源时返回 null（整块不渲染） */
    if (page.overview) shell.appendChild(page.overview);

    /* 页面契约要求 body 为 MUI.screen(...)；若视图已自行包裹则直接复用，
     * 避免双层 padding 与嵌套滚动容器 */
    if (page.body && page.body.classList && page.body.classList.contains('screen')) {
      screenEl = page.body;
      if (page.flush) screenEl.classList.add('flush');
    } else {
      screenEl = MUI.screen(page.body, { flush: page.flush });
    }
    shell.appendChild(screenEl);

    if (page.tab !== false && nav.isTab(entry.route)) {
      shell.appendChild(MUI.tabbar({
        value: nav.activeTab,
        items: TABS.map(function (t) {
          var badge = null;
          if (t === 'security') badge = S.get('unread') || null;
          if (t === 'sync') {
            var q = S.get('queue').filter(function (x) { return x.state === 'running' || x.state === 'queued'; }).length;
            badge = q || null;
          }
          return { value: t, label: TAB_META[t].label, icon: TAB_META[t].icon, badge: badge };
        }),
        onChange: function (t) { nav.switchTab(t); }
      }));
    }

    if (page.fab) shell.appendChild(page.fab);

    host.appendChild(shell);
    if (page.body && page.body.classList) page.body.classList.add('route-enter');

    /* 恢复滚动位置（Tab 记忆） */
    if (screenEl && entry.scrollTop) screenEl.scrollTop = entry.scrollTop;
    if (screenEl) {
      screenEl.addEventListener('scroll', VS.util.throttle(function () {
        var cur = top();
        if (cur) cur.scrollTop = screenEl.scrollTop;
      }, 120));
    }
    S.emit({ type: 'route', route: entry.route });
  }
  VS.paint = paint;
  VS.render = render;

  function bannersEl() {
    var nodes = [];
    if (S.get('readOnly')) {
      nodes.push(MUI.maintBanner('stale', '只读 · 租约被另一进程占用',
        '写操作已禁用；导出到外部路径与仅读操作仍可用。'));
    }
    if (S.get('maintenance')) {
      var isMig = S.get('maintenance') === 'migrate';
      nodes.push(MUI.maintBanner(isMig ? 'migrate' : 'rotation',
        isMig ? '只读 · 迁移进行中' : '只读 · MK 轮换进行中',
        '业务写入已冻结；可随时关闭应用，下次启动自动继续。'));
    }
    if (S.get('session') === 'NeedsRecovery') {
      nodes.push(UI.alertbar({
        tone: 'danger', title: '需要恢复 · 无法给出确定视图',
        text: '未确认前不会写入任何数据。',
        actions: [UI.btn({ label: '进入恢复', size: 'sm', variant: 'danger', onClick: function () { nav.go('recover'); } })]
      }));
    }
    if (!nodes.length) return null;
    var wrap = h('div', { class: 'col gap-2', style: { padding: '8px 16px 0' } });
    nodes.forEach(function (n) { wrap.appendChild(n); });
    return wrap;
  }

  /* 维护 / 只读降级横幅的移动端实现见 mui.js 的 MUI.maintBanner（稳定签名，供各视图复用） */

  /* ======================================================================
   * 3. 锁屏（生物识别为主路径）
   * ==================================================================== */
  var DEMO_PW = '1234';
  var DEMO_DUMMY = '88888888';

  function renderLock() {
    var host = document.querySelector('.phone-screen') || root;
    host.innerHTML = '';
    var wrap = h('div', { class: 'mlock' });
    var errHost = h('div', { class: 'col gap-2', style: { width: '100%' } });
    var pwInput = UI.input({ type: 'password', placeholder: '主密码', id: 'm-lock-pw', size: 'lg' });
    pwInput.style.textAlign = 'center';
    pwInput.style.letterSpacing = '0.2em';

    var bioBtn = h('button', {
      class: 'mbtn-block', type: 'button', dataset: { variant: 'accent' },
      onclick: function () {
        if (!S.get('biometricAvailable') || S.get('caps').CAP_BIO === false) return;
        MUI.haptic();
        UI.toast({ tone: 'info', title: '生物识别', msg: '平台安全区校验通过（原型模拟）' });
        setTimeout(function () { grant('Unlocked'); }, 300);
      }
    }, [h('span', { html: VS.icon('fingerprint', 20) }), h('span', { text: '用生物识别解锁' })]);

    var pwBtn = h('button', {
      class: 'mbtn-block', type: 'button', dataset: { variant: 'primary' },
      onclick: doUnlock
    }, [h('span', { text: '解锁' })]);

    pwInput.addEventListener('keydown', function (e) { if (e.key === 'Enter') doUnlock(); });

    function showErr(code, extra) {
      errHost.innerHTML = '';
      errHost.appendChild(UI.alertbar({
        tone: VS.ERR[code].tone === 'stale' ? 'info' : VS.ERR[code].tone,
        title: '错误码 ' + code + ' · ' + VS.ERR[code].title,
        text: extra || VS.ERR[code].hint
      }));
    }
    function doUnlock() {
      var now = Date.now();
      if (now < S.get('cooldownUntil')) { showErr(2, '冷却中，请等待倒计时结束'); return; }
      var pw = pwInput.value;
      if (!pw) { pwInput.focus(); return; }
      if (pw === DEMO_PW) { UI.toast({ tone: 'success', title: '解锁成功' }); grant('Unlocked'); }
      else if (pw === DEMO_DUMMY) { UI.toast({ tone: 'info', title: '已进入伪空间' }); grant('Dummy'); }
      else {
        var n = S.get('unlockAttempts') + 1;
        S.set({ unlockAttempts: n }, true);
        if (n >= 3) {
          S.set({ cooldownUntil: now + 30000 }, true);
          showErr(2, '连续失败 3 次，已触发暴力破解保护（30s）');
        } else showErr(1, '第 ' + n + ' 次失败');
      }
    }

    wrap.appendChild(h('div', {
      style: {
        width: '60px', height: '60px', borderRadius: '18px', display: 'grid', placeItems: 'center',
        background: 'linear-gradient(140deg, var(--c-accent), #8A6A1E)', color: '#14100A'
      }, html: VS.icon('vault', 30)
    }));
    wrap.appendChild(h('div', { class: 'ml-brand', text: 'VaultSync' }));
    wrap.appendChild(h('div', { class: 't-caption', style: { letterSpacing: '.3em', fontSize: '10px' }, text: 'YOUR DATA · EVERYWHERE · ENCRYPTED' }));
    wrap.appendChild(errHost);
    wrap.appendChild(bioBtn);

    wrap.appendChild(h('div', { class: 'row', style: { width: '100%', gap: '8px', alignItems: 'center' } }, [
      h('span', { style: { flex: '1', height: '1px', background: 'var(--c-border)' } }),
      h('span', { class: 't-caption', text: '或使用主密码' }),
      h('span', { style: { flex: '1', height: '1px', background: 'var(--c-border)' } })
    ]));
    wrap.appendChild(h('div', { style: { width: '100%' } }, [pwInput]));
    wrap.appendChild(pwBtn);

    wrap.appendChild(h('div', { class: 'row gap-4', style: { marginTop: '4px' } }, [
      h('button', { class: 't-caption', type: 'button', text: '忘记主密码？', style: { color: 'var(--c-text-2)' },
        onclick: function () {
          MUI.sheet({ title: '忘记主密码', body: [
            h('p', { class: 't-body', text: 'MK 由 CSPRNG 生成且永不变更；主密码只是解封 MK 的 KEK 之一。没有主密码或生物识别，任何人都无法解封。' }),
            h('div', { class: 'mt-3' }, [UI.alertbar({ tone: 'warn', title: '可选路径', text: '① 已绑定生物识别；② 另一台已配对设备导出恢复票据；③ 两把 KEK 全丢则只能安全重置（数据不可恢复）。' })])
          ] });
        } }),
      (S.get('session') === 'NeedsRecovery' || S.get('recovery'))
        ? h('button', { class: 't-caption', type: 'button', style: { color: 'var(--c-danger)' },
            text: '⚠ 进入恢复向导', onclick: function () { grant('Unlocked'); nav.go('recover'); } })
        : null
    ]));

    wrap.appendChild(h('div', { class: 't-caption', style: { marginTop: 'auto', paddingTop: '16px' }, text: '演示口令 1234 / 伪空间 88888888' }));
    host.appendChild(wrap);
  }

  var VS_SCREEN = null;
  function grant(mode) {
    S.mutate(function (st) {
      st.lock = mode; st.unlockAttempts = 0; st.cooldownUntil = 0;
      st.session = 'Active'; st.readOnly = false; st.maintenance = null;
    }, true);
    nav.activeTab = 'vault';
    nav.reset('vault');
    MUI.closeAll();
    VS_SCREEN.innerHTML = '';
    render();
  }

  /* ======================================================================
   * 4. 演示控制台（移动端）
   * ==================================================================== */
  function openDemo() {
    function sw(label, sub, checked, onChange) {
      return UI.switchCtl({ label: label, sub: sub, checked: checked,
        onChange: function (e) { onChange(e.target.checked); } });
    }
    var capsList = h('div', { class: 'col gap-3' }, Object.keys(VS.CAP).map(function (k) {
      return UI.switchCtl({
        label: k, sub: VS.CAP[k].name + ' · bit ' + VS.CAP[k].bit,
        checked: S.get('caps')[k] !== false,
        onChange: function (e) {
          S.mutate(function (st) { st.caps[k] = e.target.checked; }, true);
          UI.toast({ tone: 'info', title: k + (e.target.checked ? ' 已置位' : ' 未置位'),
            msg: e.target.checked ? '按真值档渲染' : '按「隐藏或置灰」处理' });
          VS_SCREEN.innerHTML = ''; render();
        }
      });
    }));

    MUI.sheet({
      title: '演示控制台', sub: '切换引擎状态与能力位以验收状态呈现', size: 'tall',
      body: [
        MUI.mcard({ title: '会话与租约', body: h('div', { class: 'col gap-3' }, [
          sw('只读降级（码 9）', '另一实例持有写租约', S.get('readOnly'), function (v) {
            S.set({ readOnly: v, readonlyReason: v ? '另一实例持有写租约（演示）' : '' }, true); repaint(); }),
          sw('维护态（码 10）', '业务写入冻结，数据可读', !!S.get('maintenance'), function (v) {
            S.set({ maintenance: v ? 'rotation' : null }, true);
            S.set({ session: v ? 'Maintenance' : 'Active' }, true); repaint(); }),
          sw('需要恢复（码 11）', '启动恢复未完成', S.get('session') === 'NeedsRecovery', function (v) {
            S.set({ session: v ? 'NeedsRecovery' : 'Active' }, true); repaint(); }),
          sw('同步中', '顶栏进度指示', S.get('syncState') === 'Syncing', function (v) {
            S.set({ syncState: v ? 'Syncing' : 'Idle' }, true); repaint(); })
        ]) }),
        h('div', { class: 'mt-3' }, [MUI.mcard({
          title: '能力位（capability_bits）',
          sub: '未置位 → 隐藏或置灰，绝不降级为假装成功',
          body: capsList
        })]),
        h('div', { class: 'mt-3' }, [MUI.limitedNote('移动端减配项（后台同步尽力而为、无分栏视图、入站监听仅前台、配对以扫码为主）不会因为能力位开关而改变——它们是本端外壳的固有约束，必须在界面上如实声明。')])
      ],
      footer: function (close) {
        return [
          h('button', { class: 'mbtn-block', type: 'button', dataset: { variant: 'danger' },
            onclick: function () { close(); nav.go('alarm'); } },
            [h('span', { html: VS.icon('danger', 18) }), h('span', { text: '触发全屏强告警（/alarm）' })]),
          h('button', { class: 'mbtn-block', type: 'button', dataset: { variant: 'ghost' },
            onclick: close }, [h('span', { text: '关闭' })])
        ];
      }
    });

    function repaint() {
      MUI.closeAll();
      VS_SCREEN.innerHTML = '';
      render();
    }
  }
  VS.actions['mobile.demo'] = openDemo;
  VS.actions['lock'] = function () { lockNow(); };

  function lockNow() {
    S.mutate(function (st) { st.lock = 'Locked'; st.session = 'Active'; st.readOnly = false; st.maintenance = null; }, true);
    MUI.closeAll();
    renderLock();
  }

  /* ======================================================================
   * 5. 启动
   * ==================================================================== */
  function boot() {
    root = document.getElementById('root');
    /* 手机外框 */
    var stage = h('div', { class: 'stage' });
    stage.appendChild(h('div', { class: 'stage-side' }, [
      h('div', { class: 'ss-title', text: 'VaultSync · 移动端原型' }),
      h('div', { class: 'ss-desc', text: '同一套内核、受管外壳。本端为 <720 档：底部 Tab（5 项）、单栏列表、详情全屏路由、二级界面用底部面板。' }),
      h('div', { class: 'ss-desc', text: '演示口令 1234（真实保险箱）/ 88888888（伪空间）。' }),
      h('div', { class: 'ss-desc', text: '触控模拟：长按列表项 = 动作面板；左滑 = 等价降级。' }),
      h('div', { class: 'ss-desc' }, [h('span', { text: 'Esc = 返回 / 关闭浮层 · ' }), h('span', { class: 'ss-kbd', text: '窗口 <720px 自动铺满' })])
    ]));
    var phone = h('div', { class: 'phone' }, [
      h('div', { class: 'phone-notch' }),
      h('div', { class: 'phone-screen', id: 'screen' })
    ]);
    stage.appendChild(phone);
    root.appendChild(stage);
    VS_SCREEN = phone.querySelector('.phone-screen');

    /* Toast 宿主默认挂在 body 上且为 position:fixed，在桌面浏览器预览时会飘到手机外框之外。
     * 必须挂在 .phone 上而不是 .phone-screen 上 —— 后者会被 grant() / 演示控制台的
     * innerHTML='' 清空，宿主会被一并移除，UI.toast 随后又会在 body 上重建，问题复现。 */
    var host = document.querySelector('.toast-host');
    if (host && host.parentNode !== phone) phone.appendChild(host);

    document.documentElement.dataset.bp = 'phone';

    /* 初始：锁屏；若启动恢复未完成则给出恢复入口 */
    S.set({ recovery: null }, true);
    renderLock();

    /* 返回键 / Esc */
    document.addEventListener('keydown', function (e) {
      if (e.key === 'Escape') {
        if (MUI.back()) return;
        if (S.get('lock') !== 'Locked') nav.back();
      }
    });
    window.addEventListener('popstate', function () { if (MUI.back()) return; nav.back(); });

    setTimeout(function () {
      if (S.get('lock') !== 'Unlocked' && S.get('lock') !== 'Dummy') return;
      S.mutate(function (st) {
        st.notifications.unshift({ id: 'm-live', tone: 'warn', title: '链路已降级为中继',
          msg: 'MacBook-Air：打洞超时（hole_punch_timeout）', tsMs: Date.now(), read: false,
          source: 'PATH_DEGRADED(13)', route: 'devices' });
        st.unread++;
      });
      UI.toast({ tone: 'warn', title: '链路已降级为中继', msg: 'MacBook-Air：打洞超时 · 不静默', duration: 6000 });
    }, 6000);
  }

  VS.app = { render: render, renderLock: renderLock, lockNow: lockNow, screen: function () { return VS_SCREEN; } };

  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', boot);
  else boot();

})(window);
