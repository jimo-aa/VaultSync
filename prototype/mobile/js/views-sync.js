/* ============================================================================
 * VaultSync V2.0 原型 · 移动端 —— 设备 / 同步 / 队列 / 配对 / 策略 / 阅后即焚
 * 纯静态、零依赖、经典脚本（不使用 ES Module / fetch / 外部库）
 * 依赖：../v2.0/js/core.js（VS.util / VS.fmt / VS.store / VS.ERR / VS.RELAY_ERR / VS.derive）
 *       ../v2.0/js/ui.js（VS.ui）/ ../v2.0/js/data.js（VS.data）/ js/mui.js（VS.mui）
 *
 * 移动端形态约定（docs/v2.0/03 §4.3、04 §1.2、§六）：
 *   · 二级/三级界面一律 MUI.sheet / actionSheet / confirmSheet —— 不出现居中弹窗原语
 *   · 详情为全屏路由（tab:false + back:true）；Tab 页 tab:true
 *   · 列表项长按 → 动作面板；左滑 → MUI.mrow({swipe}) / .queue-row + .swipe-row
 *   · 触控目标 ≥ 44×44；页边距由 .screen 的 --sp-4 提供
 *   · P2P 入站监听仅前台：本端未前台时不显示「在线」而显示「待唤醒」
 *   · 配对载体顺序：扫码 / 发现式 / 邀请码 / 手动；相机不可用 → 不渲染「扫码」
 *   · 能力位未置位 → 整块不渲染；其次禁用并写明原因；绝不假装成功
 *
 * 注册：VS.pages['devices'|'device'|'pairing'|'sync'|'queue'|'task'|'sync-policy'|'shares']
 *       + VS.actions[...]
 * ==========================================================================*/
(function (global) {
  'use strict';

  var VS = global.VS;
  var h = VS.util.h, UI = VS.ui, MUI = VS.mui, F = VS.fmt, D = VS.data, S = VS.store, U = VS.util;

  VS.pages = VS.pages || {};
  VS.actions = VS.actions || {};

  /* ==========================================================================
   * 0. 常量（枚举中文映射：界面不得出现裸 R 码 / 裸英文枚举）
   * ========================================================================*/

  /* 路径降级原因枚举（docs/v2.0/02 路径状态机） */
  var PATH_REASON = {
    direct_timeout:     { label: '直连超时',     hint: '超时窗口内未完成直连握手，路径状态机已尝试下一档。' },
    hole_punch_timeout: { label: '打洞超时',     hint: '双方 NAT 映射未能在窗口内对齐，打洞协助无果。' },
    symmetric_nat:      { label: '对称型 NAT',   hint: '对端为对称型 NAT，外部端口不可预测，打洞不可行。' },
    relay_limit:        { label: '中继额度受限', hint: '中继限速或流量配额耗尽，吞吐被压低。' },
    relay_unavailable:  { label: '中继不可用',   hint: '中继节点当前不可达，路径保持降级且不再自动回退。' },
    padding_downgrade:  { label: '填充档位降级', hint: '为维持吞吐，填充档位已下调（抗流量分析强度同步下降）。' },
    proto_downgrade:    { label: '协议版本降级', hint: '对端协议版本较低，已回退到兼容协商集。' }
  };
  var PATH_ORDER = ['direct', 'hole_punch', 'relay'];
  var PATH_LABEL = { direct: '直连', hole_punch: '打洞', relay: '中继' };

  /* 中继错误建议动作：R1–R17 必须映射成人话（标题/结论来自 VS.RELAY_ERR） */
  var RELAY_SUGGEST = {
    R1:  '稍后重试；若持续失败，检查防火墙是否放行中继端点。',
    R2:  '在设备页解除配对后重新发起配对，刷新中继会话凭据。',
    R3:  '确认对端在线并已重新开启分享房间。',
    R4:  '稍后重试，或改用直连 / 打洞路径。',
    R5:  '降低并发数或上行限速，等待中继队列回落。',
    R6:  '调小传输块或改用直连；该错误不影响已同步数据。',
    R7:  '等待配额重置，或在同步策略中关闭中继回退。',
    R8:  '等待限速窗口结束；同步会在恢复后自动续传。',
    R9:  '升级客户端到与对端一致的协议版本。',
    R10: '不要继续该会话；在设备详情核对指纹后再重新配对。',
    R11: '检查网络连通性；心跳恢复后路径会自动重协商。',
    R12: '无需手动处理，中继已重传该帧。',
    R13: '在设备页确认本设备是否仍在对方的授权清单中。',
    R14: '改用直连或手动输入地址；信令格式由版本差异导致。',
    R15: '确认双方均可访问打洞协助端点，或直接走中继。',
    R16: '如确需中继，请在同步策略中重新开启中继回退。',
    R17: '已记录诊断；请导出审计与日志后上报。'
  };

  var PADDING_LEVELS = [
    { value: 0, label: 'T0 · 关闭',         sub: '不填充；吞吐最高，抗流量分析最弱' },
    { value: 1, label: 'T1 · 最小',         sub: '仅对齐帧头；开销约 2%' },
    { value: 2, label: 'T2 · 标准（默认）', sub: '按块对齐填充；开销约 6%' },
    { value: 3, label: 'T3 · 增强',         sub: '定长块 + 随机抖动；开销约 14%' },
    { value: 4, label: 'T4 · 最大',         sub: '恒速伪流；开销约 30%，弱网下显著降速' }
  ];
  var CONFLICT_STRATEGIES = [
    { value: 'keep_both',     label: '保留两者',   sub: '生成冲突副本，不覆盖任何一方' },
    { value: 'prefer_newer',  label: '较新者优先', sub: '按修改时间取新，旧版进保护副本' },
    { value: 'prefer_local',  label: '本机优先',   sub: '始终保留本机版本' },
    { value: 'prefer_remote', label: '对端优先',   sub: '始终保留对端版本' }
  ];
  var SYNC_MODES = [
    { value: 'auto',      label: '自动', sub: '变更即同步' },
    { value: 'manual',    label: '手动', sub: '仅在我点击「立即同步」时同步' },
    { value: 'scheduled', label: '定时', sub: '仅在允许时段内同步' }
  ];
  var SHARE_STATUS = {
    waiting: { label: '等待接受', tone: 'info' },
    active:  { label: '使用中',   tone: 'success' },
    burned:  { label: '已焚毁',   tone: 'stale' },
    revoked: { label: '已撤销',   tone: 'warn' }
  };
  var TASK_STATE = {
    running:     { label: '传输中', tone: 'info' },
    paused:      { label: '已暂停', tone: 'warn' },
    queued:      { label: '排队中', tone: 'info' },
    failed:      { label: '失败',   tone: 'danger' },
    interrupted: { label: '已中断', tone: 'warn' },
    done:        { label: '已完成', tone: 'success' },
    cancelled:   { label: '已取消', tone: 'stale' }
  };

  var RETRY_LIMIT = 3;
  var CHUNK_SIZE = 262144;

  /* 仅页面级 UI 状态（跨 repaint() 重建保持；不污染 VS.store） */
  var ui = {
    queueFilter: 'all',
    mergeFailDemo: false,
    policy: null,          /* 编辑中的策略副本 */
    policyReadFailed: false,
    policyError: null      /* {code, text} 原子拒绝后置顶展示 */
  };

  /* ==========================================================================
   * 1. 通用原语
   * ========================================================================*/

  function now() { return Date.now(); }

  /** 重绘当前路由（外壳由 app.js 提供；未挂载外壳时静默跳过） */
  function repaint() { if (typeof VS.render === 'function') VS.render(); }

  /** 只读降级 / 维护态的写门禁：返回 null 或 {code, reason} */
  function denyInfo() {
    var code = VS.derive.denyWrite();
    if (code === null || code === undefined) return null;
    var e = VS.ERR[code] || VS.ERR[8];
    return { code: code, reason: '错误码 ' + code + ' · ' + e.title + '：' + e.hint };
  }

  /** 本端是否前台：持续监听（P2P 入站）仅前台有效 */
  function isForeground() { return S.get('foreground') !== false; }
  /** 相机是否可用（权限被拒 / 无相机 → false） */
  function cameraOk() { return S.get('cameraOk') !== false; }

  function msection(text) { return h('div', { class: 'msection-title' }, [h('span', { text: text })]); }
  function caption(text) { return h('div', { class: 't-caption', text: text }); }
  function mcard(title, sub, body, actions) {
    return MUI.mcard({ title: title, sub: sub, actions: actions, body: body });
  }

  /** 整宽按钮（底部主操作）：disabled + title 给出原因 */
  function mbtn(o) {
    o = o || {};
    return h('button', {
      class: 'mbtn-block', type: 'button',
      dataset: { variant: o.variant || '' },
      disabled: !!o.disabled,
      'aria-disabled': o.disabled ? 'true' : null,
      'aria-label': o.label + (o.disabled && o.reason ? '（不可用：' + o.reason + '）' : ''),
      title: o.disabled && o.reason ? o.reason : (o.title || o.label),
      onclick: o.disabled ? null : (o.onClick || null)
    }, [o.icon ? h('span', { html: VS.icon(o.icon, 18) }) : null, h('span', { text: o.label })]);
  }

  /** 写门禁说明条：只读降级 / 维护态下置顶声明，避免「点了没反应」 */
  function writeGateNote() {
    var d = denyInfo();
    if (!d) return null;
    return MUI.limitedNote(d.reason + ' 写操作已禁用；只读查看不受影响。');
  }

  function pathReasonText(code) {
    var r = PATH_REASON[code];
    return r ? r.label : (code ? String(code) : '未登记原因');
  }
  function pathReasonHint(code) {
    var r = PATH_REASON[code];
    return r ? r.hint : '该原因未在原型枚举中登记。';
  }
  /** 中继错误 → 人话（禁止原样显示 R 码） */
  function relayUser(code) {
    var e = VS.RELAY_ERR[code];
    return e ? e.user : '中继返回未登记错误，已记录诊断。';
  }
  function relayTitle(code) {
    var e = VS.RELAY_ERR[code];
    return e ? e.title : '中继错误';
  }

  function dirIcon(dir) { return dir === 'up' ? 'import' : 'export'; }
  function dirLabel(dir) { return dir === 'up' ? '上传' : '下载'; }

  function openTask(taskId) { VS.nav.go('task', { taskId: taskId }); }
  function openDevice(devId) { VS.nav.go('device', { deviceId: devId }); }

  /* ---------------------------------------------------------------- 统计 */

  function queueStats() {
    var q = (D.queue || []).slice();
    var active = 0, queued = 0, doneB = 0, totalB = 0, up = 0, down = 0, eta = null;
    q.forEach(function (t) {
      if (t.state === 'running') active++;
      if (t.state === 'queued') queued++;
      doneB += t.doneBytes || 0;
      totalB += t.totalBytes || 0;
      if (t.state === 'running') {
        if (t.dir === 'up') up += t.rateBps || 0; else down += t.rateBps || 0;
      }
      if (t.state === 'running' && t.etaMs) eta = eta === null ? t.etaMs : Math.max(eta, t.etaMs);
    });
    return {
      count: q.length, active: active, queued: queued,
      doneBytes: totalB > 0 ? doneB : null,
      totalBytes: totalB > 0 ? totalB : null,
      pct: totalB > 0 ? doneB / totalB : null,
      up: up || null, down: down || null, eta: eta
    };
  }

  function onlineDeviceCount() {
    var n = 0;
    (D.devices || []).forEach(function (d) { if (!d.self && d.online) n++; });
    return n;
  }
  function conflictCount() { return D.conflictPair ? 1 : 0; }
  function conflictSides() {
    var cp = D.conflictPair;
    if (!cp) return null;
    var a = VS.data.entry(cp.keep), b = VS.data.entry(cp.drop);
    if (!a || !b) return null;
    return { keep: a, drop: b };
  }

  /** 最近 N 条同步记录：data.js 无设备级同步历史，由 D.queue 确定性派生 */
  function deviceSyncRecords(dev, n) {
    var out = [];
    var base = (D.queue && D.queue.length) ? D.queue : [{ name: '—', dir: 'up', totalBytes: 0, state: 'done', errCode: null }];
    for (var i = 0; i < n; i++) {
      var t = base[i % base.length];
      var ok = !(t.state === 'failed' || t.state === 'interrupted');
      out.push({
        id: 'rec-' + dev.id + '-' + i,
        seq: n - i,
        timeMs: D.NOW - i * (7 * D.MIN) - (i * 131 % 60000),
        name: t.name,
        dir: t.dir,
        size: (t.totalBytes || 0) + (i * 1024) % 65536,
        state: ok ? 'done' : 'failed',
        path: PATH_ORDER[i % 3],
        peer: dev.name,
        errCode: ok ? 0 : (t.errCode || 3)
      });
    }
    return out;
  }

  /* ------------------------------------------------------------ 左滑增强 */
  /** mobile.css 的 .swipe-row 需要 data-open 才展开；此处补齐手势（不改动公共文件） */
  function attachSwipe(row) {
    var x0 = null, y0 = null;
    row.addEventListener('touchstart', function (e) {
      if (!e.touches || !e.touches.length) return;
      x0 = e.touches[0].clientX; y0 = e.touches[0].clientY;
    }, { passive: true });
    row.addEventListener('touchend', function (e) {
      if (x0 === null) return;
      var t = e.changedTouches && e.changedTouches[0];
      if (!t) { x0 = null; return; }
      var dx = t.clientX - x0, dy = Math.abs(t.clientY - y0);
      if (dx < -34 && dy < 44) row.dataset.open = 'true';
      else if (dx > 24) row.dataset.open = 'false';
      x0 = null;
    });
    row.addEventListener('contextmenu', function (e) {
      e.preventDefault();
      row.dataset.open = row.dataset.open === 'true' ? 'false' : 'true';
    });
    Array.prototype.forEach.call(row.querySelectorAll('.swipe-actions button'), function (b) {
      b.addEventListener('click', function () { row.dataset.open = 'false'; });
    });
  }

  /* ==========================================================================
   * 2. 传输队列行（.queue-row，桌面端同构、移动端更紧凑）
   * ========================================================================*/

  function taskPercent(t) {
    return t.totalBytes > 0 ? (t.doneBytes || 0) / t.totalBytes : null;
  }

  /** 失败 / 中断行的行内错误码人话（码 12 用 info 语气，不用红色错误态） */
  function taskErrorLine(t, onRetry) {
    var exhausted = (t.retries || 0) >= RETRY_LIMIT;
    if (t.errCode === 12) {
      return UI.alertbar({ tone: 'info', icon: 'info', title: '已取消', text: VS.ERR[12].hint + '（视为正常结果，不报错）' });
    }
    var e = VS.ERR[t.errCode];
    if (!e) e = { tone: 'danger', title: '未登记错误', hint: '任务失败但未携带可映射的错误码，已记录诊断。' };
    var exhaustedText = exhausted ? ' · 重试已达上限 ' + F.num(RETRY_LIMIT) + ' 次：请检查网络或磁盘空间' : '';
    return UI.alertbar({
      tone: e.tone === 'stale' ? 'warn' : (e.tone === 'danger' ? 'danger' : 'info'),
      icon: e.tone === 'stale' ? 'ban' : (e.tone === 'danger' ? 'danger' : 'info'),
      title: '错误码 ' + t.errCode + ' · ' + e.title,
      text: e.hint + exhaustedText,
      actions: [mbtnRetry(t, onRetry, exhausted)]
    });
  }

  function mbtnRetry(t, onRetry, exhausted) {
    return h('button', {
      class: 'btn btn-sm', type: 'button',
      disabled: exhausted,
      title: exhausted ? '重试已达上限：请检查网络或磁盘空间' : '重试该任务',
      onclick: function () {
        if (exhausted) { UI.toast({ tone: 'warn', title: '重试已达上限', msg: '请检查网络或磁盘空间' }); return; }
        t.retries = (t.retries || 0) + 1;
        t.state = 'running';
        t.errCode = null;
        if (onRetry) onRetry();
      }
    }, [h('span', { html: VS.icon('refresh', 13) }), h('span', { text: exhausted ? '已达上限' : '重试' })]);
  }

  /** 行主体（.queue-row 的子节点），供普通行与左滑行共用 */
  function queueRowBody(t, opts) {
    opts = opts || {};
    var st = TASK_STATE[t.state] || { label: t.state, tone: 'info' };
    var pct = taskPercent(t);
    var d = denyInfo();
    var cancelToneState = t.errCode === 12 ? 'cancelled' : t.state;

    var line1 = h('div', { class: 'qr-line' }, [
      h('span', { class: 'qr-name t-truncate', text: t.name, title: t.name }),
      UI.badge({ text: st.label, tone: st.tone }),
      t.retries ? UI.badge({ text: '已重试 ' + F.num(t.retries) + ' 次', tone: 'warn' }) : null,
      UI.pathBadge(t.path)
    ]);

    var meta = [];
    meta.push(F.bytes(t.doneBytes || 0) + ' / ' + F.bytes(t.totalBytes));
    if (pct !== null) meta.push(F.pct(pct));
    if (t.state === 'running' && t.rateBps) meta.push(F.rate(t.rateBps));
    if (t.state === 'running' && t.etaMs) meta.push('剩余 ' + F.duration(t.etaMs));
    meta.push('对端 ' + (t.peer || '—'));

    var main = h('div', { class: 'qr-main' }, [
      line1,
      cancelToneState !== 'done' && cancelToneState !== 'cancelled' && pct !== null
        ? MUI.bar(pct, t.errCode === 12 ? null : (cancelToneState === 'failed' ? 'danger' : null), 'sm') : null,
      h('div', { class: 'qr-line' }, [h('span', { class: 'qr-meta', text: meta.join(' · ') })]),
      t.state === 'interrupted' && t.resumable && t.resumePct !== null
        ? UI.alertbar({ tone: 'warn', icon: 'history', title: '续传 ' + F.pct(t.resumePct) + '（跨会话）', text: '本次会话从已落盘的块边界继续，不重传已完成块。' })
        : null,
      (cancelToneState === 'failed' || (t.state === 'interrupted' && t.errCode !== null))
        ? taskErrorLine(t, opts.onChange) : null,
      cancelToneState === 'cancelled' && t.errCode !== null
        ? UI.alertbar({ tone: 'info', icon: 'info', title: '已取消', text: '错误码 ' + t.errCode + ' · ' + VS.ERR[t.errCode].title + '（视为正常结果，不报错）' })
        : null
    ]);

    var actions = h('div', { class: 'qr-actions' });
    actions.appendChild(UI.iconBtn({
      icon: 'more', size: 'sm', label: '更多操作：' + t.name,
      onClick: function () { openTaskSheet(t, opts.onChange, d); }
    }));

    return {
      state: cancelToneState,
      nodes: [
        h('span', { class: 'qr-ico', html: VS.icon(dirIcon(t.dir), 18), title: dirLabel(t.dir) }),
        main,
        actions
      ]
    };
  }

  function queueRowNode(t, opts) {
    var body = queueRowBody(t, opts);
    return h('div', {
      class: 'queue-row', dataset: { state: body.state, dir: t.dir },
      title: t.name + ' · ' + (TASK_STATE[t.state] ? TASK_STATE[t.state].label : t.state)
    }, body.nodes);
  }

  /** 左滑行：暂停/继续 + 取消（.swipe-row / .swipe-content / .swipe-actions） */
  function queueSwipeRow(t, opts) {
    var d = denyInfo();
    var body = queueRowBody(t, opts);
    var actions = [
      t.state === 'running'
        ? { icon: 'pause', label: '暂停', onClick: function () { t.state = 'paused'; t.rateBps = 0; t.etaMs = null; opts.onChange(); } }
        : { icon: 'play', label: '继续', onClick: function () { if (d) return; t.state = 'running'; opts.onChange(); } },
      { icon: 'cancel', label: '取消', tone: 'danger', onClick: function () { t.state = 'cancelled'; t.errCode = 12; t.rateBps = 0; opts.onChange(); } }
    ];
    var acts = h('div', { class: 'swipe-actions' }, actions.map(function (a) {
      return h('button', {
        type: 'button', dataset: { tone: a.tone || '' },
        'aria-label': a.label + ' ' + t.name, title: a.label + ' ' + t.name,
        onclick: function (e) {
          e.stopPropagation();
          if (d) { UI.toast({ tone: 'warn', title: a.label + '：不可用', msg: d.reason }); return; }
          a.onClick();
        }
      }, [h('span', { html: VS.icon(a.icon, 18) }), h('span', { text: a.label })]);
    }));
    var content = h('div', {
      class: 'swipe-content', role: 'button', tabindex: '0',
      'aria-label': t.name + ' ' + (TASK_STATE[t.state] ? TASK_STATE[t.state].label : t.state),
      onclick: function () { openTask(t.taskId); }
    }, body.nodes);
    /* .swipe-row 不是 .queue-row，失败/中断的语义底色在此显式补齐（Token 与 .queue-row 一致） */
    var rowTone = body.state === 'failed' ? { background: 'var(--c-danger-soft)' }
      : body.state === 'interrupted' ? { background: 'var(--c-warning-soft)' } : null;
    var row = h('div', { class: 'swipe-row', dataset: { state: body.state }, style: rowTone }, [acts, content]);
    attachSwipe(row);
    return row;
  }

  function openTaskSheet(t, onChange, d) {
    var paused = t.state === 'paused';
    MUI.actionSheet({
      title: t.name,
      sub: (TASK_STATE[t.state] ? TASK_STATE[t.state].label : t.state) + ' · ' + dirLabel(t.dir) + ' · 对端 ' + (t.peer || '—'),
      items: [
        { label: '查看同步详情', icon: 'eye', sub: 'CDC 块矩阵 / 续传位置 / 事件来源', onClick: function () { openTask(t.taskId); } },
        paused
          ? { label: '继续传输', icon: 'play', disabled: !!d, reason: d ? d.reason : null, onClick: function () { t.state = 'running'; onChange(); } }
          : { label: '暂停传输', icon: 'pause', disabled: !!d, reason: d ? d.reason : null, onClick: function () { t.state = 'paused'; t.rateBps = 0; t.etaMs = null; onChange(); } },
        {
          label: '重试', icon: 'refresh',
          sub: '已重试 ' + F.num(t.retries || 0) + ' / ' + F.num(RETRY_LIMIT) + ' 次',
          disabled: !!d || (t.retries || 0) >= RETRY_LIMIT,
          reason: d ? d.reason : ((t.retries || 0) >= RETRY_LIMIT ? '重试已达上限，请检查网络或磁盘空间' : null),
          onClick: function () { t.retries = (t.retries || 0) + 1; t.state = 'running'; t.errCode = null; onChange(); }
        },
        { label: '取消任务', icon: 'cancel', danger: true, disabled: !!d, reason: d ? d.reason : null,
          onClick: function () { t.state = 'cancelled'; t.errCode = 12; t.rateBps = 0; onChange(); } }
      ]
    });
  }

  /* ==========================================================================
   * 3. 底部面板：路径诊断 / 远程锁定 / 解除配对 / 任务详情入口
   * ========================================================================*/

  /** 路径诊断：中继错误一律映射成人话，界面不出现裸 R 码 */
  function openPathDiag(dev) {
    var current = (dev && dev.reason) ? dev.reason : 'symmetric_nat';
    var errRows = Object.keys(VS.RELAY_ERR).map(function (code) {
      return h('div', { class: 'mrow', style: { cursor: 'default', alignItems: 'flex-start' }, title: relayTitle(code) }, [
        h('span', { class: 'mr-ico', html: VS.icon('relay', 20) }),
        h('div', { class: 'mr-main' }, [
          h('div', { class: 'mr-title' }, [h('span', { class: 't-truncate', text: relayTitle(code) })]),
          h('div', { class: 'mr-sub', text: relayUser(code) }),
          h('div', { class: 'mr-sub', text: '建议：' + (RELAY_SUGGEST[code] || '已记录诊断，请导出日志后上报。') })
        ])
      ]);
    });
    MUI.sheet({
      title: '路径诊断',
      sub: '直连 / 打洞 / 中继 三档路径与降级原因',
      size: 'tall',
      body: h('div', { class: 'col gap-3' }, [
        UI.alertbar({
          tone: 'info', icon: 'info', title: '中继错误以人话呈现',
          text: '中继侧有独立错误空间；界面只展示映射后的结论与建议动作，不原样显示 R 码。'
        }),
        msection('当前路径与降级原因'),
        MUI.mkv([
          { k: '当前路径', v: dev ? PATH_LABEL[dev.path || 'direct'] : PATH_LABEL.direct },
          { k: '降级原因', v: pathReasonText(current) },
          { k: '原因说明', v: pathReasonHint(current) },
          { k: '中继回退', v: (D.syncPolicy && D.syncPolicy.relayEnabled) ? '已开启' : '已关闭' }
        ]),
        msection('降级原因枚举（中文映射）'),
        MUI.mlist(Object.keys(PATH_REASON).map(function (k) {
          return h('div', { class: 'mrow', style: { cursor: 'default', alignItems: 'flex-start' }, title: k }, [
            h('div', { class: 'mr-main' }, [
              h('div', { class: 'mr-title' }, [h('span', { text: PATH_REASON[k].label })]),
              h('div', { class: 'mr-sub', text: PATH_REASON[k].hint })
            ])
          ]);
        })),
        msection('中继错误与建议动作'),
        MUI.mlist(errRows)
      ]),
      footer: [mbtn({ label: '关闭', variant: 'ghost', onClick: function () { MUI.closeAll(); } })]
    });
  }

  /** 远程锁定：目标不在线 → 「待投递」，绝不显示「已锁定对端」 */
  function openRemoteLock(dev) {
    var mode = 'immediate';
    var delayMin = 5;
    var body = h('div', { class: 'col gap-3' });
    var sheet;
    function render() {
      body.innerHTML = '';
      body.appendChild(UI.alertbar({
        tone: 'warn', icon: 'alert', title: '远程锁定为不可回退的高影响操作',
        text: '指令送达对端后，对端将立即进入锁定态。本原型不执行真实锁定。'
      }));
      body.appendChild(UI.radioCards({
        value: mode, onChange: function (v) { mode = v; render(); },
        options: [
          { value: 'immediate', title: '立即锁定', desc: '指令送达即锁定' },
          { value: 'delayed', title: '延迟锁定', desc: '在指定分钟数后锁定，便于撤回误操作' }
        ]
      }));
      if (mode === 'delayed') {
        var input = UI.input({ type: 'number', value: delayMin, mono: true });
        input.addEventListener('input', function () { delayMin = parseInt(input.value, 10) || 0; });
        body.appendChild(MUI.field({ label: '延迟分钟数', control: input, hint: '范围 1–1440 分钟。' }));
      }
      body.appendChild(h('div', { class: 'row gap-2' }, [
        h('span', { class: 'status-dot ' + (dev.online ? 'online' : 'offline') }),
        h('span', { class: 't-caption', text: dev.online ? '目标在线：指令可直接投递' : '目标离线：指令将保持「待投递」' })
      ]));
    }
    render();
    sheet = MUI.sheet({
      title: '远程锁定 · ' + dev.name,
      sub: '模式 / 时效 / 投递状态',
      body: body,
      footer: function (close) {
        return [
          mbtn({
            label: mode === 'delayed' ? ('延迟 ' + F.num(delayMin) + ' 分钟锁定') : '立即锁定',
            variant: 'danger', icon: 'lock',
            onClick: function () {
              close();
              var offline = !dev.online;
              UI.toast({
                tone: offline ? 'warn' : 'info',
                title: offline ? '待投递' : '已下发，等待对端确认',
                msg: offline ? '目标离线，指令保持待投递；不代表对端已锁定' : '对端确认前不会显示为已锁定'
              });
            }
          }),
          mbtn({ label: '取消', variant: 'ghost', onClick: function () { close(); } })
        ];
      }
    });
    return sheet;
  }

  function confirmUnpair(dev, onDone) {
    return MUI.confirmSheet({
      title: '解除配对',
      sub: dev.name,
      tone: 'danger',
      body: h('div', { class: 'col gap-2' }, [
        caption('解除后本机丢弃该设备的公钥绑定与节点清单条目。'),
        caption('已同步的文件不会被删除；该设备不再被视为可信，需要重新配对。'),
        h('div', { class: 't-mono' }, MUI.copyField(dev.fingerprint, '复制设备指纹'))
      ]),
      confirmLabel: '解除配对',
      onConfirm: function () {
        UI.toast({ tone: 'warn', title: '已解除配对', msg: dev.name + ' 已从节点清单移除（Mock）' });
        if (onDone) onDone();
      }
    });
  }

  /** 设备长按动作面板（长按 = 动作面板；左滑为等价降级） */
  function openDeviceSheet(dev) {
    var d = denyInfo();
    MUI.actionSheet({
      title: dev.name,
      sub: (dev.online ? '在线' : '待唤醒') + ' · ' + PATH_LABEL[dev.path || 'direct'] + (dev.reason ? '（降级：' + pathReasonText(dev.reason) + '）' : ''),
      items: [
        { label: '查看详情', icon: 'eye', sub: 'Noise 信道 / 指纹 / 同步记录', onClick: function () { openDevice(dev.id); } },
        {
          label: '立即同步', icon: 'refresh',
          disabled: !dev.online || !!d,
          reason: !dev.online ? '设备未确认在线（P2P 入站监听仅前台），无法立即同步' : (d ? d.reason : null),
          onClick: function () { VS.actions['syncNow']({ device: dev.id }); }
        },
        { label: '路径诊断', icon: 'discover', sub: '三档路径与降级原因', onClick: function () { openPathDiag(dev); } },
        {
          label: '远程锁定…', icon: 'lock',
          disabled: !!d, reason: d ? d.reason : null,
          onClick: function () { openRemoteLock(dev); }
        },
        {
          label: '路径降级说明', icon: 'info',
          disabled: !dev.reason, reason: !dev.reason ? '该设备当前无降级原因' : null,
          onClick: function () { UI.toast({ tone: 'warn', title: pathReasonText(dev.reason), msg: pathReasonHint(dev.reason), duration: 5000 }); }
        },
        { group: '危险操作' },
        {
          label: '解除配对', icon: 'trash', danger: true,
          disabled: !!d, reason: d ? d.reason : null,
          onClick: function () { confirmUnpair(dev, function () { repaint(); }); }
        }
      ]
    });
  }

  /* ==========================================================================
   * 4. 配对向导（五阶段；扫码为主路径，相机不可用则不渲染「扫码」）
   * ========================================================================*/

  function carrierList() {
    var out = [];
    if (cameraOk()) out.push({ value: 'qr', label: '扫码', desc: '对方扫码加入（推荐主路径）', icon: 'qr' });
    if (S.cap('CAP_DISCOVERY')) out.push({ value: 'discover', label: '发现式', desc: '同网段已发现设备直接选取', icon: 'discover' });
    out.push({ value: 'invite', label: '邀请码', desc: '生成一次性邀请码交给对方输入', icon: 'key' });
    out.push({ value: 'manual', label: '手动输入地址', desc: '跨网段场景的兜底方式', icon: 'edit' });
    return out;
  }

  VS.pages['pairing'] = function (ctx) {
    var step = 1;                                   /* 1..5 */
    var carriers = carrierList();
    var carrier = (ctx.params && ctx.params.carrier) || (carriers[0] ? carriers[0].value : 'invite');
    var authPct = 0, authTimer = null, codeLeft = 120, codeTimer = null;
    var shortCode = null;
    var inviteCode = 'VS-' + D.hash(6).toUpperCase();
    var pickedDiscover = null;
    var addrValue = '';

    var body = h('div', { class: 'col gap-3' });
    var root = MUI.screen([
      MUI.limitedNote('配对载体以扫码为主：顺序为「扫码 / 发现式 / 邀请码 / 手动」。相机不可用或权限被拒时，「扫码」载具不渲染（不是置灰）。'),
      body
    ]);

    function cleanup() {
      if (authTimer) { clearInterval(authTimer); authTimer = null; }
      if (codeTimer) { clearInterval(codeTimer); codeTimer = null; }
    }
    function alive() { return document.body.contains(root); }
    function carrierLabel(v) {
      var hit = null;
      carriers.forEach(function (c) { if (c.value === v) hit = c.label; });
      return hit || v;
    }
    function goto(n) { step = n; render(); }
    function stepsNode() {
      return MUI.steps({
        current: step - 1,
        items: ['选载具', '交换', '双向认证', '核验短码', '完成'].map(function (l) { return { label: l }; })
      });
    }

    function stageOne() {
      var rows = carriers.map(function (c) {
        var on = c.value === carrier;
        return MUI.mrow({
          icon: c.icon, title: c.label, sub: c.desc,
          trail: on ? [UI.badge({ text: '已选', tone: 'accent', icon: 'check' })] : null,
          chevron: false,
          onClick: function () { carrier = c.value; render(); }
        });
      });
      var extra = [];
      if (!cameraOk()) {
        extra.push(UI.alertbar({
          tone: 'info', icon: 'ban', title: '相机不可用',
          text: '相机不可用或权限被拒：不渲染「扫码」载具。请改用发现式 / 邀请码 / 手动输入地址。'
        }));
      }
      if (!S.cap('CAP_DISCOVERY')) {
        extra.push(UI.alertbar({
          tone: 'info', icon: 'ban', title: '局域网发现不可用',
          text: '能力位 CAP_DISCOVERY 未置位：不渲染「发现式」载具，也不显示任何发现结果。'
        }));
      }
      return h('div', { class: 'col gap-3' }, [
        caption('载体决定后续交换阶段可用的通道。'),
        MUI.mlist(rows)
      ].concat(extra));
    }

    function stageTwo() {
      var nodes = [];
      if (carrier === 'qr') {
        nodes.push(caption('对方扫码后进入双向认证。二维码只含配对载荷，不含密钥明文。'));
        nodes.push(h('div', { class: 'col', style: { alignItems: 'center', gap: '8px' } }, [
          MUI.qrPlaceholder(31),
          h('div', { class: 't-mono', text: inviteCode })
        ]));
      } else if (carrier === 'discover') {
        nodes.push(caption('未配对设备只暴露指纹与候选地址数量，不暴露 IP / 端口 / 名称。'));
        var list = (D.discovered || []).map(function (dd) {
          return MUI.mrow({
            icon: 'devices',
            title: '局域网设备 · 指纹 ' + dd.fingerprint,
            sub: '候选地址 ' + F.num((dd.addrs || []).length) + ' 个 · 类型 ' +
              (dd.addrs || []).map(function (a) { return String(a.type).toUpperCase(); }).join(' / '),
            trail: pickedDiscover === dd.id ? [UI.badge({ text: '已选', tone: 'accent', icon: 'check' })] : null,
            chevron: false,
            onClick: function () { pickedDiscover = dd.id; render(); }
          });
        });
        nodes.push(MUI.mlist(list.length ? list : [
          h('div', { class: 'mrow', style: { cursor: 'default' } }, [
            h('div', { class: 'mr-main' }, [h('div', { class: 'mr-title' }, [h('span', { text: '局域网发现无候选' })]),
              h('div', { class: 'mr-sub', text: '请确认对端与本机处于同一网段；本端未置位能力位时不显示任何发现结果。' })])
          ])
        ]));
      } else if (carrier === 'invite') {
        nodes.push(caption('把一次性邀请码交给对方输入。邀请码单次有效。'));
        nodes.push(h('div', { class: 't-mono' }, MUI.copyField(inviteCode, '复制邀请码')));
      } else {
        var input = UI.input({ value: addrValue, placeholder: '例如 192.168.1.24:51234', mono: true });
        input.addEventListener('input', function () { addrValue = input.value; });
        nodes.push(MUI.field({
          label: '对端地址', for: 'm-pair-addr', control: input,
          hint: '跨网段场景的兜底方式；地址仅用于本次握手。'
        }));
      }
      nodes.push(mbtn({
        label: '手动输入地址（跨网段兜底）', icon: 'edit', variant: 'ghost',
        onClick: function () { openManualSheet(); }
      }));
      return h('div', { class: 'col gap-3' }, nodes);
    }

    function openManualSheet() {
      var v = addrValue;
      var input = UI.input({ value: v, placeholder: '例如 192.168.1.24:51234', mono: true });
      input.addEventListener('input', function () { v = input.value; });
      MUI.sheet({
        title: '手动输入地址',
        sub: '跨网段兜底通道',
        body: h('div', { class: 'col gap-3' }, [
          MUI.limitedNote('手动输入地址是跨网段兜底：当双方不在同一网段、且打洞与中继都不可用时使用。'),
          caption('地址仅用于本次握手，不写入节点清单；配对成功后仍以设备指纹为准。'),
          MUI.field({ label: '对端地址', control: input, hint: '形如 host:port；仅接受用户显式输入。' })
        ]),
        footer: function (close) {
          return [
            mbtn({
              label: '使用该地址', variant: 'primary', icon: 'check',
              onClick: function () {
                if (!v.trim()) { UI.toast({ tone: 'warn', title: '地址为空', msg: '请输入 host:port。' }); return; }
                addrValue = v.trim(); carrier = 'manual'; close(); render();
              }
            }),
            mbtn({ label: '取消', variant: 'ghost', onClick: function () { close(); } })
          ];
        }
      });
    }

    function stageThree() {
      return h('div', { class: 'col gap-3' }, [
        caption('双向认证进行中：双方各自验证对端身份，任一步失败即整体中止。'),
        MUI.bar(authPct, 'accent', 'lg'),
        h('div', { class: 't-mono', text: F.pct(authPct) }),
        UI.alertbar({ tone: 'info', icon: 'info', title: '认证内容', text: 'Noise 信道握手 + 静态公钥绑定校验；本原型不执行真实密码学运算。' })
      ]);
    }

    function stageFour() {
      if (!shortCode) shortCode = String(Math.floor(100000 + Math.random() * 900000));
      var mm = Math.floor(codeLeft / 60), ss = codeLeft % 60;
      return h('div', { class: 'col gap-3' }, [
        caption('请与对端屏幕上的 6 位短码逐位比对。短码不一致说明存在中间人。'),
        h('div', {
          class: 't-mono', 'aria-label': '核验短码 ' + shortCode, title: '本次核验短码',
          style: { fontSize: '40px', letterSpacing: '.32em', fontWeight: '600' }, text: shortCode
        }),
        h('div', { class: 'row gap-2' }, [
          h('span', { class: 'status-dot ' + (codeLeft > 0 ? 'warn' : 'danger') }),
          h('span', { class: 't-caption', text: codeLeft > 0 ? ('剩余 ' + F.num(mm) + ':' + (ss < 10 ? '0' : '') + F.num(ss)) : '已超时：请重新发起配对' })
        ])
      ]);
    }

    function stageFive() {
      return h('div', { class: 'col gap-3' }, [
        UI.alertbar({ tone: 'success', icon: 'check-circle', title: '配对完成', text: '已写入节点清单并生成审计条目。' }),
        MUI.mkv([
          { k: '配对载体', v: carrierLabel(carrier) },
          { k: '对端指纹', v: 'A3:F1:…:9C', mono: true },
          { k: '写入内容', v: '节点清单（设备名 / 指纹 / 公钥绑定 / 信任级别）' },
          { k: '审计事件', v: 'p2p.pair', mono: true }
        ])
      ]);
    }

    function render() {
      body.innerHTML = '';
      body.appendChild(stepsNode());
      if (step === 1) body.appendChild(stageOne());
      else if (step === 2) body.appendChild(stageTwo());
      else if (step === 3) body.appendChild(stageThree());
      else if (step === 4) body.appendChild(stageFour());
      else body.appendChild(stageFive());

      var foot = h('div', { class: 'col gap-2' });
      if (step === 1) {
        foot.appendChild(mbtn({
          label: '下一步：交换载荷', variant: 'primary', icon: 'chevron-right',
          onClick: function () { goto(2); }
        }));
      } else if (step === 2) {
        if (carrier === 'discover' && !pickedDiscover) {
          foot.appendChild(mbtn({
            label: '请先选择一个已发现设备', variant: 'primary', icon: 'discover', disabled: true,
            reason: '未选择候选设备；发现列表中的设备只暴露指纹'
          }));
        } else {
          foot.appendChild(mbtn({
            label: '开始双向认证', variant: 'primary', icon: 'shield',
            onClick: function () {
              goto(3);
              authPct = 0;
              authTimer = setInterval(function () {
                if (!alive()) { cleanup(); return; }
                authPct = Math.min(1, authPct + 0.12);
                if (authPct >= 1) {
                  clearInterval(authTimer); authTimer = null;
                  goto(4);
                  codeLeft = 120;
                  codeTimer = setInterval(function () {
                    if (!alive()) { cleanup(); return; }
                    codeLeft--;
                    if (codeLeft <= 0) { clearInterval(codeTimer); codeTimer = null; codeLeft = 0; }
                    if (step === 4) render();
                  }, 1000);
                  return;
                }
                if (step === 3) render();
              }, 160);
            }
          }));
        }
      } else if (step === 3) {
        foot.appendChild(mbtn({ label: '认证进行中…', variant: 'ghost', disabled: true, reason: '等待 Noise 握手与公钥绑定校验完成' }));
      } else if (step === 4) {
        foot.appendChild(mbtn({
          label: '短码一致', variant: 'primary', icon: 'check',
          onClick: function () { cleanup(); goto(5); }
        }));
        foot.appendChild(mbtn({
          label: '不一致', variant: 'danger', icon: 'danger',
          onClick: function () {
            cleanup();
            UI.toast({ tone: 'danger', title: '已中止配对', msg: '短码不一致：可能存在中间人，请检查网络环境。' });
            VS.nav.back();
          }
        }));
      } else {
        foot.appendChild(mbtn({
          label: '完成', variant: 'primary', icon: 'check',
          onClick: function () {
            cleanup();
            UI.toast({ tone: 'success', title: '配对已写入', msg: '节点清单与审计条目已生成。' });
            VS.nav.back();
          }
        }));
      }
      if (step > 1 && step !== 4) {
        foot.appendChild(mbtn({
          label: '上一步', variant: 'ghost', icon: 'chevron-left',
          onClick: function () { cleanup(); goto(step - 1); }
        }));
      }
      body.appendChild(foot);
    }

    render();
    return MUI.page({
      title: '配对向导',
      sub: '选载具 → 交换 → 双向认证 → 核验短码 → 写入节点清单 + 审计',
      back: true,
      tab: false,           /* 详情/向导为全屏路由 */
      body: root,
      onBack: function () { cleanup(); VS.nav.back(); }
    });
  };

  /* ==========================================================================
   * 5. 页面：devices（Tab）
   * ========================================================================*/

  function deviceStatusLabel(dev) {
    if (!isForeground()) return '待唤醒';
    return dev.online ? '在线' : '待唤醒';
  }
  function deviceStatusTitle(dev) {
    if (!isForeground()) return '本端未在前台：P2P 入站监听已暂停，无法确认对端在线';
    return dev.online ? '在线' : '待唤醒：未收到对端入站心跳（持续监听仅前台有效）';
  }

  function openAddDeviceSheet() {
    var d = denyInfo();
    var items = [];
    if (cameraOk()) {
      items.push({
        label: '扫码配对', sub: '相机可用：配对主路径', icon: 'qr',
        disabled: !!d, reason: d ? d.reason : null,
        onClick: function () { VS.nav.go('pairing', { carrier: 'qr' }); }
      });
    }
    if (S.cap('CAP_DISCOVERY')) {
      items.push({
        label: '发现式配对', sub: '局域网已发现设备', icon: 'discover',
        disabled: !!d, reason: d ? d.reason : null,
        onClick: function () { VS.nav.go('pairing', { carrier: 'discover' }); }
      });
    }
    items.push({
      label: '邀请码配对', sub: '生成一次性邀请码', icon: 'key',
      disabled: !!d, reason: d ? d.reason : null,
      onClick: function () { VS.nav.go('pairing', { carrier: 'invite' }); }
    });
    items.push({
      label: '手动输入地址', sub: '跨网段兜底', icon: 'edit',
      disabled: !!d, reason: d ? d.reason : null,
      onClick: function () { VS.nav.go('pairing', { carrier: 'manual' }); }
    });
    MUI.actionSheet({
      title: '添加设备',
      sub: cameraOk()
        ? '载体顺序：扫码 / 发现式 / 邀请码 / 手动'
        : '相机不可用：扫码载具不渲染 · 顺序为 发现式 / 邀请码 / 手动',
      items: items
    });
  }

  VS.pages['devices'] = function (ctx) {
    var self = null, peers = [];
    (D.devices || []).forEach(function (dev) { if (dev.self) self = dev; else peers.push(dev); });

    var nodes = [];
    var gate = writeGateNote();
    if (gate) nodes.push(gate);

    if (!isForeground()) {
      nodes.push(MUI.limitedNote('本端未处于前台：P2P 入站监听已暂停，已配对设备一律显示「待唤醒」而非「在线」。'));
    }

    /* --- 本机 --- */
    if (self) {
      nodes.push(mcard('本机', '当前实例的设备身份；指纹用于配对时与对端核对。', h('div', { class: 'col gap-3' }, [
        MUI.mkv([
          { k: '设备名', v: self.name },
          { k: '手机型号', v: 'Android 15 · ' + (self.os || '—') },
          { k: '监听端口', v: F.num(self.port), mono: true },
          { k: '协议版本', v: self.version, mono: true },
          { k: '监听路径', v: '直连（本端前台时持续监听入站）' }
        ]),
        h('div', { class: 'row gap-2' }, [UI.pathBadge('direct'), UI.badge({ text: isForeground() ? '前台监听中' : '前台监听已暂停', tone: isForeground() ? 'success' : 'warn' })]),
        h('div', { class: 'col gap-2' }, [
          h('div', { class: 'msection-title' }, [h('span', { text: '设备指纹' })]),
          h('div', { class: 't-mono' }, MUI.copyField(self.fingerprint, '复制本机指纹'))
        ])
      ])));
    }

    /* --- 已配对设备 --- */
    var rows = peers.map(function (dev, idx) {
      var meta = [];
      meta.push('上次同步 ' + F.relative(dev.lastSyncMs));
      meta.push('延迟 ' + (typeof dev.latencyMs === 'number' ? F.num(dev.latencyMs) + ' ms' : '—'));
      if (dev.reason) meta.push('降级：' + pathReasonText(dev.reason));

      var note = h('div', {
        class: 'mr-sub', style: { padding: '0 0 0 0', display: dev.reason ? '' : 'none' },
        text: dev.reason ? (pathReasonText(dev.reason) + ' — ' + pathReasonHint(dev.reason)) : ''
      });
      var timer = null;
      function schedule() {
        if (timer) clearTimeout(timer);
        timer = setTimeout(function () { note.style.display = 'none'; timer = null; }, 3000);
      }
      if (dev.reason) schedule();

      var row = MUI.mrow({
        icon: 'devices',
        title: dev.name,
        badges: [
          h('span', { class: 'status-dot ' + (dev.online && isForeground() ? 'online' : 'warn'), title: deviceStatusTitle(dev) }),
          UI.badge({ text: deviceStatusLabel(dev), tone: dev.online && isForeground() ? 'success' : 'warn' })
        ],
        sub: meta.join(' · '),
        trail: [UI.pathBadge(dev.path, dev.reason ? pathReasonText(dev.reason) : null)],
        onClick: function () { openDevice(dev.id); },
        onLongPress: function () { openDeviceSheet(dev); }
      });
      /* 行下方路径降级说明：3 秒后自动收起；点击路径徽标可再次展开 */
      row.addEventListener('click', function () {
        if (!dev.reason) return;
        note.style.display = '';
        schedule();
      });
      return h('div', {
        class: 'col', style: {
          gap: 0, borderTop: idx ? '1px solid var(--c-border)' : 'none',
          paddingBottom: dev.reason ? '8px' : 0
        }
      }, [row, note]);
    });

    nodes.push(mcard(
      '已配对设备',
      '共 ' + F.num(peers.length) + ' 台 · 在线 ' + F.num(peers.filter(function (x) { return x.online; }).length) + ' 台',
      rows.length ? MUI.mlist(rows) : MUI.empty({ icon: 'devices', title: '还没有已配对设备', desc: '用右下角按钮发起配对。' }),
      [UI.badge({ text: '长按行 = 动作面板', tone: 'outline' })]
    ));

    /* --- 发现到（未配对）--- */
    if (S.cap('CAP_DISCOVERY')) {
      var discovered = D.discovered || [];
      var cards = discovered.map(function (dd) {
        return h('div', { class: 'discover-card', dataset: { path: dd.path }, title: '局域网设备 · 指纹 ' + dd.fingerprint }, [
          h('span', { class: 'dc-ico', html: VS.icon('discover', 22) }),
          h('div', { class: 'grow' }, [
            h('div', { class: 't-mono', text: '局域网设备 · 指纹 ' + dd.fingerprint }),
            h('div', {
              class: 't-caption dc-path',
              text: PATH_LABEL[dd.path] + ' · 候选地址 ' + F.num((dd.addrs || []).length) + ' 个 · 类型 ' +
                (dd.addrs || []).map(function (a) { return String(a.type).toUpperCase(); }).join(' / ')
            }),
            h('div', { class: 't-caption', text: '未配对设备只显示指纹，不显示 IP / 端口 / 名称。' })
          ]),
          h('button', {
            class: 'btn btn-sm btn-primary', type: 'button', title: '与该设备进入配对向导',
            'aria-label': '与局域网设备 ' + dd.fingerprint + ' 配对',
            onclick: function () { VS.nav.go('pairing', { carrier: 'discover' }); }
          }, [h('span', { html: VS.icon('link', 13) }), h('span', { text: '配对' })])
        ]);
      });
      nodes.push(mcard(
        '发现到（未配对）',
        '共 ' + F.num(discovered.length) + ' 台（仅局域网候选）',
        h('div', { class: 'col gap-2' }, cards.length ? cards : [
          UI.empty({ icon: 'discover', title: '未被发现', desc: '局域网发现本次未返回候选。' })
        ])
      ));
    } else {
      nodes.push(mcard(
        '局域网发现不可用',
        '能力位 CAP_DISCOVERY 未置位：不渲染任何发现结果',
        h('div', { class: 'col gap-3' }, [
          UI.alertbar({
            tone: 'info', icon: 'ban', title: '局域网发现不可用',
            text: '能力位 CAP_DISCOVERY 未置位，本端不进行自动发现，也不渲染任何发现结果。可改用下列降级入口完成配对。'
          }),
          h('div', { class: 'col gap-2' }, [
            mbtn({ label: '邀请码', icon: 'key', onClick: function () { VS.nav.go('pairing', { carrier: 'invite' }); } }),
            cameraOk()
              ? mbtn({ label: '扫码', icon: 'qr', onClick: function () { VS.nav.go('pairing', { carrier: 'qr' }); } })
              : null,
            mbtn({ label: '手动输入地址', icon: 'edit', onClick: function () { VS.nav.go('pairing', { carrier: 'manual' }); } }),
            cameraOk() ? null : caption('相机不可用或权限被拒：不渲染「扫码」载具（不是置灰）。')
          ])
        ])
      ));
    }

    var overviewItems = [
      { icon: 'devices', label: '在线设备', value: F.num(onlineDeviceCount()) }
    ];
    if (S.cap('CAP_DISCOVERY')) {
      overviewItems.push({ icon: 'discover', label: '发现到', value: F.num((D.discovered || []).length) + ' 台' });
    } else {
      overviewItems.push({ icon: 'ban', label: '局域网发现', value: '不可用' });
    }

    return MUI.page({
      title: '设备',
      back: false,
      tab: true,
      actions: [
        { icon: 'refresh', label: '立即同步', onClick: function () { VS.actions['syncNow']({ all: true }); } }
      ],
      overflow: [
        { label: '路径诊断', sub: '三档路径与降级原因', icon: 'discover', onClick: function () { openPathDiag(null); } },
        { group: '演示控制台' },
        {
          label: '本端前台监听', sub: isForeground() ? '前台：可显示「在线」' : '后台：设备一律「待唤醒」',
          icon: 'wifi', onClick: function () { VS.actions['mobile.foreground'](); }
        },
        {
          label: '相机可用性', sub: cameraOk() ? '可用：渲染「扫码」载具' : '不可用：不渲染「扫码」载具',
          icon: 'qr', onClick: function () { VS.actions['mobile.camera'](); }
        }
      ],
      overview: MUI.overviewBar(overviewItems),
      body: MUI.screen(nodes),
      fab: MUI.fab({ icon: 'link', label: '添加设备', onClick: openAddDeviceSheet }),
      onBack: function () { VS.nav.back(); }
    });
  };

  /* ==========================================================================
   * 6. 页面：device（设备详情，全屏路由）
   * ========================================================================*/

  VS.pages['device'] = function (ctx) {
    var devId = ctx.params && ctx.params.deviceId;
    var dev = null;
    (D.devices || []).forEach(function (x) { if (x.id === devId) dev = x; });
    if (!dev) dev = (D.devices || []).filter(function (x) { return !x.self; })[0];

    if (!dev) {
      return MUI.page({
        title: '设备详情', back: true, tab: false,
        body: MUI.screen([MUI.empty({ icon: 'devices', title: '设备不存在', desc: '节点清单中没有该设备。' })]),
        onBack: function () { VS.nav.back(); }
      });
    }

    var records = deviceSyncRecords(dev, 20);
    var d = denyInfo();
    var nodes = [];
    var gate = writeGateNote();
    if (gate) nodes.push(gate);

    nodes.push(mcard('身份与信道', 'Noise 信道 / 指纹 / 协议版本', h('div', { class: 'col gap-3' }, [
      MUI.mkv([
        { k: '设备名', v: dev.name },
        { k: '操作系统', v: dev.os || '—' },
        { k: '协议版本', v: dev.version || '—', mono: true },
        { k: '信道', v: 'Noise_XX_25519_ChaChaPoly_BLAKE2s', mono: true },
        { k: '握手角色', v: '发起方（本机）→ 响应方（' + dev.name + '）' },
        { k: '静态公钥绑定', v: dev.trust === 'verified' ? '已绑定并校验通过' : '未绑定' },
        { k: '监听端口', v: dev.port ? F.num(dev.port) : '—（不监听，仅出站）' },
        { k: '在线状态', v: deviceStatusLabel(dev) }
      ]),
      h('div', { class: 'row gap-2' }, [
        h('span', { class: 'status-dot ' + (dev.online && isForeground() ? 'online' : 'warn') }),
        h('span', { class: 't-caption', text: deviceStatusTitle(dev) })
      ]),
      h('div', { class: 'col gap-2' }, [
        h('div', { class: 'msection-title' }, [h('span', { text: '指纹（全量）' })]),
        h('div', { class: 't-mono', style: { wordBreak: 'break-all' }, text: dev.fingerprint }),
        MUI.copyField(dev.fingerprint, '复制设备指纹')
      ])
    ])));

    nodes.push(mcard('路径状态机', '当前态与降级原因', h('div', { class: 'col gap-3' }, [
      MUI.mkv([
        { k: '当前态', v: PATH_LABEL[dev.path || 'direct'] },
        { k: '降级原因', v: dev.reason ? pathReasonText(dev.reason) : '无（未发生降级）' },
        { k: '原因说明', v: dev.reason ? pathReasonHint(dev.reason) : '—' },
        { k: '延迟', v: typeof dev.latencyMs === 'number' ? F.num(dev.latencyMs) + ' ms' : '—' }
      ]),
      h('div', { class: 'row gap-2 wrap' }, PATH_ORDER.map(function (p) {
        var active = (dev.path || 'direct') === p;
        return UI.badge({ text: PATH_LABEL[p] + (active ? '（当前）' : ''), tone: active ? 'accent' : 'outline' });
      })),
      mbtn({ label: '打开路径诊断', icon: 'discover', variant: 'ghost', onClick: function () { openPathDiag(dev); } })
    ])));

    nodes.push(mcard('最近 20 条同步记录', '由队列确定性派生（原型无设备级历史接口）', MUI.mlist(records.map(function (r) {
      return MUI.mrow({
        icon: dirIcon(r.dir),
        title: r.name,
        sub: F.dateLong(r.timeMs) + ' · ' + dirLabel(r.dir) + ' · ' + F.bytes(r.size),
        trail: [
          r.state === 'done'
            ? UI.badge({ text: '成功', tone: 'success' })
            : UI.badge({ text: '失败 · ' + (VS.ERR[r.errCode] ? VS.ERR[r.errCode].title : '未知'), tone: 'danger' }),
          UI.pathBadge(r.path)
        ],
        onClick: function () { openTask((D.queue[0] || {}).taskId); }
      });
    }))));

    nodes.push(h('div', { class: 'col gap-2' }, [
      d ? MUI.limitedNote(d.reason + ' 以下写操作已禁用。') : null,
      mbtn({
        label: '立即同步', variant: 'primary', icon: 'refresh',
        disabled: !!d || !dev.online, reason: d ? d.reason : (!dev.online ? '设备未确认在线（持续监听仅前台有效）' : null),
        onClick: function () { VS.actions['syncNow']({ device: dev.id }); }
      }),
      mbtn({ label: '路径诊断', icon: 'discover', onClick: function () { openPathDiag(dev); } }),
      mbtn({
        label: '远程锁定…', icon: 'lock', disabled: !!d, reason: d ? d.reason : null,
        onClick: function () { openRemoteLock(dev); }
      }),
      mbtn({
        label: '解除配对…', icon: 'trash', variant: 'danger', disabled: !!d, reason: d ? d.reason : null,
        onClick: function () { confirmUnpair(dev, function () { VS.nav.back(); }); }
      })
    ]));

    return MUI.page({
      title: '设备详情',
      sub: dev.name,
      back: true,
      tab: false,
      actions: [{ icon: 'refresh', label: '立即同步', onClick: function () { VS.actions['syncNow']({ device: dev.id }); } }],
      overflow: [
        { label: '路径诊断', icon: 'discover', onClick: function () { openPathDiag(dev); } },
        { label: '远程锁定…', icon: 'lock', danger: true, disabled: !!d, reason: d ? d.reason : null, onClick: function () { openRemoteLock(dev); } }
      ],
      body: MUI.screen(nodes),
      onBack: function () { VS.nav.back(); }
    });
  };

  /* ==========================================================================
   * 7. 冲突解决（按块合并 ≠ 语义合并）
   * ========================================================================*/

  function conflictView(onResolved) {
    var sides = conflictSides();
    if (!sides) {
      return MUI.empty({ icon: 'columns', title: '当前没有冲突', desc: '两方版本一致时不会生成冲突副本。' });
    }
    function side(f, keepFlag, title) {
      return h('div', { class: 'conflict-side', dataset: { keep: keepFlag ? 'true' : 'false' } }, [
        h('div', { class: 'col gap-2' }, [
          h('div', { class: 't-strong', text: title }),
          h('div', { class: 't-truncate', text: f.name, title: f.name }),
          MUI.mkv([
            { k: '来源设备', v: f.device || '—' },
            { k: '大小', v: F.bytes(f.size) },
            { k: '修改时间', v: F.dateLong(f.modifiedMs) },
            { k: '版本 rev', v: f.rev, mono: true }
          ])
        ])
      ]);
    }
    var note = h('div', { class: 'col gap-2' });
    function resolve(strategy) {
      var indexUnavailable = strategy === 'merge' && (ui.mergeFailDemo || !sides.keep.chunkCount || !sides.drop.chunkCount);
      note.innerHTML = '';
      if (indexUnavailable) {
        note.appendChild(UI.alertbar({
          tone: 'danger', icon: 'danger', title: '解决失败',
          text: '按块合并未能完成（块索引不可用）。两个版本均已保留，未发生覆盖；可改用「保留此版本」或「保留两者」。'
        }));
        return;
      }
      var msg = strategy === 'merge'
        ? '已按块合并：以加密块为单位择优写入，非语义合并。'
        : strategy === 'keep_both'
          ? '已保留两者：对端版本另存为冲突副本，不覆盖任何一方。'
          : '已保留所选版本，另一版本进入保护副本。';
      note.appendChild(UI.alertbar({ tone: 'success', icon: 'check-circle', title: '冲突已解决', text: msg }));
      UI.toast({ tone: 'success', title: '冲突已解决', msg: msg });
      if (onResolved) onResolved();
    }
    return h('div', { class: 'col gap-3' }, [
      h('div', { class: 'conflict-pair' }, [
        side(sides.keep, true, '本机版本'),
        h('div', { class: 'conflict-vs', text: 'VS' }),
        side(sides.drop, false, '对端版本')
      ]),
      h('div', { class: 'col gap-2' }, [
        mbtn({ label: '保留此版本', icon: 'check', onClick: function () { resolve('keep'); } }),
        mbtn({ label: '保留两者', icon: 'columns', onClick: function () { resolve('keep_both'); } }),
        mbtn({ label: '按块合并', icon: 'layers', variant: 'primary', onClick: function () { resolve('merge'); } })
      ]),
      caption('「按块合并」以加密块（CDC 块）为单位择优写入，不做任何语义层合并。'),
      h('button', {
        class: 'btn btn-sm btn-ghost', type: 'button',
        title: '演示：模拟块索引不可用时的合并失败路径',
        'aria-label': '演示：块索引不可用',
        onclick: function () { ui.mergeFailDemo = !ui.mergeFailDemo; repaint(); }
      }, [h('span', { text: ui.mergeFailDemo ? '演示开关：块索引不可用（开）' : '演示开关：块索引不可用（关）' })]),
      note
    ]);
  }

  /* ==========================================================================
   * 8. 页面：sync（Tab）
   * ========================================================================*/

  function quickEntries(ctx) {
    var rows = [
      { icon: 'queue', title: '传输队列', sub: '全部分类 · 甘特视图 · 批量操作', route: 'queue' },
      { icon: 'settings', title: '同步策略', sub: '模式 / 时段 / 带宽 / 冲突 / 填充', route: 'sync-policy' }
    ];
    if (S.cap('CAP_BURN_SHARE')) {
      rows.push({ icon: 'erase', title: '阅后即焚', sub: '会话列表 / 时效 / 时间线', route: 'shares' });
    }
    var nodes = rows.map(function (r) {
      return MUI.mrow({
        icon: r.icon, title: r.title, sub: r.sub,
        onClick: function () { ctx.go(r.route); }
      });
    });
    nodes.push(MUI.mrow({
      icon: 'devices', title: '设备', sub: '切到设备 Tab',
      onClick: function () { ctx.go('devices'); }
    }));
    return MUI.mlist(nodes);
  }

  VS.pages['sync'] = function (ctx) {
    var st = queueStats();
    var capTasks = S.cap('CAP_TASKS');
    var hasQueue = st.count > 0;
    var nodes = [];
    var gate = writeGateNote();
    if (gate) nodes.push(gate);

    /* --- 五项总览卡（无数据来源显示「—」，不把「无数据」画成 0） --- */
    nodes.push(MUI.mstats([
      {
        label: '在线设备', value: F.num(onlineDeviceCount()),
        src: 'vault_core_p2p_status().peers[].online'
      },
      {
        label: '队列深度', value: (capTasks && hasQueue) ? (F.num(st.active) + ' + ' + F.num(st.queued)) : '—',
        src: capTasks ? 'TASK_QUEUED(6) / TASK_PROGRESS(7)' : null
      },
      {
        label: '冲突数', value: F.num(conflictCount()),
        src: 'conflict_pair().count'
      },
      {
        label: '双向速率', value: capTasks ? ('↑ ' + (F.rate(st.up) || '—') + '  ↓ ' + (F.rate(st.down) || '—')) : '—',
        src: capTasks ? 'TASK_PROGRESS(7).rateBps' : null
      },
      {
        label: '已同步字节', value: st.doneBytes === null ? '—' : (F.bytes(st.doneBytes) + ' / ' + F.bytes(st.totalBytes)),
        src: 'TASK_DONE(8).bytes'
      }
    ]));
    caption('任一指标无数据来源时显示「—」；「0」与「无数据」严格区分。');

    /* --- 总进度环 --- */
    nodes.push(mcard('总体进度', 'Σ doneBytes / Σ totalBytes', h('div', { class: 'row gap-4' }, [
      MUI.ring({
        size: 92, stroke: 9, pct: st.pct,
        tone: st.pct === null ? 'default' : 'accent',
        label: st.pct === null ? '空闲' : F.pct(st.pct)
      }),
      h('div', { class: 'col gap-1 grow' }, [
        h('div', { class: 't-strong', text: st.pct === null ? '空闲（队列无字节总量）' : ('已完成 ' + F.pct(st.pct)) }),
        caption('分母为 0 时画空环并显示「空闲」。'),
        caption('活动 ' + F.num(st.active) + ' · 排队 ' + F.num(st.queued) + ' · 合计 ' + F.num(st.count) + ' 条'),
        st.eta ? caption('预计剩余 ' + F.duration(st.eta)) : null
      ])
    ])));

    /* --- 传输分节：上传 / 下载 / 冲突 --- */
    if (!capTasks) {
      nodes.push(UI.alertbar({
        tone: 'info', icon: 'ban', title: '任务队列未启用',
        text: '能力位 CAP_TASKS 未置位：不渲染传输队列、速率与 ETA；总览条上的速率项同样整块隐藏。'
      }));
    } else {
      ['up', 'down'].forEach(function (dir) {
        var list = (D.queue || []).filter(function (t) {
          return t.dir === dir && t.state !== 'done' && t.state !== 'cancelled';
        });
        nodes.push(mcard(
          dirLabel(dir),
          F.num(list.length) + ' 条进行中 / 待处理',
          list.length
            ? h('div', { class: 'col' }, list.map(function (t) { return queueRowNode(t, { onChange: function () { repaint(); } }); }))
            : MUI.empty({ icon: dirIcon(dir), title: '没有' + dirLabel(dir) + '任务', desc: '队列为空时不渲染任何进度条。' })
        ));
      });
      nodes.push(caption('分节只列出进行中 / 待处理任务；已完成与已取消请在「传输队列」页按分页查看（共 ' + F.num(st.count) + ' 条）。'));
    }

    nodes.push(mcard('冲突', conflictCount() ? '存在 1 组冲突副本，需要人工选择保留策略' : '没有冲突', conflictView(function () { repaint(); })));

    nodes.push(h('div', { class: 'col gap-2' }, [msection('快捷入口'), quickEntries(ctx)]));

    /* --- 总览条（§4.3）：速率区在 CAP_TASKS 未置位时整块隐藏 --- */
    var ob = [
      { icon: 'devices', label: '在线设备', value: F.num(onlineDeviceCount()) },
      { icon: 'queue', label: '队列', value: (capTasks && hasQueue) ? F.num(st.count) : '—' },
      { icon: 'columns', label: '冲突', value: F.num(conflictCount()) }
    ];
    if (capTasks && (st.up || st.down)) {
      ob.push({ icon: 'arrow-up', label: '↑', value: F.rate(st.up) || '—', tone: 'up' });
      ob.push({ icon: 'arrow-down', label: '↓', value: F.rate(st.down) || '—', tone: 'down' });
    }

    return MUI.page({
      title: '同步',
      back: false,
      tab: true,
      actions: [{ icon: 'refresh', label: '立即同步', onClick: function () { VS.actions['syncNow']({}); } }],
      overflow: [
        { label: '同步策略', sub: '模式 / 时段 / 带宽 / 冲突 / 填充', icon: 'settings', onClick: function () { ctx.go('sync-policy'); } },
        { label: '传输队列', sub: '全部分类与甘特视图', icon: 'queue', onClick: function () { ctx.go('queue'); } },
        S.cap('CAP_BURN_SHARE')
          ? { label: '阅后即焚', sub: '会话与时间线', icon: 'erase', onClick: function () { ctx.go('shares'); } }
          : null,
        { label: '路径诊断', icon: 'discover', onClick: function () { openPathDiag(null); } }
      ].filter(Boolean),
      overview: MUI.overviewBar(ob),
      body: MUI.screen(nodes),
      fab: null,
      onBack: function () { VS.nav.back(); }
    });
  };

  /* ==========================================================================
   * 9. 页面：queue（队列页，全屏路由）
   * ========================================================================*/

  var QUEUE_FILTERS = [
    { value: 'all', label: '全部' },
    { value: 'up', label: '上传' },
    { value: 'down', label: '下载' },
    { value: 'conflict', label: '冲突' },
    { value: 'done', label: '已完成' },
    { value: 'failed', label: '失败' }
  ];

  function applyQueueFilter(list, v) {
    if (v === 'up') return list.filter(function (t) { return t.dir === 'up'; });
    if (v === 'down') return list.filter(function (t) { return t.dir === 'down'; });
    if (v === 'done') return list.filter(function (t) { return t.state === 'done'; });
    if (v === 'failed') return list.filter(function (t) { return t.state === 'failed' || t.state === 'interrupted'; });
    return list;
  }

  /** 甘特式横向视图（.gantt / .gantt-row / .gantt-track / .gantt-bar，横向可滚） */
  function ganttView(tasks) {
    var list = (tasks || []).filter(function (t) { return t.state !== 'cancelled'; });
    if (!list.length) return MUI.empty({ icon: 'gantt', title: '暂无时间线数据', desc: '队列为空时不渲染甘特轨道。' });
    var minStart = null, maxEnd = null;
    list.forEach(function (t) {
      var s = t.startedMs || now();
      var dur = t.etaMs || (t.state === 'done' ? 120000 : 300000);
      var e = s + dur;
      if (minStart === null || s < minStart) minStart = s;
      if (maxEnd === null || e > maxEnd) maxEnd = e;
    });
    var span = Math.max(1, maxEnd - minStart);
    var rows = list.map(function (t) {
      var s = t.startedMs || now();
      var dur = t.etaMs || (t.state === 'done' ? 120000 : 300000);
      var left = U.clamp((s - minStart) / span, 0, 1) * 100;
      var width = U.clamp(dur / span, 0.02, 1) * 100;
      return h('div', { class: 'gantt-row', title: t.name + ' · ' + dirLabel(t.dir) + ' · ' + F.duration(dur) }, [
        h('div', { class: 'row gap-2', style: { minWidth: 0 } }, [
          h('span', { class: 'qr-ico', html: VS.icon(dirIcon(t.dir), 14), title: dirLabel(t.dir) }),
          h('span', { class: 't-truncate', text: t.name, title: t.name })
        ]),
        h('div', { class: 'gantt-track' }, [h('div', {
          class: 'gantt-bar', dataset: { dir: t.dir, state: t.state },
          style: { left: left.toFixed(2) + '%', width: width.toFixed(2) + '%' },
          title: t.name + ' · ' + (TASK_STATE[t.state] ? TASK_STATE[t.state].label : t.state) + ' · ' + F.duration(dur)
        })])
      ]);
    });
    return h('div', { style: { overflowX: 'auto' } }, [
      h('div', { class: 'gantt', style: { minWidth: '520px' } }, [
        h('div', { class: 'gantt-row' }, [
          h('div', { class: 't-caption', text: '任务时间线' }),
          h('div', { class: 'gantt-axis' }, [
            h('span', { text: F.dateShort(minStart) }),
            h('span', { text: '现在 ' + F.dateShort(now()) }),
            h('span', { text: F.dateShort(maxEnd) })
          ])
        ]),
        h('div', { class: 'col gap-1' }, rows)
      ])
    ]);
  }

  VS.pages['queue'] = function (ctx) {
    var filter = ui.queueFilter || (ctx.params && ctx.params.filter) || 'all';
    var d = denyInfo();
    var all = D.queue || [];
    var nodes = [];
    var gate = writeGateNote();
    if (gate) nodes.push(gate);

    nodes.push(MUI.mtabs({
      value: filter,
      items: QUEUE_FILTERS.map(function (f) {
        var n = f.value === 'conflict' ? conflictCount() : applyQueueFilter(all, f.value).length;
        return { value: f.value, label: f.label + ' ' + F.num(n) };
      }),
      onChange: function (v) { filter = v; ui.queueFilter = v; repaint(); }
    }));

    nodes.push(mcard('批量操作', '受写门禁约束：只读降级 / 维护态下一律禁用', h('div', { class: 'col gap-2' }, [
      mbtn({
        label: '全部暂停', icon: 'pause', disabled: !!d, reason: d ? d.reason : null,
        onClick: function () { VS.actions['pauseAll'](); }
      }),
      mbtn({
        label: '全部恢复', icon: 'play', disabled: !!d, reason: d ? d.reason : null,
        onClick: function () { VS.actions['resumeAll'](); }
      }),
      mbtn({
        label: '清理已完成', icon: 'trash', disabled: !!d, reason: d ? d.reason : null,
        onClick: function () { VS.actions['clearDone'](); }
      })
    ])));

    if (filter === 'conflict') {
      nodes.push(mcard('冲突', '按块合并 ≠ 语义合并', conflictView(function () { repaint(); })));
    } else {
      var list = applyQueueFilter(all, filter);
      nodes.push(mcard('甘特式时间线', '按任务起止时间投影；已取消任务不绘制（横向可滚）', ganttView(list)));
      nodes.push(mcard('任务列表', F.num(list.length) + ' 条 · 左滑可暂停 / 取消', list.length
        ? h('div', { class: 'col' }, list.map(function (t) { return queueSwipeRow(t, { onChange: function () { repaint(); } }); }))
        : MUI.empty({
          icon: 'queue', title: '该分类下没有任务',
          desc: filter === 'failed' ? '没有失败或中断的任务。' : '切换分页查看更多任务。'
        })));
    }

    return MUI.page({
      title: '传输队列',
      sub: '全部 / 上传 / 下载 / 冲突 / 已完成 / 失败',
      back: true,
      tab: false,
      actions: [{ icon: 'refresh', label: '立即同步', onClick: function () { VS.actions['syncNow']({}); } }],
      overflow: [
        { label: '同步策略', icon: 'settings', onClick: function () { ctx.go('sync-policy'); } },
        { label: '返回同步总览', icon: 'sync', onClick: function () { ctx.go('sync'); } }
      ],
      body: MUI.screen(nodes),
      onBack: function () { VS.nav.back(); }
    });
  };

  /* ==========================================================================
   * 10. 页面：task（单文件同步详情，全屏路由）
   * ========================================================================*/

  function findTask(id) {
    var hit = null;
    (D.queue || []).forEach(function (t) { if (t.taskId === id) hit = t; });
    return hit || (D.queue || [])[0] || null;
  }

  VS.pages['task'] = function (ctx) {
    var t = findTask(ctx.params && ctx.params.taskId);
    if (!t) {
      return MUI.page({
        title: '同步详情', back: true, tab: false,
        body: MUI.screen([MUI.empty({ icon: 'queue', title: '任务不存在', desc: '队列中没有该任务。' })]),
        onBack: function () { VS.nav.back(); }
      });
    }
    var totalChunks = Math.max(1, t.totalChunks || 1);
    var shown = Math.min(totalChunks, 240);
    var ratio = t.totalBytes > 0 ? (t.doneBytes || 0) / t.totalBytes : 0;
    var doneCount = Math.round(shown * ratio);
    var chunks = [];
    for (var i = 0; i < shown; i++) {
      if (i < doneCount) chunks.push('done');
      else if (t.state === 'failed' && i === doneCount) chunks.push('failed');
      else if (t.state === 'running' && i === doneCount) chunks.push('running');
      else chunks.push('pending');
    }
    var st = TASK_STATE[t.state] || { label: t.state };
    var d = denyInfo();
    var nodes = [];
    var gate = writeGateNote();
    if (gate) nodes.push(gate);

    nodes.push(MUI.mstats([
      { label: '已传输', value: F.bytes(t.doneBytes || 0) + ' / ' + F.bytes(t.totalBytes), src: 'TASK_PROGRESS(7).doneBytes' },
      { label: '进度', value: t.totalBytes > 0 ? F.pct(ratio) : '—', src: 'doneBytes / totalBytes' },
      { label: '速率', value: F.rate(t.rateBps) || '—', src: 'TASK_PROGRESS(7).rateBps' },
      { label: '剩余时间', value: t.etaMs ? F.duration(t.etaMs) : '—', src: 'TASK_PROGRESS(7).etaMs' }
    ]));

    nodes.push(mcard('CDC 块矩阵', '绿=已完成 · 蓝=传输中 · 红=失败 · 空=待传', h('div', { class: 'col gap-2' }, [
      MUI.chunkMatrix(chunks),
      caption('绘制 ' + F.num(shown) + ' 块（总 ' + F.num(totalChunks) + ' 块，超过 240 块仅绘制前 240 块）。')
    ])));

    nodes.push(mcard('断点续传与分片', '跨会话续传以已落盘块边界为准', MUI.mkv([
      { k: '可续传', v: t.resumable ? '是' : '否' },
      { k: '续传位置', v: (t.resumePct !== null && t.resumePct !== undefined) ? (F.pct(t.resumePct) + '（跨会话）') : '—' },
      { k: '分片数', v: F.num(t.totalChunks || 0) + '（已完成 ' + F.num(t.doneChunks || 0) + '）', mono: true },
      { k: '块大小', v: F.bytes(CHUNK_SIZE) + '（CDC 均值，原型定值）' },
      { k: 'nonce 前缀', v: D.hash(16).slice(0, 16), mono: true }
    ])));

    nodes.push(mcard('路径与降级', '当前路径可能已降级', h('div', { class: 'col gap-2' }, [
      h('div', { class: 'row gap-2' }, [UI.pathBadge(t.path), h('span', { class: 't-caption', text: PATH_LABEL[t.path] })]),
      caption('降级原因：' + (dev_reason_for(t) ? pathReasonText(dev_reason_for(t)) + ' — ' + pathReasonHint(dev_reason_for(t)) : '未发生降级。')),
      mbtn({ label: '打开路径诊断', icon: 'discover', variant: 'ghost', onClick: function () { openPathDiag(null); } })
    ])));

    nodes.push(mcard('任务信息', '字段来自任务载荷', MUI.mkv([
      { k: '文件', v: t.name },
      { k: '方向', v: dirLabel(t.dir) },
      { k: '对端', v: t.peer || '—' },
      { k: '状态', v: st.label },
      { k: '优先级', v: t.priority === 'interactive' ? '交互' : (t.priority || '—') },
      { k: '开始时间', v: F.dateLong(t.startedMs) },
      { k: 'errCode', v: t.errCode === null ? '—' : (F.num(t.errCode) + ' · ' + (VS.ERR[t.errCode] ? VS.ERR[t.errCode].title : '未登记')), mono: true }
    ])));

    nodes.push(mcard('事件来源', '任务进度由事件流驱动', h('div', { class: 'col gap-2' }, [
      h('div', { class: 't-mono', style: { wordBreak: 'break-all' }, text: 'TASK_QUEUED(6) → TASK_PROGRESS(7) → ' + (t.state === 'done' ? 'TASK_DONE(8)' : t.state === 'failed' ? 'TASK_FAILED(9)' : t.state === 'cancelled' ? 'TASK_CANCELLED(10)' : 'TASK_PAUSED/RESUMED(11)') }),
      caption('能力位 CAP_TASKS 未置位时本页不渲染块级进度。')
    ])));

    if (t.state === 'failed' || t.state === 'interrupted') {
      nodes.push(taskErrorLine(t, function () { repaint(); }));
    }

    return MUI.page({
      title: '同步详情',
      sub: t.name,
      back: true,
      tab: false,
      actions: [{ icon: 'refresh', label: '立即同步', onClick: function () { VS.actions['syncNow']({ device: t.peer }); } }],
      overflow: [
        {
          label: '重试', icon: 'refresh', disabled: !!d || (t.retries || 0) >= RETRY_LIMIT,
          reason: d ? d.reason : ((t.retries || 0) >= RETRY_LIMIT ? '重试已达上限：请检查网络或磁盘空间' : null),
          onClick: function () { t.retries = (t.retries || 0) + 1; t.state = 'running'; t.errCode = null; repaint(); }
        },
        {
          label: '取消任务', icon: 'cancel', danger: true, disabled: !!d, reason: d ? d.reason : null,
          onClick: function () { t.state = 'cancelled'; t.errCode = 12; t.rateBps = 0; repaint(); }
        }
      ],
      body: MUI.screen(nodes),
      onBack: function () { VS.nav.back(); }
    });
  };

  /** 任务所在对端的降级原因（用于任务页展示；无则不编造） */
  function dev_reason_for(t) {
    var r = null;
    (D.devices || []).forEach(function (x) { if (x.name === t.peer && x.reason) r = x.reason; });
    return r;
  }

  /* ==========================================================================
   * 11. 页面：sync-policy（同步策略，全屏路由）
   * ========================================================================*/

  function readSyncPolicy() {
    try {
      if (!D.syncPolicy) throw new Error('syncPolicy 缺失');
      var p = JSON.parse(JSON.stringify(D.syncPolicy));
      if (!p.folderIds) p.folderIds = [];
      if (!p.excludeFolderIds) p.excludeFolderIds = [];
      return { ok: true, policy: p };
    } catch (e) {
      return {
        ok: false,
        policy: {
          enabled: true, mode: 'auto', timeWindowEnabled: false, windowStart: '22:00', windowEnd: '07:00',
          bandwidthUpKbps: 0, bandwidthDownKbps: 0, wifiOnly: false, meteredAllowed: true,
          excludePatterns: [], maxConcurrent: 3, conflictStrategy: 'keep_both', verifyAfterSync: true,
          retryLimit: 3, relayEnabled: true, relayFallbackReason: null, paddingLevel: 2,
          activeHoursOnly: false, folderIds: [], excludeFolderIds: []
        }
      };
    }
  }

  /** 策略行：标题 + 说明 + 右侧控件（.mlist 内的行，控件不弹浮层则不会被裁剪） */
  function policyRow(title, sub, control) {
    return h('div', { class: 'mrow', style: { cursor: 'default' }, title: title }, [
      h('div', { class: 'mr-main' }, [
        h('div', { class: 'mr-title' }, [h('span', { text: title })]),
        sub ? h('div', { class: 'mr-sub', text: sub }) : null
      ]),
      control ? h('span', { class: 'mr-trail' }, [control]) : null
    ]);
  }

  VS.pages['sync-policy'] = function (ctx) {
    if (!ui.policy) {
      var loaded = readSyncPolicy();
      ui.policy = loaded.policy;
      if (!loaded.ok) ui.policyReadFailed = true;
    }
    var p = ui.policy;
    var capPadding = S.cap('CAP_TX_PADDING');
    var capSelective = S.cap('CAP_SELECTIVE_SYNC');
    if (!capPadding) p.paddingLevel = 0;      /* 未置位 → T 恒为 0，且不假装成功 */

    var nodes = [];
    var d = denyInfo();

    /* 首屏固定文案（移动端后台同步为尽力而为） */
    nodes.push(MUI.limitedNote('移动端后台同步为尽力而为：系统可能在省电模式暂停'));
    if (ui.policyReadFailed) {
      nodes.push(UI.alertbar({
        tone: 'warn', icon: 'alert', title: '策略读取失败，使用默认值',
        text: '未能从引擎读到同步策略，本页展示的是内置默认值；保存后以本页为准。'
      }));
    }
    if (ui.policyError) {
      nodes.push(MUI.errorBox(ui.policyError.code, { text: ui.policyError.text }));
    }
    var gate = writeGateNote();
    if (gate) nodes.push(gate);

    /* --- 基础 --- */
    nodes.push(msection('基础'));
    nodes.push(MUI.mlist([
      policyRow('启用同步', p.enabled ? '变更后会参与同步' : '已停用：不发起任何同步',
        MUI.switchCtl({ checked: p.enabled, onChange: function (e) { p.enabled = e.target.checked; } })),
      policyRow('仅 WiFi 下同步', '勾选后计量网络下不发起传输',
        MUI.switchCtl({ checked: p.wifiOnly, onChange: function (e) { p.wifiOnly = e.target.checked; } })),
      policyRow('计量网络允许', '允许在移动数据下同步（受仅 WiFi 约束）',
        MUI.switchCtl({ checked: p.meteredAllowed, onChange: function (e) { p.meteredAllowed = e.target.checked; } }))
    ]));

    nodes.push(h('div', { class: 'col gap-2' }, [
      MUI.field({
        label: '同步模式',
        control: MUI.mseg({
          value: p.mode,
          items: SYNC_MODES.map(function (m) { return { value: m.value, label: m.label }; }),
          onChange: function (v) { p.mode = v; repaint(); }
        }),
        hint: (SYNC_MODES.filter(function (m) { return m.value === p.mode; })[0] || {}).sub
      })
    ]));

    /* --- 时段 --- */
    nodes.push(msection('允许时段'));
    nodes.push(MUI.mlist([
      policyRow('启用时段限制', '仅在下列时段内同步',
        MUI.switchCtl({ checked: p.timeWindowEnabled, onChange: function (e) { p.timeWindowEnabled = e.target.checked; repaint(); } }))
    ]));
    if (p.timeWindowEnabled) {
      var stIn = UI.input({ type: 'time', value: p.windowStart, mono: true });
      var enIn = UI.input({ type: 'time', value: p.windowEnd, mono: true });
      stIn.addEventListener('input', function () { p.windowStart = stIn.value; });
      enIn.addEventListener('input', function () { p.windowEnd = enIn.value; });
      nodes.push(h('div', { class: 'row gap-3' }, [
        h('div', { class: 'grow' }, MUI.field({ label: '起始时间', control: stIn, hint: '跨零点请让起始晚于结束。' })),
        h('div', { class: 'grow' }, MUI.field({ label: '结束时间', control: enIn }))
      ]));
    }

    /* --- 带宽 --- */
    nodes.push(msection('带宽'));
    var upIn = UI.input({ type: 'number', value: p.bandwidthUpKbps, mono: true });
    var downIn = UI.input({ type: 'number', value: p.bandwidthDownKbps, mono: true });
    upIn.addEventListener('input', function () { p.bandwidthUpKbps = parseInt(upIn.value, 10) || 0; });
    downIn.addEventListener('input', function () { p.bandwidthDownKbps = parseInt(downIn.value, 10) || 0; });
    nodes.push(h('div', { class: 'row gap-3' }, [
      h('div', { class: 'grow' }, MUI.field({ label: '上行限速（KB/s）', control: upIn, hint: '0 = 不限速。' })),
      h('div', { class: 'grow' }, MUI.field({ label: '下行限速（KB/s）', control: downIn, hint: '0 = 不限速。' }))
    ]));

    /* --- 并发与冲突（select 放在非裁剪容器内，避免被 .mlist 的 overflow 裁掉） --- */
    nodes.push(msection('并发与冲突'));
    nodes.push(h('div', { class: 'col gap-3' }, [
      MUI.field({
        label: '并发数',
        control: MUI.select({
          block: true, value: p.maxConcurrent,
          options: [1, 2, 3, 4, 5, 6, 7, 8].map(function (n) {
            return { value: n, label: F.num(n), sub: n === 3 ? '默认值' : null };
          }),
          onChange: function (v) { p.maxConcurrent = v; }
        }),
        hint: '默认 3（范围 1–8）；优先级：销毁指令 > 交互 > 后台。'
      }),
      MUI.field({
        label: '冲突策略',
        control: MUI.select({
          block: true, value: p.conflictStrategy, options: CONFLICT_STRATEGIES,
          onChange: function (v) { p.conflictStrategy = v; }
        }),
        hint: (CONFLICT_STRATEGIES.filter(function (x) { return x.value === p.conflictStrategy; })[0] || {}).sub
      })
    ]));

    /* --- 校验与重试 --- */
    nodes.push(msection('校验与重试'));
    nodes.push(MUI.mlist([
      policyRow('同步后校验', '逐块校验哈希，失败块重传',
        MUI.switchCtl({ checked: p.verifyAfterSync, onChange: function (e) { p.verifyAfterSync = e.target.checked; } }))
    ]));
    var retryIn = UI.input({ type: 'number', value: p.retryLimit, mono: true });
    retryIn.addEventListener('input', function () { p.retryLimit = parseInt(retryIn.value, 10) || 0; });
    nodes.push(MUI.field({ label: '重试上限', control: retryIn, hint: '超过后提示「请检查网络或磁盘空间」。' }));

    /* --- 中继与填充 --- */
    nodes.push(msection('中继与填充'));
    nodes.push(MUI.mlist([
      policyRow('允许中继回退',
        p.relayFallbackReason ? ('最近回退原因：' + pathReasonText(p.relayFallbackReason)) : '直连 / 打洞均失败时允许走中继',
        MUI.switchCtl({ checked: p.relayEnabled, onChange: function (e) { p.relayEnabled = e.target.checked; } }))
    ]));

    if (capPadding) {
      nodes.push(MUI.field({
        label: '填充档位 T=0–4',
        control: MUI.select({
          block: true, value: p.paddingLevel, options: PADDING_LEVELS,
          onChange: function (v) { p.paddingLevel = v; repaint(); }
        }),
        hint: (PADDING_LEVELS.filter(function (x) { return x.value === p.paddingLevel; })[0] || {}).sub
      }));
    } else {
      nodes.push(UI.alertbar({
        tone: 'info', icon: 'ban', title: '流量填充不可用',
        text: '能力位 CAP_TX_PADDING 未置位：填充档位恒为 T0（关闭），界面不提供调高，也不显示「已开启填充」。'
      }));
      nodes.push(MUI.mlist([
        policyRow('填充档位', 'T0 · 关闭（不填充；吞吐最高，抗流量分析最弱）',
          UI.badge({ text: 'T' + F.num(0), tone: 'stale' }))
      ]));
    }

    /* --- 排除规则 --- */
    nodes.push(msection('排除规则'));
    var chips = h('div', { class: 'row gap-2 wrap' });
    (p.excludePatterns || []).forEach(function (pat, idx) {
      chips.appendChild(UI.chip({
        text: pat, title: '移除规则 ' + pat,
        onRemove: function () { p.excludePatterns.splice(idx, 1); repaint(); }
      }));
    });
    if (!(p.excludePatterns || []).length) chips.appendChild(caption('暂无排除规则。'));
    var patInput = UI.input({ placeholder: '例如 *.iso 或 build/**', mono: true });
    nodes.push(h('div', { class: 'col gap-2' }, [
      chips,
      h('div', { class: 'row gap-2' }, [
        h('div', { class: 'grow' }, patInput),
        h('button', {
          class: 'btn btn-sm', type: 'button', title: '添加排除规则', 'aria-label': '添加排除规则',
          onclick: function () {
            var v = (patInput.value || '').trim();
            if (!v) { UI.toast({ tone: 'warn', title: '规则为空', msg: '请输入通配模式。' }); return; }
            p.excludePatterns = p.excludePatterns || [];
            p.excludePatterns.push(v);
            repaint();
          }
        }, [h('span', { html: VS.icon('plus', 13) }), h('span', { text: '添加' })])
      ])
    ]));

    /* --- 选择性同步：能力位未置位则整块不渲染 --- */
    if (capSelective) {
      nodes.push(msection('选择性同步'));
      nodes.push(UI.alertbar({
        tone: 'info', icon: 'info', title: 'folderIds 与 excludeFolderIds 互斥',
        text: '两者不得同时非空；同给时保存会被原子拒绝（错误码 7），磁盘上的策略不发生任何变化。'
      }));
      var folders = (D.vault && D.vault.folders) ? D.vault.folders.filter(function (f) { return f.parent === 0; }) : [];
      function picker(key, label) {
        var box = h('div', { class: 'row gap-2 wrap' });
        folders.forEach(function (f) {
          var on = (p[key] || []).indexOf(f.id) >= 0;
          box.appendChild(h('button', {
            class: 'chip' + (on ? ' chip-accent' : ''), type: 'button',
            'aria-pressed': on ? 'true' : 'false',
            title: (on ? '移出' : '加入') + label + '：' + f.name,
            onclick: function () {
              p[key] = p[key] || [];
              var i = p[key].indexOf(f.id);
              if (i >= 0) p[key].splice(i, 1); else p[key].push(f.id);
              repaint();
            }
          }, [h('span', { text: f.name })]));
        });
        if (!folders.length) box.appendChild(caption('无可选文件夹。'));
        return MUI.field({ label: label, control: box, hint: '已选 ' + F.num((p[key] || []).length) + ' 项。' });
      }
      nodes.push(picker('folderIds', '仅同步这些文件夹（folderIds）'));
      nodes.push(picker('excludeFolderIds', '排除这些文件夹（excludeFolderIds）'));
    }

    /* --- 演示 --- */
    nodes.push(msection('演示'));
    nodes.push(MUI.mlist([
      policyRow('模拟「策略读取失败」', '打开后本页展示内置默认值与失败提示',
        MUI.switchCtl({
          checked: ui.policyReadFailed,
          onChange: function (e) { ui.policyReadFailed = e.target.checked; repaint(); }
        }))
    ]));

    /* --- 保存 --- */
    nodes.push(h('div', { class: 'col gap-2' }, [
      d ? MUI.limitedNote(d.reason + ' 保存已禁用。') : null,
      mbtn({
        label: '保存策略', variant: 'primary', icon: 'save',
        disabled: !!d, reason: d ? d.reason : null,
        onClick: function () {
          var inc = (p.folderIds || []).length, exc = (p.excludeFolderIds || []).length;
          if (inc > 0 && exc > 0) {
            ui.policyError = {
              code: 7,
              text: 'folderIds（' + F.num(inc) + ' 项）与 excludeFolderIds（' + F.num(exc) + ' 项）互斥，不能同时配置。' +
                '本次保存已整体拒绝，磁盘上的策略未发生任何变化。'
            };
            repaint();
            UI.toast({ tone: 'danger', title: '保存被拒绝', msg: '选择性同步字段互斥（错误码 7）' });
            return;
          }
          if (!capPadding && p.paddingLevel !== 0) p.paddingLevel = 0;
          ui.policyError = null;
          repaint();
          UI.toast({
            tone: 'success', title: '策略已保存',
            msg: '审计事件 settings.change' + (capPadding ? '' : ' · 填充档位恒为 T0（CAP_TX_PADDING 未置位）')
          });
        }
      }),
      mbtn({
        label: '恢复默认值', variant: 'ghost', icon: 'rotate-ccw',
        disabled: !!d, reason: d ? d.reason : null,
        onClick: function () {
          var fresh = readSyncPolicy();
          ui.policy = fresh.policy;
          ui.policyError = null;
          repaint();
          UI.toast({ tone: 'info', title: '已恢复默认值', msg: '尚未保存，离开前请点击「保存策略」。' });
        }
      })
    ]));

    return MUI.page({
      title: '同步策略',
      sub: '模式 / 时段 / 带宽 / 并发 / 冲突 / 中继 / 填充 / 排除规则',
      back: true,
      tab: false,
      actions: [{ icon: 'rotate-ccw', label: '恢复默认值', onClick: function () { VS.actions['resetSyncPolicy'](); } }],
      overflow: [
        { label: '路径诊断', sub: '降级原因与中继错误映射', icon: 'discover', onClick: function () { openPathDiag(null); } }
      ],
      body: MUI.screen(nodes),
      onBack: function () { VS.nav.back(); }
    });
  };

  /* ==========================================================================
   * 12. 页面：shares（阅后即焚，全屏路由）
   * ========================================================================*/

  function shareRemainText(sh) {
    if (sh.status === 'burned') return '已加密擦除（+ TRIM）';
    if (sh.status === 'revoked') return '对方未接受，分享已撤销';
    var remain = sh.expiresMs - now();
    return remain > 0 ? ('剩余 ' + F.duration(remain)) : '已过期';
  }

  function openExtendSheet(sh, onChange) {
    var addMin = 30;
    var input = UI.input({ type: 'number', value: addMin, mono: true });
    input.addEventListener('input', function () { addMin = parseInt(input.value, 10) || 0; });
    MUI.sheet({
      title: '延长时效',
      sub: sh.file,
      body: h('div', { class: 'col gap-3' }, [
        caption('当前过期时间：' + F.dateLong(sh.expiresMs)),
        caption('剩余：' + (sh.expiresMs > now() ? F.duration(sh.expiresMs - now()) : '已过期')),
        MUI.field({ label: '延长分钟数', control: input, hint: '延长后可再次调整。' })
      ]),
      footer: function (close) {
        return [
          mbtn({
            label: '确认延长', variant: 'primary', icon: 'clock',
            onClick: function () {
              sh.expiresMs = Math.max(now(), sh.expiresMs) + addMin * 60000;
              close();
              UI.toast({ tone: 'success', title: '已延长', msg: '新过期时间 ' + F.dateLong(sh.expiresMs) });
              if (onChange) onChange();
            }
          }),
          mbtn({ label: '取消', variant: 'ghost', onClick: function () { close(); } })
        ];
      }
    });
  }

  function openShareTimeline(sh) {
    var items = [
      { title: '分享已创建', time: F.dateLong(sh.createdMs), desc: '类型：' + (sh.kind === 'burn' ? '阅后即焚' : '限时分享') + ' · 会话上限 ' + F.num(sh.maxSessions || 0), tone: 'info' },
      { title: '已分发给接收设备', time: F.dateLong(sh.createdMs + 60000), desc: '接收设备：' + (sh.device || '—'), tone: 'info' }
    ];
    if (sh.status === 'burned') {
      items.push({ title: '对端已接受并读取', time: F.dateLong((sh.burnedMs || now()) - 60000), desc: '会话 ' + F.num(sh.sessions || 0) + ' / ' + F.num(sh.maxSessions || 0), tone: 'success' });
      items.push({ title: '已加密擦除（+ TRIM）', time: F.dateLong(sh.burnedMs || now()), desc: '密钥已销毁并向介质发出 TRIM 指令；不宣称物理不可恢复。', tone: 'stale' });
    } else if (sh.status === 'revoked') {
      items.push({ title: '对方未接受，分享已撤销', time: F.dateLong(sh.expiresMs), desc: '票据已作废；未产生任何会话。', tone: 'warn' });
    } else if (sh.status === 'active') {
      items.push({ title: '对端已接受', time: F.dateLong(sh.expiresMs - 3600000), desc: '会话 ' + F.num(sh.sessions || 0) + ' / ' + F.num(sh.maxSessions || 0), tone: 'success' });
    } else {
      items.push({ title: '等待对方接受', time: '尚未发生', desc: '对方接受后开始计时。', tone: 'info' });
    }
    MUI.sheet({
      title: '分享时间线',
      sub: sh.file,
      size: 'tall',
      body: h('div', { class: 'col gap-3' }, [
        MUI.timeline(items),
        UI.alertbar({
          tone: 'info', icon: 'info',
          text: '「已加密擦除（+ TRIM）」表示密钥销毁 + 介质 TRIM 指令已发出，不等于物理不可恢复。'
        })
      ])
    });
  }

  function openShareSheet(sh) {
    var d = denyInfo();
    var ended = sh.status === 'burned' || sh.status === 'revoked';
    MUI.actionSheet({
      title: sh.file,
      sub: (SHARE_STATUS[sh.status] || { label: sh.status }).label + ' · ' + shareRemainText(sh),
      items: [
        { label: '查看时间线', icon: 'history', onClick: function () { openShareTimeline(sh); } },
        {
          label: '延长时效', icon: 'clock',
          disabled: ended, reason: ended ? '已结束的分享不可延长' : null,
          onClick: function () { openExtendSheet(sh, function () { repaint(); }); }
        },
        { group: '危险操作' },
        {
          label: '撤销分享', icon: 'ban', danger: true,
          disabled: ended || !!d, reason: ended ? '已结束的分享不可撤销' : (d ? d.reason : null),
          onClick: function () {
            sh.status = 'revoked';
            UI.toast({ tone: 'warn', title: '已撤销', msg: '对方未接受，分享已撤销' });
            repaint();
          }
        }
      ]
    });
  }

  function openCreateBurn() {
    var d = denyInfo();
    var kind = 'burn';
    var ttlMin = 60;
    var device = (D.devices || []).filter(function (x) { return !x.self; })[0];
    var ttlInput = UI.input({ type: 'number', value: ttlMin, mono: true });
    ttlInput.addEventListener('input', function () { ttlMin = parseInt(ttlInput.value, 10) || 0; });
    var body = h('div', { class: 'col gap-3' });
    function render() {
      body.innerHTML = '';
      body.appendChild(MUI.limitedNote('只读降级或维护态下不能创建新的阅后即焚；已经进行中的焚毁会话会照常完成。'));
      if (d) body.appendChild(UI.alertbar({ tone: d.code === 9 ? 'warn' : 'info', icon: 'ban', title: '当前不可创建', text: d.reason }));
      body.appendChild(UI.radioCards({
        value: kind, onChange: function (v) { kind = v; render(); },
        options: [
          { value: 'burn', title: '阅后即焚', desc: '对端读取一次后立即擦除' },
          { value: 'expiring', title: '限时分享', desc: '在时效内可多次读取' }
        ]
      }));
      body.appendChild(MUI.field({ label: '时效（分钟）', control: ttlInput, hint: '到期未接受则自动撤销。' }));
      body.appendChild(MUI.mkv([{ k: '接收设备', v: device ? device.name : '—' }]));
    }
    render();
    MUI.sheet({
      title: '创建阅后即焚',
      sub: '会话上限 1；撤销与焚毁均写审计',
      body: body,
      footer: function (close) {
        return [
          mbtn({
            label: '创建', variant: 'primary', icon: 'erase',
            disabled: !!d, reason: d ? d.reason : null,
            onClick: function () {
              (D.shares = D.shares || []).push({
                id: 'shr-' + D.hash(4), fileId: 0, file: '（新选择）', kind: kind,
                createdMs: now(), expiresMs: now() + ttlMin * 60000, status: 'waiting',
                sessions: 0, maxSessions: kind === 'burn' ? 1 : 5,
                device: device ? device.name : '—', burnedMs: null
              });
              close();
              UI.toast({ tone: 'success', title: '已创建', msg: '票据已生成，等待对方接受。' });
              repaint();
            }
          }),
          mbtn({ label: '取消', variant: 'ghost', onClick: function () { close(); } })
        ];
      }
    });
  }

  VS.pages['shares'] = function (ctx) {
    if (!S.cap('CAP_BURN_SHARE')) {
      return MUI.page({
        title: '阅后即焚', back: true, tab: false,
        body: MUI.screen([
          UI.alertbar({
            tone: 'info', icon: 'ban', title: '能力不可用',
            text: '能力位 CAP_BURN_SHARE 未置位：本端不提供阅后即焚分享，入口亦不渲染。'
          })
        ]),
        onBack: function () { VS.nav.back(); }
      });
    }

    var d = denyInfo();
    var list = D.shares || [];
    var nodes = [];
    nodes.push(MUI.limitedNote('只读降级 / 维护态下不能创建新的阅后即焚；进行中的焚毁会话允许完成。'));
    var gate = writeGateNote();
    if (gate) nodes.push(gate);

    nodes.push(MUI.mstats([
      { label: '会话总数', value: F.num(list.length), src: 'p2p.burn.list()' },
      { label: '进行中', value: F.num(list.filter(function (s2) { return s2.status === 'waiting' || s2.status === 'active'; }).length), src: 'p2p.burn.list().status' },
      { label: '已焚毁', value: F.num(list.filter(function (s2) { return s2.status === 'burned'; }).length), src: 'p2p.burn.list().burnedMs' },
      { label: '已撤销', value: F.num(list.filter(function (s2) { return s2.status === 'revoked'; }).length), src: 'p2p.burn.list().status' }
    ]));

    nodes.push(mcard('分享会话', '长按行 = 撤销 / 延长 / 时间线', list.length
      ? MUI.mlist(list.map(function (sh) {
        var st = SHARE_STATUS[sh.status] || { label: sh.status, tone: 'info' };
        return MUI.mrow({
          icon: sh.kind === 'burn' ? 'erase' : 'share',
          title: sh.file,
          sub: (sh.kind === 'burn' ? '阅后即焚' : '限时分享') + ' · 接收设备 ' + (sh.device || '—') +
            ' · 会话 ' + F.num(sh.sessions || 0) + ' / ' + F.num(sh.maxSessions || 0) +
            ' · 时效：' + shareRemainText(sh),
          trail: [UI.badge({ text: st.label, tone: st.tone })],
          onClick: function () { openShareTimeline(sh); },
          onLongPress: function () { openShareSheet(sh); }
        });
      }))
      : MUI.empty({ icon: 'erase', title: '没有分享会话', desc: '创建后在这里查看时效与时间线。' })));

    nodes.push(h('div', { class: 'col gap-2' }, [
      d ? MUI.limitedNote(d.reason + ' 创建新的阅后即焚已禁用。') : null,
      mbtn({
        label: '创建阅后即焚', variant: 'primary', icon: 'erase',
        disabled: !!d, reason: d ? d.reason : null,
        onClick: openCreateBurn
      })
    ]));

    nodes.push(mcard('文案口径（必须一致）', '避免过度承诺', h('div', { class: 'col gap-2' }, [
      UI.alertbar({ tone: 'warn', icon: 'info', title: '对方未接受，分享已撤销', text: '撤销只作废票据，不涉及对端已落盘内容。' }),
      UI.alertbar({ tone: 'stale', icon: 'erase', title: '已加密擦除（+ TRIM）', text: '只声明密钥销毁 + 介质 TRIM 已执行，不作物理不可恢复的承诺。' })
    ])));

    return MUI.page({
      title: '阅后即焚',
      sub: '共 ' + F.num(list.length) + ' 个会话',
      back: true,
      tab: false,
      actions: [
        { icon: 'erase', label: '创建阅后即焚', onClick: function () {
          if (d) { UI.toast({ tone: 'warn', title: '当前不可创建', msg: d.reason }); return; }
          openCreateBurn();
        } }
      ],
      overflow: [
        { label: '设备', sub: '切到设备 Tab', icon: 'devices', onClick: function () { ctx.go('devices'); } },
        { label: '同步总览', icon: 'sync', onClick: function () { ctx.go('sync'); } }
      ],
      body: MUI.screen(nodes),
      onBack: function () { VS.nav.back(); }
    });
  };

  /* ==========================================================================
   * 13. 全局动作
   * ========================================================================*/

  VS.actions['syncNow'] = function (opts) {
    opts = opts || {};
    var d = denyInfo();
    if (d) {
      UI.toast({ tone: d.code === 9 ? 'warn' : 'info', title: '无法发起同步', msg: d.reason });
      return false;
    }
    if (!S.cap('CAP_TASKS')) {
      UI.toast({ tone: 'info', title: '同步不可用', msg: '本端未置位能力位 CAP_TASKS（任务队列）。' });
      return false;
    }
    S.set({ syncState: 'Syncing' }, true);
    UI.toast({
      tone: 'info', title: '已发起同步',
      msg: opts.device ? ('目标设备 ' + opts.device) : (opts.all ? '全部在线设备' : '事件 TASK_QUEUED(6) → TASK_PROGRESS(7)')
    });
    return true;
  };

  VS.actions['openPairing'] = function (carrier) { return VS.nav.go('pairing', { carrier: carrier || null }); };
  VS.actions['openPathDiag'] = function (dev) { return openPathDiag(dev || null); };
  VS.actions['openSyncPolicy'] = function () { return VS.nav.go('sync-policy'); };
  VS.actions['openTaskDetail'] = function (taskId) { return VS.nav.go('task', { taskId: taskId }); };
  VS.actions['openCreateBurn'] = function () { return openCreateBurn(); };

  VS.actions['resetSyncPolicy'] = function () {
    var fresh = readSyncPolicy();
    ui.policy = fresh.policy;
    ui.policyError = null;
    ui.policyReadFailed = !fresh.ok;
    repaint();
    UI.toast({ tone: 'info', title: '已恢复默认值', msg: '尚未保存，离开前请点击「保存策略」。' });
    return true;
  };

  VS.actions['pauseAll'] = function () {
    var d = denyInfo();
    if (d) { UI.toast({ tone: 'warn', title: '无法暂停', msg: d.reason }); return false; }
    (D.queue || []).forEach(function (t) {
      if (t.state === 'running') { t.state = 'paused'; t.rateBps = 0; t.etaMs = null; }
    });
    repaint();
    UI.toast({ tone: 'info', title: '已全部暂停', msg: '活动任务已转入暂停态。' });
    return true;
  };

  VS.actions['resumeAll'] = function () {
    var d = denyInfo();
    if (d) { UI.toast({ tone: 'warn', title: '无法恢复', msg: d.reason }); return false; }
    (D.queue || []).forEach(function (t) { if (t.state === 'paused') t.state = 'running'; });
    repaint();
    UI.toast({ tone: 'info', title: '已全部恢复', msg: '已暂停任务已恢复传输。' });
    return true;
  };

  VS.actions['clearDone'] = function () {
    var d = denyInfo();
    if (d) { UI.toast({ tone: 'warn', title: '无法清理', msg: d.reason }); return false; }
    var before = (D.queue || []).length;
    var kept = (D.queue || []).filter(function (t) { return t.state !== 'done'; });
    D.queue.length = 0;
    kept.forEach(function (t) { D.queue.push(t); });
    repaint();
    UI.toast({ tone: 'success', title: '已清理', msg: '移除 ' + F.num(before - kept.length) + ' 条已完成记录。' });
    return true;
  };

  /* 演示控制台补充项：本端前台状态与相机可用性（app.js 的演示控制台不在此文件内） */
  VS.actions['mobile.foreground'] = function () {
    var v = !isForeground();
    S.set({ foreground: v }, true);
    repaint();
    UI.toast({
      tone: v ? 'info' : 'warn', title: v ? '本端已切到前台' : '本端已切到后台',
      msg: v ? 'P2P 入站监听恢复，可显示「在线」' : '持续监听暂停：设备一律显示「待唤醒」，不显示「在线」'
    });
    return v;
  };
  VS.actions['mobile.camera'] = function () {
    var v = !cameraOk();
    S.set({ cameraOk: v }, true);
    repaint();
    UI.toast({
      tone: 'info', title: v ? '相机可用' : '相机不可用',
      msg: v ? '配对时渲染「扫码」载具' : '配对时「扫码」载具不渲染（不是置灰）'
    });
    return v;
  };

})(window);
