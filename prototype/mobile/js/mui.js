/* ============================================================================
 * VaultSync V2.0 原型 · 移动端 UI 原语（MUI）
 * 依赖 ../v2.0/js/{core,ui}.js —— 与桌面端共用同一套内核、Token 与控件语汇
 * 约定：移动端二级界面用「底部面板（bottom sheet）」；详情为全屏路由；
 *       列表项操作为长按动作面板，左滑为等价降级（docs/v2.0/03 §4.3）
 * ==========================================================================*/
(function (global) {
  'use strict';

  var VS = global.VS || (global.VS = {});
  var h = VS.util.h, icon = VS.icon, F = VS.fmt;
  var UI = VS.ui;
  var MUI = VS.mui = {};

  /* ======================= 0. 浮层挂载点与返回栈 ====================== */
  var layers = [];   /* {el, onBack} */

  function mountRoot() {
    return document.querySelector('.phone-screen') || document.body;
  }
  MUI.mountRoot = mountRoot;

  MUI.pushLayer = function (el, onBack) {
    var layer = { el: el, onBack: onBack };
    layers.push(layer);
    mountRoot().appendChild(el);
    return layer;
  };
  MUI.popLayer = function (layer) {
    layers = layers.filter(function (l) { return l !== layer; });
    if (layer.el.parentNode) layer.el.parentNode.removeChild(layer.el);
  };
  MUI.layerCount = function () { return layers.length; };
  /** 返回手势 / Esc / 硬件返回：优先关最上层浮层；返回 true 表示已消费 */
  MUI.back = function () {
    var layer = layers[layers.length - 1];
    if (!layer) return false;
    if (typeof layer.onBack === 'function') layer.onBack();
    else MUI.popLayer(layer);
    return true;
  };
  MUI.closeAll = function () {
    layers.slice().forEach(function (l) { MUI.popLayer(l); });
  };

  /* ======================= 1. 外壳零件 ============================== */
  MUI.statusbar = function (o) {
    o = o || {};
    var now = new Date();
    var time = ('0' + now.getHours()).slice(-2) + ':' + ('0' + now.getMinutes()).slice(-2);
    return h('div', { class: 'statusbar', 'aria-hidden': 'true' }, [
      h('span', { text: o.time || time }),
      h('div', { class: 'sb-right' }, [
        h('span', { html: icon('wifi', 14) }),
        h('span', { html: icon('drive', 14) }),
        h('span', { text: o.battery || '86%' })
      ])
    ]);
  };

  /**
   * 应用栏。actions: [{icon,label,onClick}]；overflow: 菜单项数组（点「⋯」弹动作面板）
   */
  MUI.appbar = function (o) {
    o = o || {};
    var bar = h('header', { class: 'appbar' });
    if (o.back) {
      bar.appendChild(h('button', {
        class: 'ab-back', type: 'button', 'aria-label': '返回',
        onclick: o.onBack || function () { VS.nav.back(); }
      }, [VS.iconEl('chevron-left', 24)]));
    } else if (o.leading) {
      bar.appendChild(o.leading);
    }
    bar.appendChild(h('div', { class: 'ab-title-wrap' }, [
      h('div', { class: 'ab-title', text: o.title || '' }),
      o.sub ? h('div', { class: 'ab-sub', text: o.sub }) : null
    ]));
    var actions = h('div', { class: 'ab-actions' });
    (o.actions || []).forEach(function (a) {
      actions.appendChild(h('button', {
        class: 'ab-action', type: 'button', 'aria-label': a.label || '', title: a.label || '',
        onclick: a.onClick
      }, [VS.iconEl(a.icon, 20)]));
    });
    if (o.overflow && o.overflow.length) {
      actions.appendChild(h('button', {
        class: 'ab-action', type: 'button', 'aria-label': '更多操作',
        onclick: function () { MUI.actionSheet({ title: o.title, items: o.overflow }); }
      }, [VS.iconEl('more', 20)]));
    }
    bar.appendChild(actions);
    return bar;
  };

  MUI.tabbar = function (o) {
    o = o || {};
    var bar = h('nav', { class: 'tabbar', role: 'tablist', 'aria-label': '主导航' });
    (o.items || []).forEach(function (it) {
      bar.appendChild(h('button', {
        class: 'tabbar-item', type: 'button', role: 'tab',
        'aria-selected': it.value === o.value ? 'true' : 'false',
        onclick: function () { o.onChange && o.onChange(it.value); }
      }, [
        h('span', { html: icon(it.icon, 22) }),
        h('span', { text: it.label }),
        it.badge ? h('span', { class: 'tb-dot', text: it.badge > 9 ? '9+' : String(it.badge) }) : null
      ]));
    });
    return bar;
  };

  /** 总览条（§4.3）：items = [{icon, label, value, tone}]；空数组 → 整块不渲染 */
  MUI.overviewBar = function (items) {
    if (!items || !items.length) return null;
    var bar = h('div', { class: 'overview-bar', role: 'status' });
    items.forEach(function (it, i) {
      if (i) bar.appendChild(h('span', { text: '·' }));
      bar.appendChild(h('span', { class: 'ob-item' }, [
        it.icon ? h('span', { html: icon(it.icon, 13) }) : null,
        it.label ? h('span', { text: it.label }) : null,
        it.value ? h('span', { class: 'ob-val ' + (it.tone ? 'ob-' + it.tone : ''), text: it.value }) : null
      ]));
    });
    return bar;
  };

  /** 内容滚动区 */
  MUI.screen = function (children, opts) {
    opts = opts || {};
    return h('div', { class: 'screen' + (opts.flush ? ' flush' : ''), dataset: opts.dataset || {} }, children);
  };

  /* ======================= 2. 页面契约 ============================== */
  /**
   * 页面工厂：返回 { title, body, back, actions, overflow, tab, fab, onEnter }
   * 由 app.js 的外壳消费。
   */
  MUI.page = function (o) {
    o = o || {};
    return {
      title: o.title || '',
      sub: o.sub || null,
      back: !!o.back,
      onBack: o.onBack || null,
      actions: o.actions || [],
      overflow: o.overflow || null,
      body: o.body || h('div'),
      tab: o.tab !== false,
      fab: o.fab || null,
      flush: !!o.flush,
      keepScroll: o.keepScroll !== false
    };
  };

  /* ======================= 3. 列表与卡片 ============================ */
  /**
   * 移动端列表行。
   * { icon, title, sub, trail, chevron, onClick, onLongPress, disabled, reason, danger, swipe }
   */
  MUI.mrow = function (o) {
    o = o || {};
    var inner = [
      o.icon ? h('span', { class: 'mr-ico', html: icon(o.icon, 22) }) : null,
      h('div', { class: 'mr-main' }, [
        h('div', { class: 'mr-title' }, [
          h('span', { class: 't-truncate', text: o.title }),
          o.badges || null
        ]),
        o.sub ? h('div', { class: 'mr-sub', text: o.sub }) : null
      ]),
      o.trail ? h('span', { class: 'mr-trail' }, o.trail) : null,
      o.chevron !== false && o.onClick ? h('span', { class: 'mr-chev', html: icon('chevron-right', 18) }) : null
    ];

    if (o.swipe && o.swipe.length) {
      var content = h('button', {
        class: 'swipe-content', type: 'button',
        'aria-disabled': o.disabled ? 'true' : null,
        onclick: function () {
          if (row.dataset.open === 'true') { closeSwipe(); return; }
          if (!o.disabled && o.onClick) o.onClick();
        },
        oncontextmenu: function (e) {
          e.preventDefault();
          if (o.onLongPress) { MUI.haptic(); o.onLongPress(); }
        }
      }, inner);
      var actions = h('div', { class: 'swipe-actions' });
      o.swipe.forEach(function (a) {
        actions.appendChild(h('button', {
          type: 'button', dataset: { tone: a.tone || '' }, title: a.label,
          onclick: function (e) { e.stopPropagation(); closeSwipe(); a.onClick && a.onClick(); }
        }, [a.icon ? h('span', { html: icon(a.icon, 18) }) : null, h('span', { text: a.label })]));
      });
      var row = h('div', { class: 'swipe-row', dataset: { open: 'false' } }, [actions, content]);

      function closeSwipe() { row.dataset.open = 'false'; content.style.transform = ''; }
      MUI.attachSwipe(row, content, {
        close: closeSwipe,
        disabled: !!o.disabled,
        onLongPress: o.onLongPress
      });
      return row;
    }

    return h('button', {
      class: 'mrow', type: 'button',
      dataset: { danger: o.danger ? 'true' : '', disabled: o.disabled ? 'true' : '' },
      'aria-disabled': o.disabled ? 'true' : null,
      title: o.disabled && o.reason ? o.reason : '',
      onclick: function () {
        if (o.disabled) {
          if (o.reason) UI.toast({ tone: 'warn', title: (o.title || '') + '：不可用', msg: o.reason });
          return;
        }
        o.onClick && o.onClick();
      },
      oncontextmenu: function (e) {
        if (!o.onLongPress) return;
        e.preventDefault();
        MUI.haptic();
        o.onLongPress();
      }
    }, inner);
  };

  MUI.mlist = function (rows, opts) {
    opts = opts || {};
    return h('div', { class: 'mlist' + (opts.flush ? ' flush' : '') }, rows);
  };

  /**
   * 给左滑行绑定拖拽与长按。
   * 原型在桌面浏览器里通过鼠标拖拽演示左滑；真机上则是触摸滑动。
   * opts: { close, disabled, onLongPress }
   */
  MUI.attachSwipe = function (row, content, opts) {
    opts = opts || {};
    function actionsWidth() {
      var a = row.querySelector('.swipe-actions');
      var n = a && a.children ? a.children.length : 2;
      return Math.max(76, n * 76);
    }
    function apply(x) { content.style.transform = x ? ('translateX(' + x + 'px)') : ''; }
    var startX = 0, startY = 0, baseX = 0, moving = false, axis = null, longTimer = null;

    function clearLong() { if (longTimer) { clearTimeout(longTimer); longTimer = null; } }
    function onDown(e) {
      var pt = e.touches ? e.touches[0] : e;
      if (e.pointerType === 'mouse' && e.button > 0) return;
      startX = pt.clientX; startY = pt.clientY;
      baseX = row.dataset.open === 'true' ? -actionsWidth() : 0;
      moving = true; axis = null;
      if (opts.onLongPress) {
        clearLong();
        longTimer = setTimeout(function () {
          longTimer = null; moving = false;
          MUI.haptic();
          opts.onLongPress();
        }, 520);
      }
    }
    function onMove(e) {
      if (!moving) return;
      var pt = e.touches ? e.touches[0] : e;
      var dx = pt.clientX - startX, dy = pt.clientY - startY;
      if (!axis) {
        if (Math.abs(dx) < 6 && Math.abs(dy) < 6) return;
        axis = Math.abs(dx) > Math.abs(dy) ? 'x' : 'y';
        if (axis === 'y') { clearLong(); return; }
        clearLong();
      }
      if (axis !== 'x') return;
      if (e.cancelable) e.preventDefault();
      var w = actionsWidth();
      apply(VS.util.clamp(baseX + dx, -w, 0));
    }
    function onUp() {
      if (!moving) { clearLong(); return; }
      moving = false; clearLong();
      if (axis !== 'x') return;
      var m = /translateX\((-?[\d.]+)px\)/.exec(content.style.transform || '');
      var x = m ? parseFloat(m[1]) : 0;
      if (x < -actionsWidth() / 2) { row.dataset.open = 'true'; apply(-actionsWidth()); }
      else opts.close && opts.close();
    }

    if (global.PointerEvent) {
      content.addEventListener('pointerdown', onDown);
      content.addEventListener('pointermove', onMove);
      content.addEventListener('pointerup', onUp);
      content.addEventListener('pointercancel', onUp);
      content.addEventListener('pointerleave', function () { if (moving && axis !== 'x') onUp(); });
    } else {
      content.addEventListener('touchstart', onDown, { passive: true });
      content.addEventListener('touchmove', onMove, { passive: false });
      content.addEventListener('touchend', onUp);
      content.addEventListener('mousedown', onDown);
      document.addEventListener('mousemove', onMove);
      document.addEventListener('mouseup', onUp);
    }
    row._swipeClose = opts.close;
  };
  MUI.mgroupTitle = function (t) { return h('div', { class: 'mgroup-title', text: t }); };
  MUI.mgroupNote = function (t) { return h('div', { class: 'mgroup-note', text: t }); };

  MUI.mcard = function (o) {
    o = o || {};
    return h('section', { class: 'mcard' }, [
      (o.title || o.actions) ? h('div', { class: 'mcard-head' }, [
        h('div', { class: 'grow' }, [
          h('div', { class: 't-strong', text: o.title || '' }),
          o.sub ? h('div', { class: 't-caption', text: o.sub }) : null
        ]),
        o.actions ? h('div', { class: 'row gap-2' }, o.actions) : null
      ]) : null,
      h('div', { class: 'mcard-body' + (o.flush ? ' flush' : '') }, o.body)
    ]);
  };

  MUI.mstat = function (o) {
    return h('div', { class: 'mstat' }, [
      h('div', { class: 'ms-val', text: o.value }),
      h('div', { class: 'ms-label', text: o.label }),
      o.src ? h('div', { class: 'ms-src', text: o.src }) : null
    ]);
  };

  MUI.mstats = function (items) {
    return h('div', { class: 'mstats' }, items.map(function (it) { return MUI.mstat(it); }));
  };

  MUI.mkv = function (pairs) {
    return h('div', { class: 'mkv' }, (pairs || []).filter(Boolean).map(function (p) {
      var v = h('div', { class: 'mkv-v' + (p.mono ? ' mono' : '') });
      if (p.node) v.appendChild(p.node);
      else v.textContent = p.v === null || p.v === undefined || p.v === '' ? '—' : String(p.v);
      return h('div', { class: 'mkv-row' }, [
        h('div', { class: 'mkv-k', text: p.k }),
        v
      ]);
    }));
  };

  MUI.mseg = function (o) {
    o = o || {};
    var wrap = h('div', { class: 'mseg', role: 'tablist' });
    (o.items || []).forEach(function (it) {
      wrap.appendChild(h('button', {
        type: 'button', role: 'tab',
        'aria-selected': it.value === o.value ? 'true' : 'false',
        onclick: function () { o.onChange && o.onChange(it.value); }
      }, [it.icon ? h('span', { html: icon(it.icon, 15) }) : null, h('span', { text: it.label })]));
    });
    return wrap;
  };

  MUI.mtabs = function (o) {
    o = o || {};
    var wrap = h('div', { class: 'mtabs-scroll', role: 'tablist' });
    (o.items || []).forEach(function (it) {
      wrap.appendChild(h('button', {
        type: 'button', role: 'tab',
        'aria-selected': it.value === o.value ? 'true' : 'false',
        onclick: function () { o.onChange && o.onChange(it.value); }
      }, [it.label]));
    });
    return wrap;
  };

  MUI.limitedNote = function (text) {
    return h('div', { class: 'limited-note' }, [
      h('span', { html: icon('info', 15) }),
      h('div', { class: 'grow', text: text })
    ]);
  };

  /**
   * 移动端维护 / 只读降级横幅（稳定签名，供所有视图模块调用）。
   * tone: 'maintenance' | 'rotation' | 'migrate' | 'stale'
   */
  MUI.maintBanner = function (tone, title, sub, actions) {
    var iconName = tone === 'stale' ? 'lock' : tone === 'migrate' ? 'migrate' : 'refresh';
    return h('div', {
      class: 'maint-banner',
      dataset: { tone: tone === 'stale' ? 'stale' : 'maintenance' },
      role: 'status'
    }, [
      h('span', { class: 'mb-ico', html: icon(iconName, 18) }),
      h('div', { class: 'mb-text' }, [
        h('div', { class: 'mb-title', text: title }),
        sub ? h('div', { class: 'mb-sub', text: sub }) : null
      ]),
      actions && actions.length ? h('div', { class: 'mb-actions row gap-2' }, actions) : null
    ]);
  };

  MUI.fab = function (o) {
    return h('button', {
      class: 'fab', type: 'button', 'aria-label': o.label, title: o.label, onclick: o.onClick
    }, [VS.iconEl(o.icon || 'plus', 24)]);
  };

  MUI.haptic = function () {
    try { if (navigator.vibrate) navigator.vibrate(8); } catch (e) { /* 原型：忽略 */ }
  };

  /* ======================= 4. 底部面板 ============================== */
  /**
   * sheet({title, sub, body, footer, size, dismissible, onClose})
   * footer 既可是按钮数组，也可传 {primary, secondary} 便捷形式
   */
  MUI.sheet = function (o) {
    o = o || {};
    var backdrop = h('div', { class: 'msheet-backdrop' });
    var sheet = h('div', { class: 'msheet', dataset: { size: o.size || '' }, role: 'dialog', 'aria-modal': 'true' });
    var layer;

    function close() {
      MUI.popLayer(layer);
      if (o.onClose) o.onClose();
    }
    sheet.appendChild(h('div', { class: 'msheet-grip' }, [h('i')]));
    if (o.title || o.sub || o.dismissible !== false) {
      sheet.appendChild(h('div', { class: 'msheet-head' }, [
        h('div', { class: 'grow' }, [
          h('div', { class: 'msheet-title', text: o.title || '' }),
          o.sub ? h('div', { class: 'msheet-sub', text: o.sub }) : null
        ]),
        o.dismissible === false ? null : UI.iconBtn({ icon: 'x', label: '关闭', onClick: close })
      ]));
    }
    var bodyEl = typeof o.body === 'function' ? o.body() : o.body;
    if (bodyEl) sheet.appendChild(h('div', { class: 'msheet-body' }, bodyEl));
    if (o.footer) {
      sheet.appendChild(h('div', { class: 'msheet-foot' },
        typeof o.footer === 'function' ? o.footer(close) : o.footer));
    }
    backdrop.appendChild(sheet);
    if (o.dismissible !== false) {
      backdrop.addEventListener('click', function (e) { if (e.target === backdrop) close(); });
    }
    layer = MUI.pushLayer(backdrop, o.dismissible === false ? function () { } : close);

    var first = sheet.querySelector('input,textarea');
    if (first && o.autofocus !== false) setTimeout(function () { first.focus(); }, 60);
    return { el: sheet, close: close };
  };

  /** 危险确认（底部面板 + 可要求输入确认短语） */
  MUI.confirmSheet = function (o) {
    o = o || {};
    var input = o.phrase ? UI.input({ placeholder: o.phrase, mono: true }) : null;
    var body = [o.body || null];
    if (input) body.push(h('div', { class: 'mt-3' }, [UI.field({ label: '请输入「' + o.phrase + '」以继续', control: input, hint: o.hint })]));
    var m = MUI.sheet({
      title: o.title, sub: o.sub, body: body, dismissible: o.dismissible !== false,
      footer: function (close) {
        var ok = h('button', {
          class: 'mbtn-block', type: 'button',
          dataset: { variant: o.tone === 'danger' ? 'danger' : 'primary' },
          disabled: !!o.phrase,
          onclick: function () { close(); o.onConfirm && o.onConfirm(); }
        }, [o.confirmIcon ? h('span', { html: icon(o.confirmIcon, 18) }) : null, h('span', { text: o.confirmLabel || '确认' })]);
        if (input) {
          input.addEventListener('input', function () { ok.disabled = input.value.trim() !== o.phrase; });
        }
        return [
          ok,
          h('button', { class: 'mbtn-block', type: 'button', dataset: { variant: 'ghost' }, onclick: function () { close(); o.onCancel && o.onCancel(); } },
            [h('span', { text: o.cancelLabel || '取消' })])
        ];
      }
    });
    return m;
  };

  /** 动作面板（长按 / 溢出菜单）：items = [{label, sub, icon, danger, disabled, reason, onClick}] */
  MUI.actionSheet = function (o) {
    o = o || {};
    var body = h('div');
    (o.items || []).forEach(function (it) {
      if (!it) return;
      if (it.group) { body.appendChild(h('div', { class: 'mgroup-title', text: it.group })); return; }
      body.appendChild(h('button', {
        class: 'as-item', type: 'button',
        dataset: { danger: it.danger ? 'true' : '' },
        'aria-disabled': it.disabled ? 'true' : null,
        /* 无障碍：禁用原因必须对读屏可见（不能只靠角标与点击后的 toast） */
        title: it.disabled && it.reason ? (it.label + '：不可用（' + it.reason + '）') : (it.sub || it.label || ''),
        'aria-label': it.disabled && it.reason ? (it.label + '，不可用：' + it.reason) : (it.label || ''),
        onclick: function () {
          if (it.disabled) { if (it.reason) UI.toast({ tone: 'warn', title: it.label, msg: it.reason }); return; }
          m.close();
          it.onClick && it.onClick();
        }
      }, [
        it.icon ? h('span', { html: icon(it.icon, 20) }) : null,
        h('div', { class: 'grow' }, [
          h('span', { text: it.label }),
          it.sub ? h('span', { class: 'as-sub', text: it.sub }) : null
        ]),
        it.disabled && it.reason ? h('span', { class: 't-caption', text: '不可用' }) : null
      ]));
    });
    var m = MUI.sheet({
      title: o.title || null, sub: o.sub || null, body: body,
      footer: [h('button', { class: 'mbtn-block', type: 'button', dataset: { variant: 'ghost' }, onclick: function () { m.close(); } },
        [h('span', { text: '取消' })])]
    });
    m.el.classList.add('actionsheet');
    return m;
  };

  /* ======================= 5. 全屏层 ================================ */
  MUI.mfullscreen = function (o) {
    o = o || {};
    var el = h('div', { class: 'mfullscreen', dataset: { tone: o.tone || '' }, role: 'alertdialog', 'aria-modal': 'true' });
    if (o.content) el.appendChild(o.content);
    else {
      el.appendChild(h('span', { style: { color: o.tone === 'danger' ? 'var(--c-danger)' : 'var(--c-accent)' }, html: icon(o.icon || 'danger', 52) }));
      el.appendChild(h('div', { class: 't-h1', text: o.title || '' }));
      if (o.lines) el.appendChild(h('div', { class: 'col gap-2' }, o.lines.map(function (l) {
        return h('div', { class: 't-body', text: l });
      })));
      if (o.source) el.appendChild(h('div', { class: 't-caption t-mono', text: o.source }));
    }
    if (o.actions) el.appendChild(h('div', { class: 'col gap-2', style: { width: '100%' } }, o.actions));
    var layer = MUI.pushLayer(el, o.dismissible === false ? function () { } : null);
    return { el: el, close: function () { MUI.popLayer(layer); } };
  };

  /* ======================= 6. 分页 / 步骤条（移动端横向可滚）========= */
  MUI.steps = function (o) {
    return h('div', { class: 'mtabs-scroll' }, (o.items || []).map(function (it, i) {
      var state = it.state || (i < o.current ? 'done' : i === o.current ? 'active' : 'todo');
      var tone = state === 'done' ? 'var(--c-success)' : state === 'active' ? 'var(--c-accent)' : 'var(--c-text-2)';
      return h('div', {
        class: 'chip', style: { borderColor: state === 'todo' ? '' : tone, color: tone, cursor: 'default' }
      }, [
        state === 'done' ? h('span', { html: icon('check', 12) }) : h('span', { text: String(i + 1) }),
        h('span', { text: it.label })
      ]);
    }));
  };

  /* ======================= 7. 便捷 ================================== */
  MUI.muted = UI.muted;
  MUI.empty = UI.empty;
  MUI.toast = UI.toast;
  MUI.badge = UI.badge;
  MUI.chip = UI.chip;
  MUI.bar = UI.bar;
  MUI.ring = UI.ring;
  MUI.select = UI.select;
  MUI.input = UI.input;
  MUI.field = UI.field;
  MUI.checkbox = UI.checkbox;
  MUI.switchCtl = UI.switchCtl;
  MUI.radioCards = UI.radioCards;
  MUI.eraseBadge = UI.eraseBadge;
  MUI.pathBadge = UI.pathBadge;
  MUI.tierMark = UI.tierMark;
  MUI.errorBox = UI.errorBox;
  MUI.codeBlock = UI.codeBlock;
  MUI.copyField = UI.copyField;
  MUI.gauge = UI.gauge;
  MUI.timeline = UI.timeline;
  MUI.table = UI.table;
  MUI.qrPlaceholder = UI.qrPlaceholder;
  MUI.chunkMatrix = UI.chunkMatrix;

})(window);
