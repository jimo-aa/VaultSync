/* ============================================================================
 * VaultSync V2.0 原型 — 设备 / 同步总览 / 传输队列（views-sync.js）
 * 纯静态、零依赖、经典脚本（不使用 ES Module / fetch / 外部库）
 * 依赖：core.js（VS.util / VS.fmt / VS.store / VS.ERR / VS.RELAY_ERR / VS.derive）
 *       ui.js（VS.ui）/ data.js（VS.data）
 * 注册：VS.pages['devices'] / ['sync'] / ['queue'] + 若干 VS.actions
 * ==========================================================================*/
(function (global) {
  'use strict';

  var VS = global.VS;
  var h = VS.util.h, UI = VS.ui, F = VS.fmt, D = VS.data, S = VS.store;

  VS.pages = VS.pages || {};
  VS.actions = VS.actions || {};

  /* ==========================================================================
   * 0. 常量：路径降级原因 / 中继错误 / 填充档位 / 冲突与同步枚举
   * ========================================================================*/

  /* 路径降级原因枚举（docs/v2.0/02 路径状态机）：R 码不得原样显示 */
  var PATH_REASON = {
    direct_timeout:     { label: '直连超时',       hint: '超时窗口内未完成直连握手，路径状态机已尝试下一档。' },
    hole_punch_timeout: { label: '打洞超时',       hint: '双方 NAT 映射未能在窗口内对齐，打洞协助无果。' },
    symmetric_nat:      { label: '对称型 NAT',     hint: '对端为对称型 NAT，外部端口不可预测，打洞不可行。' },
    relay_limit:        { label: '中继额度受限',   hint: '中继限速或流量配额耗尽，吞吐被压低。' },
    relay_unavailable:  { label: '中继不可用',     hint: '中继节点当前不可达，路径保持降级且不再自动回退。' },
    padding_downgrade:  { label: '填充档位降级',   hint: '为维持吞吐，填充档位已下调（抗流量分析强度同步下降）。' },
    proto_downgrade:    { label: '协议版本降级',   hint: '对端协议版本较低，已回退到兼容协商集。' }
  };
  var PATH_ORDER = ['direct', 'hole_punch', 'relay'];

  /* 中继错误建议动作（R1–R17 必须映射成人话，不得原样返回 R 码） */
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

  /* 填充档位 T=0–4 */
  var PADDING_LEVELS = [
    { value: 0, label: 'T0 · 关闭',        sub: '不填充；吞吐最高，抗流量分析最弱' },
    { value: 1, label: 'T1 · 最小',        sub: '仅对齐帧头；开销约 2%' },
    { value: 2, label: 'T2 · 标准（默认）', sub: '按块对齐填充；开销约 6%' },
    { value: 3, label: 'T3 · 增强',        sub: '定长块 + 随机抖动；开销约 14%' },
    { value: 4, label: 'T4 · 最大',        sub: '恒速伪流；开销约 30%，弱网下显著降速' }
  ];
  var CONFLICT_STRATEGIES = [
    { value: 'keep_both',     label: '保留两者',   sub: '生成冲突副本，不覆盖任何一方' },
    { value: 'prefer_newer',  label: '较新者优先', sub: '按修改时间取新，旧版进保护副本' },
    { value: 'prefer_local',  label: '本机优先',   sub: '始终保留本机版本' },
    { value: 'prefer_remote', label: '对端优先',   sub: '始终保留对端版本' }
  ];
  var SYNC_MODES = [
    { value: 'auto',      label: '自动',     sub: '变更即同步' },
    { value: 'manual',    label: '手动',     sub: '仅在我点击「立即同步」时同步' },
    { value: 'scheduled', label: '定时',     sub: '仅在允许时段内同步' }
  ];
  var SHARE_STATUS = {
    waiting: { label: '等待接受', tone: 'info' },
    active:  { label: '使用中',   tone: 'success' },
    burned:  { label: '已焚毁',   tone: 'stale' },
    revoked: { label: '已撤销',   tone: 'warn' }
  };
  var TASK_STATE = {
    running:     { label: '传输中',   tone: 'info' },
    paused:      { label: '已暂停',   tone: 'warn' },
    queued:      { label: '排队中',   tone: 'info' },
    failed:      { label: '失败',     tone: 'danger' },
    interrupted: { label: '已中断',   tone: 'warn' },
    done:        { label: '已完成',   tone: 'success' },
    cancelled:   { label: '已取消',   tone: 'stale' }
  };

  var RETRY_LIMIT = 3;

  /* ==========================================================================
   * 1. 本地原语（VS.ui 没有的：骨架屏 / 队列行 / 甘特 / 冲突对 / 分享行 …）
   * ========================================================================*/

  function now() { return Date.now(); }

  /** 只读降级 / 维护态的写门禁：返回 null 或 {code, reason} */
  function denyInfo() {
    var code = VS.derive.denyWrite();
    if (code === null || code === undefined) return null;
    var e = VS.ERR[code] || VS.ERR[8];
    return { code: code, reason: '错误码 ' + code + ' · ' + e.title + '：' + e.hint };
  }

  /** 受写门禁保护的按钮 */
  function guardedBtn(o) {
    var d = denyInfo();
    return UI.btn({
      label: o.label, icon: o.icon, variant: o.variant, size: o.size,
      disabled: !!d,
      title: d ? d.reason : (o.title || o.label),
      onClick: o.onClick
    });
  }

  function sectionTitle(text) { return h('div', { class: 'section-title', text: text }); }

  function caption(text) { return h('div', { class: 't-caption', text: text }); }

  function statCard(label, value, sub) {
    return h('div', { class: 'card stat-card panel-alt' }, [
      h('div', { class: 'stat-label', text: label }),
      h('div', { class: 'stat-value', text: value }),
      sub ? h('div', { class: 't-caption', text: sub }) : null
    ]);
  }

  /** 骨架屏（.skeleton 自身只有背景，必须给尺寸） */
  function skeletonRows(n, widths) {
    var out = [];
    for (var i = 0; i < n; i++) {
      out.push(h('div', {
        class: 'skeleton',
        style: { height: '16px', width: (widths && widths[i % widths.length]) || (58 + (i * 11) % 34) + '%' }
      }));
    }
    return h('div', { class: 'col gap-2', 'aria-busy': 'true', 'aria-label': '加载中' }, out);
  }

  function menuFrom(el, items, opts) {
    var r = el.getBoundingClientRect();
    return UI.menu(r.right, r.bottom + 4, items, opts || { alignRight: true });
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

  /** 队列统计（总览卡数据来源；无来源一律返回 null 由调用方显示「—」） */
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
      count: q.length,
      active: active,
      queued: queued,
      doneBytes: totalB > 0 ? doneB : null,
      totalBytes: totalB > 0 ? totalB : null,
      pct: totalB > 0 ? doneB / totalB : null,
      up: up || null,
      down: down || null,
      eta: eta
    };
  }

  function onlineDeviceCount() {
    var list = D.devices || [];
    var n = 0;
    list.forEach(function (d) { if (!d.self && d.online) n++; });
    return n;
  }

  function conflictCount() { return D.conflictPair ? 1 : 0; }

  /** 冲突两侧实体（data.js 只给 id，实体从 vault 取） */
  function conflictSides() {
    var cp = D.conflictPair;
    if (!cp) return null;
    var a = VS.data.entry(cp.keep), b = VS.data.entry(cp.drop);
    if (!a || !b) return null;
    return { keep: a, drop: b };
  }

  /* ---------------------------------------------------------------- 队列行 */
  /** 单条传输任务行（.queue-row 结构由 components.css 定义） */
  function queueRow(t, ctx, onChange) {
    var st = TASK_STATE[t.state] || { label: t.state, tone: 'info' };
    var pct = t.totalBytes > 0 ? (t.doneBytes || 0) / t.totalBytes : null;
    var d = denyInfo();

    var ico = h('span', { class: 'qr-ico', html: VS.icon(dirIcon(t.dir), 18), title: dirLabel(t.dir) });

    var line1 = h('div', { class: 'qr-line' }, [
      h('span', { class: 'qr-name t-truncate', text: t.name, title: t.name }),
      UI.badge({ text: st.label, tone: st.tone }),
      t.retries ? UI.badge({ text: '已重试 ' + F.num(t.retries) + ' 次', tone: 'warn' }) : null,
      UI.pathBadge(t.path, PATH_REASON[t.path] ? PATH_REASON[t.path].label : null)
    ]);

    var metaBits = [];
    metaBits.push(F.bytes(t.doneBytes || 0) + ' / ' + F.bytes(t.totalBytes));
    if (pct !== null) metaBits.push(F.pct(pct));
    if (t.state === 'running' && t.rateBps) metaBits.push(F.rate(t.rateBps));
    if (t.state === 'running' && t.etaMs) metaBits.push('剩余 ' + F.duration(t.etaMs));
    metaBits.push('对端 ' + (t.peer || '—'));
    metaBits.push('分片 ' + F.num(t.doneChunks || 0) + ' / ' + F.num(t.totalChunks || 0));

    var line2 = h('div', { class: 'qr-line' }, [
      h('span', { class: 'qr-meta', text: metaBits.join(' · ') })
    ]);

    var main = h('div', { class: 'qr-main' }, [
      line1,
      t.state !== 'done' && t.state !== 'cancelled' && pct !== null ? UI.bar(pct, t.state === 'failed' ? 'danger' : null, 'sm') : null,
      line2,
      /* 跨会话续传 */
      t.state === 'interrupted' && t.resumable && t.resumePct !== null
        ? UI.alertbar({ tone: 'warn', icon: 'history', title: '续传 ' + F.pct(t.resumePct) + '（跨会话）', text: '本次会话将从已落盘的分片边界继续，不会重传已完成块。' })
        : null,
      /* 失败行：错误提示（精简版） */
      t.state === 'failed' || (t.errCode !== null && t.state === 'interrupted')
        ? taskError(t, onChange)
        : null,
      /* 已取消：码 12 用 info 语气，不用红色错误态 */
      t.state === 'cancelled' && t.errCode !== null
        ? UI.alertbar({ tone: 'info', icon: 'info', title: '已取消', text: '错误码 ' + t.errCode + ' · ' + VS.ERR[t.errCode].title + '（视为正常结果，不报错）' })
        : null
    ]);

    /* 操作区 */
    var actions = h('div', { class: 'qr-actions' });
    if (t.state === 'running') {
      actions.appendChild(UI.iconBtn({
        icon: 'pause', size: 'sm', label: '暂停 ' + t.name,
        disabled: !!d, title: d ? d.reason : '暂停',
        onClick: function () { t.state = 'paused'; t.rateBps = 0; t.etaMs = null; onChange(); }
      }));
    } else if (t.state === 'paused') {
      actions.appendChild(UI.iconBtn({
        icon: 'play', size: 'sm', label: '继续 ' + t.name,
        disabled: !!d, title: d ? d.reason : '继续',
        onClick: function () { t.state = 'running'; onChange(); }
      }));
    } else if (t.state === 'failed' || t.state === 'interrupted') {
      var exhausted = (t.retries || 0) >= RETRY_LIMIT;
      actions.appendChild(UI.btn({
        label: exhausted ? '已达重试上限' : '重试', icon: 'refresh', size: 'sm',
        disabled: !!d || exhausted,
        title: d ? d.reason : (exhausted ? '重试已达上限 ' + F.num(RETRY_LIMIT) + ' 次：请检查网络或磁盘空间' : '重试该任务'),
        onClick: function () {
          if ((t.retries || 0) >= RETRY_LIMIT) {
            UI.toast({ tone: 'warn', title: '重试已达上限', msg: '请检查网络或磁盘空间' });
            return;
          }
          t.retries = (t.retries || 0) + 1;
          t.state = 'running';
          onChange();
        }
      }));
    }
    if (t.state !== 'done' && t.state !== 'cancelled') {
      actions.appendChild(UI.iconBtn({
        icon: 'cancel', size: 'sm', label: '取消 ' + t.name,
        disabled: !!d, title: d ? d.reason : '取消',
        onClick: function () { t.state = 'cancelled'; t.errCode = 12; t.rateBps = 0; onChange(); }
      }));
    }
    actions.appendChild(UI.iconBtn({
      icon: 'eye', size: 'sm', label: '查看 ' + t.name + ' 详情',
      onClick: function () { openTaskDetail(t); }
    }));

    return h('div', {
      class: 'queue-row', dataset: { state: t.state, dir: t.dir },
      title: t.name + ' · ' + st.label
    }, [ico, main, actions]);
  }

  /** 失败行的错误提示：码 12 走 info 语气，其余走 UI.errorBox */
  function taskError(t, onChange) {
    if (t.errCode === 12) {
      return UI.alertbar({ tone: 'info', icon: 'info', title: '已取消', text: VS.ERR[12].hint });
    }
    var e = VS.ERR[t.errCode];
    if (!e) return UI.errorBox(8, { text: '任务失败但未携带可映射的错误码。' });
    var exhausted = (t.retries || 0) >= RETRY_LIMIT;
    if (e.tone === 'stale' && t.errCode === 9) {
      /* 码 9 不弹错误框，切只读提示 */
      return UI.alertbar({ tone: 'warn', icon: 'ban', title: '错误码 9 · ' + e.title, text: e.hint });
    }
    if (e.tone === 'maintenance' || t.errCode === 10) {
      return UI.alertbar({ tone: 'info', icon: 'refresh', title: '错误码 10 · ' + e.title, text: e.hint });
    }
    return UI.errorBox(t.errCode, {
      text: e.hint + (exhausted ? ' · 重试已达上限 ' + F.num(RETRY_LIMIT) + ' 次：请检查网络或磁盘空间' : ''),
      actions: exhausted ? [
        UI.btn({ label: '检查磁盘', icon: 'drive', size: 'sm', title: '打开磁盘诊断', onClick: function () { openPathDiag(); } })
      ] : null
    });
  }

  /* ------------------------------------------------------------------ 甘特 */
  /** 甘特式视图（.gantt / .gantt-row / .gantt-track / .gantt-bar） */
  function ganttView(tasks) {
    var list = (tasks || []).filter(function (t) { return t.state !== 'cancelled'; });
    if (!list.length) return UI.empty({ icon: 'gantt', title: '暂无时间线数据', desc: '队列为空时不渲染甘特轨道。' });
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
      var left = VS.util.clamp((s - minStart) / span, 0, 1) * 100;
      var width = VS.util.clamp(dur / span, 0.02, 1) * 100;
      return h('div', { class: 'gantt-row', title: t.name + ' · ' + dirLabel(t.dir) + ' · ' + F.duration(dur) }, [
        h('div', { class: 'row gap-2', style: { minWidth: 0 } }, [
          h('span', { class: 'qr-ico', html: VS.icon(dirIcon(t.dir), 14), title: dirLabel(t.dir) }),
          h('span', { class: 't-truncate', text: t.name, title: t.name })
        ]),
        h('div', { class: 'gantt-track' }, [
          h('div', {
            class: 'gantt-bar',
            dataset: { dir: t.dir, state: t.state },
            style: { left: left.toFixed(2) + '%', width: width.toFixed(2) + '%' },
            title: t.name + ' · ' + (TASK_STATE[t.state] ? TASK_STATE[t.state].label : t.state)
          })
        ])
      ]);
    });
    return h('div', { class: 'gantt' }, [
      h('div', { class: 'gantt-row' }, [h('div'), h('div', { class: 'gantt-axis' }, [
        h('span', { text: F.dateShort(minStart) }),
        h('span', { text: '现在 ' + F.dateShort(now()) }),
        h('span', { text: F.dateShort(maxEnd) })
      ])]),
      h('div', { class: 'col gap-1' }, rows)
    ]);
  }

  /* -------------------------------------------------------------- 冲突对 */
  function conflictPairView(onResolved) {
    var sides = conflictSides();
    if (!sides) return UI.empty({ icon: 'columns', title: '当前没有冲突', desc: '两方版本一致时不会生成冲突副本。' });
    function sideBox(f, sideKey, title) {
      return h('div', { class: 'conflict-side', dataset: { keep: sideKey === 'keep' ? 'true' : 'false' } }, [
        h('div', { class: 'col gap-2' }, [
          h('div', { class: 't-strong', text: title }),
          h('div', { class: 't-truncate', text: f.name, title: f.name }),
          UI.kv([
            { k: '来源设备', v: f.device || '—' },
            { k: '大小', v: F.bytes(f.size) },
            { k: '修改时间', v: F.dateLong(f.modifiedMs) },
            { k: '版本 rev', v: f.rev, mono: true },
            { k: 'SHA-256', v: VS.util.shortHash(D.hash(64), 10, 6), mono: true, copy: true }
          ])
        ])
      ]);
    }
    var wrap = h('div', { class: 'conflict-pair' }, [
      sideBox(sides.keep, 'keep', '本机版本'),
      h('div', { class: 'conflict-vs', text: 'VS' }),
      sideBox(sides.drop, 'drop', '对端版本')
    ]);
    var note = h('div', { class: 'col gap-2' });

    function resolve(strategy) {
      /* 原型：按块合并与保留两者都在本地完成，失败则两项都保留 */
      var failed = strategy === 'merge' && sides.keep.size === 0;
      if (failed) {
        note.innerHTML = '';
        note.appendChild(UI.alertbar({
          tone: 'danger', icon: 'danger',
          title: '解决失败',
          text: '按块合并未能完成（块索引不可用）。两个版本均已保留，未发生覆盖。'
        }));
        return;
      }
      var msg = strategy === 'merge'
        ? '已按块合并：以块为单位择优写入，非语义合并。'
        : strategy === 'keep_both'
          ? '已保留两者：对端版本另存为冲突副本。'
          : '已保留所选版本，另一版本进入保护副本。';
      note.innerHTML = '';
      note.appendChild(UI.alertbar({ tone: 'success', icon: 'check-circle', title: '冲突已解决', text: msg }));
      UI.toast({ tone: 'success', title: '冲突已解决', msg: msg });
      if (onResolved) onResolved(strategy);
    }

    return h('div', { class: 'col gap-4' }, [
      wrap,
      h('div', { class: 'row gap-2 wrap' }, [
        UI.btn({ label: '保留此版本', icon: 'check', size: 'sm', title: '保留本机版本，对端版本进入保护副本', onClick: function () { resolve('keep'); } }),
        UI.btn({ label: '保留两者', icon: 'columns', size: 'sm', title: '两个版本都保留，不覆盖任何一方', onClick: function () { resolve('keep_both'); } }),
        UI.btn({ label: '按块合并', icon: 'layers', size: 'sm', title: '按加密块择优合并（不是语义合并）', onClick: function () { resolve('merge'); } })
      ]),
      h('div', { class: 't-caption', text: '「按块合并」按加密块（CDC 块）为单位择优写入，不做任何语义层合并。' }),
      note
    ]);
  }

  /* ---------------------------------------------------------- 分享 / 焚毁 */
  function shareRow(sh, onChange) {
    var st = SHARE_STATUS[sh.status] || { label: sh.status, tone: 'info' };
    var d = denyInfo();
    var remainMs = sh.expiresMs - now();
    var isBurn = sh.kind === 'burn';

    var line2 = [
      h('span', { text: isBurn ? '阅后即焚' : '限时分享' }),
      h('span', { text: '·' }),
      h('span', { text: '接收设备 ' + (sh.device || '—') }),
      h('span', { text: '·' }),
      h('span', { text: '会话 ' + F.num(sh.sessions || 0) + ' / ' + F.num(sh.maxSessions || 0) })
    ];

    var remainText = sh.status === 'burned'
      ? '已加密擦除（+ TRIM）'
      : sh.status === 'revoked'
        ? '对方未接受，分享已撤销'
        : remainMs > 0 ? ('剩余 ' + F.duration(remainMs)) : '已过期';

    return h('div', {
      class: 'list-row',
      dataset: { selected: sh.status === 'active' ? 'true' : 'false' },
      title: sh.file + ' · ' + st.label
    }, [
      h('span', { class: 'lr-ico', html: VS.icon(isBurn ? 'erase' : 'share', 18), title: isBurn ? '阅后即焚' : '限时分享' }),
      h('div', { class: 'lr-main' }, [
        h('div', { class: 'lr-title' }, [
          h('span', { class: 't-truncate', text: sh.file, title: sh.file }),
          UI.badge({ text: st.label, tone: st.tone })
        ]),
        h('div', { class: 'lr-sub' }, line2),
        h('div', { class: 'lr-sub', text: '时效：' + remainText + ' · 创建于 ' + F.dateLong(sh.createdMs) })
      ]),
      h('div', { class: 'lr-actions' }, [
        UI.btn({
          label: '撤销', icon: 'ban', size: 'sm',
          disabled: sh.status === 'burned' || sh.status === 'revoked',
          title: sh.status === 'burned' ? '已焚毁的分享不可撤销' : sh.status === 'revoked' ? '已撤销' : '撤销该分享',
          onClick: function () {
            sh.status = 'revoked';
            UI.toast({ tone: 'warn', title: '已撤销', msg: '对方未接受，分享已撤销' });
            onChange();
          }
        }),
        UI.btn({
          label: '延长时效', icon: 'clock', size: 'sm',
          disabled: sh.status === 'burned' || sh.status === 'revoked',
          title: sh.status === 'burned' || sh.status === 'revoked' ? '已结束的分享不可延长' : '延长该分享的时效',
          onClick: function () { openExtendShare(sh, onChange); }
        }),
        UI.btn({
          label: '查看时间线', icon: 'history', size: 'sm',
          title: '查看该分享的时间线',
          onClick: function () { openShareTimeline(sh); }
        })
      ])
    ]);
  }

  /* ==========================================================================
   * 2. 弹窗：配对向导（5 阶段）
   * ========================================================================*/
  function openPairingWizard(seedCarrier) {
    var step = 1;
    var carrier = seedCarrier || (S.cap('CAP_DISCOVERY') ? 'discover' : 'invite');
    var cameraOk = true;          /* 演示开关：相机不可用时不渲染「扫码」载体 */
    var authPct = 0;
    var authTimer = null;
    var codeLeft = 120;
    var codeTimer = null;
    var shortCode = null;
    var inviteCode = 'VS-' + D.hash(6).toUpperCase();

    var body = h('div', { class: 'col gap-4' });
    var foot = h('div', { class: 'row gap-2' });

    function cleanup() {
      if (authTimer) { clearInterval(authTimer); authTimer = null; }
      if (codeTimer) { clearInterval(codeTimer); codeTimer = null; }
    }

    function carriers() {
      return (D.pairCarriers || []).filter(function (c) { return cameraOk || c.value !== 'qr'; });
    }

    function carrierLabel(v) {
      var found = null;
      (D.pairCarriers || []).forEach(function (c) { if (c.value === v) found = c.label; });
      return found || v;
    }

    function stepsNode() {
      var items = ['选择载体', '交换', '双向认证', '人工核验短码', '写入节点清单'].map(function (label, i) {
        return { label: label, state: (i + 1) < step ? 'done' : ((i + 1) === step ? 'current' : 'todo') };
      });
      return UI.steps({ items: items });
    }

    function gotoStep(n) { step = n; render(); }

    function render() {
      body.innerHTML = '';
      foot.innerHTML = '';
      body.appendChild(stepsNode());

      if (step === 1) {
        body.appendChild(h('div', { class: 'col gap-3' }, [
          caption('选择本次配对的载体。载体决定后续交换阶段可用的通道。'),
          UI.radioCards({
            value: carrier,
            onChange: function (v) { carrier = v; },
            options: carriers().map(function (c) {
              return { value: c.value, title: c.label, desc: c.desc };
            })
          }),
          UI.switchCtl({
            label: '本机相机可用（演示开关）',
            sub: cameraOk ? '关闭后「扫码」载体不渲染' : '相机不可用：「扫码」载体已隐藏',
            checked: cameraOk,
            onChange: function (e) {
              cameraOk = e.target.checked;
              if (!cameraOk && carrier === 'qr') carrier = 'invite';
              render();
            }
          })
        ]));
      } else if (step === 2) {
        var exchange = [];
        if (carrier === 'invite') {
          exchange.push(caption('把一次性邀请码交给对方输入。邀请码单次有效。'));
          exchange.push(h('div', { class: 't-mono' }, UI.copyField(inviteCode, '复制邀请码')));
        } else if (carrier === 'qr') {
          exchange.push(caption('对方扫码后进入双向认证。二维码仅含配对载荷，不含密钥明文。'));
          exchange.push(h('div', { class: 'col', style: { alignItems: 'center' } }, [
            UI.qrPlaceholder(seedCarrier ? 77 : 31),
            h('div', { class: 't-mono', text: inviteCode })
          ]));
        } else if (carrier === 'manual') {
          var addrInput = UI.input({ placeholder: '例如 192.168.1.24:51234', mono: true });
          exchange.push(UI.field({
            label: '对端地址', for: 'pair-addr', control: addrInput,
            hint: '跨网段场景的兜底方式；地址仅用于本次握手。'
          }));
        } else {
          var dis = (D.discovered || []).map(function (d) {
            return h('div', { class: 'row gap-2' }, [
              h('span', { class: 't-mono', text: '局域网设备 · 指纹 ' + d.fingerprint }),
              UI.badge({ text: F.num((d.addrs || []).length) + ' 个候选地址', tone: 'info' })
            ]);
          });
          exchange.push(caption('未配对设备只暴露指纹与候选地址数量，不暴露 IP / 端口 / 名称。'));
          exchange.push(h('div', { class: 'col gap-2' }, dis.length ? dis : [UI.empty({ icon: 'discover', title: '未被发现', desc: '局域网发现未返回候选。' })]));
        }
        body.appendChild(h('div', { class: 'col gap-3' }, exchange));
      } else if (step === 3) {
        body.appendChild(h('div', { class: 'col gap-3' }, [
          caption('双向认证进行中：双方各自验证对端身份，任一步失败即整体中止。'),
          UI.bar(authPct, 'accent', 'lg'),
          h('div', { class: 't-mono', text: F.pct(authPct) }),
          UI.alertbar({ tone: 'info', icon: 'info', title: '认证内容', text: 'Noise 信道握手 + 静态公钥绑定校验；本原型不执行真实密码学运算。' })
        ]));
      } else if (step === 4) {
        if (!shortCode) shortCode = String(Math.floor(100000 + Math.random() * 900000));
        var mm = Math.floor(codeLeft / 60), ss = codeLeft % 60;
        var countdownText = F.num(mm) + ':' + (ss < 10 ? '0' : '') + F.num(ss);
        body.appendChild(h('div', { class: 'col gap-3' }, [
          caption('请与对端屏幕上的 6 位短码逐位比对。短码不一致说明存在中间人。'),
          h('div', {
            class: 't-mono', style: { fontSize: '38px', letterSpacing: '.32em', fontWeight: '600' },
            text: shortCode, title: '本次核验短码'
          }),
          h('div', { class: 'status-line' }, [
            h('span', { class: 'status-dot ' + (codeLeft > 0 ? 'warn' : 'danger') }),
            h('span', { class: 't-caption', text: codeLeft > 0 ? ('剩余 ' + countdownText) : '已超时：请重新发起配对' })
          ])
        ]));
      } else {
        body.appendChild(h('div', { class: 'col gap-2' }, [
          UI.alertbar({ tone: 'success', icon: 'check-circle', title: '配对完成', text: '已写入节点清单并生成审计条目。' }),
          UI.kv([
            { k: '配对载体', v: carrierLabel(carrier) },
            { k: '对端指纹', v: 'A3:F1:…:9C', mono: true, copy: true },
            { k: '写入内容', v: '节点清单（设备名 / 指纹 / 公钥绑定 / 信任级别）' },
            { k: '审计事件', v: 'p2p.pair', mono: true }
          ])
        ]));
      }

      /* 页脚 */
      if (step > 1) {
        foot.appendChild(UI.btn({ label: '上一步', icon: 'chevron-down', title: '返回上一步', onClick: function () { gotoStep(step - 1); } }));
      }
      foot.appendChild(UI.btn({ label: '取消', title: '关闭配对向导', onClick: function () { m.close(); } }));

      if (step === 1) {
        foot.appendChild(UI.btn({
          label: '下一步', variant: 'primary', icon: 'chevron-down',
          title: '进入交换阶段',
          onClick: function () { gotoStep(2); }
        }));
      } else if (step === 2) {
        foot.appendChild(UI.btn({
          label: '开始双向认证', variant: 'primary', title: '进入双向认证阶段',
          onClick: function () {
            gotoStep(3);
            authPct = 0;
            authTimer = setInterval(function () {
              authPct = Math.min(1, authPct + 0.1);
              if (authPct >= 1) {
                clearInterval(authTimer); authTimer = null;
                gotoStep(4);
                codeLeft = 120;
                codeTimer = setInterval(function () {
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
      } else if (step === 4) {
        foot.appendChild(UI.btn({
          label: '不一致', variant: 'danger', icon: 'danger',
          title: '短码不一致，中止配对',
          onClick: function () {
            cleanup();
            UI.toast({ tone: 'danger', title: '已中止配对', msg: '短码不一致：可能存在中间人，请检查网络环境。' });
            m.close();
          }
        }));
        foot.appendChild(UI.btn({
          label: '短码一致', variant: 'primary', icon: 'check', title: '短码一致，完成配对',
          onClick: function () { cleanup(); gotoStep(5); }
        }));
      } else {
        foot.appendChild(UI.btn({
          label: '完成', variant: 'primary', icon: 'check', title: '完成配对',
          onClick: function () { cleanup(); m.close(); UI.toast({ tone: 'success', title: '配对已写入', msg: '节点清单与审计条目已生成。' }); }
        }));
      }
    }

    var m = UI.modal({
      title: '配对向导',
      sub: '五个阶段：选择载体 → 交换 → 双向认证 → 人工核验短码 → 写入节点清单 + 审计',
      size: 'lg',
      body: body,
      footer: foot,
      onClose: cleanup
    });
    render();
    return m;
  }

  /* ==========================================================================
   * 3. 弹窗：设备详情 / 远程锁定 / 路径诊断 / 单文件详情 / 策略 / 分享
   * ========================================================================*/

  /** 最近 N 条同步记录（data.js 无设备级同步历史，此处由队列确定性派生） */
  function deviceSyncRecords(dev, n) {
    var out = [];
    var base = (D.queue && D.queue.length) ? D.queue : [{ name: '—', dir: 'up', totalBytes: 0, state: 'done', errCode: null, path: 'direct' }];
    for (var i = 0; i < n; i++) {
      var t = base[i % base.length];
      var ok = !(t.state === 'failed' || t.state === 'interrupted');
      out.push({
        id: 'rec-' + i,
        seq: n - i,
        timeMs: D.NOW - i * (7 * D.MIN) - (i * 131 % 60000),
        name: t.name,
        dir: t.dir,
        size: t.totalBytes + (i * 1024) % 65536,
        state: ok ? 'done' : 'failed',
        path: PATH_ORDER[i % 3],
        errCode: ok ? 0 : (t.errCode || 3)
      });
    }
    return out;
  }

  function openDeviceDetail(dev) {
    var records = deviceSyncRecords(dev, 20);
    var reasonKey = dev.reason || null;
    var noise = [
      { k: '信道', v: 'Noise_XX_25519_ChaChaPoly_BLAKE2s', mono: true },
      { k: '握手角色', v: '发起方（本机）→ 响应方（' + dev.name + '）' },
      { k: '静态公钥绑定', v: dev.trust === 'verified' ? '已绑定并校验通过' : '未绑定' },
      { k: '重协商次数', v: F.num(2), mono: true },
      { k: '最近重协商', v: F.relative(D.NOW - 12 * D.MIN) }
    ];
    var body = h('div', { class: 'col gap-4' }, [
      UI.kv([
        { k: '设备名', v: dev.name },
        { k: '操作系统', v: dev.os || '—' },
        { k: '协议版本', v: dev.version || '—', mono: true },
        { k: '在线状态', v: dev.online ? '在线' : '离线' },
        { k: '监听端口', v: dev.port ? F.num(dev.port) : '—（不监听，仅出站）' },
        { k: '信任级别', v: dev.trust === 'verified' ? '已验证' : '未验证' }
      ]),
      sectionTitle('指纹（全量）'),
      h('div', { class: 't-mono' }, UI.copyField(dev.fingerprint, '复制设备指纹')),
      sectionTitle('Noise 信道'),
      UI.kv(noise),
      sectionTitle('路径状态机'),
      UI.kv([
        { k: '当前态', v: (dev.path || 'direct') },
        { k: '降级原因', v: reasonKey ? pathReasonText(reasonKey) : '无（未发生降级）' },
        { k: '原因说明', v: reasonKey ? pathReasonHint(reasonKey) : '—' }
      ]),
      h('div', { class: 'row gap-2' }, PATH_ORDER.map(function (p) {
        var active = (dev.path || 'direct') === p;
        return UI.badge({ text: (p === 'direct' ? '直连' : p === 'hole_punch' ? '打洞' : '中继') + (active ? '（当前）' : ''), tone: active ? 'accent' : 'outline' });
      })),
      sectionTitle('最近 20 条同步记录'),
      UI.table({
        maxHeight: '280px',
        columns: [
          { key: 'seq', label: '#', width: '48px', align: 'right' },
          { key: 'timeMs', label: '时间', width: '150px', render: function (r) { return F.dateLong(r.timeMs); } },
          { key: 'name', label: '文件' },
          { key: 'dir', label: '方向', width: '72px', render: function (r) { return dirLabel(r.dir); } },
          { key: 'size', label: '大小', width: '92px', align: 'right', render: function (r) { return F.bytes(r.size); } },
          {
            key: 'state', label: '结果', width: '130px', render: function (r) {
              if (r.state === 'done') return UI.badge({ text: '成功', tone: 'success' });
              return UI.badge({ text: '失败 · ' + (VS.ERR[r.errCode] ? VS.ERR[r.errCode].title : '未知'), tone: 'danger' });
            }
          },
          {
            key: 'path', label: '路径', width: '96px', render: function (r) {
              return UI.pathBadge(r.path, PATH_REASON[r.path] ? PATH_REASON[r.path].label : null);
            }
          }
        ],
        rows: records
      })
    ]);
    return UI.modal({ title: '设备详情 · ' + dev.name, sub: 'Noise 信道 / 指纹 / 路径状态机 / 同步记录', size: 'lg', body: body });
  }

  /** 远程锁定发起：目标离线 → 状态「待投递」，绝不显示「已锁定对端」 */
  function openRemoteLock(dev) {
    var mode = 'immediate';
    var delayMin = 5;
    var ttlDays = 7;
    var body = h('div', { class: 'col gap-4' });
    var foot = h('div', { class: 'row gap-2' });

    function renderForm() {
      body.innerHTML = '';
      foot.innerHTML = '';
      body.appendChild(UI.alertbar({
        tone: 'warn', icon: 'alert', title: '远程锁定为不可逆的高影响操作',
        text: '指令送达对端后，对端将立即进入锁定态。本原型不执行真实锁定。'
      }));
      body.appendChild(UI.field({
        label: '锁定模式',
        control: UI.radioCards({
          value: mode, onChange: function (v) { mode = v; },
          options: [
            { value: 'immediate', title: '立即锁定', desc: '指令送达即锁定' },
            { value: 'delayed', title: '延迟锁定', desc: '在指定分钟数后锁定，便于撤回误操作' }
          ]
        })
      }));
      if (mode === 'delayed') {
        var delayInput = UI.input({ type: 'number', value: delayMin, mono: true });
        delayInput.addEventListener('input', function () { delayMin = parseInt(delayInput.value, 10) || 0; });
        body.appendChild(UI.field({ label: '延迟分钟数', control: delayInput, hint: '范围 1–1440 分钟。' }));
      }
      body.appendChild(UI.field({
        label: '指令时效',
        control: UI.radioCards({
          value: ttlDays, onChange: function (v) { ttlDays = v; },
          options: [
            { value: 7, title: '7 天（默认）', desc: '7 天后指令自动失效' },
            { value: 0, title: '不过期', desc: '指令长期有效；选择后需要二次确认' }
          ]
        }),
        hint: ttlDays === 0 ? '0 = 不过期：需要二次确认。' : null
      }));
      body.appendChild(h('div', { class: 'status-line' }, [
        h('span', { class: 'status-dot ' + (dev.online ? 'online' : 'offline') }),
        h('span', { class: 't-caption', text: dev.online ? '目标在线：指令可直接投递' : '目标离线：指令将保持「待投递」' })
      ]));

      foot.appendChild(UI.btn({ label: '取消', title: '关闭', onClick: function () { m.close(); } }));
      foot.appendChild(UI.btn({
        label: '发起锁定', variant: 'danger', icon: 'lock', title: '发起远程锁定',
        onClick: function () {
          if (ttlDays === 0) {
            UI.confirm({
              title: '确认「不过期」时效', tone: 'danger',
              body: h('div', { class: 'col gap-2' }, [
                caption('「不过期」的锁定指令无法自动失效，需由对端人工解锁。'),
                caption('确认继续？')
              ]),
              confirmLabel: '确认不过期',
              onConfirm: function () { settle(); }
            });
            return;
          }
          settle();
        }
      }));
    }

    function settle() {
      body.innerHTML = '';
      foot.innerHTML = '';
      var offline = !dev.online;
      body.appendChild(UI.alertbar({
        tone: offline ? 'warn' : 'info',
        icon: offline ? 'clock' : 'info',
        title: offline ? '待投递' : '指令已下发',
        text: offline
          ? '目标当前离线：指令已进入待投递队列，将在对端上线后投递。当前状态为「待投递」，不代表对端已锁定。'
          : '指令已下发，等待对端确认。对端确认前不会显示为已锁定。'
      }));
      body.appendChild(UI.kv([
        { k: '目标设备', v: dev.name },
        { k: '锁定模式', v: mode === 'immediate' ? '立即锁定' : ('延迟 ' + F.num(delayMin) + ' 分钟') },
        { k: '指令时效', v: ttlDays === 0 ? '不过期（需二次确认）' : (F.num(ttlDays) + ' 天') },
        { k: '投递状态', v: offline ? '待投递' : '已下发，等待对端确认' },
        { k: '审计事件', v: 'p2p.remote_lock.request', mono: true }
      ]));
      foot.appendChild(UI.btn({ label: '关闭', variant: 'primary', title: '关闭', onClick: function () { m.close(); } }));
      UI.toast({
        tone: offline ? 'warn' : 'info',
        title: offline ? '待投递' : '已下发',
        msg: offline ? '目标离线，指令保持待投递' : '等待对端确认'
      });
    }

    var m = UI.modal({ title: '远程锁定 · ' + dev.name, sub: '模式 / 时效 / 投递状态', size: 'md', body: body, footer: foot });
    renderForm();
    return m;
  }

  /** 路径诊断：把当前可能的中继错误映射成人话 + 建议动作 */
  function openPathDiag(dev) {
    var current = dev && dev.reason ? dev.reason : 'symmetric_nat';
    var rows = Object.keys(VS.RELAY_ERR).map(function (code) {
      var e = VS.RELAY_ERR[code];
      return {
        id: code, code: code, title: relayTitle(code), user: relayUser(code),
        suggest: RELAY_SUGGEST[code] || '已记录诊断，请导出日志后上报。'
      };
    });
    var body = h('div', { class: 'col gap-4' }, [
      UI.alertbar({
        tone: 'info', icon: 'info', title: '中继错误以人话呈现',
        text: '中继侧有独立错误空间；界面只展示映射后的结论与建议动作，不原样显示 R 码。'
      }),
      sectionTitle('当前路径与降级原因'),
      UI.kv([
        { k: '当前路径', v: dev ? (dev.path || 'direct') : 'direct' },
        { k: '降级原因', v: pathReasonText(current) },
        { k: '原因说明', v: pathReasonHint(current) },
        { k: '中继回退', v: (D.syncPolicy && D.syncPolicy.relayEnabled) ? '已开启' : '已关闭' }
      ]),
      sectionTitle('降级原因枚举（全量映射）'),
      UI.table({
        columns: [
          { key: 'code', label: '枚举值', width: '190px' },
          { key: 'label', label: '中文', width: '130px' },
          { key: 'hint', label: '说明' }
        ],
        rows: Object.keys(PATH_REASON).map(function (k) {
          return { id: k, code: k, label: PATH_REASON[k].label, hint: PATH_REASON[k].hint };
        })
      }),
      sectionTitle('当前可能的中继错误与建议动作'),
      UI.table({
        maxHeight: '300px',
        columns: [
          { key: 'title', label: '情形', width: '140px' },
          { key: 'user', label: '对用户呈现', width: '230px' },
          { key: 'suggest', label: '建议动作' }
        ],
        rows: rows
      })
    ]);
    return UI.modal({ title: '路径诊断', sub: '直连 / 打洞 / 中继 三档路径与降级原因', size: 'lg', body: body });
  }

  /** 单文件同步详情：CDC 块矩阵 / 断点续传 / 分片 / nonce / 路径 / 事件来源 */
  function openTaskDetail(t) {
    var totalChunks = Math.max(1, t.totalChunks || 1);
    var shown = Math.min(totalChunks, 240);
    var doneRatio = t.totalBytes > 0 ? (t.doneBytes || 0) / t.totalBytes : 0;
    var doneCount = Math.round(shown * doneRatio);
    var chunks = [];
    for (var i = 0; i < shown; i++) {
      if (i < doneCount) chunks.push('done');
      else if (t.state === 'failed' && i === doneCount) chunks.push('failed');
      else if (t.state === 'running' && i === doneCount) chunks.push('running');
      else chunks.push('pending');
    }
    var body = h('div', { class: 'col gap-4' }, [
      UI.kv([
        { k: '文件', v: t.name },
        { k: '方向', v: dirLabel(t.dir) },
        { k: '对端', v: t.peer || '—' },
        { k: '状态', v: (TASK_STATE[t.state] || {}).label || t.state },
        { k: '已传输', v: F.bytes(t.doneBytes || 0) + ' / ' + F.bytes(t.totalBytes) + '（' + F.pct(doneRatio) + '）' },
        { k: '速率', v: F.rate(t.rateBps) || '—' },
        { k: '剩余时间', v: t.etaMs ? F.duration(t.etaMs) : '—' }
      ]),
      sectionTitle('CDC 块矩阵'),
      UI.chunkMatrix(chunks),
      caption('共 ' + F.num(shown) + ' 块（总 ' + F.num(totalChunks) + ' 块，超过 240 块仅绘制前 240 块）。绿=已完成，蓝=传输中，红=失败，空=待传。'),
      sectionTitle('断点续传与分片'),
      UI.kv([
        { k: '可续传', v: t.resumable ? '是' : '否' },
        { k: '续传位置', v: t.resumePct !== null && t.resumePct !== undefined ? F.pct(t.resumePct) + '（跨会话）' : '—' },
        { k: '分片数', v: F.num(t.totalChunks || 0) + '（已完成 ' + F.num(t.doneChunks || 0) + '）', mono: true },
        { k: '块大小', v: F.bytes(262144) + '（CDC 均值，原型定值）' },
        { k: 'nonce 前缀', v: D.hash(16).slice(0, 16), mono: true, copy: true }
      ]),
      sectionTitle('路径与降级'),
      h('div', { class: 'row gap-2' }, [
        UI.pathBadge(t.path, PATH_REASON[t.path] ? PATH_REASON[t.path].label : null),
        h('span', { class: 't-caption', text: PATH_REASON[t.path] ? PATH_REASON[t.path].hint : '未发生降级。' })
      ]),
      sectionTitle('事件来源'),
      h('div', { class: 't-caption t-mono', text: 'TASK_QUEUED(6) → TASK_PROGRESS(7) → ' + (t.state === 'done' ? 'TASK_DONE(8)' : t.state === 'failed' ? 'TASK_FAILED(9)' : 'TASK_PAUSED/RESUMED') })
    ]);
    return UI.modal({ title: '同步详情 · ' + t.name, sub: '块级进度 / 续传 / 分片 / 路径', size: 'lg', body: body });
  }

  /* ------------------------------------------------------------ 同步策略 */
  function readSyncPolicy() {
    try {
      if (!D.syncPolicy) throw new Error('syncPolicy 缺失');
      var p = JSON.parse(JSON.stringify(D.syncPolicy));
      /* data.js 未提供选择性同步字段，此处补齐为可编辑的空集合 */
      if (!p.folderIds) p.folderIds = [];
      if (!p.excludeFolderIds) p.excludeFolderIds = [];
      return { ok: true, policy: p };
    } catch (e) {
      return {
        ok: false,
        policy: {
          enabled: true, mode: 'auto', timeWindowEnabled: false, windowStart: '22:00', windowEnd: '07:00',
          bandwidthUpKbps: 0, bandwidthDownKbps: 0, wifiOnly: false, excludePatterns: [],
          maxConcurrent: 3, conflictStrategy: 'keep_both', verifyAfterSync: true, retryLimit: 3,
          relayEnabled: true, relayFallbackReason: null, paddingLevel: 2, activeHoursOnly: false,
          folderIds: [], excludeFolderIds: []
        }
      };
    }
  }

  function openSyncPolicy() {
    var loaded = readSyncPolicy();
    var p = loaded.policy;
    var readFailed = !loaded.ok;
    var body = h('div', { class: 'col gap-4' });
    var foot = h('div', { class: 'row gap-2' });

    function render() {
      body.innerHTML = '';
      foot.innerHTML = '';

      if (readFailed) {
        body.appendChild(UI.alertbar({
          tone: 'warn', icon: 'alert',
          title: '策略读取失败，使用默认值',
          text: '未能从引擎读到同步策略，本次界面展示的是内置默认值；保存后将以本页为准。'
        }));
      }

      /* 基础 */
      body.appendChild(sectionTitle('基础'));
      body.appendChild(UI.switchCtl({
        label: '启用同步', checked: p.enabled,
        onChange: function (e) { p.enabled = e.target.checked; }
      }));
      body.appendChild(UI.field({
        label: '同步模式',
        control: UI.select({
          value: p.mode, options: SYNC_MODES,
          onChange: function (v) { p.mode = v; render(); }
        }),
        hint: p.mode === 'scheduled' ? '定时模式只在允许时段内同步。' : null
      }));

      /* 时段 */
      body.appendChild(sectionTitle('允许时段'));
      body.appendChild(UI.switchCtl({
        label: '启用时段限制', checked: p.timeWindowEnabled,
        onChange: function (e) { p.timeWindowEnabled = e.target.checked; render(); }
      }));
      if (p.timeWindowEnabled) {
        var st = UI.input({ type: 'time', value: p.windowStart, mono: true });
        var en = UI.input({ type: 'time', value: p.windowEnd, mono: true });
        st.addEventListener('input', function () { p.windowStart = st.value; });
        en.addEventListener('input', function () { p.windowEnd = en.value; });
        body.appendChild(h('div', { class: 'row gap-3' }, [
          UI.field({ label: '起始时间', control: st, hint: '跨零点时段请让起始晚于结束。' }),
          UI.field({ label: '结束时间', control: en })
        ]));
      }

      /* 带宽 */
      body.appendChild(sectionTitle('带宽与网络'));
      var upIn = UI.input({ type: 'number', value: p.bandwidthUpKbps, mono: true });
      var downIn = UI.input({ type: 'number', value: p.bandwidthDownKbps, mono: true });
      upIn.addEventListener('input', function () { p.bandwidthUpKbps = parseInt(upIn.value, 10) || 0; });
      downIn.addEventListener('input', function () { p.bandwidthDownKbps = parseInt(downIn.value, 10) || 0; });
      body.appendChild(h('div', { class: 'row gap-3' }, [
        UI.field({ label: '上行限速（KB/s）', control: upIn, hint: '0 = 不限速。' }),
        UI.field({ label: '下行限速（KB/s）', control: downIn, hint: '0 = 不限速。' })
      ]));
      body.appendChild(UI.switchCtl({
        label: '仅 WiFi 下同步', checked: p.wifiOnly,
        sub: '勾选后计量网络下不发起传输',
        onChange: function (e) { p.wifiOnly = e.target.checked; }
      }));

      /* 并发与冲突 */
      body.appendChild(sectionTitle('并发与冲突'));
      body.appendChild(UI.field({
        label: '并发数',
        control: UI.select({
          value: p.maxConcurrent,
          options: [1, 2, 3, 4, 5, 6, 7, 8].map(function (n) {
            return { value: n, label: F.num(n), sub: n === 3 ? '默认值' : null };
          }),
          onChange: function (v) { p.maxConcurrent = v; }
        }),
        hint: '默认 3：优先级顺序为 销毁指令 > 交互 > 后台。'
      }));
      body.appendChild(UI.field({
        label: '冲突策略',
        control: UI.select({
          value: p.conflictStrategy, options: CONFLICT_STRATEGIES,
          onChange: function (v) { p.conflictStrategy = v; }
        })
      }));

      /* 校验与重试 */
      body.appendChild(sectionTitle('校验与重试'));
      body.appendChild(UI.switchCtl({
        label: '同步后校验', checked: p.verifyAfterSync,
        sub: '逐块校验哈希，失败块重传',
        onChange: function (e) { p.verifyAfterSync = e.target.checked; }
      }));
      var retryIn = UI.input({ type: 'number', value: p.retryLimit, mono: true });
      retryIn.addEventListener('input', function () { p.retryLimit = parseInt(retryIn.value, 10) || 0; });
      body.appendChild(UI.field({ label: '重试上限', control: retryIn, hint: '超过后提示「请检查网络或磁盘空间」。' }));

      /* 中继与填充 */
      body.appendChild(sectionTitle('中继与填充'));
      body.appendChild(UI.switchCtl({
        label: '允许中继回退', checked: p.relayEnabled,
        sub: p.relayFallbackReason ? ('最近回退原因：' + pathReasonText(p.relayFallbackReason)) : null,
        onChange: function (e) { p.relayEnabled = e.target.checked; }
      }));
      body.appendChild(UI.field({
        label: '填充档位',
        control: UI.select({
          value: p.paddingLevel, options: PADDING_LEVELS,
          onChange: function (v) { p.paddingLevel = v; }
        }),
        hint: '填充越强，抗流量分析越好、吞吐越低。'
      }));

      /* 排除规则 */
      body.appendChild(sectionTitle('排除规则'));
      var chips = h('div', { class: 'row gap-2 wrap' });
      (p.excludePatterns || []).forEach(function (pat, idx) {
        chips.appendChild(UI.chip({
          text: pat,
          title: '移除规则 ' + pat,
          onRemove: function () { p.excludePatterns.splice(idx, 1); render(); }
        }));
      });
      if (!(p.excludePatterns || []).length) chips.appendChild(h('span', { class: 't-caption', text: '暂无排除规则。' }));
      var patInput = UI.input({ placeholder: '例如 *.iso 或 build/**', mono: true });
      var addPat = UI.btn({
        label: '添加', icon: 'plus', size: 'sm', title: '添加排除规则',
        onClick: function () {
          var v = (patInput.value || '').trim();
          if (!v) { UI.toast({ tone: 'warn', title: '规则为空', msg: '请输入通配模式。' }); return; }
          p.excludePatterns = p.excludePatterns || [];
          p.excludePatterns.push(v);
          render();
        }
      });
      body.appendChild(chips);
      body.appendChild(h('div', { class: 'row gap-2' }, [h('div', { class: 'grow' }, patInput), addPat]));

      /* 选择性同步：能力位未置位则整块不渲染 */
      if (S.cap('CAP_SELECTIVE_SYNC')) {
        body.appendChild(sectionTitle('选择性同步'));
        body.appendChild(UI.alertbar({
          tone: 'info', icon: 'info', title: 'folderIds 与 excludeFolderIds 互斥',
          text: '两者不得同时非空；同给时保存会被原子拒绝（错误码 7）。'
        }));
        var folders = (D.vault && D.vault.folders) ? D.vault.folders.filter(function (f) { return f.parent === 0; }) : [];
        function folderPicker(key, label) {
          var box = h('div', { class: 'row gap-2 wrap' });
          folders.forEach(function (f) {
            var on = (p[key] || []).indexOf(f.id) >= 0;
            box.appendChild(h('button', {
              class: 'chip' + (on ? ' chip-accent' : ''),
              type: 'button',
              title: (on ? '移出' : '加入') + label + '：' + f.name,
              onclick: function () {
                p[key] = p[key] || [];
                var i = p[key].indexOf(f.id);
                if (i >= 0) p[key].splice(i, 1); else p[key].push(f.id);
                render();
              }
            }, [h('span', { text: f.name })]));
          });
          if (!folders.length) box.appendChild(h('span', { class: 't-caption', text: '无可选文件夹。' }));
          return UI.field({
            label: label,
            control: box,
            hint: '已选 ' + F.num((p[key] || []).length) + ' 项。'
          });
        }
        body.appendChild(folderPicker('folderIds', '仅同步这些文件夹（folderIds）'));
        body.appendChild(folderPicker('excludeFolderIds', '排除这些文件夹（excludeFolderIds）'));
      }

      /* 演示开关 */
      body.appendChild(sectionTitle('演示'));
      body.appendChild(UI.switchCtl({
        label: '模拟「策略读取失败」', checked: readFailed,
        sub: '打开后展示「策略读取失败，使用默认值」提示',
        onChange: function (e) { readFailed = e.target.checked; render(); }
      }));

      foot.appendChild(UI.btn({ label: '取消', title: '放弃修改', onClick: function () { m.close(); } }));
      foot.appendChild(UI.btn({
        label: '保存策略', variant: 'primary', icon: 'save', title: '保存同步策略',
        onClick: function () {
          /* 原子拒绝：folderIds 与 excludeFolderIds 互斥 */
          var inc = (p.folderIds || []).length, exc = (p.excludeFolderIds || []).length;
          if (inc > 0 && exc > 0) {
            body.insertBefore(UI.errorBox(7, {
              text: 'folderIds（' + F.num(inc) + ' 项）与 excludeFolderIds（' + F.num(exc) + ' 项）互斥，不能同时配置。本次保存已整体拒绝，磁盘上的策略未发生任何变化。'
            }), body.firstChild);
            UI.toast({ tone: 'danger', title: '保存被拒绝', msg: '选择性同步字段互斥（错误码 7）' });
            return;
          }
          var d = denyInfo();
          if (d) {
            UI.toast({ tone: d.code === 9 ? 'warn' : 'info', title: '无法保存策略', msg: d.reason });
            return;
          }
          m.close();
          UI.toast({ tone: 'success', title: '策略已保存', msg: '审计事件 sync.policy.update' });
        }
      }));
    }

    var m = UI.modal({ title: '同步策略', sub: '模式 / 时段 / 带宽 / 并发 / 冲突 / 中继 / 填充 / 排除规则', size: 'lg', body: body, footer: foot });
    render();
    return m;
  }

  /* -------------------------------------------------------- 分享相关弹窗 */
  function openExtendShare(sh, onChange) {
    var addMin = 30;
    var input = UI.input({ type: 'number', value: addMin, mono: true });
    input.addEventListener('input', function () { addMin = parseInt(input.value, 10) || 0; });
    var m = UI.modal({
      title: '延长时效 · ' + sh.file, size: 'sm',
      body: h('div', { class: 'col gap-3' }, [
        caption('当前过期时间：' + F.dateLong(sh.expiresMs)),
        caption('剩余：' + (sh.expiresMs > now() ? F.duration(sh.expiresMs - now()) : '已过期')),
        UI.field({ label: '延长分钟数', control: input, hint: '延长后可再次调整。' })
      ]),
      footer: function () {
        return [
          UI.btn({ label: '取消', onClick: function () { m.close(); } }),
          UI.btn({
            label: '确认延长', variant: 'primary', title: '确认延长时效',
            onClick: function () {
              sh.expiresMs = Math.max(now(), sh.expiresMs) + addMin * 60000;
              m.close();
              UI.toast({ tone: 'success', title: '已延长', msg: '新过期时间 ' + F.dateLong(sh.expiresMs) });
              if (onChange) onChange();
            }
          })
        ];
      }
    });
    return m;
  }

  function openShareTimeline(sh) {
    var items = [
      { title: '分享已创建', time: F.dateLong(sh.createdMs), desc: '类型：' + (sh.kind === 'burn' ? '阅后即焚' : '限时分享') + ' · 会话上限 ' + F.num(sh.maxSessions || 0), tone: 'info' },
      { title: '已分发给接收设备', time: F.dateLong(sh.createdMs + 60000), desc: '接收设备：' + (sh.device || '—'), tone: 'info' }
    ];
    if (sh.status === 'burned') {
      items.push({ title: '对端已接受并读取', time: F.dateLong((sh.burnedMs || now()) - 60000), desc: '会话 ' + F.num(sh.sessions || 0) + ' / ' + F.num(sh.maxSessions || 0), tone: 'success' });
      items.push({ title: '已加密擦除（+ TRIM）', time: F.dateLong(sh.burnedMs || now()), desc: '密钥已销毁并向介质发出 TRIM；不宣称物理不可恢复。', tone: 'stale' });
    } else if (sh.status === 'revoked') {
      items.push({ title: '对方未接受，分享已撤销', time: F.dateLong(sh.expiresMs), desc: '票据已作废；未产生任何会话。', tone: 'warn' });
    } else if (sh.status === 'active') {
      items.push({ title: '对端已接受', time: F.dateLong(sh.expiresMs - 3600000), desc: '会话 ' + F.num(sh.sessions || 0) + ' / ' + F.num(sh.maxSessions || 0), tone: 'success' });
    } else {
      items.push({ title: '等待对方接受', time: '尚未发生', desc: '对方接受后开始计时。', tone: 'info' });
    }
    return UI.modal({
      title: '分享时间线 · ' + sh.file, size: 'md',
      body: h('div', { class: 'col gap-3' }, [
        UI.timeline(items),
        UI.alertbar({
          tone: 'info', icon: 'info',
          text: '「已加密擦除（+ TRIM）」表示密钥销毁 + 介质 TRIM 指令已发出，不等于物理不可恢复。'
        })
      ])
    });
  }

  function openCreateBurn(onDone) {
    var d = denyInfo();
    var kind = 'burn';
    var ttlMin = 60;
    var device = (D.devices || []).filter(function (x) { return !x.self; })[0];
    var ttlInput = UI.input({ type: 'number', value: ttlMin, mono: true });
    ttlInput.addEventListener('input', function () { ttlMin = parseInt(ttlInput.value, 10) || 0; });
    var m = UI.modal({
      title: '创建阅后即焚', sub: '只读降级或维护态下不可创建；进行中的焚毁不受影响', size: 'md',
      body: h('div', { class: 'col gap-3' }, [
        d ? UI.alertbar({ tone: d.code === 9 ? 'warn' : 'info', icon: 'ban', title: '当前不可创建', text: d.reason + '；已在进行中的焚毁会话仍会正常完成。' }) : null,
        UI.field({
          label: '类型',
          control: UI.radioCards({
            value: kind, onChange: function (v) { kind = v; },
            options: [
              { value: 'burn', title: '阅后即焚', desc: '对端读取一次后立即擦除' },
              { value: 'expiring', title: '限时分享', desc: '在时效内可多次读取' }
            ]
          })
        }),
        UI.field({ label: '时效（分钟）', control: ttlInput, hint: '到期未接受则自动撤销。' }),
        UI.kv([{ k: '接收设备', v: device ? device.name : '—' }])
      ]),
      footer: function () {
        return [
          UI.btn({ label: '取消', onClick: function () { m.close(); } }),
          UI.btn({
            label: '创建', variant: 'primary', icon: 'erase',
            disabled: !!d, title: d ? d.reason : '创建分享票据',
            onClick: function () {
              (D.shares = D.shares || []).push({
                id: 'shr-' + D.hash(4), fileId: 0, file: '（新选择）', kind: kind,
                createdMs: now(), expiresMs: now() + ttlMin * 60000, status: 'waiting',
                sessions: 0, maxSessions: kind === 'burn' ? 1 : 5,
                device: device ? device.name : '—', burnedMs: null
              });
              m.close();
              UI.toast({ tone: 'success', title: '已创建', msg: '票据已生成，等待对方接受。' });
              if (onDone) onDone();
            }
          })
        ];
      }
    });
    return m;
  }

  /* ==========================================================================
   * 4. 页面：devices
   * ========================================================================*/
  VS.pages['devices'] = function (ctx) {
    var body = h('div', { class: 'page-body' });

    function render() {
      body.innerHTML = '';
      var self = null, peers = [];
      (D.devices || []).forEach(function (d) { if (d.self) self = d; else peers.push(d); });

      /* --- 本机 --- */
      if (self) {
        body.appendChild(UI.card({
          title: '本机',
          sub: '这是当前实例的设备身份；指纹用于在配对时与对端核对。',
          actions: [
            UI.btn({
              label: '路径诊断', icon: 'discover', size: 'sm', title: '打开路径诊断',
              onClick: function () { openPathDiag(null); }
            })
          ],
          body: h('div', { class: 'col gap-3' }, [
            h('div', { class: 'row gap-3 wrap' }, [
              h('div', { class: 'grow' }, UI.kv([
                { k: '设备名', v: self.name },
                { k: '操作系统', v: self.os || '—' },
                { k: '监听端口', v: F.num(self.port), mono: true },
                { k: '协议版本', v: self.version, mono: true }
              ])),
              h('div', { class: 'col gap-2' }, [
                h('div', { class: 'stat-label', text: '监听路径' }),
                UI.pathBadge('direct')
              ])
            ]),
            h('div', { class: 't-mono' }, UI.copyField(self.fingerprint, '复制本机指纹'))
          ])
        }));
      }

      /* --- 已配对设备 --- */
      var rows = h('div', { class: 'col' });
      if (!peers.length) {
        rows.appendChild(UI.empty({ icon: 'devices', title: '还没有已配对设备', desc: '通过「发起配对」添加你的第一台设备。' }));
      }
      peers.forEach(function (dev) {
        var note = h('div', {
          class: 'panel-alt', style: {
            display: 'none', padding: '8px 12px', borderRadius: 'var(--r-sm)',
            marginTop: '4px', fontSize: 'var(--fs-caption)'
          }
        });
        var timer = null;
        note.textContent = pathReasonText(dev.reason) + ' — ' + pathReasonHint(dev.reason);

        var metaBits = [];
        metaBits.push('上次同步 ' + F.relative(dev.lastSyncMs));
        if (typeof dev.latencyMs === 'number') metaBits.push('延迟 ' + F.num(dev.latencyMs) + ' ms');
        else metaBits.push('延迟 —');
        if (dev.reason) metaBits.push('降级：' + pathReasonText(dev.reason));

        function toggleNote(e) {
          var host = e.currentTarget;
          if (note.style.display === 'none') {
            note.style.display = '';
            if (timer) clearTimeout(timer);
            timer = setTimeout(function () { note.style.display = 'none'; timer = null; }, 3000);
          } else {
            note.style.display = 'none';
            if (timer) { clearTimeout(timer); timer = null; }
          }
          host.setAttribute('aria-expanded', note.style.display === 'none' ? 'false' : 'true');
        }

        var row = h('div', {
          class: 'list-row', title: dev.name + ' · ' + (dev.online ? '在线' : '离线')
        }, [
          h('span', { class: 'status-dot ' + (dev.online ? 'online' : 'offline'), title: dev.online ? '在线' : '离线' }),
          h('div', { class: 'lr-main' }, [
            h('div', { class: 'lr-title' }, [
              h('span', { class: 't-strong t-truncate', text: dev.name, title: dev.name }),
              UI.pathBadge(dev.path, dev.reason ? pathReasonText(dev.reason) : null),
              dev.trust === 'verified' ? UI.badge({ text: '已验证', tone: 'success', icon: 'check' }) : UI.badge({ text: '未验证', tone: 'warn' })
            ]),
            h('div', { class: 'lr-sub', text: metaBits.join(' · ') }),
            note
          ]),
          h('div', { class: 'lr-actions' }, [
            UI.btn({
              label: '路径降级说明', icon: 'info', size: 'sm',
              disabled: !dev.reason,
              title: !dev.reason ? '该设备当前无降级原因' : '展开 3 秒后自动收起',
              onClick: toggleNote,
              attrs: { 'aria-expanded': 'false' }
            }),
            UI.btn({
              label: '同步', icon: 'refresh', size: 'sm',
              disabled: !dev.online || !!denyInfo(),
              title: !dev.online ? '设备离线，无法立即同步' : (denyInfo() ? denyInfo().reason : '立即与该设备同步'),
              onClick: function () { VS.actions['syncNow'] ? VS.actions['syncNow']({ device: dev.id }) : null; }
            }),
            UI.iconBtn({
              icon: 'more', label: '更多操作：' + dev.name,
              onClick: function (e) {
                var d = denyInfo();
                var items = [
                  { label: '查看详情', icon: 'eye', onClick: function () { openDeviceDetail(dev); } },
                  {
                    label: '立即同步', icon: 'refresh',
                    disabled: !dev.online || !!d,
                    reason: !dev.online ? '设备离线，无法立即同步' : (d ? d.reason : null),
                    onClick: function () { VS.actions['syncNow'] ? VS.actions['syncNow']({ device: dev.id }) : null; }
                  },
                  { label: '诊断路径', icon: 'discover', onClick: function () { openPathDiag(dev); } },
                  {
                    label: '远程锁定', icon: 'lock',
                    disabled: !!d, reason: d ? d.reason : null,
                    onClick: function () { openRemoteLock(dev); }
                  },
                  { sep: true },
                  {
                    label: '解除配对', icon: 'trash', danger: true,
                    disabled: !!d, reason: d ? d.reason : null,
                    onClick: function () { confirmUnpair(dev, render); }
                  }
                ];
                menuFrom(e.currentTarget, items);
              }
            })
          ])
        ]);
        rows.appendChild(row);
      });

      body.appendChild(UI.card({
        title: '已配对设备',
        sub: '共 ' + F.num(peers.length) + ' 台 · 在线 ' + F.num(peers.filter(function (d) { return d.online; }).length) + ' 台',
        actions: [
          UI.btn({
            label: '全部同步', icon: 'sync', size: 'sm',
            disabled: !!denyInfo(),
            title: denyInfo() ? denyInfo().reason : '同步全部在线设备',
            onClick: function () { VS.actions['syncNow'] ? VS.actions['syncNow']({ all: true }) : null; }
          })
        ],
        body: rows
      }));

      /* --- 发现到（未配对）--- */
      if (S.cap('CAP_DISCOVERY')) {
        var discovered = D.discovered || [];
        var dBody = h('div', { class: 'col gap-2' });
        if (!discovered.length) {
          dBody.appendChild(UI.empty({ icon: 'discover', title: '未发现局域网设备', desc: '请确认对端与本机处于同一网段。' }));
        }
        discovered.forEach(function (d) {
          dBody.appendChild(h('div', { class: 'discover-card', dataset: { path: d.path } }, [
            h('span', { class: 'dc-ico', html: VS.icon('discover', 22), title: '局域网设备' }),
            h('div', { class: 'grow' }, [
              h('div', { class: 't-mono', text: '局域网设备 · 指纹 ' + d.fingerprint }),
              h('div', { class: 't-caption dc-path', text: '候选地址 ' + F.num((d.addrs || []).length) + ' 个 · 类型 ' + (d.addrs || []).map(function (a) { return String(a.type).toUpperCase(); }).join(' / ') })
            ]),
            UI.btn({
              label: '配对', icon: 'link', size: 'sm', variant: 'primary',
              title: '与该设备进入配对向导',
              onClick: function () { openPairingWizard(d.path === 'direct' ? 'discover' : 'invite'); }
            })
          ]));
        });
        body.appendChild(UI.card({
          title: '发现到（未配对）',
          sub: '未配对设备只暴露指纹与候选地址数量，不暴露 IP / 端口 / 名称。',
          actions: [
            UI.btn({ label: '邀请码', icon: 'key', size: 'sm', title: '用邀请码配对', onClick: function () { openPairingWizard('invite'); } }),
            UI.btn({ label: '扫码', icon: 'qr', size: 'sm', title: '用二维码配对', onClick: function () { openPairingWizard('qr'); } }),
            UI.btn({ label: '手动输入地址', icon: 'edit', size: 'sm', title: '手动输入对端地址配对', onClick: function () { openPairingWizard('manual'); } })
          ],
          body: dBody
        }));
      } else {
        /* 能力位未置位：整块不渲染发现结果，只保留降级入口 */
        body.appendChild(UI.card({
          title: '局域网发现不可用',
          sub: '本端未置位能力位 CAP_DISCOVERY：不显示任何发现结果。',
          body: h('div', { class: 'col gap-3' }, [
            UI.alertbar({
              tone: 'info', icon: 'ban', title: '局域网发现不可用',
              text: '能力位 CAP_DISCOVERY 未置位，本端不进行自动发现，也不渲染任何发现结果。可改用下列降级入口完成配对。'
            }),
            h('div', { class: 'row gap-2 wrap' }, [
              UI.btn({ label: '邀请码', icon: 'key', title: '用邀请码配对', onClick: function () { openPairingWizard('invite'); } }),
              UI.btn({ label: '扫码', icon: 'qr', title: '用二维码配对', onClick: function () { openPairingWizard('qr'); } }),
              UI.btn({ label: '手动输入地址', icon: 'edit', title: '手动输入对端地址配对', onClick: function () { openPairingWizard('manual'); } })
            ])
          ])
        }));
      }

      /* --- 阅后即焚 / 分享入口 --- */
      if (S.cap('CAP_BURN_SHARE')) {
        var active = (D.shares || []).filter(function (s2) { return s2.status === 'waiting' || s2.status === 'active'; });
        body.appendChild(UI.card({
          title: '阅后即焚 / 分享',
          sub: '进行中 ' + F.num(active.length) + ' 个会话',
          actions: [
            UI.btn({
              label: '打开会话列表', icon: 'share', size: 'sm', title: '前往同步页的阅后即焚分页',
              onClick: function () { ctx.go('sync', { tab: 'shares' }); }
            }),
            guardedBtn({
              label: '创建焚毁', icon: 'erase', size: 'sm',
              title: '创建阅后即焚分享',
              onClick: function () { openCreateBurn(render); }
            })
          ],
          body: (active.length
            ? h('div', { class: 'col' }, active.map(function (s2) { return shareRow(s2, render); }))
            : UI.empty({ icon: 'erase', title: '没有进行中的分享', desc: '创建后可在同步页查看时间线。' }))
        }));
      }

      /* --- 设备发现能力位状态提示 --- */
      body.appendChild(UI.card({
        title: '能力位与路径',
        sub: '前端按能力位决定「渲染 / 置灰 / 不渲染」，不降级为假装成功。',
        body: h('div', { class: 'gauge-grid' }, [
          UI.gauge({ key: 'discovery', icon: 'discover', name: '局域网发现', state: S.cap('CAP_DISCOVERY') ? 'ok' : 'off', badge: UI.tierMark(S.capTier('CAP_DISCOVERY')), src: 'capability_bits.CAP_DISCOVERY' }),
          UI.gauge({ key: 'relay', icon: 'relay', name: '中继回退', state: S.cap('CAP_RELAY') && D.syncPolicy.relayEnabled ? 'ok' : 'off', badge: UI.tierMark(S.capTier('CAP_RELAY')), src: 'capability_bits.CAP_RELAY' }),
          UI.gauge({ key: 'punch', icon: 'link', name: 'NAT 打洞', state: S.cap('CAP_HOLE_PUNCH') ? 'ok' : 'off', badge: UI.tierMark(S.capTier('CAP_HOLE_PUNCH')), src: 'capability_bits.CAP_HOLE_PUNCH' })
        ])
      }));
    }

    render();
    return h('div', { class: 'page' }, [
      h('div', { class: 'page-head' }, [
        h('div', { class: 'page-titles' }, [
          h('h1', { class: 't-h1', text: '设备' }),
          h('div', { class: 't-caption', text: '本机身份 · 已配对设备 · 局域网发现 · 配对向导' })
        ]),
        h('div', { class: 'page-actions' }, [
          UI.btn({ label: '路径诊断', icon: 'discover', title: '打开路径诊断', onClick: function () { openPathDiag(null); } }),
          guardedBtn({
            label: '发起配对', icon: 'link', variant: 'primary',
            title: '打开配对向导',
            onClick: function () { openPairingWizard(null); }
          })
        ])
      ]),
      body
    ]);
  };

  /** 解除配对确认（danger） */
  function confirmUnpair(dev, onDone) {
    return UI.confirm({
      title: '解除配对 · ' + dev.name,
      tone: 'danger',
      body: h('div', { class: 'col gap-2' }, [
        caption('解除后本机将丢弃该设备的公钥绑定与节点清单条目。'),
        caption('已同步的文件不会被删除；但该设备不再被视为可信，需要重新配对。'),
        UI.kv([{ k: '设备指纹', v: dev.fingerprint, mono: true, copy: true }])
      ]),
      confirmLabel: '解除配对',
      onConfirm: function () {
        UI.toast({ tone: 'warn', title: '已解除配对', msg: dev.name + ' 已从节点清单移除（Mock）' });
        if (onDone) onDone();
      }
    });
  }

  /* ==========================================================================
   * 5. 页面：sync
   * ========================================================================*/
  VS.pages['sync'] = function (ctx) {
    var tab = (ctx.params && ctx.params.tab) || 'transfer';
    var loading = false;
    var body = h('div', { class: 'page-body' });

    function refresh() { render(); }

    function overviewCard() {
      var st = queueStats();
      var hasQueue = st.count > 0;
      var capTasks = S.cap('CAP_TASKS');
      var cards = h('div', { class: 'grid-4', style: { gap: 'var(--sp-3)' } }, [
        statCard('在线设备', F.num(onlineDeviceCount()), '共 ' + F.num((D.devices || []).length - 1) + ' 台已配对'),
        statCard('队列深度', hasQueue ? (F.num(st.active) + ' + ' + F.num(st.queued)) : '—', '活动 + 排队'),
        statCard('冲突', F.num(conflictCount()), conflictCount() ? '存在未解决冲突副本' : '无冲突'),
        statCard('已同步字节', (st.doneBytes === null ? '—' : F.bytes(st.doneBytes) + ' / ' + F.bytes(st.totalBytes)), '本次会话累计')
      ]);

      var rateBlock = null;
      if (capTasks) {
        rateBlock = h('div', { class: 'col gap-2' }, [
          h('div', { class: 'row gap-3 wrap' }, [
            h('span', { class: 't-mono', text: '↑ ' + (F.rate(st.up) || '—') + '  ↓ ' + (F.rate(st.down) || '—'), title: '上行 / 下行速率' }),
            st.eta ? h('span', { class: 't-caption', text: '预计剩余 ' + F.duration(st.eta) }) : null
          ]),
          caption('速率与 ETA 依赖能力位 CAP_TASKS；未置位时整块隐藏。')
        ]);
      }

      return UI.card({
        title: '总览',
        sub: '任一指标无数据来源时显示「—」；不把「无数据」画成 0。',
        actions: [
          UI.btn({
            label: '刷新', icon: 'refresh', size: 'sm', title: '刷新总览',
            onClick: function () { loading = true; render(); setTimeout(function () { loading = false; render(); }, 420); }
          }),
          UI.btn({ label: '同步策略', icon: 'settings', size: 'sm', title: '打开同步策略设置', onClick: function () { openSyncPolicy(); } })
        ],
        body: loading
          ? skeletonRows(3, ['70%', '52%', '61%'])
          : h('div', { class: 'row gap-4', style: { alignItems: 'center' } }, [
            h('div', { class: 'col gap-3 grow' }, [
              cards,
              rateBlock,
              h('div', { class: 'row gap-2 wrap' }, [
                UI.badge({ text: '同步状态：' + String(S.get('syncState')), tone: S.get('syncState') === 'Syncing' ? 'accent' : 'outline' }),
                UI.badge({ text: '中继回退：' + (D.syncPolicy.relayEnabled ? '开' : '关'), tone: 'outline' }),
                UI.badge({ text: '填充档位 T' + F.num(D.syncPolicy.paddingLevel), tone: 'outline' })
              ])
            ]),
            UI.ring({
              size: 96, stroke: 9,
              pct: st.pct,
              tone: st.pct === null ? 'default' : 'accent',
              label: st.pct === null ? '—' : F.pct(st.pct)
            })
          ])
      });
    }

    function transferSection(dir) {
      var list = (D.queue || []).filter(function (t) { return t.dir === dir; });
      var card = UI.card({
        title: dir === 'up' ? '上传' : '下载',
        sub: F.num(list.length) + ' 个任务',
        body: list.length
          ? h('div', { class: 'col' }, list.map(function (t) { return queueRow(t, ctx, refresh); }))
          : UI.empty({ icon: dir === 'up' ? 'import' : 'export', title: dir === 'up' ? '没有上传任务' : '没有下载任务', desc: '队列为空时不显示任何进度条。' })
      });
      return card;
    }

    function conflictSection() {
      return UI.card({
        title: '冲突',
        sub: conflictCount() ? '存在 1 组冲突副本，需要人工选择保留策略' : '没有冲突',
        body: conflictPairView(function () { refresh(); })
      });
    }

    function sharesSection() {
      var list = D.shares || [];
      return UI.card({
        title: '阅后即焚 / 分享',
        sub: '共 ' + F.num(list.length) + ' 个会话',
        actions: [
          guardedBtn({
            label: '创建焚毁', icon: 'erase', size: 'sm', title: '创建阅后即焚分享',
            onClick: function () { openCreateBurn(refresh); }
          })
        ],
        body: list.length
          ? h('div', { class: 'col' }, list.map(function (s2) { return shareRow(s2, refresh); }))
          : UI.empty({ icon: 'erase', title: '没有分享会话', desc: '创建后在这里查看时效与时间线。' })
      });
    }

    function render() {
      body.innerHTML = '';
      body.appendChild(overviewCard());

      var tabs = [];
      tabs.push({ value: 'transfer', label: '传输' });
      tabs.push({ value: 'conflict', label: '冲突', count: conflictCount() });
      if (S.cap('CAP_BURN_SHARE')) tabs.push({ value: 'shares', label: '阅后即焚' });

      body.appendChild(UI.tabs({
        value: tab, items: tabs,
        onChange: function (v) { tab = v; render(); }
      }));

      if (tab === 'transfer') {
        if (!S.cap('CAP_TASKS')) {
          body.appendChild(UI.alertbar({
            tone: 'info', icon: 'ban', title: '任务队列未启用',
            text: '能力位 CAP_TASKS 未置位：不渲染传输队列、速率与 ETA。'
          }));
        } else if (loading) {
          body.appendChild(UI.card({ title: '传输队列', body: skeletonRows(5) }));
        } else {
          body.appendChild(transferSection('up'));
          body.appendChild(transferSection('down'));
        }
      } else if (tab === 'conflict') {
        body.appendChild(conflictSection());
      } else if (tab === 'shares') {
        body.appendChild(sharesSection());
      }
    }

    render();
    return h('div', { class: 'page' }, [
      h('div', { class: 'page-head' }, [
        h('div', { class: 'page-titles' }, [
          h('h1', { class: 't-h1', text: '同步' }),
          h('div', { class: 't-caption', text: '总览 · 传输队列 · 冲突解决 · 阅后即焚' })
        ]),
        h('div', { class: 'page-actions' }, [
          UI.btn({ label: '同步策略', icon: 'settings', title: '打开同步策略设置', onClick: function () { openSyncPolicy(); } }),
          UI.btn({ label: '前往队列', icon: 'queue', title: '打开传输队列页', onClick: function () { ctx.go('queue'); } }),
          guardedBtn({ label: '立即同步', icon: 'refresh', variant: 'primary', title: '立即同步', onClick: function () { VS.actions['syncNow'] ? VS.actions['syncNow']({}) : null; } })
        ])
      ]),
      body
    ]);
  };

  /* ==========================================================================
   * 6. 页面：queue
   * ========================================================================*/
  VS.pages['queue'] = function (ctx) {
    var filter = (ctx.params && ctx.params.filter) || 'all';
    var body = h('div', { class: 'page-body' });

    var FILTERS = [
      { value: 'all', label: '全部' },
      { value: 'up', label: '上传' },
      { value: 'down', label: '下载' },
      { value: 'conflict', label: '冲突' },
      { value: 'done', label: '已完成' },
      { value: 'failed', label: '失败' }
    ];

    function applyFilter(list) {
      if (filter === 'all' || filter === 'conflict') return list;
      if (filter === 'up') return list.filter(function (t) { return t.dir === 'up'; });
      if (filter === 'down') return list.filter(function (t) { return t.dir === 'down'; });
      if (filter === 'done') return list.filter(function (t) { return t.state === 'done'; });
      if (filter === 'failed') return list.filter(function (t) { return t.state === 'failed' || t.state === 'interrupted'; });
      return list;
    }

    function countFor(v) {
      var list = D.queue || [];
      if (v === 'conflict') return conflictCount();
      return applyFilterTo(list, v).length;
    }
    /* countFor 与 applyFilter 共用同一规则，但需显式传值 */
    function applyFilterTo(list, v) {
      if (v === 'all' || v === 'conflict') return list;
      if (v === 'up') return list.filter(function (t) { return t.dir === 'up'; });
      if (v === 'down') return list.filter(function (t) { return t.dir === 'down'; });
      if (v === 'done') return list.filter(function (t) { return t.state === 'done'; });
      if (v === 'failed') return list.filter(function (t) { return t.state === 'failed' || t.state === 'interrupted'; });
      return list;
    }

    function render() {
      body.innerHTML = '';
      var all = D.queue || [];
      var d = denyInfo();

      /* 批量操作 */
      body.appendChild(UI.card({
        title: '队列操作',
        sub: '批量操作受写门禁约束',
        actions: [
          UI.btn({
            label: '全部暂停', icon: 'pause', size: 'sm',
            disabled: !!d, title: d ? d.reason : '暂停所有活动任务',
            onClick: function () {
              all.forEach(function (t) { if (t.state === 'running') { t.state = 'paused'; t.rateBps = 0; t.etaMs = null; } });
              render();
              UI.toast({ tone: 'info', title: '已全部暂停', msg: '活动任务已转入暂停态。' });
            }
          }),
          UI.btn({
            label: '全部恢复', icon: 'play', size: 'sm',
            disabled: !!d, title: d ? d.reason : '恢复所有已暂停任务',
            onClick: function () {
              all.forEach(function (t) { if (t.state === 'paused') t.state = 'running'; });
              render();
              UI.toast({ tone: 'info', title: '已全部恢复', msg: '已暂停任务已恢复传输。' });
            }
          }),
          UI.btn({
            label: '清理已完成', icon: 'trash', size: 'sm',
            disabled: !!d, title: d ? d.reason : '从队列移除已完成任务',
            onClick: function () {
              var before = all.length;
              var kept = all.filter(function (t) { return t.state !== 'done'; });
              D.queue.length = 0;
              kept.forEach(function (t) { D.queue.push(t); });
              render();
              UI.toast({ tone: 'success', title: '已清理', msg: '移除 ' + F.num(before - kept.length) + ' 条已完成记录。' });
            }
          })
        ],
        body: h('div', { class: 'row gap-3 wrap' }, [
          h('span', { class: 't-caption', text: '活动 ' + F.num(queueStats().active) + ' · 排队 ' + F.num(queueStats().queued) + ' · 共 ' + F.num(all.length) + ' 条' }),
          h('span', { class: 't-caption', text: '并发上限 ' + F.num(D.syncPolicy.maxConcurrent) + '（优先级：销毁指令 > 交互 > 后台）' })
        ])
      }));

      /* 分页 */
      body.appendChild(UI.tabs({
        value: filter,
        items: FILTERS.map(function (f) { return { value: f.value, label: f.label, count: countFor(f.value) }; }),
        onChange: function (v) { filter = v; render(); }
      }));

      if (filter === 'conflict') {
        body.appendChild(UI.card({
          title: '冲突', sub: '按块合并 ≠ 语义合并',
          body: conflictPairView(function () { render(); })
        }));
        return;
      }

      var list = applyFilter(all);

      /* 甘特 */
      body.appendChild(UI.card({
        title: '甘特式时间线',
        sub: '按任务起止时间投影；已取消任务不绘制',
        body: ganttView(list)
      }));

      /* 任务列表 */
      body.appendChild(UI.card({
        title: '任务列表',
        sub: F.num(list.length) + ' 条',
        body: list.length
          ? h('div', { class: 'col' }, list.map(function (t) { return queueRow(t, ctx, render); }))
          : UI.empty({
            icon: 'queue', title: '该分类下没有任务',
            desc: filter === 'failed' ? '没有失败或中断的任务。' : '切换分页查看更多任务。'
          })
      }));
    }

    render();
    return h('div', { class: 'page' }, [
      h('div', { class: 'page-head' }, [
        h('div', { class: 'page-titles' }, [
          h('h1', { class: 't-h1', text: '传输队列' }),
          h('div', { class: 't-caption', text: '全部分类 · 甘特视图 · 批量操作' })
        ]),
        h('div', { class: 'page-actions' }, [
          UI.btn({ label: '同步总览', icon: 'sync', title: '返回同步总览', onClick: function () { ctx.go('sync'); } }),
          guardedBtn({ label: '立即同步', icon: 'refresh', variant: 'primary', title: '立即同步', onClick: function () { VS.actions['syncNow'] ? VS.actions['syncNow']({}) : null; } })
        ])
      ]),
      body
    ]);
  };

  /* ==========================================================================
   * 7. 全局动作注册
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

  VS.actions['openPairing'] = function () { return openPairingWizard(null); };
  VS.actions['openPathDiag'] = function () { return openPathDiag(null); };
  VS.actions['openSyncPolicy'] = function () { return openSyncPolicy(); };
  VS.actions['openTaskDetail'] = function (task) { return openTaskDetail(task); };
  VS.actions['openCreateBurn'] = function () { return openCreateBurn(null); };

  VS.actions['pauseAll'] = function () {
    var d = denyInfo();
    if (d) { UI.toast({ tone: 'warn', title: '无法暂停', msg: d.reason }); return false; }
    (D.queue || []).forEach(function (t) { if (t.state === 'running') { t.state = 'paused'; t.rateBps = 0; t.etaMs = null; } });
    VS.renderRoute && VS.renderRoute();
    return true;
  };
  VS.actions['resumeAll'] = function () {
    var d = denyInfo();
    if (d) { UI.toast({ tone: 'warn', title: '无法恢复', msg: d.reason }); return false; }
    (D.queue || []).forEach(function (t) { if (t.state === 'paused') t.state = 'running'; });
    VS.renderRoute && VS.renderRoute();
    return true;
  };
  VS.actions['clearDone'] = function () {
    var d = denyInfo();
    if (d) { UI.toast({ tone: 'warn', title: '无法清理', msg: d.reason }); return false; }
    var kept = (D.queue || []).filter(function (t) { return t.state !== 'done'; });
    D.queue.length = 0;
    kept.forEach(function (t) { D.queue.push(t); });
    VS.renderRoute && VS.renderRoute();
    return true;
  };

})(window);
