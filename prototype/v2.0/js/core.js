/* ============================================================================
 * VaultSync V2.0 原型 — 内核：工具 / 图标 / 错误码 / 状态仓库 / 事件模拟
 * 纯静态、零依赖、file:// 双击可用（全部经典脚本，不使用 ES Module）
 * ==========================================================================*/
(function (global) {
  'use strict';

  var VS = global.VS = global.VS || {};

  /* ======================= 1. 工具 ===================================== */
  var U = VS.util = {
    /** 创建元素：h('div', {class:'x', onclick:fn}, [child|string]) */
    h: function (tag, attrs, children) {
      var el = document.createElement(tag);
      if (attrs) {
        Object.keys(attrs).forEach(function (k) {
          var v = attrs[k];
          if (v === null || v === undefined || v === false) return;
          if (k === 'class' || k === 'className') { el.className = v; }
          else if (k === 'html') { el.innerHTML = v; }
          else if (k === 'text') { el.textContent = v; }
          else if (k === 'style' && typeof v === 'object') { Object.assign(el.style, v); }
          else if (k.indexOf('on') === 0 && typeof v === 'function') { el.addEventListener(k.slice(2), v); }
          else if (k === 'dataset' && typeof v === 'object') { Object.assign(el.dataset, v); }
          else { el.setAttribute(k, v === true ? '' : v); }
        });
      }
      U.append(el, children);
      return el;
    },
    append: function (el, children) {
      if (children === null || children === undefined || children === false) return el;
      if (Array.isArray(children)) {
        children.forEach(function (c) { U.append(el, c); });
      } else if (children instanceof Node) {
        el.appendChild(children);
      } else {
        el.appendChild(document.createTextNode(String(children)));
      }
      return el;
    },
    frag: function (children) { return U.append(document.createDocumentFragment(), children); },
    svg: function (inner, size) {
      return '<svg viewBox="0 0 24 24" width="' + (size || 18) + '" height="' + (size || 18) +
        '" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" ' +
        'stroke-linejoin="round" aria-hidden="true">' + inner + '</svg>';
    },
    esc: function (s) {
      return String(s === null || s === undefined ? '' : s)
        .replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
        .replace(/"/g, '&quot;').replace(/'/g, '&#39;');
    },
    uid: (function () { var n = 0; return function (p) { return (p || 'id') + '-' + (++n); }; })(),
    clamp: function (v, a, b) { return Math.min(b, Math.max(a, v)); },
    debounce: function (fn, ms) {
      var t; return function () { var a = arguments, s = this; clearTimeout(t); t = setTimeout(function () { fn.apply(s, a); }, ms); };
    },
    throttle: function (fn, ms) {
      var last = 0, timer = null;
      return function () {
        var a = arguments, s = this, now = Date.now();
        if (now - last >= ms) { last = now; fn.apply(s, a); }
        else if (!timer) { timer = setTimeout(function () { timer = null; last = Date.now(); fn.apply(s, a); }, ms - (now - last)); }
      };
    },
    /** 简易语法高亮无关的哈希缩写 */
    shortHash: function (s, head, tail) {
      head = head || 6; tail = tail === undefined ? 4 : tail;
      s = String(s || '');
      if (s.length <= head + tail + 1) return s;
      return s.slice(0, head) + '…' + s.slice(s.length - tail);
    },
    /** 确定性伪随机（保证每次打开原型数据一致） */
    seeded: function (seed) {
      var s = seed >>> 0;
      return function () { s = (s * 1664525 + 1013904223) >>> 0; return s / 4294967296; };
    }
  };

  /* ======================= 2. 格式化（对齐 docs/v2.0/03 §5.3 intl 规则） */
  var LOCALE = 'zh-CN';
  var F = VS.fmt = {
    locale: function () { return LOCALE; },
    setLocale: function (l) { LOCALE = l; },
    /** 千分位数字（P9-6：禁止 `'$num'` 直出） */
    num: function (n, opt) {
      if (n === null || n === undefined || isNaN(n)) return '—';
      return new Intl.NumberFormat(LOCALE, opt).format(n);
    },
    /** 百分比，进度固定 0 位小数 */
    pct: function (v, digits) {
      if (v === null || v === undefined || isNaN(v)) return '—';
      return new Intl.NumberFormat(LOCALE, { style: 'percent', minimumFractionDigits: digits || 0, maximumFractionDigits: digits || 0 }).format(v);
    },
    /** 文件体积：数值走 NumberFormat，单位走词条，1024 阈值，≤1 位小数 */
    bytes: function (n) {
      if (n === null || n === undefined || isNaN(n)) return '—';
      if (n < 1024) return U.num ? F.num(n) + ' B' : n + ' B';
      var units = ['B', 'KB', 'MB', 'GB', 'TB', 'PB'], i = 0, v = n;
      while (v >= 1024 && i < units.length - 1) { v /= 1024; i++; }
      var s = new Intl.NumberFormat(LOCALE, { maximumFractionDigits: 1, minimumFractionDigits: 0 }).format(v);
      return s + ' ' + units[i];
    },
    /** 速率：数值本地化，单位符号不本地化 */
    rate: function (bps) {
      if (bps === null || bps === undefined || bps <= 0) return null;
      return F.bytes(bps) + '/s';
    },
    /** 列表内时间：跟随 locale 的 MMMd + Hm */
    dateShort: function (ms) {
      if (!ms) return '—';
      var d = new Date(ms);
      return new Intl.DateTimeFormat(LOCALE, { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit', hour12: false }).format(d);
    },
    /** 长日期 / 审计时间 */
    dateLong: function (ms) {
      if (!ms) return '—';
      return new Intl.DateTimeFormat(LOCALE, { year: 'numeric', month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit', second: '2-digit', hour12: false }).format(new Date(ms));
    },
    /** 相对时间（复数规则走词条） */
    relative: function (ms, now) {
      if (!ms) return '—';
      var diff = (now || Date.now()) - ms;
      var abs = Math.abs(diff);
      var min = 60000, hour = 3600000, day = 86400000;
      if (abs < min) return '刚刚';
      if (abs < hour) { var m = Math.round(abs / min); return m + ' 分钟前'; }
      if (abs < day) { var hh = Math.round(abs / hour); return hh + ' 小时前'; }
      if (abs < 30 * day) { var dd = Math.round(abs / day); return dd + ' 天前'; }
      return F.dateShort(ms);
    },
    /** 时长（etaMs） */
    duration: function (ms) {
      if (ms === null || ms === undefined || ms < 0) return '—';
      var s = Math.round(ms / 1000);
      if (s < 60) return s + ' s';
      var m = Math.floor(s / 60), r = s % 60;
      if (m < 60) return m + ' 分' + (r ? ' ' + r + ' 秒' : '');
      return Math.floor(m / 60) + ' 小时 ' + (m % 60) + ' 分';
    },
    /** 单位换算 helper（原型用，非真实路径） */
    pctOf: function (a, b) { return b > 0 ? U.clamp(a / b, 0, 1) : 0; }
  };

  /* ======================= 3. 错误码（docs/v2.0/03 §3.2 全 14 码） ==== */
  VS.ERR = {
    0:  { key: 'ok',            tone: 'success', title: '成功',           hint: '' },
    1:  { key: 'pwWrong',       tone: 'danger',  title: '密码错误',        hint: '第 n 次失败后进入冷却' },
    2:  { key: 'cooldown',      tone: 'warn',    title: '冷却中',          hint: '请等待倒计时结束后重试' },
    3:  { key: 'io',            tone: 'danger',  title: '磁盘不可写',      hint: '检查权限或空间；可重试或打开诊断' },
    4:  { key: 'noSecureStore', tone: 'stale',   title: '安全存储不可用',  hint: '生物识别入口按能力隐藏，不报错' },
    5:  { key: 'bioUnbound',    tone: 'info',    title: '未绑定生物识别',  hint: '引导绑定' },
    6:  { key: 'badFormat',     tone: 'danger',  title: '格式错误',        hint: '不可重试；给出取证建议与迁移入口' },
    7:  { key: 'badArg',        tone: 'danger',  title: '参数非法',        hint: '前端缺陷：上报诊断，不向用户暴露技术细节' },
    8:  { key: 'internal',      tone: 'danger',  title: '引擎内部错误',    hint: '上报诊断 + 收集日志入口' },
    9:  { key: 'leaseBusy',     tone: 'stale',   title: '租约被占 · 只读', hint: '另一实例持有写租约；不弹错误框，切只读 UI' },
    10: { key: 'maintenance',   tone: 'maintenance', title: '维护态',      hint: '业务写入冻结；数据可读' },
    11: { key: 'needRecovery',  tone: 'danger',  title: '需要恢复',        hint: '无法给出确定视图；进入恢复向导' },
    12: { key: 'cancelled',     tone: 'info',    title: '已取消',          hint: '视为正常结果，不报错' },
    13: { key: 'unsupported',   tone: 'stale',   title: '能力不支持',      hint: '隐藏或置灰，不假装成功' }
  };

  /* 中继侧独立错误空间 R1–R17：必须显式映射，不得原样返回（02 §六.6） */
  VS.RELAY_ERR = {
    R1:  { title: '中继无法连接',        user: '中继暂不可达，已尝试直连' },
    R2:  { title: '中继鉴权失败',        user: '中继会话凭据无效，请重新配对' },
    R3:  { title: '房间不存在',          user: '对端房间已关闭' },
    R4:  { title: '房间已满',            user: '中继房间达到上限，稍后重试' },
    R5:  { title: '转发队列溢出',        user: '中继繁忙，传输已降速' },
    R6:  { title: '帧超限',              user: '传输块超出中继上限，已改用直连' },
    R7:  { title: '配额耗尽',            user: '中继流量配额已用尽' },
    R8:  { title: '速率限制',            user: '中继限速，正在排队' },
    R9:  { title: '协议版本不符',        user: '中继协议版本不兼容，请升级客户端' },
    R10: { title: '指纹不匹配',          user: '中继证书指纹与记录不符' },
    R11: { title: '心跳超时',            user: '与中继的心跳中断' },
    R12: { title: '载荷校验失败',        user: '中继转发帧校验失败，已重传' },
    R13: { title: '未授权设备',          user: '本设备未被该房间授权' },
    R14: { title: '信令格式错误',        user: '信令交换失败，已回退直连' },
    R15: { title: '打洞协助失败',        user: '打洞协助无响应' },
    R16: { title: '中继已停用',          user: '本端已关闭中继回退' },
    R17: { title: '未知中继错误',        user: '中继返回未登记错误，已记录诊断' }
  };

  /* ======================= 4. 能力位（capability_bits）=============== */
  /* 位图未置位 → 前端按「隐藏或置灰」处理，绝不降级为假装成功（§1.2）
   * 位号对齐 docs/v2.0/02 §六.3；位只增不复用。 */
  VS.CAP = {
    CAP_EVENTS:         { bit: 0,  name: '事件流',       tier: 'real' },
    CAP_TASKS:          { bit: 1,  name: '任务队列',     tier: 'real' },
    CAP_NOTIFICATIONS:  { bit: 2,  name: '系统通知',     tier: 'real' },
    CAP_DISCOVERY:      { bit: 3,  name: '设备发现',     tier: 'real' },
    CAP_HOLE_PUNCH:     { bit: 4,  name: 'NAT 打洞',     tier: 'real' },
    CAP_RELAY:          { bit: 5,  name: '中继回退',     tier: 'real' },
    CAP_ROTATION_SESSION:{ bit: 6, name: '轮换三件套',   tier: 'real' },
    CAP_MIGRATION:      { bit: 7,  name: '格式迁移',     tier: 'real' },
    CAP_SELECTIVE_SYNC: { bit: 8,  name: '选择性同步',   tier: 'real' },
    CAP_PQ_HYBRID:      { bit: 11, name: '混合 KEM',     tier: 'real' },
    CAP_BIO:            { bit: 12, name: '生物识别',     tier: 'real' },
    CAP_BURN_SHARE:     { bit: 13, name: '阅后即焚分享', tier: 'real' },
    CAP_STEGO:          { bit: 14, name: '隐写载荷',     tier: 'real' },
    CAP_TX_PADDING:     { bit: 15, name: '流量填充',     tier: 'real' },
    CAP_SECURE_STORE:   { bit: 16, name: '平台安全存储', tier: 'real' },
    CAP_THUMBNAIL:      { bit: 17, name: '缩略图',       tier: 'real' },
    CAP_RECOVERY:       { bit: 18, name: '恢复日志',     tier: 'real' },
    CAP_MOBILE_MANAGED: { bit: 19, name: '移动受管后台', tier: 'real' },
    CAP_LICENSE:        { bit: 20, name: '授权与订单',   tier: 'real' },
    CAP_FULLTEXT:       { bit: 21, name: '全文检索',     tier: 'real' },
    CAP_ERASE_CLASS:    { bit: 22, name: '擦除强度探测', tier: 'real' }
  };

  /* ======================= 5. 状态仓库 ================================ */
  var listeners = [];
  var state = {
    /* 锁状态：Locked / Unlocking / Unlocked / Dummy / Destroyed */
    lock: 'Locked',
    /* 会话：Active / Expired / Revoked / Maintenance */
    session: 'Active',
    /* 只读降级（码 9） */
    readOnly: false,
    readonlyReason: '',
    /* 维护态：None / rotation / migrate / recovery */
    maintenance: null,
    /* 锁屏 */
    unlockAttempts: 0,
    cooldownUntil: 0,
    biometricAvailable: true,
    bioBound: true,
    /* 导航 */
    route: 'lock',
    routeParams: {},
    /* 主题与语言 */
    theme: 'dark',
    locale: 'zh-CN',
    sidebarCollapsed: false,
    bp: 'desktop',
    /* 能力位（默认全部置位；可在设置页逐位开关以验证三档） */
    caps: {},
    /* 保险箱 */
    cwd: 0,
    view: 'grid',
    selection: [],
    sort: { key: 'name', dir: 'asc' },
    filterTags: [],
    searchQuery: '',
    /* 右详情面板（超宽档常驻） */
    detailId: null,
    /* 队列 / 同步 */
    queue: [],
    syncState: 'Idle',
    /* 安全中心 */
    secTab: 'checks',
    stegoEnabled: false,
    /* 维护进度 */
    rotation: null,
    migration: null,
    recovery: null,
    /* 隐写 */
    stego: { ratio: 0.8, disperse: true, shuffle: false, codec: 'png', maxBytes: 4294967296 },
    /* 通知 */
    notifications: [],
    unread: 0,
    /* 命令面板 */
    paletteOpen: false
  };

  /* 默认全部能力位置位 */
  Object.keys(VS.CAP).forEach(function (k) { state.caps[k] = true; });

  var S = VS.store = {
    get: function (k) { return k === undefined ? state : state[k]; },
    set: function (patch, silent) {
      var changed = [];
      Object.keys(patch).forEach(function (k) {
        if (state[k] !== patch[k]) { state[k] = patch[k]; changed.push(k); }
      });
      if (!silent && changed.length) S.emit({ type: 'state', keys: changed });
      return changed;
    },
    /** 深度设置（不改引用比较语义） */
    mutate: function (fn, silent) {
      fn(state);
      if (!silent) S.emit({ type: 'state', keys: ['*'] });
    },
    /* --- 订阅 --- */
    on: function (fn) { listeners.push(fn); return function () { listeners = listeners.filter(function (f) { return f !== fn; }); }; },
    emit: function (ev) {
      listeners.slice().forEach(function (fn) { try { fn(ev); } catch (e) { console.error('[VS] listener error', e); } });
    },
    /* --- 能力位 --- */
    cap: function (name) {
      if (!(name in state.caps)) return true;
      return !!state.caps[name];
    },
    capTier: function (name) {
      /* 三档判定：real（真值）/ design（设计保证）/ hidden（不渲染） */
      if (state.caps[name] === false) return 'hidden';
      if (!(name in state.caps)) return 'design';
      return 'real';
    }
  };

  /* ======================= 6. 状态机推导 ============================= */
  VS.derive = {
    /** 是否处于写冻结（维护态或只读降级） */
    writeFrozen: function () {
      return state.readOnly || !!state.maintenance || state.session === 'Maintenance';
    },
    /** 写操作是否允许：返回 null 表示允许，否则返回错误码 */
    denyWrite: function () {
      if (state.readOnly) return 9;
      if (state.maintenance || state.session === 'Maintenance') return 10;
      if (state.lock !== 'Unlocked') return 5;
      return null;
    },
    /** 只读允许的操作（docs/v2.0/05-02 §三.5 可用性矩阵） */
    readOnlyAllowed: [
      '列表 / 详情 / 文件夹树 / 标签过滤',
      '检索 V2（全文 + 片段）',
      '缩略图生成',
      '导出到外部路径',
      '审计查看 / 校验 / 导出',
      '完整性校验（全库）',
      '隐写提取（只读）'
    ],
    readOnlyDenied: [
      '导入', '原地编辑', '重命名', '打标签', '删除', '擦除',
      '创建分享 / 阅后即焚', '同步出站', '同步入站',
      '发起轮换', '发起迁移'
    ],
    maintenanceDenied: [
      '导入', '原地编辑', '重命名', '打标签', '删除', '擦除',
      '创建分享 / 阅后即焚', '同步', '发起轮换', '发起迁移'
    ],
    maintenanceAllowed: ['导出到外部路径', '隐写提取（只读）', '审计查看', '列表 / 详情']
  };

  /* ======================= 7. 图标库 ================================= */
  /* 24×24 viewBox，描边型；V2.0 需补 pause/play/cancel/discover/erase/recover/migrate/path-relay */
  var P = {
    vault:        '<path d="M4 6a2 2 0 0 1 2-2h12a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2z"/><circle cx="12" cy="12" r="3.2"/><path d="M12 8.8V6.6M12 15.2v2.2"/>',
    devices:      '<rect x="2.5" y="4" width="13" height="10" rx="1.6"/><path d="M6 17h6"/><rect x="16.5" y="9" width="5" height="11" rx="1.4"/>',
    sync:         '<path d="M20 11a8 8 0 0 0-13.7-5.6L4 7.2"/><path d="M4 4.5v3.2h3.2"/><path d="M4 13a8 8 0 0 0 13.7 5.6L20 16.8"/><path d="M20 19.5v-3.2h-3.2"/>',
    security:     '<path d="M12 3l7.5 3v5.5c0 4.6-3 8-7.5 9.5-4.5-1.5-7.5-4.9-7.5-9.5V6z"/><path d="M9.2 12.2l2 2 3.6-4"/>',
    stego:        '<rect x="3" y="4.5" width="18" height="15" rx="2"/><circle cx="8.5" cy="9.5" r="1.5"/><path d="M3.5 17l5-4.5 3.5 3 3-2.5 5.5 4.5"/>',
    settings:     '<circle cx="12" cy="12" r="3"/><path d="M12 2.8v2.4M12 18.8v2.4M21.2 12h-2.4M5.2 12H2.8M18.5 5.5l-1.7 1.7M7.2 16.8l-1.7 1.7M18.5 18.5l-1.7-1.7M7.2 7.2L5.5 5.5"/>',
    search:       '<circle cx="11" cy="11" r="6.5"/><path d="M20 20l-3.6-3.6"/>',
    bell:         '<path d="M18 15V10a6 6 0 1 0-12 0v5l-1.6 2.4h15.2z"/><path d="M10.2 20.2a2 2 0 0 0 3.6 0"/>',
    'bell-off':   '<path d="M18 15V10a6 6 0 0 0-8.4-5.4"/><path d="M6.3 6.9A6 6 0 0 0 6 10v5l-1.6 2.4h12"/><path d="M3 3l18 18"/>',
    lock:         '<rect x="4.5" y="10.5" width="15" height="10" rx="2"/><path d="M8 10.5V7.5a4 4 0 0 1 8 0v3"/>',
    unlock:       '<rect x="4.5" y="10.5" width="15" height="10" rx="2"/><path d="M8 10.5V7.5a4 4 0 0 1 7.5-2"/>',
    plus:         '<path d="M12 5v14M5 12h14"/>',
    import:       '<path d="M12 15V4"/><path d="M8 7.5L12 3.6l4 3.9"/><path d="M4.5 15v3.5a2 2 0 0 0 2 2h11a2 2 0 0 0 2-2V15"/>',
    export:       '<path d="M12 4v11"/><path d="M16 11.5L12 15.4l-4-3.9"/><path d="M4.5 15v3.5a2 2 0 0 0 2 2h11a2 2 0 0 0 2-2V15"/>',
    folder:       '<path d="M3 7.5a2 2 0 0 1 2-2h3.6l2 2.2H19a2 2 0 0 1 2 2v7.8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/>',
    file:         '<path d="M6 3.5h7.2L19 9.2v11.3a1.5 1.5 0 0 1-1.5 1.5h-11A1.5 1.5 0 0 1 5 20.5v-15A1.5 1.5 0 0 1 6.5 3.5z"/><path d="M13.2 3.5v6h5.6"/>',
    'file-image': '<rect x="4" y="5" width="16" height="14" rx="2"/><circle cx="9" cy="10" r="1.6"/><path d="M4.5 16.5l4.5-4 3.2 2.8 2.8-2.3 4.5 4"/>',
    'file-text':  '<path d="M6 3.5h7.2L19 9.2v11.3a1.5 1.5 0 0 1-1.5 1.5h-11A1.5 1.5 0 0 1 5 20.5v-15A1.5 1.5 0 0 1 6.5 3.5z"/><path d="M8.5 13h7M8.5 16.5h5"/>',
    'file-archive':'<path d="M6 3.5h7.2L19 9.2v11.3a1.5 1.5 0 0 1-1.5 1.5h-11A1.5 1.5 0 0 1 5 20.5v-15A1.5 1.5 0 0 1 6.5 3.5z"/><path d="M9.5 3.5v3M11.5 6.5v3M9.5 9.5v3M11.5 12.5v3"/>',
    image:        '<rect x="4" y="5" width="16" height="14" rx="2"/><circle cx="9" cy="10" r="1.6"/><path d="M4.5 16.5l4.5-4 3.2 2.8 2.8-2.3 4.5 4"/>',
    tag:          '<path d="M3.5 11.4V5a1.5 1.5 0 0 1 1.5-1.5h6.4a2 2 0 0 1 1.4.6l7.1 7.1a2 2 0 0 1 0 2.8l-6.4 6.4a2 2 0 0 1-2.8 0L3.9 12.8a2 2 0 0 1-.4-1.4z"/><circle cx="8" cy="8" r="1.3"/>',
    trash:        '<path d="M4.5 6.5h15"/><path d="M9 6.5V4.8a1.3 1.3 0 0 1 1.3-1.3h3.4A1.3 1.3 0 0 1 15 4.8v1.7"/><path d="M6.5 6.5l.9 12.2a1.5 1.5 0 0 0 1.5 1.4h6.2a1.5 1.5 0 0 0 1.5-1.4l.9-12.2"/>',
    erase:        '<path d="M8.6 19.5H19"/><path d="M4.9 15.1l6.6-6.6a1.8 1.8 0 0 1 2.6 0l3.4 3.4a1.8 1.8 0 0 1 0 2.6l-3.9 3.9H7.5z"/>',
    edit:         '<path d="M4.5 19.5h4l10-10a2.1 2.1 0 0 0-3-3l-10 10z"/><path d="M14.5 6.5l3 3"/>',
    share:        '<circle cx="6.5" cy="12" r="2.5"/><circle cx="17.5" cy="6.5" r="2.5"/><circle cx="17.5" cy="17.5" r="2.5"/><path d="M8.7 10.8l6.6-3.1M8.7 13.2l6.6 3.1"/>',
    eye:          '<path d="M2.5 12S6 5.5 12 5.5 21.5 12 21.5 12S18 18.5 12 18.5 2.5 12 2.5 12z"/><circle cx="12" cy="12" r="3"/>',
    'eye-off':    '<path d="M10.6 6.1A9.4 9.4 0 0 1 12 6c6 0 9.5 6 9.5 6a17 17 0 0 1-2.8 3.5"/><path d="M6.4 7.8A16.6 16.6 0 0 0 2.5 12S6 18 12 18a9.6 9.6 0 0 0 4.2-.9"/><path d="M3 3l18 18"/>',
    check:        '<path d="M4.5 12.5l5 5 10-11"/>',
    'check-circle':'<circle cx="12" cy="12" r="8.5"/><path d="M8.3 12.2l2.6 2.6 5-5.4"/>',
    x:            '<path d="M6 6l12 12M18 6L6 18"/>',
    'x-circle':   '<circle cx="12" cy="12" r="8.5"/><path d="M9.2 9.2l5.6 5.6M14.8 9.2l-5.6 5.6"/>',
    'chevron-down':'<path d="M6 9.5l6 6 6-6"/>',
    'chevron-right':'<path d="M9.5 6l6 6-6 6"/>',
    'chevron-left':'<path d="M14.5 6l-6 6 6 6"/>',
    'chevron-up': '<path d="M6 14.5l6-6 6 6"/>',
    'arrow-up':   '<path d="M12 20V4.5M6 10.5L12 4.5l6 6"/>',
    'arrow-down': '<path d="M12 4v15.5M6 13.5L12 19.5l6-6"/>',
    more:         '<circle cx="5.5" cy="12" r="1.4"/><circle cx="12" cy="12" r="1.4"/><circle cx="18.5" cy="12" r="1.4"/>',
    refresh:      '<path d="M20 12a8 8 0 1 1-2.4-5.7"/><path d="M20 4.5V10h-5.4"/>',
    'rotate-ccw': '<path d="M4 12a8 8 0 1 0 2.4-5.7"/><path d="M4 4.5V10h5.4"/>',
    pause:        '<rect x="7" y="5" width="3.4" height="14" rx="1"/><rect x="13.6" y="5" width="3.4" height="14" rx="1"/>',
    play:         '<path d="M7.5 5.2l11 6.8-11 6.8z"/>',
    cancel:       '<circle cx="12" cy="12" r="8.5"/><path d="M8.5 15.5l7-7"/>',
    alert:        '<path d="M12 4.2l8.4 15H3.6z"/><path d="M12 9.6v4.2M12 17.2h.01"/>',
    danger:       '<path d="M8.6 3.5h6.8l5.1 5.1v6.8l-5.1 5.1H8.6l-5.1-5.1V8.6z"/><path d="M12 8v4.6M12 15.8h.01"/>',
    info:         '<circle cx="12" cy="12" r="8.5"/><path d="M12 11v5.2M12 7.9h.01"/>',
    shield:       '<path d="M12 3l7.5 3v5.5c0 4.6-3 8-7.5 9.5-4.5-1.5-7.5-4.9-7.5-9.5V6z"/>',
    cpu:          '<rect x="7" y="7" width="10" height="10" rx="1.6"/><path d="M10 3.5v3M14 3.5v3M10 17.5v3M14 17.5v3M3.5 10h3M3.5 14h3M17.5 10h3M17.5 14h3"/>',
    link:         '<path d="M10.2 13.8a3.4 3.4 0 0 0 4.8 0l2.6-2.6a3.4 3.4 0 0 0-4.8-4.8l-1 1"/><path d="M13.8 10.2a3.4 3.4 0 0 0-4.8 0l-2.6 2.6a3.4 3.4 0 0 0 4.8 4.8l1-1"/>',
    fingerprint:  '<path d="M12 3.5c-3 0-5.4 1.6-6.7 4"/><path d="M4.2 11.4A8 8 0 0 1 12 4.5"/><path d="M8.4 20.2A9 9 0 0 1 6.2 14a5.8 5.8 0 0 1 11.6 0c0 2-.4 4-1.2 5.6"/><path d="M12 14v4.2"/>',
    key:          '<circle cx="8" cy="14" r="3.4"/><path d="M10.6 11.8L19 3.4M16.4 6l2 2M14 8.4l2 2"/>',
    hash:         '<path d="M5 9h14M5 15h14M10 4.5L8.5 19.5M15.5 4.5L14 19.5"/>',
    clock:        '<circle cx="12" cy="12" r="8.5"/><path d="M12 7.5V12l3.2 2"/>',
    calendar:     '<rect x="3.8" y="5.5" width="16.4" height="14" rx="2"/><path d="M3.8 10h16.4M8.5 3.5v4M15.5 3.5v4"/>',
    wifi:         '<path d="M2.8 9.5a13 13 0 0 1 18.4 0"/><path d="M6.2 13a8.6 8.6 0 0 1 11.6 0"/><path d="M9.5 16.4a4.2 4.2 0 0 1 5 0"/><path d="M12 19.8h.01"/>',
    globe:        '<circle cx="12" cy="12" r="8.5"/><path d="M3.5 12h17M12 3.5c2.4 2.5 2.4 14.5 0 17M12 3.5c-2.4 2.5-2.4 14.5 0 17"/>',
    relay:        '<circle cx="12" cy="12" r="2.6"/><path d="M7.8 7.8a6 6 0 0 0 0 8.4M16.2 16.2a6 6 0 0 0 0-8.4"/><path d="M4.9 4.9a10 10 0 0 0 0 14.2M19.1 19.1a10 10 0 0 0 0-14.2"/>',
    discover:     '<circle cx="12" cy="12" r="8.5"/><path d="M12 12l4.2-4.2"/><path d="M12 12l-2.4 5"/><circle cx="12" cy="12" r="1"/>',
    migrate:      '<path d="M3.5 8.5h13M13 5l3.5 3.5L13 12"/><path d="M20.5 15.5h-13M11 12l-3.5 3.5L11 19"/>',
    recover:      '<path d="M4 12a8 8 0 1 0 2.6-5.9"/><path d="M4 4.5V10h5.3"/><path d="M12 8.5v4l2.8 1.8"/>',
    queue:        '<path d="M4 6.5h16M4 12h16M4 17.5h10"/>',
    gantt:        '<path d="M4 6.5h8M7 12h9M4 17.5h6"/>',
    filter:       '<path d="M3.5 5.5h17l-6.4 7.4v5.6l-4.2 2v-7.6z"/>',
    sort:         '<path d="M7 4.5v15M4 16.5l3 3 3-3"/><path d="M14 7h6M14 12h4.5M14 17h3"/>',
    grid:         '<rect x="4" y="4" width="7" height="7" rx="1.4"/><rect x="13" y="4" width="7" height="7" rx="1.4"/><rect x="4" y="13" width="7" height="7" rx="1.4"/><rect x="13" y="13" width="7" height="7" rx="1.4"/>',
    list:         '<path d="M4 7h16M4 12h16M4 17h16"/>',
    columns:      '<rect x="3.5" y="4.5" width="5.5" height="15" rx="1.4"/><rect x="10.5" y="4.5" width="10" height="15" rx="1.4"/>',
    copy:         '<rect x="8.5" y="8.5" width="11.5" height="11.5" rx="1.8"/><path d="M5.5 15.5H4.8A1.8 1.8 0 0 1 3 13.7V4.8A1.8 1.8 0 0 1 4.8 3h8.9a1.8 1.8 0 0 1 1.8 1.8v.7"/>',
    qr:           '<rect x="4" y="4" width="6" height="6" rx="1"/><rect x="14" y="4" width="6" height="6" rx="1"/><rect x="4" y="14" width="6" height="6" rx="1"/><path d="M14 14h2.5v2.5H14zM19.5 14v2.5M14 19.5h2.5M19.5 19.5h.01"/>',
    scan:         '<path d="M4 8.5V6a2 2 0 0 1 2-2h2.5M15.5 4H18a2 2 0 0 1 2 2v2.5M20 15.5V18a2 2 0 0 1-2 2h-2.5M8.5 20H6a2 2 0 0 1-2-2v-2.5"/><path d="M4 12h16"/>',
    card:         '<rect x="3" y="5.5" width="18" height="13" rx="2"/><path d="M3 10h18M6.5 14.5h3"/>',
    receipt:      '<path d="M6 3.5h12v17l-2.4-1.6L13.2 20.5 12 19.3 10.8 20.5 8.4 18.9 6 20.5z"/><path d="M9 8.5h6M9 12h6"/>',
    award:        '<circle cx="12" cy="9.5" r="4.8"/><path d="M8.6 13.6L7 20.5l5-2.6 5 2.6-1.6-6.9"/>',
    user:         '<circle cx="12" cy="8.5" r="3.6"/><path d="M4.8 20c0-3.7 3.2-6 7.2-6s7.2 2.3 7.2 6"/>',
    logout:       '<path d="M15 8.5V6a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2h7a2 2 0 0 0 2-2v-2.5"/><path d="M10.5 12h9.5M17 9l3 3-3 3"/>',
    sun:          '<circle cx="12" cy="12" r="4"/><path d="M12 2.8v2.2M12 19v2.2M4.6 4.6l1.6 1.6M17.8 17.8l1.6 1.6M2.8 12H5M19 12h2.2M4.6 19.4l1.6-1.6M17.8 6.2l1.6-1.6"/>',
    moon:         '<path d="M20 14.5A8.5 8.5 0 0 1 9.5 4a8.5 8.5 0 1 0 10.5 10.5z"/>',
    monitor:      '<rect x="3" y="4.5" width="18" height="12" rx="2"/><path d="M9 20h6M12 16.5V20"/>',
    drive:        '<rect x="3.5" y="6" width="17" height="12" rx="2"/><path d="M3.5 12h17"/><circle cx="8" cy="15" r=".9"/><path d="M7 9h4"/>',
    external:     '<path d="M14 4.5h5.5V10"/><path d="M19.5 4.5L11 13"/><path d="M18 14v4.5a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h4.5"/>',
    help:         '<circle cx="12" cy="12" r="8.5"/><path d="M9.6 9.6A2.5 2.5 0 0 1 14.5 10c0 1.6-2.5 2-2.5 3.4"/><path d="M12 17h.01"/>',
    keyboard:     '<rect x="2.5" y="6" width="19" height="12" rx="2"/><path d="M6 9.5h.01M9.5 9.5h.01M13 9.5h.01M16.5 9.5h.01M8 14h8"/>',
    languages:    '<path d="M3.5 6.5h9M8 4.5v2M10.5 6.5c0 4-3 8-7 8"/><path d="M6 11c1.5 2.5 4 4 6 4"/><path d="M13 19.5l3.5-9 3.5 9M14.3 16.5h4.4"/>',
    gauge:        '<path d="M4 17a8.5 8.5 0 1 1 16 0"/><path d="M12 17l4-5"/>',
    layers:       '<path d="M12 3.5l8.5 4.6L12 12.7 3.5 8.1z"/><path d="M3.5 12.4L12 17l8.5-4.6"/><path d="M3.5 16.4L12 21l8.5-4.6"/>',
    sparkle:      '<path d="M12 3.5l1.9 5 5 1.9-5 1.9L12 17.3l-1.9-5-5-1.9 5-1.9z"/><path d="M18.5 16l.7 1.8 1.8.7-1.8.7-.7 1.8-.7-1.8-1.8-.7 1.8-.7z"/>',
    pin:          '<path d="M9 3.5h6l-.8 5.2 3.3 3.2v1.6H6.5v-1.6l3.3-3.2z"/><path d="M12 13.5V20.5"/>',
    ban:          '<circle cx="12" cy="12" r="8.5"/><path d="M6 18L18 6"/>',
    loader:       '<path d="M12 3.5v3.2M12 17.3v3.2M20.5 12h-3.2M6.7 12H3.5M18.1 5.9l-2.3 2.3M8.2 15.8l-2.3 2.3M18.1 18.1l-2.3-2.3M8.2 8.2L5.9 5.9"/>',
    shieldoff:    '<path d="M12 3l7.5 3v5.5c0 1.5-.3 2.9-.9 4.1"/><path d="M14.7 18.3A9.9 9.9 0 0 1 12 19.5c-4.5-1.5-7.5-4.9-7.5-9.5V6l2.3.9"/><path d="M3 3l18 18"/>',
    save:         '<path d="M5 4.5h10.5L19.5 8.5V19a1.5 1.5 0 0 1-1.5 1.5H5A1.5 1.5 0 0 1 3.5 19V6A1.5 1.5 0 0 1 5 4.5z"/><path d="M8 4.5v5h7v-5M8 20.5V15h8v5.5"/>',
    cart:         '<circle cx="9.5" cy="19" r="1.4"/><circle cx="17" cy="19" r="1.4"/><path d="M3 4.5h2.4l2.3 11.2h10.6l1.7-8H6"/>',
    unlockkey:    '<rect x="3.5" y="10" width="17" height="10" rx="2"/><path d="M8 10V7.5a4 4 0 0 1 7.7-1.5"/><path d="M12 14v2.5"/>',
    history:      '<path d="M4 12a8 8 0 1 0 2.6-5.9"/><path d="M4 4.5V10h5.3"/><path d="M12 8.5v4l3 1.8"/>',
    book:         '<path d="M4 4.5h6a2.5 2.5 0 0 1 2 2.4v13a2 2 0 0 0-2-1.9H4z"/><path d="M20 4.5h-6a2.5 2.5 0 0 0-2 2.4v13a2 2 0 0 1 2-1.9h6z"/>'
  };

  VS.icon = function (name, size, cls) {
    var inner = P[name] || P.file;
    var s = U.svg(inner, size || 18);
    if (cls) s = s.replace('<svg ', '<svg class="' + cls + '" ');
    else s = s.replace('<svg ', '<svg class="ico" ');
    return s;
  };
  /** 返回 DOM 节点形式的图标 */
  VS.iconEl = function (name, size, cls) {
    var wrap = document.createElement('span');
    wrap.innerHTML = VS.icon(name, size, cls);
    return wrap.firstChild;
  };
  VS.iconNames = Object.keys(P);

  /* ======================= 8. 数据格式化小工具（展示层） ============== */
  VS.tone = function (code) {
    var e = VS.ERR[code];
    return e ? e.tone : 'info';
  };
  VS.errTitle = function (code) {
    var e = VS.ERR[code];
    return e ? e.title : '未知错误';
  };

})(window);
