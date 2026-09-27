/* ============================================================================
 * VaultSync V2.0 原型 — UI 原语库
 * 所有弹层一律「居中弹窗（modal）」，不使用滑入抽屉（仓库约定）
 * 依赖：core.js（VS.util / VS.icon / VS.fmt / VS.store）
 * ==========================================================================*/
(function (global) {
  'use strict';

  var VS = global.VS || (global.VS = {});
  var h = VS.util.h, icon = VS.icon, F = VS.fmt;

  var UI = VS.ui = {};

  /* ======================= 0. 层管理 ================================= */
  var layerSeq = 0;
  var openLayers = [];   /* {el, kind, onEsc, dismissible} */

  function pushLayer(el, kind, opts) {
    opts = opts || {};
    var layer = { el: el, kind: kind, onEsc: opts.onEsc, dismissible: opts.dismissible !== false };
    layer.id = ++layerSeq;
    openLayers.push(layer);
    return layer;
  }
  function popLayer(layer) {
    openLayers = openLayers.filter(function (l) { return l !== layer; });
  }
  UI.layerCount = function () { return openLayers.length; };
  UI.closeTop = function () {
    var layer = openLayers[openLayers.length - 1];
    if (!layer) return false;
    if (!layer.dismissible) return true;   /* 吞掉，不允许关闭 */
    if (typeof layer.onEsc === 'function') layer.onEsc();
    else layer.el.remove();
    popLayer(layer);
    return true;
  };
  UI.closeAll = function () {
    openLayers.slice().forEach(function (l) { if (l.dismissible) { l.el.remove(); popLayer(l); } });
  };

  /* ======================= 1. 基础视觉件 ============================== */
  UI.icon = function (name, size, cls) {
    var span = document.createElement('span');
    span.className = 'ico-wrap';
    span.innerHTML = icon(name, size, cls);
    return span.firstChild;
  };

  UI.btn = function (o) {
    o = o || {};
    var cls = 'btn' + (o.variant ? ' btn-' + o.variant : '') + (o.size ? ' btn-' + o.size : '') + (o.block ? ' btn-block' : '');
    var el = h('button', {
      class: cls,
      type: 'button',
      title: o.title || '',
      disabled: !!o.disabled,
      'aria-disabled': o.disabled ? 'true' : null,
      onclick: o.disabled ? null : (o.onClick || null)
    }, [
      o.icon ? UI.icon(o.icon, o.size === 'sm' ? 14 : 16) : null,
      o.label ? h('span', { text: o.label }) : null
    ]);
    if (o.attrs) Object.keys(o.attrs).forEach(function (k) { el.setAttribute(k, o.attrs[k]); });
    return el;
  };

  UI.iconBtn = function (o) {
    o = o || {};
    var cls = 'btn btn-icon' + (o.variant ? ' btn-' + o.variant : ' btn-ghost') + (o.size ? ' btn-' + o.size : '');
    var el = h('button', {
      class: cls, type: 'button',
      title: o.title || o.label || '',
      'aria-label': o.label || o.title || '',
      disabled: !!o.disabled,
      onclick: o.disabled ? null : (o.onClick || null)
    }, [UI.icon(o.icon, o.size === 'sm' ? 14 : 16)]);
    return el;
  };

  UI.badge = function (o) {
    o = typeof o === 'string' ? { text: o } : (o || {});
    var cls = 'badge' + (o.tone ? ' badge-' + o.tone : '') + (o.design ? ' badge-design' : '') + (o.outline ? ' badge-outline' : '');
    return h('span', { class: cls, title: o.title || '' }, [
      o.icon ? UI.icon(o.icon, 12) : null,
      o.dot ? h('i', { class: 'dot' }) : null,
      h('span', { text: o.text })
    ]);
  };

  UI.chip = function (o) {
    o = typeof o === 'string' ? { text: o } : (o || {});
    return h('span', {
      class: 'chip' + (o.tone ? ' chip-' + o.tone : ''),
      dataset: { removable: o.onRemove ? 'true' : 'false' },
      title: o.title || '',
      onclick: o.onRemove || null
    }, [
      h('span', { text: o.text }),
      o.onRemove ? UI.icon('x', 11, 'chip-x') : null
    ]);
  };

  /** 擦除强度徽标（05-02 §三.4 / §四.3：必须显示引擎原值） */
  var ERASE_CLASS = {
    1: { key: 'crypto_only',     label: '加密擦除',  note: '已加密擦除；介质覆写未执行' },
    2: { key: 'zero_overwrite',  label: '零覆写',    note: '已执行单轮零覆写' },
    3: { key: 'platform_secure', label: '安全擦除',  note: '平台安全删除指令已执行' },
    4: { key: 'unknown',         label: '无法判定',  note: '无法判定介质类型；介质覆写未执行' }
  };
  UI.ERASE_CLASS = ERASE_CLASS;
  UI.eraseBadge = function (eraseClass, opts) {
    opts = opts || {};
    var meta = ERASE_CLASS[eraseClass] || ERASE_CLASS[4];
    return h('span', {
      class: 'erase-badge', dataset: { class: meta.key },
      title: meta.note + (opts.title ? ' · ' + opts.title : '')
    }, [UI.icon('erase', 12), h('span', { text: meta.label })]);
  };

  /** 路径徽标：直连 / 打洞 / 中继 */
  var PATHS = {
    direct:     { label: '直连',   icon: 'link' },
    hole_punch: { label: '打洞',   icon: 'discover' },
    relay:      { label: '中继',   icon: 'relay' }
  };
  UI.pathBadge = function (path, reason) {
    var meta = PATHS[path] || PATHS.direct;
    return h('span', {
      class: 'path-badge', dataset: { path: path },
      title: reason ? '降级原因：' + reason : meta.label
    }, [UI.icon(meta.icon, 12), h('span', { text: meta.label })]);
  };

  UI.tierMark = function (tier, label) {
    var map = { real: ['check-circle', '真值'], design: ['edit', '设计保证'], hidden: ['ban', '不渲染'] };
    var m = map[tier] || map.design;
    return h('span', { class: 'tier-mark', dataset: { tier: tier }, title: '能力三档：' + m[1] },
      [UI.icon(m[0], 12), h('span', { text: label || m[1] })]);
  };

  /* ======================= 2. 表单控件 =============================== */
  UI.field = function (o) {
    o = o || {};
    return h('div', { class: 'field' }, [
      o.label ? h('label', { class: 'field-label', for: o.for || null }, [
        h('span', { text: o.label }), o.req ? h('span', { class: 'req', text: '*' }) : null
      ]) : null,
      o.control,
      o.hint ? h('div', { class: 'field-hint', text: o.hint }) : null,
      o.error ? h('div', { class: 'field-error' }, [UI.icon('alert', 12), h('span', { text: o.error })]) : null
    ]);
  };

  UI.input = function (o) {
    o = o || {};
    var el = h('input', {
      class: 'input' + (o.mono ? ' input-mono' : '') + (o.size === 'lg' ? ' input-lg' : ''),
      type: o.type || 'text',
      value: o.value === undefined || o.value === null ? '' : o.value,
      placeholder: o.placeholder || '',
      id: o.id || null,
      maxlength: o.maxLength || null,
      autocomplete: 'off',
      'aria-invalid': o.invalid ? 'true' : null,
      oninput: o.onInput || null,
      onkeydown: o.onKeyDown || null
    });
    if (o.disabled) el.disabled = true;
    return el;
  };

  UI.textarea = function (o) {
    o = o || {};
    var el = h('textarea', {
      class: 'textarea', placeholder: o.placeholder || '', rows: o.rows || 3,
      oninput: o.onInput || null
    });
    el.value = o.value || '';
    return el;
  };

  UI.searchBox = function (o) {
    o = o || {};
    var input = UI.input({ value: o.value, placeholder: o.placeholder || '搜索…', onInput: o.onInput, onKeyDown: o.onKeyDown });
    var clear = UI.iconBtn({ icon: 'x', size: 'sm', label: '清除', onClick: function () { input.value = ''; if (o.onInput) o.onInput({ target: input }); input.focus(); } });
    clear.classList.add('search-clear');
    return h('div', { class: 'search', style: o.style || null }, [
      h('span', { class: 'search-ico', html: icon('search', 15) }),
      input, clear
    ]);
  };

  /** 自定义下拉选择器：options = [{value,label,sub,group,disabled}] */
  UI.select = function (o) {
    o = o || {};
    var options = o.options || [];
    var current = o.value;
    var disabled = !!o.disabled;
    var wrap = h('div', { class: 'select' + (o.block ? ' select-block' : '') });
    var btn, pop = null, layer = null;

    function findLabel(v) {
      for (var i = 0; i < options.length; i++) if (String(options[i].value) === String(v)) return options[i];
      return null;
    }
    function close() {
      if (pop) { pop.remove(); pop = null; }
      if (layer) { popLayer(layer); layer = null; }
      wrap.dataset.open = 'false';
      document.removeEventListener('mousedown', onDocDown, true);
    }
    function onDocDown(e) { if (!wrap.contains(e.target)) close(); }
    function open() {
      if (pop || disabled) return;
      pop = h('div', { class: 'select-pop', role: 'listbox' });
      options.forEach(function (opt) {
        if (opt.group) { pop.appendChild(h('div', { class: 'select-group', text: opt.group })); return; }
        pop.appendChild(h('div', {
          class: 'select-opt', role: 'option',
          'aria-selected': String(opt.value) === String(current) ? 'true' : 'false',
          'aria-disabled': opt.disabled ? 'true' : null,
          dataset: { value: opt.value },
          onclick: function () {
            if (opt.disabled) return;
            current = opt.value;
            renderBtn();
            close();
            if (o.onChange) o.onChange(opt.value, opt);
          }
        }, [
          h('div', { class: 'grow' }, [
            h('div', { text: opt.label }),
            opt.sub ? h('span', { class: 'opt-sub', text: opt.sub }) : null
          ])
        ]));
      });
      wrap.appendChild(pop);
      wrap.dataset.open = 'true';
      setTimeout(function () { document.addEventListener('mousedown', onDocDown, true); }, 0);
      layer = pushLayer(pop, 'select', { onEsc: close });
    }
    function renderBtn() {
      var meta = findLabel(current);
      var label = meta ? meta.label : (o.placeholder || '请选择');
      var content = [
        h('span', { class: 'select-value' + (meta ? '' : ' t-muted'), text: label }),
        h('span', { class: 'caret', html: icon('chevron-down', 14) })
      ];
      if (btn) { btn.innerHTML = ''; content.forEach(function (c) { btn.appendChild(c); }); }
      else {
        btn = h('button', {
          class: 'select-btn', type: 'button', role: 'combobox',
          'aria-disabled': disabled ? 'true' : null,
          onclick: function () { pop ? close() : open(); }
        }, content);
        wrap.appendChild(btn);
      }
    }
    renderBtn();
    wrap.getValue = function () { return current; };
    wrap.setValue = function (v) { current = v; renderBtn(); };
    return wrap;
  };

  UI.switchCtl = function (o) {
    o = o || {};
    var input = h('input', { type: 'checkbox', onchange: o.onChange || null });
    input.checked = !!o.checked;
    if (o.disabled) input.disabled = true;
    var label = h('label', { class: 'switch' }, [
      input,
      h('span', { class: 'track' }),
      (o.label || o.sub) ? h('span', { class: 'check-text' }, [
        o.label ? h('span', { text: o.label }) : null,
        o.sub ? h('span', { class: 'switch-sub', text: o.sub }) : null
      ]) : null
    ]);
    label.input = input;
    return label;
  };

  UI.checkbox = function (o) {
    o = o || {};
    var input = h('input', { type: 'checkbox', onchange: o.onChange || null });
    input.checked = !!o.checked;
    if (o.disabled) input.disabled = true;
    var el = h('label', { class: 'check' }, [
      input,
      h('span', { class: 'box' + (o.radio ? ' radio' : '') }),
      h('span', { class: 'check-text' }, [
        h('span', { text: o.label || '' }),
        o.sub ? h('span', { class: 'check-sub', text: o.sub }) : null
      ])
    ]);
    el.input = input;
    return el;
  };

  /** 单选卡片组：options = [{value,title,desc}] */
  UI.radioCards = function (o) {
    o = o || {};
    var wrap = h('div', { class: 'col gap-2', role: 'radiogroup' });
    var opts = o.options || [];
    function render() {
      wrap.innerHTML = '';
      opts.forEach(function (opt) {
        wrap.appendChild(h('div', {
          class: 'radio-card', role: 'radio', tabindex: '0',
          'aria-checked': String(opt.value) === String(o.value) ? 'true' : 'false',
          onclick: function () { if (opt.disabled) return; o.value = opt.value; render(); if (o.onChange) o.onChange(opt.value, opt); },
          onkeydown: function (e) { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); if (!opt.disabled) { o.value = opt.value; render(); if (o.onChange) o.onChange(opt.value, opt); } } }
        }, [
          h('span', { class: 'rc-mark' }),
          h('div', { class: 'grow' }, [
            h('div', { class: 'rc-title', text: opt.title }),
            opt.desc ? h('div', { class: 'rc-desc', text: opt.desc }) : null,
            opt.extra || null
          ])
        ]));
      });
    }
    render();
    return wrap;
  };

  UI.slider = function (o) {
    o = o || {};
    var el = h('input', {
      class: 'slider', type: 'range',
      min: o.min === undefined ? 0 : o.min, max: o.max === undefined ? 100 : o.max,
      step: o.step || 1, value: o.value, oninput: o.onInput || null
    });
    return el;
  };

  UI.seg = function (o) {
    o = o || {};
    var wrap = h('div', { class: 'seg', role: 'tablist' });
    (o.items || []).forEach(function (it) {
      wrap.appendChild(h('button', {
        class: 'seg-item', type: 'button', role: 'tab',
        'aria-selected': it.value === o.value ? 'true' : 'false',
        disabled: !!it.disabled,
        title: it.title || '',
        onclick: function () { if (!it.disabled && o.onChange) o.onChange(it.value, it); }
      }, [it.icon ? UI.icon(it.icon, 14) : null, h('span', { text: it.label })]));
    });
    return wrap;
  };

  UI.tabs = function (o) {
    o = o || {};
    var wrap = h('div', { class: 'tabs', role: 'tablist' });
    (o.items || []).forEach(function (it) {
      wrap.appendChild(h('button', {
        class: 'tab', type: 'button', role: 'tab',
        'aria-selected': it.value === o.value ? 'true' : 'false',
        onclick: function () { if (o.onChange) o.onChange(it.value, it); }
      }, [
        it.icon ? UI.icon(it.icon, 14) : null,
        h('span', { text: it.label }),
        it.count !== undefined && it.count !== null ? h('span', { class: 'tab-count', text: F.num(it.count) }) : null
      ]));
    });
    return wrap;
  };

  UI.steps = function (o) {
    o = o || {};
    var wrap = h('div', { class: 'steps' });
    (o.items || []).forEach(function (it, i, arr) {
      wrap.appendChild(h('div', { class: 'step', dataset: { state: it.state || 'todo' } }, [
        h('span', { class: 'step-dot' }, [it.state === 'done' ? UI.icon('check', 12) : h('span', { text: String(i + 1) })]),
        h('span', { class: 'step-label', text: it.label }),
        i < arr.length - 1 ? h('span', { class: 'step-bar' }) : null
      ]));
    });
    return wrap;
  };

  /* ======================= 3. 数据展示 =============================== */
  UI.bar = function (pct, tone, size) {
    return h('div', { class: 'bar' + (tone ? ' bar-' + tone : '') + (size ? ' bar-' + size : ''), role: 'progressbar',
      'aria-valuenow': Math.round((pct || 0) * 100), 'aria-valuemin': 0, 'aria-valuemax': 100 }, [
      h('i', { style: { width: (Math.round((pct || 0) * 1000) / 10) + '%' } })
    ]);
  };

  UI.ring = function (o) {
    o = o || {};
    var size = o.size || 64, stroke = o.stroke || 6, r = (size - stroke) / 2;
    var c = 2 * Math.PI * r, pct = o.pct === null || o.pct === undefined ? 0 : o.pct;
    var label = o.label === undefined ? (o.pct === null || o.pct === undefined ? '—' : F.pct(pct)) : o.label;
    var svg = h('div', { class: 'ring', dataset: { tone: o.tone || 'default' }, style: { width: size + 'px', height: size + 'px' } });
    svg.innerHTML =
      '<svg width="' + size + '" height="' + size + '">' +
      '<circle class="ring-track" cx="' + size / 2 + '" cy="' + size / 2 + '" r="' + r + '" fill="none" stroke-width="' + stroke + '"/>' +
      '<circle class="ring-fill" cx="' + size / 2 + '" cy="' + size / 2 + '" r="' + r + '" fill="none" stroke-width="' + stroke + '" ' +
      'stroke-dasharray="' + c.toFixed(2) + '" stroke-dashoffset="' + (c * (1 - pct)).toFixed(2) + '"/></svg>' +
      '<span class="ring-label">' + VS.util.esc(label) + '</span>';
    return svg;
  };

  UI.empty = function (o) {
    o = o || {};
    return h('div', { class: 'empty' }, [
      h('span', { class: 'empty-ico', html: icon(o.icon || 'file', 44) }),
      o.title ? h('div', { class: 'empty-title', text: o.title }) : null,
      o.desc ? h('div', { class: 'empty-desc', text: o.desc }) : null,
      o.actions ? h('div', { class: 'row gap-2', style: { marginTop: '4px' } }, o.actions) : null
    ]);
  };

  UI.kv = function (pairs) {
    var dl = h('dl', { class: 'kv' });
    (pairs || []).forEach(function (p) {
      if (!p) return;
      dl.appendChild(h('dt', { text: p.k }));
      var dd = h('dd', { class: p.mono ? 'mono' : null });
      if (p.node) dd.appendChild(p.node);
      else dd.textContent = p.v === null || p.v === undefined || p.v === '' ? '—' : String(p.v);
      if (p.copy && p.v) dd.appendChild(UI.iconBtn({ icon: 'copy', size: 'sm', label: '复制', onClick: function () { UI.copy(String(p.v)); } }));
      dl.appendChild(dd);
    });
    return dl;
  };

  UI.card = function (o) {
    o = o || {};
    return h('section', { class: 'card' + (o.class ? ' ' + o.class : '') }, [
      (o.title || o.actions) ? h('header', { class: 'card-head' }, [
        h('div', { class: 'grow' }, [
          h('div', { class: 't-h2', text: o.title || '' }),
          o.sub ? h('div', { class: 't-caption', text: o.sub }) : null
        ]),
        o.actions ? h('div', { class: 'row gap-2' }, o.actions) : null
      ]) : null,
      h('div', { class: o.flush ? null : 'card-body' }, o.body)
    ]);
  };

  /** columns: [{key,label,width,align,sortable,render(row)}] */
  UI.table = function (o) {
    o = o || {};
    if (!o.rows || !o.rows.length) return o.empty || UI.empty({ icon: 'list', title: '暂无数据' });
    var thead = h('thead', {}, [h('tr', {}, (o.columns || []).map(function (c) {
      return h('th', {
        style: c.width ? { width: c.width } : null,
        dataset: { sortable: c.sortable ? 'true' : 'false' },
        'aria-sort': o.sort && o.sort.key === c.key ? (o.sort.dir === 'asc' ? 'ascending' : 'descending') : null,
        onclick: c.sortable && o.onSort ? function () { o.onSort(c.key); } : null
      }, [c.label, c.sortable ? h('span', { class: 'sort-ind', text: o.sort && o.sort.key === c.key ? (o.sort.dir === 'asc' ? '▲' : '▼') : '⇅' }) : null]);
    }))]);
    var tbody = h('tbody', {}, o.rows.map(function (row, i) {
      return h('tr', {
        dataset: { selected: o.selectedId !== undefined && o.selectedId === (row.id !== undefined ? row.id : i) ? 'true' : 'false' },
        onclick: o.onRowClick ? function (e) { o.onRowClick(row, e); } : null,
        oncontextmenu: o.onRowContext ? function (e) { e.preventDefault(); o.onRowContext(row, e); } : null
      }, (o.columns || []).map(function (c) {
        var td = h('td', { class: (c.align === 'right' ? 'num ' : '') + (c.class || '') });
        if (c.render) { var r = c.render(row); if (r instanceof Node) td.appendChild(r); else td.innerHTML = r === null || r === undefined ? '' : String(r); }
        else td.textContent = row[c.key] === null || row[c.key] === undefined ? '—' : String(row[c.key]);
        return td;
      }));
    }));
    return h('div', { class: 'table-wrap', style: o.maxHeight ? { maxHeight: o.maxHeight } : null }, [h('table', { class: 'data' }, [thead, tbody])]);
  };

  UI.timeline = function (items) {
    return h('div', { class: 'timeline' }, (items || []).map(function (it) {
      return h('div', { class: 'timeline-item', dataset: { tone: it.tone || '' } }, [
        h('div', { class: 'row-between' }, [
          h('div', { class: 't-strong', text: it.title }),
          it.time ? h('div', { class: 't-caption', text: it.time }) : null
        ]),
        it.desc ? h('div', { class: 't-caption', text: it.desc }) : null,
        it.body || null
      ]);
    }));
  };

  UI.alertbar = function (o) {
    o = o || {};
    return h('div', { class: 'alertbar', dataset: { tone: o.tone || 'warn' }, role: 'alert' }, [
      UI.icon(o.icon || (o.tone === 'danger' ? 'danger' : o.tone === 'info' ? 'info' : 'alert'), 16),
      h('div', { class: 'grow' }, [
        o.title ? h('div', { class: 't-strong', text: o.title }) : null,
        o.text ? h('div', { class: o.title ? 't-caption' : null, text: o.text }) : null
      ]),
      o.actions ? h('div', { class: 'alertbar-actions' }, o.actions) : null,
      o.onClose ? UI.iconBtn({ icon: 'x', size: 'sm', label: '关闭', onClick: o.onClose }) : null
    ]);
  };

  /** MaintenanceBanner — 三态 maintenance / migrate / stale（§5.6） */
  UI.maintBanner = function (o) {
    o = o || {};
    return h('div', { class: 'maint-banner', dataset: { tone: o.tone || 'maintenance' }, role: 'status' }, [
      h('span', { class: 'mb-ico', html: icon(o.icon || (o.tone === 'stale' ? 'lock' : 'refresh'), 18) }),
      h('div', { class: 'mb-text' }, [
        h('div', { class: 'mb-title', text: o.title }),
        o.sub ? h('div', { class: 'mb-sub', text: o.sub }) : null,
        o.progress !== undefined && o.progress !== null ? h('div', { style: { marginTop: '6px', maxWidth: '420px' } }, [UI.bar(o.progress, 'maintenance', 'sm')]) : null
      ]),
      o.actions ? h('div', { class: 'mb-actions' }, o.actions) : null
    ]);
  };

  UI.codeBlock = function (text, opts) {
    return h('div', { class: 'codeblock' }, [
      h('span', { text: text }),
      h('button', {
        class: 'btn btn-icon btn-sm btn-ghost copy-btn', type: 'button', title: '复制',
        onclick: function () { UI.copy(text); }
      }, [UI.icon('copy', 13)])
    ]);
  };

  UI.copyField = function (value, label) {
    return h('div', { class: 'copyfield' }, [
      h('span', { class: 'copyfield-val', text: value }),
      UI.iconBtn({ icon: 'copy', size: 'sm', label: label || '复制', onClick: function () { UI.copy(value); } })
    ]);
  };

  UI.copy = function (text) {
    try {
      if (navigator.clipboard && navigator.clipboard.writeText) navigator.clipboard.writeText(text);
      else {
        var ta = document.createElement('textarea');
        ta.value = text; document.body.appendChild(ta); ta.select();
        document.execCommand('copy'); ta.remove();
      }
      UI.toast({ tone: 'info', title: '已复制', msg: String(text).slice(0, 48) + (String(text).length > 48 ? '…' : ''), duration: 1800 });
    } catch (e) { UI.toast({ tone: 'warn', title: '复制失败', msg: '浏览器拒绝了剪贴板访问' }); }
  };

  UI.chunkMatrix = function (chunks) {
    var wrap = h('div', { class: 'chunk-matrix' });
    (chunks || []).forEach(function (st) {
      wrap.appendChild(h('span', { class: 'chunk', dataset: { s: st }, title: st }));
    });
    return wrap;
  };

  UI.gauge = function (o) {
    o = o || {};
    return h('div', { class: 'gauge', dataset: { state: o.state || 'unknown' } }, [
      h('div', { class: 'g-head' }, [
        h('span', { class: 'g-ico', html: icon(o.icon || 'shield', 16) }),
        h('span', { class: 'g-name', text: o.name }),
        h('span', { class: 'g-state' }, [o.badge])
      ]),
      o.note ? h('div', { class: 't-caption', text: o.note }) : null,
      h('div', { class: 'g-src', text: o.src || '' })
    ]);
  };

  UI.qrPlaceholder = function (seed) {
    /* 纯几何伪二维码（原型用，不含真实数据） */
    var rnd = VS.util.seeded(seed || 42), cells = [];
    for (var y = 0; y < 21; y++) for (var x = 0; x < 21; x++) {
      var corner = (x < 7 && y < 7) || (x > 13 && y < 7) || (x < 7 && y > 13);
      var on = corner ? ((x % 6 === 0 || y % 6 === 0 || (x > 1 && x < 5 && y > 1 && y < 5)) ? 1 : 0) : (rnd() > 0.5 ? 1 : 0);
      if (on) cells.push('<rect x="' + (x * 8) + '" y="' + (y * 8) + '" width="8" height="8"/>');
    }
    var wrap = h('div', { class: 'qr' });
    wrap.innerHTML = '<svg viewBox="0 0 168 168" fill="currentColor" style="color:#10131A">' + cells.join('') + '</svg>';
    return wrap;
  };

  /* ======================= 4. 弹窗 =================================== */
  /** body 可为 Node / 数组 / 函数(返回 Node)；footer 为按钮数组 */
  UI.modal = function (o) {
    o = o || {};
    var bodyEl = typeof o.body === 'function' ? o.body() : o.body;
    var footEl = typeof o.footer === 'function' ? o.footer() : o.footer;
    var modal = h('div', { class: 'modal', dataset: { size: o.size || 'md', tone: o.tone || '' }, role: 'dialog', 'aria-modal': 'true', 'aria-label': o.title || '' });
    var overlay = h('div', { class: 'overlay', dataset: { variant: o.variant || 'dim' } }, [modal]);

    function close() {
      overlay.remove(); popLayer(layer);
      if (o.onClose) o.onClose();
    }
    modal.appendChild(h('header', { class: 'modal-head' }, [
      h('div', { class: 'modal-title-wrap' }, [
        h('div', { class: 'modal-title', text: o.title || '' }),
        o.sub ? h('div', { class: 'modal-sub', text: o.sub }) : null
      ]),
      o.dismissible === false ? null : UI.iconBtn({ icon: 'x', label: '关闭', onClick: close })
    ]));
    if (bodyEl) modal.appendChild(h('div', { class: 'modal-body' + (o.flush ? ' flush' : '') }, bodyEl));
    if (footEl) modal.appendChild(h('footer', { class: 'modal-foot' }, footEl));
    if (o.dismissible !== false) overlay.addEventListener('mousedown', function (e) { if (e.target === overlay) close(); });

    document.body.appendChild(overlay);
    var layer = pushLayer(overlay, 'modal', { dismissible: o.dismissible !== false, onEsc: close });

    var first = modal.querySelector('input,textarea,button.btn-primary,button.btn-accent,button');
    if (o.autofocus !== false && first) setTimeout(function () { first.focus(); }, 30);

    return { el: modal, overlay: overlay, close: close };
  };

  /** 确认框（危险操作统一走这里） */
  UI.confirm = function (o) {
    o = o || {};
    return UI.modal({
      title: o.title || '请确认',
      sub: o.sub,
      size: o.size || 'sm',
      tone: o.tone,
      body: o.body,
      autofocus: false,
      footer: [
        UI.btn({ label: o.cancelLabel || '取消', onClick: function () { m.close(); if (o.onCancel) o.onCancel(); } }),
        UI.btn({
          label: o.confirmLabel || '确认', variant: o.confirmVariant || (o.tone === 'danger' ? 'danger' : 'primary'),
          disabled: o.confirmDisabled,
          onClick: function () { m.close(); if (o.onConfirm) o.onConfirm(); }
        })
      ]
    });
  };
  /* confirm 内部需要 m 的引用，改写为闭包形式 */
  UI.confirm = function (o) {
    o = o || {};
    var m = UI.modal({
      title: o.title || '请确认', sub: o.sub, size: o.size || 'sm', tone: o.tone,
      body: o.body, autofocus: false,
      footer: function () {
        return [
          UI.btn({ label: o.cancelLabel || '取消', onClick: function () { m.close(); if (o.onCancel) o.onCancel(); } }),
          UI.btn({
            label: o.confirmLabel || '确认',
            variant: o.confirmVariant || (o.tone === 'danger' ? 'danger' : 'primary'),
            disabled: o.confirmDisabled,
            onClick: function () { m.close(); if (o.onConfirm) o.onConfirm(); }
          })
        ];
      }
    });
    return m;
  };

  /** 需要输入确认短语的高危操作 */
  UI.confirmPhrase = function (o) {
    o = o || {};
    var input = UI.input({ placeholder: o.phrase, mono: true });
    var btnHolder = h('div');
    var m = UI.modal({
      title: o.title, sub: o.sub, size: 'sm', tone: o.tone || 'danger',
      body: [
        o.body || null,
        UI.field({ label: '请输入确认短语「' + o.phrase + '」以继续', control: input, hint: o.hint })
      ],
      autofocus: false,
      footer: function () {
        var confirmBtn = UI.btn({ label: o.confirmLabel || '确认', variant: 'danger', disabled: true, onClick: function () { m.close(); if (o.onConfirm) o.onConfirm(); } });
        input.addEventListener('input', function () { confirmBtn.disabled = input.value.trim() !== o.phrase; });
        return [UI.btn({ label: '取消', onClick: function () { m.close(); } }), confirmBtn];
      }
    });
    setTimeout(function () { input.focus(); }, 40);
    return m;
  };

  /** 全屏阻塞页（紧急销毁 / 全屏强告警 /alarm） */
  UI.fullscreen = function (o) {
    o = o || {};
    var el = h('div', { class: 'fullscreen-blocker', dataset: { tone: o.tone || '' }, role: 'alertdialog', 'aria-modal': 'true' });
    var inner = h('div', { class: 'col gap-4', style: { width: 'min(620px, 92vw)', textAlign: 'center', alignItems: 'center' } });
    if (o.content) inner.appendChild(o.content);
    else {
      inner.appendChild(h('span', { style: { color: o.tone === 'danger' ? 'var(--c-danger)' : 'var(--c-accent)' }, html: icon(o.icon || 'danger', 56) }));
      inner.appendChild(h('div', { class: 't-display', text: o.title || '' }));
      if (o.lines) inner.appendChild(h('div', { class: 'col gap-2', style: { alignItems: 'center' } },
        o.lines.map(function (l) { return h('div', { class: 't-body', text: l }); })));
      if (o.source) inner.appendChild(h('div', { class: 't-caption t-mono', text: o.source }));
    }
    if (o.actions) inner.appendChild(h('div', { class: 'row gap-3', style: { marginTop: '8px' } }, o.actions));
    el.appendChild(inner);
    document.body.appendChild(el);
    var layer = pushLayer(el, 'fullscreen', { dismissible: false });
    return { el: el, close: function () { el.remove(); popLayer(layer); } };
  };

  /* ======================= 5. 菜单 / 右键菜单 / 提示 ================== */
  /** items: [{label, icon, sub, shortcut, danger, disabled, reason, onClick} | {sep:true} | {group:'标题'}] */
  UI.menu = function (x, y, items, opts) {
    opts = opts || {};
    var menu = h('div', { class: 'menu', role: 'menu' });
    (items || []).forEach(function (it) {
      if (!it) return;
      if (it.sep) { menu.appendChild(h('div', { class: 'menu-sep' })); return; }
      if (it.group) { menu.appendChild(h('div', { class: 'menu-label', text: it.group })); return; }
      var dis = it.disabled;
      menu.appendChild(h('button', {
        class: 'menu-item', type: 'button', role: 'menuitem',
        dataset: { danger: it.danger ? 'true' : 'false' },
        'aria-disabled': dis ? 'true' : null,
        title: dis && it.reason ? it.reason : (it.title || ''),
        onclick: function (e) {
          e.stopPropagation();
          if (dis) { if (it.reason) UI.toast({ tone: 'warn', title: it.label + '：不可用', msg: it.reason }); return; }
          close();
          if (it.onClick) it.onClick();
        }
      }, [
        it.icon ? UI.icon(it.icon, 15) : null,
        h('span', { class: 'grow' }, [
          h('span', { text: it.label }),
          it.sub ? h('span', { class: 'menu-sub', text: it.sub }) : null
        ]),
        it.shortcut ? h('span', { class: 'shortcut', text: it.shortcut }) : null
      ]));
    });
    document.body.appendChild(menu);
    /* 定位与翻转 */
    var rect = menu.getBoundingClientRect();
    var left = opts.alignRight ? x - rect.width : x;
    left = Math.max(8, Math.min(left, window.innerWidth - rect.width - 8));
    var top = y;
    if (top + rect.height > window.innerHeight - 8) top = Math.max(8, y - rect.height);
    menu.style.left = left + 'px';
    menu.style.top = top + 'px';

    function close() {
      menu.remove();
      popLayer(layer);
      document.removeEventListener('mousedown', onDown, true);
      document.removeEventListener('scroll', close, true);
    }
    function onDown(e) { if (!menu.contains(e.target)) close(); }
    setTimeout(function () {
      document.addEventListener('mousedown', onDown, true);
      document.addEventListener('scroll', close, true);
    }, 0);
    var layer = pushLayer(menu, 'menu', { onEsc: close });
    return { el: menu, close: close };
  };

  UI.contextMenu = function (e, items) {
    e.preventDefault();
    return UI.menu(e.clientX, e.clientY, items);
  };

  UI.tooltip = function (anchorEl, text) {
    if (!text) return function () {};
    var tip = null, timer = null;
    function show() {
      var r = anchorEl.getBoundingClientRect();
      tip = h('div', { class: 'tooltip', text: text });
      document.body.appendChild(tip);
      var tr = tip.getBoundingClientRect();
      tip.style.left = Math.max(6, Math.min(r.left + r.width / 2 - tr.width / 2, window.innerWidth - tr.width - 6)) + 'px';
      var top = r.top - tr.height - 6;
      tip.style.top = (top < 4 ? r.bottom + 6 : top) + 'px';
    }
    function hide() { clearTimeout(timer); if (tip) { tip.remove(); tip = null; } }
    anchorEl.addEventListener('mouseenter', function () { timer = setTimeout(show, 380); });
    anchorEl.addEventListener('mouseleave', hide);
    anchorEl.addEventListener('focus', function () { timer = setTimeout(show, 200); });
    anchorEl.addEventListener('blur', hide);
    anchorEl.addEventListener('click', hide);
    return hide;
  };

  /* ======================= 6. Toast（info 级，§5.6）================== */
  UI.toast = function (o) {
    o = o || {};
    var host = document.querySelector('.toast-host');
    if (!host) { host = h('div', { class: 'toast-host', 'aria-live': 'polite' }); document.body.appendChild(host); }
    var icons = { success: 'check-circle', warn: 'alert', danger: 'danger', info: 'info' };
    var el = h('div', { class: 'toast', dataset: { tone: o.tone || 'info' }, role: 'status' }, [
      UI.icon(o.icon || icons[o.tone || 'info'], 16),
      h('div', { class: 'toast-body' }, [
        h('div', { class: 'toast-title', text: o.title || '' }),
        o.msg ? h('div', { class: 'toast-msg', text: o.msg }) : null,
        o.actions ? h('div', { class: 'toast-actions' }, o.actions) : null
      ]),
      UI.iconBtn({ icon: 'x', size: 'sm', label: '关闭', onClick: function () { el.remove(); } })
    ]);
    host.appendChild(el);
    var dur = o.duration === undefined ? 3600 : o.duration;
    if (dur > 0) setTimeout(function () { if (el.parentNode) el.remove(); }, dur);
    return el;
  };

  /* ======================= 7. 快捷构造 =============================== */
  UI.stack = function (children, gap) {
    return h('div', { class: 'col', style: { gap: (gap || 12) + 'px' } }, children);
  };
  UI.muted = function (text) { return h('div', { class: 't-caption', text: text }); };
  UI.h2 = function (text) { return h('div', { class: 't-h2', text: text }); };
  UI.divider = function () { return h('hr'); };
  UI.badgeForCode = function (code) {
    var e = VS.ERR[code];
    if (!e) return null;
    return UI.badge({ text: e.title, tone: e.tone === 'maintenance' ? 'maintenance' : e.tone });
  };

  /** 错误提示：统一按码给文案与动作（§3.2 全 14 码） */
  UI.errorBox = function (code, opts) {
    opts = opts || {};
    var e = VS.ERR[code] || VS.ERR[8];
    var tone = e.tone === 'maintenance' ? 'info' : e.tone;
    return UI.alertbar({
      tone: tone === 'success' ? 'info' : tone,
      icon: e.tone === 'stale' ? 'ban' : (e.tone === 'danger' ? 'danger' : 'alert'),
      title: '错误码 ' + code + ' · ' + e.title,
      text: opts.text || e.hint,
      actions: opts.actions
    });
  };

})(window);
