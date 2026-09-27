/* ============================================================================
 * VaultSync V2.0 原型 — Mock 数据（全部为前端假数据，不含任何真实加密逻辑）
 * 语料依据：docs/v2.0 各模块设计文档；字段名对齐文档中的载荷定义
 * ==========================================================================*/
(function (global) {
  'use strict';
  var VS = global.VS || (global.VS = {});
  var rnd = VS.util.seeded(20260915);

  function pick(arr) { return arr[Math.floor(rnd() * arr.length)]; }
  function hash(n) {
    var chars = '0123456789abcdef', s = '';
    for (var i = 0; i < (n || 64); i++) s += chars[Math.floor(rnd() * 16)];
    return s;
  }
  function hexByte() { return ('0' + Math.floor(rnd() * 256).toString(16)).toUpperCase().slice(-2); }
  function fingerprint() {
    var p = []; for (var i = 0; i < 6; i++) p.push(hexByte());
    return p.join(':') + ':…:' + hexByte() + hexByte();
  }
  function fingerprintFull() {
    var p = []; for (var i = 0; i < 32; i++) p.push(hexByte());
    return p.join(':');
  }
  var NOW = Date.now();
  var MIN = 60000, HOUR = 3600000, DAY = 86400000;

  /* ======================================================================
   * 1. 文件夹与文件（vault_list 分页载荷字段：kind/name/size/modifiedMs/tags/rev…）
   * ==================================================================== */
  var FOLDER_NAMES = ['工作', '个人', '归档', '共享', '影音'];
  var FILES = [
    { name: '季度营收报表.xlsx', mime: 'application/vnd.ms-excel', size: 1286400, tags: ['财务', '2026Q3'], type: 'sheet' },
    { name: '项目计划书.docx', mime: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document', size: 486400, tags: ['项目'], type: 'doc' },
    { name: '产品图集-2026.zip', mime: 'application/zip', size: 42991616, tags: ['素材'], type: 'archive' },
    { name: '架构评审纪要.md', mime: 'text/markdown', size: 20480, tags: ['会议', '项目'], type: 'doc' },
    { name: '客户名单.csv', mime: 'text/csv', size: 96256, tags: ['财务', '敏感'], type: 'doc' },
    { name: '现场照片.png', mime: 'image/png', size: 3145728, tags: ['素材'], type: 'image' },
    { name: '密钥轮换说明.pdf', mime: 'application/pdf', size: 819200, tags: ['安全'], type: 'doc' },
    { name: '同步冲突-副本.docx', mime: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document', size: 512000, tags: ['冲突'], type: 'doc', conflict: true },
    { name: '备份归档-2025.tar', mime: 'application/x-tar', size: 268435456, tags: ['归档'], type: 'archive' },
    { name: '白皮书-v2.pdf', mime: 'application/pdf', size: 2457600, tags: ['素材'], type: 'doc' },
    { name: '设备清单.json', mime: 'application/json', size: 40960, tags: ['安全'], type: 'doc' },
    { name: '演示录屏.mp4', mime: 'video/mp4', size: 157286400, tags: ['素材'], type: 'video' },
    { name: '产品原型.html', mime: 'text/html', size: 133120, tags: ['项目'], type: 'doc' },
    { name: '隐写载体-01.png', mime: 'image/png', size: 2097152, tags: ['隐写'], type: 'image' },
    { name: '隐写载体-02.png', mime: 'image/png', size: 1887436, tags: ['隐写'], type: 'image' },
    { name: '隐写载体-03.png', mime: 'image/png', size: 2411724, tags: ['隐写'], type: 'image' }
  ];
  var ERASE_CLASSES = [1, 1, 1, 2, 3, 4];
  var CIPHER = [1, 1, 1, 2];

  var vault = { folders: [], entries: [], nextId: 100 };

  function mkFolder(name, parent, depth) {
    var id = ++vault.nextId;
    vault.folders.push({ id: id, kind: 'folder', name: name, parent: parent, size: 0, createdMs: NOW - (30 + Math.floor(rnd() * 200)) * DAY, modifiedMs: NOW - Math.floor(rnd() * 20) * DAY, tags: [], depth: depth || 0 });
    return id;
  }
  var rootChildren = [mkFolder('工作', 0, 1), mkFolder('个人', 0, 1), mkFolder('归档', 0, 1)];
  mkFolder('2026', rootChildren[0], 2);
  mkFolder('2025', rootChildren[0], 2);
  mkFolder('合同', rootChildren[1], 2);
  mkFolder('发票', rootChildren[1], 2);
  mkFolder('只读镜像', rootChildren[2], 2);

  FILES.forEach(function (f, i) {
    var parent = vault.folders[Math.floor(rnd() * vault.folders.length)].id;
    if (i < 5) parent = vault.folders[1].id;
    if (i >= 13) parent = vault.folders[2].id;
    /* 根目录保留一批条目：原型首屏即为「有内容」状态，避免空壳观感 */
    if (i >= 8 && i <= 12) parent = 0;
    var chunks = Math.max(1, Math.round(f.size / 262144) + Math.floor(rnd() * 4));
    vault.entries.push({
      id: ++vault.nextId,
      kind: 'file',
      name: f.name,
      parent: parent,
      mime: f.mime,
      type: f.type,
      size: f.size,
      createdMs: NOW - (5 + Math.floor(rnd() * 300)) * DAY,
      modifiedMs: NOW - Math.floor(rnd() * 30) * DAY,
      tags: f.tags.slice(),
      rev: 1 + Math.floor(rnd() * 6),
      /* 加密元数据（05-02 §三.1） */
      cipherSuite: f.conflict ? 2 : pick(CIPHER),
      kdfVer: 2,
      chunkCfg: 1,
      formatVer: 3,
      flags: { eraseMeta: true, metaSharded: true, finalized: true },
      metaShardCount: 1 + Math.floor(rnd() * 3),
      metaShardSpan: 4096,
      chunkCount: chunks,
      chunkSize: 262144,
      noncePrefix: hexByte() + hexByte() + hexByte() + hexByte(),
      fileSha256: hash(64),
      hdrCrc32: hash(8),
      fskeyAlgo: 'HKDF-SHA256 "fskey/' + (++vault.nextId) + '"',
      rotateWith: hash(32),
      /* 误删保护（05-03 §六 规则 1：24h） */
      deleteProtectedUntil: f.conflict ? null : null,
      /* 检索（05-02 §四.5） */
      extractVer: 3,
      truncated: f.size > 64 * 1024 * 1024,
      fulltextSupported: /text|json|markdown|csv|html|pdf/.test(f.mime),
      /* 擦除强度（05-02 §三.4） */
      eraseClass: pick(ERASE_CLASSES),
      mediaKind: pick([1, 1, 1, 2, 3, 4]),
      conflict: !!f.conflict,
      /* 缩略图（05-02 §四.6） */
      thumb: f.type === 'image'
    });
  });

  /* 冲突副本对（ConflictPair） */
  var conflictPair = (function () {
    var a = vault.entries.find(function (e) { return e.conflict; });
    var b = JSON.parse(JSON.stringify(a));
    b.id = ++vault.nextId; b.name = '同步冲突-副本 (WorkBook).docx'; b.rev = a.rev + 1;
    b.modifiedMs = a.modifiedMs + HOUR * 3; b.size = a.size + 2048;
    b.device = 'WorkBook';
    a.device = 'Phone-01';
    vault.entries.push(b);
    return { keep: a.id, drop: b.id };
  })();

  /* 已删除 / 待清理条目（05-02 §三.4） */
  var deletedEntries = [
    { id: 9001, name: '旧版合同.docx', size: 128000, deletedMs: NOW - 2 * HOUR, protectUntil: NOW + 22 * HOUR, eraseClass: 1, mediaKind: 1, eraseTsMs: NOW - 2 * HOUR, fsKeyDestroyedMs: NOW - 2 * HOUR, pendingCleanup: false },
    { id: 9002, name: '临时导出.zip', size: 8800000, deletedMs: NOW - 26 * HOUR, protectUntil: null, eraseClass: 4, mediaKind: 4, eraseTsMs: NOW - 26 * HOUR, fsKeyDestroyedMs: NOW - 26 * HOUR, pendingCleanup: true }
  ];

  /* 分享票据（阅后即焚，05-04 burn 通道） */
  var shares = [
    { id: 'shr-7f21', fileId: 104, file: '季度营收报表.xlsx', kind: 'burn', createdMs: NOW - 8 * MIN, expiresMs: NOW + 52 * MIN, status: 'waiting', sessions: 0, maxSessions: 1, device: 'WorkBook', burnedMs: null },
    { id: 'shr-6a90', fileId: 106, file: '客户名单.csv', kind: 'burn', createdMs: NOW - 3 * HOUR, expiresMs: NOW - 2 * HOUR, status: 'burned', sessions: 1, maxSessions: 1, device: 'Phone-01', burnedMs: NOW - 2 * HOUR + 5 * MIN },
    { id: 'shr-5c33', fileId: 110, file: '白皮书-v2.pdf', kind: 'expiring', createdMs: NOW - 1 * DAY, expiresMs: NOW + 2 * DAY, status: 'active', sessions: 2, maxSessions: 5, device: 'MacBook-Air', burnedMs: null }
  ];

  /* ======================================================================
   * 2. 设备 / 发现 / 路径状态机
   * ==================================================================== */
  var devices = [
    { id: 'dev-self', name: 'DESKTOP-7F2A', self: true, fingerprint: fingerprintFull(), port: 51234, path: 'direct', online: true, lastSyncMs: NOW - 2 * MIN, os: 'Windows 11', version: '2.0.0' },
    { id: 'dev-wb', name: 'WorkBook', self: false, fingerprint: fingerprintFull(), port: 51234, path: 'direct', online: true, lastSyncMs: NOW - 2 * MIN, os: 'Windows 11', version: '2.0.0', trust: 'verified', latencyMs: 6 },
    { id: 'dev-ph', name: 'Phone-01', self: false, fingerprint: fingerprintFull(), port: 0, path: 'relay', online: false, lastSyncMs: NOW - 3 * DAY, os: 'Android 15', version: '2.0.0', trust: 'verified', reason: 'relay_unavailable' },
    { id: 'dev-mac', name: 'MacBook-Air', self: false, fingerprint: fingerprintFull(), port: 0, path: 'hole_punch', online: true, lastSyncMs: NOW - 45 * MIN, os: 'macOS 15', version: '2.0.0', trust: 'verified', latencyMs: 84, reason: 'symmetric_nat' }
  ];
  var discovered = [
    { id: 'dis-1', deviceId: null, name: null, fingerprint: 'A3:F1:…:9C', matchCode: null, addrs: [{ kind: 'lan', type: 'tcp' }, { kind: 'lan', type: 'quic' }], path: 'direct', matchCodeExpiresMs: null },
    { id: 'dis-2', deviceId: null, name: null, fingerprint: 'B7:22:…:41', matchCode: null, addrs: [{ kind: 'lan', type: 'tcp' }], path: 'hole_punch', matchCodeExpiresMs: null },
    { id: 'dis-3', deviceId: null, name: null, fingerprint: 'C9:0E:…:77', matchCode: null, addrs: [{ kind: 'lan', type: 'tcp' }], path: 'relay', matchCodeExpiresMs: null }
  ];
  /* 配对五阶段：载体 → 交换 → 双向认证 → 人工核验短码 → 写节点清单 + 审计 */
  var pairCarriers = [
    { value: 'discover', label: '局域网发现', desc: '同网段自动发现，无需手填地址', icon: 'discover' },
    { value: 'invite',   label: '邀请码',     desc: '生成一次性邀请码交给对方输入', icon: 'key' },
    { value: 'qr',       label: '扫码',       desc: '显示二维码，对方扫码加入', icon: 'qr' },
    { value: 'manual',   label: '手动输入地址', desc: '跨网段场景的兜底方式', icon: 'edit' }
  ];

  /* ======================================================================
   * 3. 同步 / 队列 / 冲突（事件 TASK_* 6–10）
   * ==================================================================== */
  var queue = [];
  function mkTask(o) {
    var total = o.totalBytes || 1024 * 1024 * 10;
    var done = o.doneBytes === undefined ? Math.floor(total * (0.05 + rnd() * 0.7)) : o.doneBytes;
    return {
      taskId: 'task-' + (1000 + queue.length),
      name: o.name,
      dir: o.dir || (rnd() > 0.5 ? 'up' : 'down'),
      kind: o.kind || 'sync',
      state: o.state || 'running',
      doneBytes: done,
      totalBytes: total,
      doneChunks: Math.round(done / 262144),
      totalChunks: Math.round(total / 262144),
      rateBps: o.state === 'paused' || o.state === 'failed' ? 0 : Math.floor((0.4 + rnd() * 3) * 1024 * 1024),
      etaMs: o.state === 'running' ? Math.floor(3000 + rnd() * 60000) : null,
      resumable: !!o.resumable,
      resumePct: o.resumePct || null,
      errCode: o.errCode === undefined ? null : o.errCode,
      retries: o.retries || 0,
      peer: o.peer || 'WorkBook',
      path: o.path || 'direct',
      startedMs: NOW - Math.floor(rnd() * 40) * MIN,
      priority: o.priority || 'interactive'
    };
  }
  queue.push(mkTask({ name: 'report_v2.docx', dir: 'down', totalBytes: 3145728, state: 'running' }));
  queue.push(mkTask({ name: 'archive.zip', dir: 'up', totalBytes: 268435456, state: 'running' }));
  queue.push(mkTask({ name: '产品图集-2026.zip', dir: 'up', totalBytes: 42991616, state: 'paused', resumable: true, resumePct: 0.62 }));
  queue.push(mkTask({ name: '演示录屏.mp4', dir: 'down', totalBytes: 157286400, state: 'queued' }));
  queue.push(mkTask({ name: '客户名单.csv', dir: 'down', totalBytes: 96256, state: 'failed', errCode: 3, retries: 2 }));
  queue.push(mkTask({ name: '备份归档-2025.tar', dir: 'up', totalBytes: 268435456, state: 'interrupted', errCode: 9, resumePct: 0.31, resumable: true }));
  queue.push(mkTask({ name: '白皮书-v2.pdf', dir: 'down', totalBytes: 2457600, state: 'done', doneBytes: 2457600 }));
  queue.push(mkTask({ name: '设备清单.json', dir: 'up', totalBytes: 40960, state: 'cancelled', errCode: 12, doneBytes: 0 }));

  var syncPolicy = {
    enabled: true,
    mode: 'auto',                  /* auto | manual | scheduled */
    timeWindowEnabled: true,
    windowStart: '22:00',
    windowEnd: '07:00',
    bandwidthUpKbps: 0,            /* 0 = 不限 */
    bandwidthDownKbps: 2048,
    wifiOnly: false,
    meteredAllowed: true,
    /* 选择性同步：folderIds 与 excludeFolderIds 互斥，同给 → 错误码 7 原子拒绝（CAP_SELECTIVE_SYNC） */
    folderIds: [],
    excludeFolderIds: [],
    minBytes: 0,                   /* 0 = 不限制 */
    maxBytes: 0,
    perPeerModes: { 'dev-wb': 'full', 'dev-ph': 'excludeFolders', 'dev-mac': 'full' },
    excludePatterns: ['node_modules/**', '*.tmp', 'Thumbs.db'],
    maxConcurrent: 3,              /* 默认 3：销毁指令 > 交互 > 后台 */
    conflictStrategy: 'keep_both', /* keep_both | prefer_newer | prefer_local | prefer_remote */
    verifyAfterSync: true,
    retryLimit: 3,
    relayEnabled: true,
    relayFallbackReason: 'symmetric_nat',
    paddingLevel: 2,               /* 0–4，0 = 关闭（CAP_TX_PADDING 未置位时恒为 0） */
    activeHoursOnly: false
  };

  /* ======================================================================
   * 4. 安全中心（六项仪表，docs/v2.0/04 §三.5）
   * ==================================================================== */
  var securityChecks = [
    { key: 'bruteforce', icon: 'shield', name: '暴力破解防护', state: 'ok',       note: '近 24h 0 次失败尝试', src: 'vault_core_diagnostics().bruteforce' },
    { key: 'devices',    icon: 'devices', name: '异常设备',     state: 'ok',       note: '3 台已配对设备指纹全部匹配', src: 'vault_core_p2p_status()' },
    { key: 'integrity',  icon: 'layers', name: '文件完整性',    state: 'unknown',  note: '尚未扫描（上次扫描 12 天前）', src: 'vault_core_verify_all → TASK_*' },
    { key: 'sentinel',   icon: 'cpu',    name: '内存哨兵',      state: 'design',   note: '契约清单中无此接口', src: '（无导出，标为设计保证）' },
    { key: 'audit',      icon: 'link',   name: '审计链',        state: 'ok',       note: '链头校验通过 · 4 812 条', src: 'vault_core_audit_verify()' },
    { key: 'transport',  icon: 'wifi',   name: '传输链路',      state: 'warn',     note: '1 条链路已降级为中继', src: '事件 PATH_DEGRADED(13)' }
  ];

  var AUDIT_OPS = [
    { op: 'vault.import',   cat: 'vault',    label: '导入文件' },
    { op: 'vault.export',   cat: 'vault',    label: '导出文件' },
    { op: 'vault.delete',   cat: 'vault',    label: '删除文件' },
    { op: 'vault.rename',   cat: 'vault',    label: '重命名' },
    { op: 'vault.tag',      cat: 'vault',    label: '编辑标签' },
    { op: 'vault.search',   cat: 'vault',    label: '检索' },
    { op: 'vault.verify',   cat: 'vault',    label: '完整性校验' },
    { op: 'security.unlock',cat: 'security', label: '解锁' },
    { op: 'security.lock',  cat: 'security', label: '锁定' },
    { op: 'security.erase', cat: 'security', label: '安全擦除' },
    { op: 'security.destroy',cat:'security', label: '紧急销毁' },
    { op: 'security.stego', cat: 'security', label: '隐写操作' },
    { op: 'p2p.pair',       cat: 'p2p',      label: '设备配对' },
    { op: 'p2p.unpair',     cat: 'p2p',      label: '解除配对' },
    { op: 'p2p.sync',       cat: 'p2p',      label: '同步' },
    { op: 'p2p.burn',       cat: 'p2p',      label: '阅后即焚' },
    { op: 'rotate.begin',   cat: 'maintain', label: '轮换开始' },
    { op: 'rotate.done',    cat: 'maintain', label: '轮换完成' },
    { op: 'migrate.begin',  cat: 'maintain', label: '迁移开始' },
    { op: 'migrate.file',   cat: 'maintain', label: '迁移单文件' },
    { op: 'index.compact',  cat: 'maintain', label: '索引压实' },
    { op: 'license.activate',cat:'license',  label: '许可证激活' },
    { op: 'settings.change',cat: 'system',   label: '设置变更' }
  ];
  var auditLog = [];
  (function () {
    var chain = hash(64);
    for (var i = 0; i < 46; i++) {
      var op = AUDIT_OPS[Math.floor(rnd() * AUDIT_OPS.length)];
      var prev = chain;
      chain = hash(64);
      auditLog.push({
        seq: 4812 - i,
        tsMs: NOW - i * (7 * MIN + Math.floor(rnd() * 40 * MIN)),
        op: op.op, cat: op.cat, opLabel: op.label,
        level: op.cat === 'security' ? 'security' : (op.cat === 'maintain' ? 'maintain' : 'normal'),
        /* 脱敏 detail（05-02 §七.1）：不含目标路径 / 名称原文 / 查询词 */
        detail: {
          file_id: op.cat === 'vault' ? 'f_' + hash(8) : null,
          bytes: op.cat === 'vault' ? Math.floor(rnd() * 50000000) : null,
          chunks: op.cat === 'vault' ? Math.floor(rnd() * 200) : null,
          cipher_suite: op.cat === 'vault' ? pick([1, 1, 2]) : null,
          erase_class: op.op === 'security.erase' ? pick([1, 2, 3, 4]) : null,
          media_kind: op.op === 'security.erase' ? pick([1, 2, 3, 4]) : null,
          method_bits: op.op === 'security.erase' ? '0b' + pick(['0001', '0011', '0100', '0000']) : null,
          result: rnd() > 0.08 ? 0 : pick([3, 9, 12])
        },
        prevHash: prev, headHash: chain
      });
    }
  })();
  var auditHead = auditLog[0].headHash;
  var auditVerified = true;

  /* 紧急销毁范围 */
  var destroyScopes = [
    { value: 'vault',   label: '仅本机保险箱数据', desc: '删除本机加密数据库与索引；已配对设备不受影响' },
    { value: 'device',  label: '本机 + 已配对设备', desc: '广播签名销毁指令，对端下次上线执行' },
    { value: 'account', label: '全部设备 + 中继凭据', desc: '含撤销中继会话；不可恢复' }
  ];

  /* ======================================================================
   * 5. 维护：轮换 / 迁移 / 恢复
   * ==================================================================== */
  var rotationState = {
    rotationId: 'rot-' + hash(8),
    phase: 'rekey',                  /* mark | rekey | propagate | done */
    progress: { done: 1284, total: 3040 },
    filesDone: 1190, foldersDone: 94,
    interrupted: false,
    startedMs: NOW - 26 * MIN,
    propagation: [
      { device: 'WorkBook',  state: 'confirmed', note: '已确认 ✔' },
      { device: 'Phone-01',  state: 'offline',   note: '离线（待下次上线补推）' },
      { device: 'MacBook-Air', state: 'pending', note: '传播中 68%' }
    ]
  };

  var migrationState = {
    srcVer: 2, dstVer: 3,
    phase: 'verifying',              /* scanning | converting | verifying | done | failed */
    doneFiles: 892, totalFiles: 1014,
    currentFileId: 'f_' + hash(8),
    precheck: [
      { key: 'lease',    label: '独占写租约',       ok: true,  note: '本进程持有 vault.lock' },
      { key: 'maint',    label: '当前非维护态',     ok: true,  note: 'canonical 头部为 VSVB v3' },
      { key: 'space',    label: '磁盘余量 ≥ 库大小 × 1.2', ok: true, note: '需 4.8 GB · 可用 32.1 GB' }
    ],
    files: []
  };
  (function () {
    for (var i = 0; i < 18; i++) {
      var st = i < 14 ? 'done' : (i === 14 ? 'failed' : (i === 15 ? 'skipped' : 'pending'));
      migrationState.files.push({
        fileId: 'f_' + hash(10),
        name: ['季度营收报表.xlsx', '项目计划书.docx', '产品图集-2026.zip', '架构评审纪要.md', '客户名单.csv', '现场照片.png', '密钥轮换说明.pdf', '备份归档-2025.tar'][i % 8] + (i > 7 ? ' (副本' + i + ')' : ''),
        srcVer: 2, dstVer: 3,
        sha256: hash(64),
        status: st,
        tsMs: NOW - (18 - i) * MIN,
        note: st === 'failed' ? '前后 SHA-256 不一致 → 已删除新文件，旧副本仍在' : (st === 'skipped' ? '会话取消时未处理' : '')
      });
    }
  })();

  var recoveryReport = {
    uncommittedChanges: 3,
    segmentStatus: 'torn_append_detected',
    snapshotStatus: 'ok',
    leaseStatus: 'stale_lock_present',
    auditHead: hash(64),
    suggested: 'rollback',           /* rollback | replay */
    detail: [
      { key: 'pos_log',      label: '位置日志',     value: '段 12 尾部不完整（torn append）', ok: false },
      { key: 'lease_file',   label: '租约文件',     value: 'vault.lock 存在但持有进程已退出', ok: false },
      { key: 'snapshot',     label: '快照',         value: 'seq 4 812 可读，CRC 通过', ok: true },
      { key: 'audit_chain',  label: '审计链头',     value: hash(16), ok: true },
      { key: 'crash_loop',   label: '崩溃循环保护', value: '未触发（近 60s 失租约 0 次）', ok: true }
    ]
  };

  /* ======================================================================
   * 6. 隐写术（05-05）
   * ==================================================================== */
  var stegoPlanSample = {
    fileLen: 1258291,
    name: '客户名单.csv',
    perImage: [
      { path: '隐写载体-01.png', width: 1920, height: 1080, capacity: 777596, usable: 622076, shardBytes: 621990 },
      { path: '隐写载体-02.png', width: 1920, height: 1080, capacity: 777596, usable: 622076, shardBytes: 622034 },
      { path: '隐写载体-03.png', width: 1920, height: 1080, capacity: 777596, usable: 622076, shardBytes: 14267, error: null }
    ],
    imagesNeeded: 3, imagesGiven: 3, fits: true, shortfallBytes: 0,
    notes: []
  };
  /* flags 位：bit0 首片 / bit1 单图模式 / bit2 位分散已启用 / bit3 位序置乱已启用
   * 多片载荷下 bit1 必须为 0（否则与 shard_total > 1 语义矛盾）。 */
  var stegoPaths = [
    { shardIndex: 0, shardTotal: 3, fileLen: 1258291, payloadVer: 2, flags: '0b0101', flagsNote: '首片 + 位分散', shardCrc32: hash(8), carrier: '隐写载体-01.png' },
    { shardIndex: 1, shardTotal: 3, fileLen: 1258291, payloadVer: 2, flags: '0b0100', flagsNote: '位分散', shardCrc32: hash(8), carrier: '隐写载体-02.png' },
    { shardIndex: 2, shardTotal: 3, fileLen: 1258291, payloadVer: 2, flags: '0b0100', flagsNote: '位分散（末片，带 file_hdr 与整文件 SHA-256）', shardCrc32: hash(8), carrier: '隐写载体-03.png' }
  ];
  var stegoBoundaries = [
    '隐写不替代保险箱：单图容量有限（1920×1080 精确为 777 596 B，约 760 KB），首选仍是正常加密存储。',
    '不可检测性没有安全保证：只降低统计检测显著性，不承诺「无法被检测」；持原图比对必然可检出。',
    '抗审查请走填充档位 + 正常密文 P2P / 中继流量，而不是寄望于隐写。',
    '双重保护的真实含义：载荷是 AEAD 密文，检出隐写也拿不到内容；载荷密钥为从属密钥（key_id=5），MK 轮换后仍可解。',
    '启用即受审计：开关切换、每次嵌入 / 提取均写 vault-audit（security 类别）；审计失败不阻断业务但发 ERROR_DIAG。',
    'Dart 只传路径：载荷拼接与 AEAD 全在 vault-core，外壳侧不见明文。'
  ];

  /* ======================================================================
   * 7. 授权与订阅（docs/v2.0/12）—— 注意：无商业订单 / 支付 / 试用 / 发票
   * ==================================================================== */
  var licenseState = {
    tier: 'free',                  /* free | pro | team | enterprise */
    state: 'active',               /* active | grace | expired | revoked | offline_grace */
    activatedMs: NOW - 40 * DAY,
    expiresMs: NOW + 325 * DAY,
    seats: 1, seatsUsed: 1,
    relayToken: 'rt_' + hash(24),
    relayTokenState: 'valid',      /* valid | expiring | expired | revoked */
    relayTokenExpiresMs: NOW + 300 * DAY,
    relayQuotaUsedBytes: 4.2 * 1024 * 1024 * 1024,
    relayQuotaTotalBytes: 20 * 1024 * 1024 * 1024,
    exchangeState: 'idle',         /* idle | requesting | awaiting | granted | failed | expired */
    offlineDaysLeft: 27,
    features: {
      feat_multi_device: true, feat_collab: false, feat_relay_quota: true,
      feat_managed_ops: false, feat_burn_share: true, feat_stego: true,
      feat_audit_export: false, feat_sso: false
    }
  };
  /* 跨设备指令信封（不是商业订单） */
  var orderEnvelopes = [
    { id: 'ord-' + hash(6), kind: 1, kindLabel: '销毁指令', createdMs: NOW - 3 * DAY, targets: ['Phone-01'], status: 'delivered', expiresMs: NOW + 4 * DAY },
    { id: 'ord-' + hash(6), kind: 2, kindLabel: '锁定指令', createdMs: NOW - 11 * DAY, targets: ['WorkBook', 'MacBook-Air'], status: 'acked', expiresMs: NOW - 4 * DAY },
    { id: 'ord-' + hash(6), kind: 3, kindLabel: '轮换传播', createdMs: NOW - 26 * MIN, targets: ['WorkBook', 'Phone-01', 'MacBook-Air'], status: 'in_flight', expiresMs: NOW + 30 * DAY }
  ];
  var plans = [
    {
      id: 'free', name: 'Free', priceLabel: '¥0', per: '永久免费', featured: false,
      feats: [
        { t: '端到端加密保险箱', on: true }, { t: '2 台设备配对', on: true },
        { t: '本地 P2P 直连同步', on: true }, { t: '中继回退（1 GB/月）', on: true },
        { t: '阅后即焚分享', on: true }, { t: '隐写术引擎', on: true },
        { t: '审计链 + 加密导出', on: false }, { t: '协作空间与多端托管', on: false }
      ]
    },
    {
      id: 'pro', name: 'Pro', priceLabel: '即将公布', per: '价格可配置占位', featured: true,
      feats: [
        { t: 'Free 全部能力', on: true }, { t: '5 台设备配对', on: true },
        { t: '中继回退（100 GB/月）', on: true }, { t: '审计链加密导出', on: true },
        { t: '全文检索增强', on: true }, { t: '优先中继通道', on: true },
        { t: '协作空间', on: false }, { t: '托管运维', on: false }
      ]
    },
    {
      id: 'team', name: 'Team', priceLabel: '即将公布', per: '按席位', featured: false,
      feats: [
        { t: 'Pro 全部能力', on: true }, { t: '协作空间与共享时效', on: true },
        { t: '席位管理与统一策略', on: true }, { t: '团队审计导出', on: true },
        { t: 'SSO 接入', on: true }, { t: '托管运维', on: false }
      ]
    },
    {
      id: 'enterprise', name: 'Enterprise', priceLabel: '联系商务', per: '含托管运维', featured: false,
      feats: [
        { t: 'Team 全部能力', on: true }, { t: '自建中继与私有部署', on: true },
        { t: '托管运维与 SLA', on: true }, { t: '合规与取证支持', on: true },
        { t: '专属技术支持', on: true }, { t: '定制集成', on: true }
      ]
    }
  ];
  var licenseSteps = ['输入许可证密钥', '校验签名与有效期', '绑定设备指纹', '写入授权记录', '兑换中继凭据', '完成'];

  /* ======================================================================
   * 8. 设置（锁定策略四开关 + 更新通道）
   * ==================================================================== */
  var settings = {
    theme: 'dark',
    locale: 'zh-CN',
    autoLockIdleMs: 15 * MIN,
    lockOnMinimize: true,
    lockOnScreenSaver: true,
    lockOnSleep: true,
    lockOnUsbRemoval: false,
    lockPolicy: { idle: true, minimize: true, screensaver: true, sleep: true },
    startAtLogin: false,
    updateChannel: 'stable',        /* stable | beta | nightly */
    autoCheckUpdate: true,
    telemetry: false,
    screenshotProtect: true,
    disguiseEnabled: true,
    thumbnailsEnabled: true,
    notifications: { info: true, warn: true, danger: true, systemNotify: true, sound: false },
    dndEnabled: false,
    dndStart: '23:00', dndEnd: '07:00',
    syncPolicy: syncPolicy,
    advanced: { workIsolate: true, renderThrottleMs: 33, eventRingBuffer: 1024 }
  };

  /* ======================================================================
   * 9. 通知中心
   * ==================================================================== */
  var notifications = [
    { id: 'n1', tone: 'danger', title: '启动恢复未完成', msg: '检测到 3 处未提交变更，需要你确认回滚或重放', tsMs: NOW - 4 * MIN, read: false, source: 'ENGINE_READY(1).recoveryReport', route: 'recover' },
    { id: 'n2', tone: 'warn',   title: '链路已降级为中继', msg: 'MacBook-Air：直连超时（symmetric_nat）', tsMs: NOW - 12 * MIN, read: false, source: 'PATH_DEGRADED(13)', route: 'devices' },
    { id: 'n3', tone: 'warn',   title: '传输失败', msg: '客户名单.csv · 磁盘不可写（码 3），已重试 2 次', tsMs: NOW - 38 * MIN, read: false, source: 'TASK_FAILED(9)', route: 'queue' },
    { id: 'n4', tone: 'info',   title: '同步完成', msg: '白皮书-v2.pdf 已同步至 WorkBook', tsMs: NOW - 2 * HOUR, read: true, source: 'TASK_DONE(8)', route: 'queue' },
    { id: 'n5', tone: 'info',   title: '设备配对成功', msg: 'MacBook-Air 已加入你的设备清单', tsMs: NOW - 6 * HOUR, read: true, source: 'p2p.pair', route: 'devices' },
    { id: 'n6', tone: 'warn',   title: '订阅将在 30 天后到期', msg: '到期后自动回落 Free 档，同步不会被阻断', tsMs: NOW - 1 * DAY, read: true, source: 'license', route: 'license' }
  ];

  /* ======================================================================
   * 10. 命令面板条目
   * ==================================================================== */
  var commands = [
    { id: 'go-vault',    label: '前往 保险箱',        group: '导航', icon: 'vault',    route: 'vault' },
    { id: 'go-devices',  label: '前往 设备',          group: '导航', icon: 'devices',  route: 'devices' },
    { id: 'go-sync',     label: '前往 同步',          group: '导航', icon: 'sync',     route: 'sync' },
    { id: 'go-queue',    label: '前往 队列',          group: '导航', icon: 'queue',    route: 'queue' },
    { id: 'go-security', label: '前往 安全中心',      group: '导航', icon: 'security', route: 'security' },
    { id: 'go-stego',    label: '前往 隐写术',        group: '导航', icon: 'stego',    route: 'stego' },
    { id: 'go-settings', label: '前往 设置',          group: '导航', icon: 'settings', route: 'settings' },
    { id: 'go-license',  label: '前往 授权与订阅',    group: '导航', icon: 'award',    route: 'license' },
    { id: 'act-import',  label: '导入文件…',          group: '操作', icon: 'import',   action: 'import' },
    { id: 'act-newdir',  label: '新建文件夹…',        group: '操作', icon: 'folder',   action: 'newFolder' },
    { id: 'act-search',  label: '在保险箱中检索…',    group: '操作', icon: 'search',   action: 'search' },
    { id: 'act-lock',    label: '立即锁定',           group: '操作', icon: 'lock',     action: 'lock', shortcut: 'Ctrl+L' },
    { id: 'act-theme',   label: '切换 亮/暗主题',     group: '操作', icon: 'sun',      action: 'theme' },
    { id: 'act-syncnow', label: '立即同步全部设备',   group: '操作', icon: 'sync',     action: 'syncNow' },
    { id: 'act-bio',     label: '绑定 / 管理生物识别',group: '操作', icon: 'fingerprint', action: 'bio' },
    { id: 'act-duty',    label: '打开 值班模式（演示）', group: '演示', icon: 'sparkle', action: 'demo' }
  ];

  /* ======================================================================
   * 导出
   * ==================================================================== */
  VS.data = {
    NOW: NOW,
    MIN: MIN,
    HOUR: HOUR,
    DAY: DAY,
    vault: vault,
    deletedEntries: deletedEntries,
    shares: shares,
    conflictPair: conflictPair,
    devices: devices,
    discovered: discovered,
    pairCarriers: pairCarriers,
    queue: queue,
    syncPolicy: syncPolicy,
    securityChecks: securityChecks,
    auditLog: auditLog,
    auditHead: auditHead,
    auditVerified: auditVerified,
    auditOps: AUDIT_OPS,
    destroyScopes: destroyScopes,
    rotationState: rotationState,
    migrationState: migrationState,
    recoveryReport: recoveryReport,
    stegoPlanSample: stegoPlanSample,
    stegoPaths: stegoPaths,
    stegoBoundaries: stegoBoundaries,
    licenseState: licenseState,
    orderEnvelopes: orderEnvelopes,
    plans: plans,
    licenseSteps: licenseSteps,
    settings: settings,
    notifications: notifications,
    commands: commands,
    folderNames: FOLDER_NAMES,
    fingerprint: fingerprint,
    fingerprintFull: fingerprintFull,
    hash: hash
  };

  /* 便捷查询 */
  VS.data.childFolders = function (parentId) {
    return vault.folders.filter(function (f) { return f.parent === parentId; });
  };
  VS.data.childFiles = function (parentId) {
    return vault.entries.filter(function (e) { return e.parent === parentId && e.kind === 'file'; });
  };
  VS.data.children = function (parentId) {
    return VS.data.childFolders(parentId).concat(VS.data.childFiles(parentId));
  };
  VS.data.entry = function (id) {
    return vault.entries.find(function (e) { return e.id === id; }) || vault.folders.find(function (f) { return f.id === id; }) || null;
  };
  VS.data.allTags = function () {
    var s = {};
    vault.entries.forEach(function (e) { (e.tags || []).forEach(function (t) { s[t] = (s[t] || 0) + 1; }); });
    return Object.keys(s).sort().map(function (t) { return { tag: t, count: s[t] }; });
  };
  VS.data.pathOf = function (id) {
    var path = [], cur = VS.data.entry(id), guard = 0;
    while (cur && guard++ < 20) { path.unshift(cur); cur = cur.parent ? VS.data.entry(cur.parent) : null; }
    return path;
  };
  VS.data.totals = function () {
    var size = 0;
    vault.entries.forEach(function (e) { size += e.size; });
    return { files: vault.entries.length, folders: vault.folders.length, size: size };
  };

})(window);
