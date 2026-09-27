/* ============================================================
   VaultSync 桌面端原型 · Mock 数据与交互
   仅原型演示：无真实加密/同步逻辑
   ============================================================ */
"use strict";

const $ = (s, r = document) => r.querySelector(s);
const $$ = (s, r = document) => [...r.querySelectorAll(s)];
const el = (tag, cls, html) => { const e = document.createElement(tag); if (cls) e.className = cls; if (html != null) e.innerHTML = html; return e; };
const esc = s => String(s).replace(/[&<>"]/g, c => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" }[c]));

/* ---------- 状态 ---------- */
const state = {
  locked: true, dummy: false, page: "vault", view: "grid", folder: "root",
  stegoOn: false, sort: "time", attempts: 0, curFile: null, curDevice: null,
  notifRead: false, devBadge: true, conflicts: 1,
  sel: new Set(), anchor: null, viewOrder: [], wipeTargets: [],
};

/* ---------- 图标映射 ---------- */
const KIND_ICON = { folder: "i-folder", img: "i-img", zip: "i-zip", pdf: "i-pdf", doc: "i-doc", txt: "i-txt", video: "i-video", file: "i-file" };
const KIND_NAME = { img: "图片", zip: "压缩包", pdf: "文档", doc: "文档", txt: "文本", video: "视频", file: "文件" };
const STATUS_BADGE = {
  synced: '<span class="badge ok">已同步</span>',
  pending: '<span class="badge warn">待同步</span>',
  conflict: '<span class="badge danger">冲突</span>',
  review: '<span class="badge info">审阅中</span>',
};

/* ---------- Mock：文件系统 ---------- */
const FS = {
  root: {
    name: "根目录", parent: null,
    items: [
      { id: "work", name: "工作", kind: "folder" },
      { id: "personal", name: "个人", kind: "folder" },
      { id: "photos", name: "相册", kind: "folder" },
      { id: "f1", name: "护照扫描件.png", kind: "img", size: "3.4 MB", time: "09-16 20:41", status: "synced", tags: ["证件", "重要"] },
      { id: "f2", name: "密码备忘.txt", kind: "txt", size: "2 KB", time: "09-16 19:02", status: "pending", tags: ["重要"] },
      { id: "f3", name: "archive_2026.zip", kind: "zip", size: "820 MB", time: "09-15 11:20", status: "synced", tags: [] },
      { id: "f4", name: "design_review.png", kind: "img", size: "6.8 MB", time: "09-14 16:33", status: "review", tags: ["工作"] },
    ],
  },
  work: {
    name: "工作", parent: "root",
    items: [
      { id: "work2026", name: "2026", kind: "folder" },
      { id: "w1", name: "合同草稿.pdf", kind: "pdf", size: "1.2 MB", time: "09-15 10:02", status: "synced", tags: ["工作", "重要"] },
      { id: "w2", name: "offer_letter.pdf", kind: "pdf", size: "420 KB", time: "09-12 09:18", status: "synced", tags: [] },
    ],
  },
  work2026: {
    name: "2026", parent: "work",
    items: [
      { id: "y1", name: "report_v2.docx", kind: "doc", size: "1.2 MB", time: "09-15 10:02", status: "pending", tags: ["工作"] },
      { id: "y2", name: "budget_2026.xlsx", kind: "doc", size: "88 KB", time: "09-15 08:44", status: "synced", tags: [] },
      { id: "y3", name: "minutes_0901.docx", kind: "doc", size: "36 KB", time: "09-01 15:12", status: "synced", tags: [] },
      { id: "y4", name: "photo_day.png", kind: "img", size: "4.1 MB", time: "09-14 08:11", status: "pending", tags: [] },
    ],
  },
  personal: {
    name: "个人", parent: "root",
    items: [
      { id: "p1", name: "简历_2026.pdf", kind: "pdf", size: "512 KB", time: "09-10 21:30", status: "synced", tags: ["重要"] },
      { id: "p2", name: "身份证正反面.png", kind: "img", size: "2.8 MB", time: "09-08 12:00", status: "synced", tags: ["证件"] },
    ],
  },
  photos: {
    name: "相册", parent: "root",
    items: [
      { id: "ph1", name: "photo_day.png", kind: "img", size: "4.1 MB", time: "09-14 08:11", status: "pending", tags: [] },
      { id: "ph2", name: "trip_barcelona.zip", kind: "zip", size: "1.6 GB", time: "09-05 22:47", status: "synced", tags: [] },
      { id: "ph3", name: "wedding.png", kind: "img", size: "8.2 MB", time: "08-30 10:05", status: "review", tags: [] },
    ],
  },
};
const DUMMY_FS = {
  root: { name: "根目录", parent: null, items: [] },
};

/* ---------- Mock：设备 ---------- */
const DEVICES = [
  { id: "local", name: "这台电脑", sub: "DESKTOP-VAULT · Windows 11", icon: "i-pc", online: true, role: "本机 · 主节点", last: "—", chan: "—" },
  { id: "wb", name: "WorkBook-X1", sub: "WorkBook · macOS", icon: "i-drive", online: true, role: "已配对", last: "2 分钟前", chan: "NAT 直连 · Noise 加密" },
  { id: "ph", name: "Phone-01", sub: "Pixel · Android", icon: "i-phone", online: false, role: "已配对", last: "昨天 23:14", chan: "等待上线" },
];
const NEARBY = { id: "lumia", name: "LumiaDevice", sub: "mDNS 局域网发现", icon: "i-pc", online: true, role: "待配对", last: "从未", chan: "未建立" };

/* ---------- Mock：同步 ---------- */
const TRANSFERS = [
  { id: "t1", dir: "up", name: "report_v2.docx", kind: "doc", pct: 45, speed: "2.1 MB/s", meta: "18 / 39 块 · 至 WorkBook-X1", paused: false },
  { id: "t2", dir: "up", name: "archive_2026.zip", kind: "zip", pct: 12, speed: "800 KB/s", meta: "断点续传 · 已传 96 MB / 820 MB", paused: false },
  { id: "t3", dir: "down", name: "photo_day.png", kind: "img", pct: 0, speed: "已排队", meta: "来自 Phone-01 · 等待带宽窗口", paused: true },
];
const CONFLICT_FILE = { name: "document.docx" };

/* ---------- Mock：审计 ---------- */
function hex(n, seed) {
  const v = Math.floor(Math.abs(Math.sin(seed * 127.1 + 311.7) * 4294967296)) % Math.pow(16, n);
  return v.toString(16).toUpperCase().padStart(n, "0");
}
const AUDIT = [
  { t: "09-17 09:41", cat: "会话", act: "解锁成功（主密码）", dev: "本机", ok: true },
  { t: "09-17 09:12", cat: "安全", act: "检测到未授权配对尝试，已拒绝", dev: "未知设备", ok: false },
  { t: "09-16 22:03", cat: "设备", act: "同步完成 · 12 项 · 无冲突", dev: "WorkBook-X1", ok: true },
  { t: "09-16 21:58", cat: "保险箱", act: "导入 photo_day.png（4.1 MB · 64 块）", dev: "本机", ok: true },
  { t: "09-16 21:40", cat: "保险箱", act: "安全擦除 old_key.txt（加密擦除 + TRIM）", dev: "本机", ok: true },
  { t: "09-16 18:22", cat: "设备", act: "配对确认 · 双向认证通过", dev: "WorkBook-X1", ok: true },
  { t: "09-16 18:21", cat: "设备", act: "指纹核验 A9F3…62B1 一致", dev: "WorkBook-X1", ok: true },
  { t: "09-16 14:10", cat: "会话", act: "自动锁定（空闲超时 5 分钟）", dev: "本机", ok: true },
  { t: "09-16 14:02", cat: "保险箱", act: "导出 report_v1.docx → D:\\Export", dev: "本机", ok: true },
  { t: "09-15 10:02", cat: "同步", act: "冲突产生 document.docx（双方均有修改）", dev: "WorkBook-X1", ok: false },
  { t: "09-15 09:47", cat: "安全", act: "暴力破解计数 +1 · 触发 30s 延时", dev: "未知设备", ok: false },
  { t: "09-15 09:30", cat: "会话", act: "解锁失败（主密码错误）", dev: "本机", ok: false },
  { t: "09-15 08:55", cat: "保险箱", act: "全文索引重建 · 42 文档 · 密文落盘", dev: "本机", ok: true },
];
let auditFilter = "全部";
const CATS = ["全部", "保险箱", "设备", "同步", "会话", "安全"];

/* ---------- Mock：通知 ---------- */
const NOTIFS = [
  { ic: "i-warn", tone: "warn", bg: "var(--warn-soft);color:var(--warn)", t: "配对请求", s: "nearby：LumiaDevice 请求配对，待确认", time: "刚刚" },
  { ic: "i-warn", tone: "warn", bg: "var(--danger-soft);color:var(--danger)", t: "入侵检测", s: "1 次未授权设备配对尝试已拒绝", time: "29 分钟前" },
  { ic: "i-sync", tone: "info", bg: "var(--blue-soft);color:var(--blue)", t: "同步完成", s: "12 项已同步至 WorkBook-X1", time: "1 小时前" },
  { ic: "i-warn", tone: "warn", bg: "var(--gold-soft);color:var(--gold)", t: "同步冲突", s: "document.docx 存在两个版本", time: "2 天前" },
];

/* ============================================================
   Toast
   ============================================================ */
function toast(title, sub = "", tone = "info") {
  const ic = { info: "i-info", ok: "i-check", warn: "i-warn", danger: "i-oct" }[tone];
  const t = el("div", `toast ${tone}`, `<svg><use href="#${ic}"/></svg><div><b>${esc(title)}</b>${sub ? `<span class="small">${esc(sub)}</span>` : ""}</div>`);
  $("#toasts").appendChild(t);
  setTimeout(() => { t.classList.add("out"); setTimeout(() => t.remove(), 260); }, 3400);
}

/* ============================================================
   弹窗 / 抽屉
   ============================================================ */
function openModal(id) {
  const ov = $("#" + id);
  $$(".modal", ov).forEach(m => m.classList.remove("on"));
  ov.classList.add("on");
  const m = ov.querySelector(".modal");
  void ov.offsetWidth;
  m.classList.add("on");
}
function closeModal(ov) { ov.classList.remove("on"); }

document.addEventListener("click", e => {
  const c = e.target.closest("[data-close]");
  if (c) {
    const sel = c.getAttribute("data-close");
    if (sel) $(sel).classList.remove("on");
    else c.closest(".overlay")?.classList.remove("on");
  }
  const ov = e.target.closest(".overlay");
  if (ov && !e.target.closest(".modal")) ov.classList.remove("on");
  if (!e.target.closest(".tb-search") && !e.target.closest("#searchDD")) $("#searchDD").classList.remove("on");
  if (!e.target.closest("#btnNotif") && !e.target.closest("#notifDD")) $("#notifDD").classList.remove("on");
  if (!e.target.closest("#ctxMenu")) $("#ctxMenu").classList.remove("on");
});
document.addEventListener("keydown", e => {
  if (e.key === "Escape") {
    if ($("#ctxMenu").classList.contains("on")) { $("#ctxMenu").classList.remove("on"); return; }
    const ov = $$(".overlay.on").pop();
    if (ov) { ov.classList.remove("on"); return; }
    if (state.sel && state.sel.size) { clearSel(); return; }
    $("#searchDD").classList.remove("on");
    $("#notifDD").classList.remove("on");
  }
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") { e.preventDefault(); $("#gSearch").focus(); }
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "a" && !state.locked && state.page === "vault"
      && !e.target.matches("input, textarea, select")) {
    e.preventDefault();
    state.sel = new Set(state.viewOrder);
    applySel();
  }
});
window.addEventListener("resize", () => $("#ctxMenu").classList.remove("on"));
$(".content").addEventListener("scroll", () => $("#ctxMenu").classList.remove("on"), { passive: true });

/* ============================================================
   锁定 / 解锁 / 伪空间
   ============================================================ */
function showScreen(id) { $$(".screen").forEach(s => s.classList.remove("on")); $("#" + id).classList.add("on"); }

function unlockReal() {
  state.locked = false; state.dummy = false;
  document.body.classList.remove("dummy");
  showScreen("scr-app");
  go(state.page);
  renderAll();
  setTimeout(() => { $("#storeBar").style.width = "46%"; }, 250);
  toast("已解锁", "保险箱密钥已解封 · 会话已签发", "ok");
}
function enterDummy() {
  state.locked = false; state.dummy = true;
  state.folder = "root";
  document.body.classList.add("dummy");
  showScreen("scr-app");
  go("vault");
  renderAll();
  toast("已进入保险箱", "当前空间为空", "info");
}
function renderAll() {
  renderVault(); renderDevices(); renderSync(); renderDetect(); renderAudit();
}
function lockAll() {
  const fly = $("#lockfly");
  fly.classList.add("on");
  setTimeout(() => {
    state.locked = true; state.dummy = false;
    document.body.classList.remove("dummy");
    clearSel();
    $$(".overlay.on").forEach(o => o.classList.remove("on"));
    showScreen("scr-lock");
    fly.classList.remove("on");
    const lb = $("#lockBox"); lb.classList.remove("pw-ok", "pw-err");
    $("#lockPw").value = ""; $("#lockErr").textContent = "";
    $("#storeBar").style.width = "0";
    $("#lockPw").focus();
  }, 650);
}

$("#btnUnlock").addEventListener("click", tryUnlock);
$("#lockPw").addEventListener("keydown", e => { if (e.key === "Enter") tryUnlock(); });
$("#lockPw").focus();
let lockTimer = null;
function tryUnlock() {
  if (lockTimer) return;
  const pw = $("#lockPw").value;
  const lb = $("#lockBox");
  if (pw === "1234") {
    lb.classList.add("pw-ok"); $("#lockErr").textContent = "";
    setTimeout(unlockReal, 850);
  } else if (pw === "88888888") {
    lb.classList.add("pw-ok"); $("#lockErr").textContent = "";
    setTimeout(enterDummy, 850);
  } else {
    state.attempts++;
    lb.classList.add("pw-err");
    $("#lockErr").textContent = `主密码错误（第 ${state.attempts} 次）`;
    setTimeout(() => lb.classList.remove("pw-err"), 600);
    if (state.attempts >= 3) {
      let s = 30;
      $("#lockErr").textContent = `尝试过于频繁，已触发暴力破解保护 · ${s}s 后可重试`;
      $("#btnUnlock").disabled = true; $("#lockPw").disabled = true;
      clearInterval(lockTimer);
      lockTimer = setInterval(() => {
        s--;
        $("#lockErr").textContent = `尝试过于频繁，已触发暴力破解保护 · ${s}s 后可重试`;
        if (s <= 0) { clearInterval(lockTimer); lockTimer = null; state.attempts = 0; $("#btnUnlock").disabled = false; $("#lockPw").disabled = false; $("#lockErr").textContent = ""; }
      }, 1000);
    }
  }
}
$("#lockEye").addEventListener("click", () => {
  const inp = $("#lockPw");
  const show = inp.type === "password";
  inp.type = show ? "text" : "password";
  $("#lockEye").innerHTML = `<use href="#${show ? "i-eye-off" : "i-eye"}"/>`;
});
$("#btnBio").addEventListener("click", () => {
  const b = $("#btnBio");
  b.disabled = true; b.innerHTML = '<svg class="ic"><use href="#i-finger"/></svg>等待生物识别…';
  setTimeout(() => {
    b.innerHTML = '<svg class="ic"><use href="#i-check"/></svg>认证通过';
    b.style.borderColor = "var(--ok)"; b.style.color = "var(--ok)";
    setTimeout(() => {
      b.disabled = false; b.style.borderColor = ""; b.style.color = "";
      b.innerHTML = '<svg class="ic"><use href="#i-finger"/></svg>生物识别';
      unlockReal();
    }, 600);
  }, 1300);
});
$("#btnRecover").addEventListener("click", () => { $("#chkRecover").checked = false; $("#btnRecoverGo").disabled = true; openModal("ov-recover"); });
$("#btnManage").addEventListener("click", () => toast("绑定 / 管理", "生物识别绑定请在解锁后前往「设置」", "info"));
$("#btnLockNow").addEventListener("click", lockAll);

/* ============================================================
   导航
   ============================================================ */
$$(".nav-item").forEach(btn => btn.addEventListener("click", () => go(btn.dataset.page)));
function go(page) {
  state.page = page;
  $$(".nav-item").forEach(b => b.classList.toggle("active", b.dataset.page === page));
  $$(".page").forEach(p => p.classList.remove("active"));
  $("#page-" + page).classList.add("active");
  const titles = { vault: "保险箱", devices: "设备", sync: "同步", security: "安全中心", stego: "隐写术", settings: "设置" };
  $("#tbTitle").textContent = titles[page];
  renderCrumbs();
}
function renderCrumbs() {
  const c = $("#tbCrumbs");
  if (state.page !== "vault") {
    const subs = { devices: `${DEVICES.length} 台设备 · 2 台在线`, sync: "3 台设备已连接 · 12 项待同步", security: "整体状态正常 · 上次校验 09-17 09:41", stego: "AES-256-GCM 密文嵌入图片 LSB", settings: "V0.2 原型" };
    c.innerHTML = `<span class="cur">${subs[state.page] || ""}</span>`;
    return;
  }
  const ids = []; let cur = state.dummy ? "root" : state.folder;
  while (cur) { ids.unshift(cur); cur = FS[cur].parent; }
  c.innerHTML = ids.map((id, i) => {
    const last = i === ids.length - 1;
    return (i ? '<svg><use href="#i-chev-r"/></svg>' : "") +
      (last ? `<span class="cur">${esc(FS[id].name)}</span>` : `<button data-nav="${id}">${esc(FS[id].name)}</button>`);
  }).join("");
}
$("#tbCrumbs").addEventListener("click", e => {
  const b = e.target.closest("[data-nav]");
  if (b && !state.dummy) { clearSel(); state.folder = b.dataset.nav; renderVault(); }
});

/* ============================================================
   保险箱
   ============================================================ */
function ficoCls(kind) { return kind === "folder" ? "folder" : (kind === "img" ? "img" : (kind === "zip" ? "zip" : "doc")); }
function sortItems(items) {
  const arr = [...items];
  if (state.sort === "name") arr.sort((a, b) => a.name.localeCompare(b.name, "zh"));
  else if (state.sort === "size") arr.sort((a, b) => (b.size || "").localeCompare(a.size || ""));
  else arr.sort((a, b) => (b.time || "").localeCompare(a.time || ""));
  return arr.sort((a, b) => (a.kind === "folder" ? -1 : 1) - (b.kind === "folder" ? -1 : 1));
}
function thumbHTML(it) {
  if (it.kind === "folder") return `<div class="thumb"><svg style="width:40px;height:40px"><use href="#i-folder"/></svg></div>`;
  const grad = it.kind === "img" ? "img" : it.kind === "zip" ? "zip" : "doc";
  return `<div class="thumb ${grad}"><svg><use href="#${KIND_ICON[it.kind]}"/></svg><span class="lockmark"><svg><use href="#i-lock"/></svg></span></div>`;
}
function cardHTML(it) {
  const st = it.kind === "folder" ? "" : STATUS_BADGE[it.status] || "";
  return `<div class="fcard" data-fid="${it.id}">${thumbHTML(it)}<div class="fname">${esc(it.name)}</div><div class="fmeta"><span>${it.kind === "folder" ? "文件夹" : (it.size || "")}</span>${st}</div></div>`;
}
function rowHTML(it) {
  const st = it.kind === "folder" ? '<span class="badge mute plain">—</span>' : (STATUS_BADGE[it.status] || "");
  return `<tr data-fid="${it.id}"><td><div class="fname-cell"><span class="fico ${ficoCls(it.kind)}"><svg><use href="#${KIND_ICON[it.kind]}"/></svg></span><span><b>${esc(it.name)}</b><span class="small">${(it.tags || []).map(t => "#" + t).join(" ") || (it.kind === "folder" ? "加密文件夹" : KIND_NAME[it.kind])}</span></span></div></td><td class="t2">${it.kind === "folder" ? "文件夹" : KIND_NAME[it.kind]}</td><td class="num t2">${it.kind === "folder" ? "—" : it.size}</td><td class="num t2">${it.time || "—"}</td><td>${st}</td><td><button class="iconbtn" data-more="${it.id}"><svg class="ic"><use href="#i-dots"/></svg></button></td></tr>`;
}
function renderVault() {
  const fs = state.dummy ? DUMMY_FS : FS;
  const folder = fs[state.folder] || fs.root;
  $("#vaultMain").innerHTML = `
    ${state.dummy ? `<div class="dummy-banner"><svg><use href="#i-eye-off"/></svg>伪空间：此保险箱为空，结构与真空间完全一致。</div>` : ""}
    ${folder.items.length === 0 ? emptyVaultHTML() : `<div class="filegrid">${sortItems(folder.items).map(cardHTML).join("")}</div><div class="filetable-wrap" style="display:none;margin-top:16px"><table class="filetable"><thead><tr><th>名称</th><th>类型</th><th>大小</th><th>修改时间</th><th>状态</th><th style="width:56px">操作</th></tr></thead><tbody>${sortItems(folder.items).map(rowHTML).join("")}</tbody></table></div>`}
  `;
  state.viewOrder = sortItems(folder.items).map(i => i.id);
  state.sel = new Set([...state.sel].filter(id => state.viewOrder.includes(id)));
  applyView();
  applySel();
  renderTree();
  renderCrumbs();
}
/* ---------- 资源管理器式选中 ---------- */
function applySel() {
  $$('#vaultMain [data-fid]').forEach(n => n.classList.toggle("selected", state.sel.has(n.dataset.fid)));
  const n = state.sel.size;
  const info = $("#selInfo");
  info.style.display = n ? "" : "none";
  if (n) info.textContent = `已选 ${n} 项`;
}
function clearSel() {
  state.sel.clear(); state.anchor = null;
  applySel();
}
function selectOnly(id) {
  state.sel = new Set([id]); state.anchor = id;
  applySel();
}
function emptyVaultHTML() {
  return `<div class="empty"><div class="eic"><svg><use href="#i-vault"/></svg></div><h3>此保险箱为空</h3><p>导入的文件将自动分块加密，密钥仅存于本机。</p><button class="btn primary" id="btnEmptyImport"><svg class="ic"><use href="#i-up"/></svg>导入文件</button></div>`;
}
function applyView() {
  const pg = $("#page-vault");
  pg.classList.remove("view-grid", "view-list", "view-split");
  pg.classList.add("view-" + state.view);
  const grid = $("#vaultMain .filegrid"), table = $("#vaultMain .filetable-wrap");
  if (grid) grid.style.display = state.view === "list" ? "none" : "";
  if (table) table.style.display = state.view === "list" ? "" : "none";
}
function renderTree() {
  const fs = state.dummy ? DUMMY_FS : FS;
  const q = [
    { sec: "快速访问" },
    { id: "root", name: "全部文件", icon: "i-vault" },
    { name: "最近", icon: "i-history" },
    { sec: "文件夹" },
    ...Object.keys(fs).filter(k => k !== "root" && fs[k].parent === "root").map(k => ({ id: k, name: fs[k].name, icon: "i-folder", sub: true })),
    { sec: "标签" },
  ];
  $("#tree").innerHTML = q.map(x => {
    if (x.sec) return `<div class="t-sec">${x.sec}</div>`;
    return `<button class="t-item ${x.id === state.folder ? "on" : ""}" ${x.id ? `data-nav="${x.id}"` : "data-soon"}><svg><use href="#${x.icon}"/></svg>${esc(x.name)}</button>`;
  }).join("") + `<div style="padding:6px 9px"><span class="tagchip">#工作</span> <span class="tagchip">#重要</span> <span class="tagchip">#证件</span></div>`;
}
$("#tree").addEventListener("click", e => {
  const b = e.target.closest("[data-nav]");
  if (b && !state.dummy) { clearSel(); state.folder = b.dataset.nav; renderVault(); }
  if (e.target.closest("[data-soon]")) toast("原型提示", "「最近」视图使用相同文件列表", "info");
});
$("#viewSeg").addEventListener("click", e => {
  const b = e.target.closest("button"); if (!b) return;
  state.view = b.dataset.view;
  $$("#viewSeg button").forEach(x => x.classList.toggle("on", x === b));
  applyView();
});
$("#selSort").addEventListener("change", e => { state.sort = { "按修改时间": "time", "按名称": "name", "按大小": "size" }[e.target.value]; renderVault(); });
$("#btnImport").addEventListener("click", () => { $("#impQueue").innerHTML = ""; openModal("ov-import"); });
document.addEventListener("click", e => { if (e.target.closest("#btnEmptyImport")) $("#btnImport").click(); });
$("#btnNewFolder").addEventListener("click", () => { $("#inpFolderName").value = ""; openModal("ov-folder"); setTimeout(() => $("#inpFolderName").focus(), 60); });
$("#btnFolderOk").addEventListener("click", () => {
  const name = $("#inpFolderName").value.trim();
  if (!name) { toast("请输入文件夹名称", "", "warn"); return; }
  const fs = state.dummy ? DUMMY_FS : FS;
  const fk = state.dummy ? "root" : state.folder;
  fs[fk].items.push({ id: "nd" + Date.now(), name, kind: "folder" });
  state.folder = fk;
  closeModal($("#ov-folder")); renderVault();
  toast("文件夹已创建", `「${name}」已独立派生子密钥 FSK`, "ok");
});
$("#btnTags").addEventListener("click", () => openTagManager(null));

/* ---------- 文件选中（单击选中 / Ctrl·Shift 多选 / 双击打开 / 右键操作） ---------- */
$("#vaultMain").addEventListener("click", e => {
  const more = e.target.closest("[data-more]");
  if (more) {
    const id = more.dataset.more;
    if (!state.sel.has(id)) selectOnly(id);
    const r = more.getBoundingClientRect();
    openCtxMenu(r.left, r.bottom + 4, id);
    return;
  }
  const node = e.target.closest("[data-fid]");
  if (!node) { clearSel(); return; }
  const id = node.dataset.fid;
  if (e.ctrlKey || e.metaKey) {
    state.sel.has(id) ? state.sel.delete(id) : state.sel.add(id);
    state.anchor = id;
  } else if (e.shiftKey && state.anchor && state.viewOrder.includes(state.anchor)) {
    const a = state.viewOrder.indexOf(state.anchor), b = state.viewOrder.indexOf(id);
    state.sel = new Set(state.viewOrder.slice(Math.min(a, b), Math.max(a, b) + 1));
  } else if (state.sel.has(id) && state.sel.size > 1) {
    /* 保留多选（资源管理器行为） */
  } else {
    selectOnly(id);
    return;
  }
  applySel();
});
$("#vaultMain").addEventListener("dblclick", e => {
  const node = e.target.closest("[data-fid]"); if (!node) return;
  const folder = (state.dummy ? DUMMY_FS : FS)[state.folder];
  const it = folder.items.find(x => x.id === node.dataset.fid); if (!it) return;
  if (it.kind === "folder") { clearSel(); state.folder = it.id; renderVault(); }
  else { selectOnly(it.id); openFileInfo(it); }
});
$("#vaultMain").addEventListener("contextmenu", e => {
  const node = e.target.closest("[data-fid]"); if (!node) return;
  e.preventDefault();
  const id = node.dataset.fid;
  if (!state.sel.has(id)) selectOnly(id);
  openCtxMenu(e.clientX, e.clientY, id);
});

/* ---------- 右键菜单 ---------- */
let ctxActions = [];
function openCtxMenu(x, y, id) {
  const it = findFile(id); if (!it) return;
  const multi = state.sel.has(id) && state.sel.size > 1;
  ctxActions = [];
  const items = [];
  const add = (label, ic, fn, cls = "") => { items.push({ label, ic, cls }); ctxActions.push(fn); };
  if (!multi) {
    add("打开", it.kind === "folder" ? "i-folder" : "i-file", () => ctxOpen(it));
    if (it.kind !== "folder") {
      add("预览", "i-eye", () => { closeModal($("#ov-fileinfo")); openPreview(it); });
      items.push({ sep: true });
      add("导出", "i-down", () => { closeModal($("#ov-fileinfo")); openExport(it); });
      add("阅后即焚分享", "i-flame", () => { closeModal($("#ov-fileinfo")); openBurn(it); });
      if (state.stegoOn) add("隐写嵌入", "i-stego", () => { closeModal($("#ov-fileinfo")); openEmbed(it); });
      items.push({ sep: true });
    }
    add("重命名", "i-edit", () => { closeModal($("#ov-fileinfo")); openRename(it); });
    add("标签", "i-tag", () => openTagManager(it));
  } else {
    add(`导出 ${state.sel.size} 个文件`, "i-down", exportMulti);
  }
  items.push({ sep: true });
  add(multi ? `安全擦除 ${state.sel.size} 个文件` : "安全擦除", "i-trash",
    () => { closeModal($("#ov-fileinfo")); openWipe(multi ? [...state.sel].map(findFile).filter(Boolean) : [it]); }, "danger");
  const menu = $("#ctxMenu");
  menu.innerHTML =
    (multi ? `<div class="ctx-head">已选 ${state.sel.size} 项</div>` : `<div class="ctx-head">${esc(it.name)}</div>`) +
    items.map((it2, i) => it2.sep ? `<div class="ctx-sep"></div>`
      : `<button data-ctx="${i}" class="${it2.cls || ""}"><svg><use href="#${it2.ic}"/></svg>${esc(it2.label)}</button>`).join("");
  menu.classList.add("on");
  const mw = menu.offsetWidth, mh = menu.offsetHeight;
  menu.style.left = Math.min(x, window.innerWidth - mw - 8) + "px";
  menu.style.top = Math.min(y, window.innerHeight - mh - 8) + "px";
}
$("#ctxMenu").addEventListener("click", e => {
  const b = e.target.closest("[data-ctx]"); if (!b) return;
  $("#ctxMenu").classList.remove("on");
  const fn = ctxActions[+b.dataset.ctx];
  if (fn) fn();
});
function ctxOpen(it) {
  if (it.kind === "folder") { clearSel(); state.folder = it.id; renderVault(); }
  else openFileInfo(it);
}
function exportMulti() {
  const n = state.sel.size;
  toast("已批量导出", `${n} 个文件已解密输出 → D:\\Export`, "ok");
  addAudit("保险箱", `批量导出 ${n} 个文件 → D:\\Export`, "本机", true);
  clearSel();
}
function findFile(id) {
  for (const k of Object.keys(FS)) { const it = FS[k].items.find(x => x.id === id); if (it) return it; }
  for (const k of Object.keys(DUMMY_FS)) { const it = DUMMY_FS[k].items.find(x => x.id === id); if (it) return it; }
  return null;
}
function openFileInfo(it) {
  state.curFile = it;
  $("#dfName").textContent = it.name;
  $("#dfSub").textContent = it.kind === "folder" ? "加密文件夹" : `${it.size} · ${KIND_NAME[it.kind]}`;
  const mic = $("#dfMic");
  mic.className = "mic " + (it.kind === "img" ? "enc" : it.kind === "zip" ? "gold" : "blue");
  mic.innerHTML = `<svg><use href="#${KIND_ICON[it.kind] || "i-file"}"/></svg>`;
  $("#dfPreview").style.display = it.kind === "folder" ? "none" : "";
  $$("#m-fileinfo [data-fileonly]").forEach(b => b.style.display = it.kind === "folder" ? "none" : "");
  $("#dfKv").innerHTML = `
    <dt>类型</dt><dd>${it.kind === "folder" ? "加密文件夹" : KIND_NAME[it.kind]}</dd>
    <dt>大小</dt><dd class="num">${it.kind === "folder" ? "—" : it.size}</dd>
    <dt>修改时间</dt><dd class="num">${it.time || "—"}</dd>
    <dt>同步状态</dt><dd>${it.kind === "folder" ? "随内容同步" : (STATUS_BADGE[it.status] || "—")}</dd>
    <dt>所在位置</dt><dd>${(state.dummy ? DUMMY_FS : FS)[state.folder].name}</dd>`;
  $("#dfTags").innerHTML = (it.tags || []).map(t => `<span class="tagchip">#${esc(t)}</span>`).join("") || '<span class="small t3">暂无标签</span>';
  const nb = hex(4, it.id.length + 7).match(/.{2}/g).join(":");
  $("#dfEnc").innerHTML = `
    <dt>算法</dt><dd>AES-256-GCM · FastCDC 64KB</dd>
    <dt>文件密钥</dt><dd class="mono">FSKey · 随机生成</dd>
    <dt>nonce 前缀</dt><dd class="mono">${nb}</dd>
    <dt>块数</dt><dd class="num">${it.kind === "folder" ? "—" : Math.max(1, Math.round(parseFloat(it.size) * (it.size.includes("MB") ? 16 : it.size.includes("GB") ? 16384 : 1) / 1) )} 块（约）</dd>
    <dt>SHA-256</dt><dd class="mono">${hex(12, it.name.length)}…</dd>`;
  openModal("ov-fileinfo");
}

/* ---------- 文件操作（弹窗按钮与右键菜单共用） ---------- */
function openPreview(it) { $("#pvTitle").textContent = "预览 · " + it.name; $("#pvSub").textContent = `${it.size} · 解锁会话内解密`; openModal("ov-preview"); }
function openExport(it) { state.curFile = it; $("#exKv").innerHTML = `<dt>文件</dt><dd>${esc(it.name)}</dd><dt>大小</dt><dd class="num">${it.size}</dd>`; openModal("ov-export"); }
function openRename(it) { state.curFile = it; $("#inpRename").value = it.name; openModal("ov-rename"); }
function openBurn(it) {
  state.curFile = it;
  $("#burnKv").innerHTML = `<dt>文件</dt><dd>${esc(it.name)}</dd><dt>大小</dt><dd class="num">${it.size}</dd>`;
  $("#burnPane1").classList.add("on"); $("#burnPane2").classList.remove("on");
  $("#burnFoot").style.display = "";
  ["#bl1", "#bl2", "#bl3", "#bl4"].forEach(s => $(s).classList.remove("on"));
  openModal("ov-burn");
}
document.addEventListener("click", e => {
  const b = e.target.closest("[data-act]"); if (!b || !state.curFile) return;
  const it = state.curFile;
  closeModal($("#ov-fileinfo"));
  if (b.dataset.act === "preview") openPreview(it);
  if (b.dataset.act === "export") openExport(it);
  if (b.dataset.act === "burn") openBurn(it);
  if (b.dataset.act === "embed") openEmbed(it);
  if (b.dataset.act === "rename") openRename(it);
  if (b.dataset.act === "tag") openTagManager(it);
  if (b.dataset.act === "wipe") openWipe([it]);
});
$("#btnRenameOk").addEventListener("click", () => {
  const v = $("#inpRename").value.trim();
  if (v && state.curFile) { state.curFile.name = v; renderVault(); }
  closeModal($("#ov-rename")); toast("已重命名", "加密元数据已更新", "ok");
});
$("#btnExportOk").addEventListener("click", () => {
  closeModal($("#ov-export"));
  toast("已导出", `${state.curFile.name} → D:\\Export · SHA-256 终验通过`, "ok");
  addAudit("保险箱", `导出 ${state.curFile.name} → D:\\Export`, "本机", true);
});
$("#btnPvExport").addEventListener("click", () => { closeModal($("#ov-preview")); toast("已导出", "解密输出完成", "ok"); });

/* ---------- 搜索 ---------- */
$("#gSearch").addEventListener("input", e => {
  const q = e.target.value.trim().toLowerCase();
  const dd = $("#searchDD");
  if (!q) { dd.classList.remove("on"); return; }
  const all = [];
  const corpus = state.dummy ? DUMMY_FS : FS;
  for (const k of Object.keys(corpus)) corpus[k].items.forEach(it => { if (it.kind !== "folder") all.push({ ...it, loc: corpus[k].name }); });
  const nameHits = all.filter(f => f.name.toLowerCase().includes(q));
  const tagHits = all.filter(f => (f.tags || []).some(t => t.toLowerCase().includes(q)) && !nameHits.includes(f));
  const fullHits = all.filter(f => !nameHits.includes(f) && !tagHits.includes(f) && (f.name.length + f.size).toLowerCase().includes(q)).slice(0, 2);
  const mk = s => esc(s).replace(new RegExp(q.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"), "gi"), m => `<mark>${m}</mark>`);
  const item = f => `<div class="dd-item" data-open="${f.id}"><span class="fico ${ficoCls(f.kind)}"><svg><use href="#${KIND_ICON[f.kind]}"/></svg></span><span>${mk(f.name)}</span><span class="sub">${f.loc} · ${f.size}</span></div>`;
  let html = "";
  if (nameHits.length) html += `<div class="dd-sec">文件名</div>` + nameHits.map(item).join("");
  if (tagHits.length) html += `<div class="dd-sec">标签</div>` + tagHits.map(item).join("");
  if (fullHits.length) html += `<div class="dd-sec">全文命中（脱敏片段）</div>` + fullHits.map(f => `<div class="dd-item" data-open="${f.id}"><span class="fico doc"><svg><use href="#i-doc"/></svg></span><span>…${mk(f.name.slice(0, 3))}… <span class="t3">命中 ${1 + f.name.length % 4} 处</span></span><span class="sub">${f.loc}</span></div>`).join("");
  dd.innerHTML = html || `<div class="dd-empty">未找到与「${esc(q)}」匹配的内容</div>`;
  dd.classList.add("on");
});
$("#searchDD").addEventListener("click", e => {
  const it = e.target.closest("[data-open]"); if (!it) return;
  $("#searchDD").classList.remove("on"); $("#gSearch").value = "";
  const f = findFile(it.dataset.open);
  if (f) {
    const corpus = state.dummy ? DUMMY_FS : FS;
    state.folder = Object.keys(corpus).find(k => corpus[k].items.includes(f)) || "root";
    state.sel = new Set([f.id]); state.anchor = f.id;
    renderVault();
    openFileInfo(f);
  }
});

/* ---------- 标签 ---------- */
const TAG_POOL = ["工作", "重要", "证件", "待处理"];
let tagTarget = null;
function openTagManager(file) {
  tagTarget = file;
  $("#tagSub").textContent = file ? `管理「${file.name}」的标签` : "管理全局标签";
  $("#tagAll").innerHTML = TAG_POOL.map((t, i) => {
    const on = file ? file.tags.includes(t) : true;
    return `<span class="tagchip" data-tag="${t}" style="${on ? "" : "opacity:.4"}">#${esc(t)} ${on ? "×" : "+"}</span>`;
  }).join("");
  openModal("ov-tag");
}
$("#tagAll").addEventListener("click", e => {
  const c = e.target.closest("[data-tag]"); if (!c || !tagTarget) return;
  const t = c.dataset.tag;
  if (tagTarget.tags.includes(t)) tagTarget.tags = tagTarget.tags.filter(x => x !== t);
  else tagTarget.tags.push(t);
  openTagManager(tagTarget); renderVault();
});
$("#btnTagAdd").addEventListener("click", () => {
  const v = $("#inpTag").value.trim(); if (!v) return;
  TAG_POOL.push(v);
  if (tagTarget) tagTarget.tags.push(v);
  $("#inpTag").value = ""; openTagManager(tagTarget); renderVault();
  toast("标签已添加", "#" + v, "ok");
});

/* ---------- 导入 ---------- */
const DEMO_IMPORTS = [
  { n: "passport_new.pdf", s: "2.1 MB", k: "pdf" },
  { n: "screen_shot_0917.png", s: "1.8 MB", k: "img" },
];
$("#btnImpStart").addEventListener("click", () => {
  const q = $("#impQueue");
  DEMO_IMPORTS.forEach((f, idx) => {
    if ($(`[data-imp="${f.n}"]`)) return;
    const row = el("div", "import-item", `<span class="fico"><svg><use href="#${KIND_ICON[f.k]}"/></svg></span><div class="fi-main"><b>${f.n}</b><span class="small">${f.s} · 正在加密…</span><div class="pbar"><i></i></div></div><span class="small t3 pct num">0%</span>`);
    row.dataset.imp = f.n; q.appendChild(row);
    let p = 0;
    const iv = setInterval(() => {
      p += 7 + idx * 3;
      if (p >= 100) {
        p = 100; clearInterval(iv);
        row.querySelector(".small").textContent = `${f.s} · 已加密入箱`;
        row.querySelector(".pbar i").style.background = "var(--ok)";
        if (idx === DEMO_IMPORTS.length - 1) {
          const fs = state.dummy ? DUMMY_FS : FS;
          const fk = state.dummy ? "root" : state.folder;
          DEMO_IMPORTS.forEach(d => fs[fk].items.push({
            id: "im" + d.n, name: d.n, kind: d.k, size: d.s,
            time: "09-17 " + new Date().toTimeString().slice(0, 5), status: "pending", tags: [],
          }));
          state.folder = fk;
          renderVault();
          toast("导入完成", "2 个文件已加密入箱 · 已记入审计", "ok");
          addAudit("保险箱", `导入 2 个文件（${DEMO_IMPORTS.map(d => d.n).join("、")}）`, "本机", true);
          setTimeout(() => closeModal($("#ov-import")), 900);
        }
      }
      row.querySelector(".pbar i").style.width = p + "%";
      row.querySelector(".pct").textContent = p + "%";
    }, 160 + idx * 40);
  });
});
$("#dz").addEventListener("click", () => $("#btnImpStart").click());

/* ---------- 安全擦除（单文件 / 多选） ---------- */
function openWipe(targets) {
  if (!targets || !targets.length) return;
  state.wipeTargets = targets;
  const multi = targets.length > 1;
  $("#wfPhraseLabel").textContent = multi ? `输入「安全擦除」以确认（${targets.length} 个项目）` : "输入文件名以确认";
  $("#inpWfPhrase").placeholder = multi ? "安全擦除" : "输入完整文件名";
  $("#wfKv").innerHTML = multi
    ? `<dt>目标</dt><dd>${targets.length} 个文件 / 文件夹</dd><dt>示例</dt><dd>${esc(targets.slice(0, 3).map(t => t.name).join("、"))}${targets.length > 3 ? " …" : ""}</dd>`
    : `<dt>目标文件</dt><dd>${esc(targets[0].name)}</dd><dt>大小</dt><dd class="num">${targets[0].size || "—"}</dd>`;
  $("#inpWfPhrase").value = ""; $("#btnWipeFileOk").disabled = true;
  openModal("ov-wipefile");
}
$("#inpWfPhrase").addEventListener("input", e => {
  const expected = state.wipeTargets.length > 1 ? "安全擦除" : (state.wipeTargets[0]?.name || "@@");
  $("#btnWipeFileOk").disabled = e.target.value.trim() !== expected;
});
$("#btnWipeFileOk").addEventListener("click", () => {
  const targets = state.wipeTargets || [];
  if (!targets.length) return;
  const ids = new Set(targets.map(t => t.id));
  closeModal($("#ov-wipefile"));
  let p = 0;
  toast("正在安全擦除…", "销毁 FSKey · 摘除块清单", "warn");
  const iv = setInterval(() => {
    p += 20;
    if (p >= 100) {
      clearInterval(iv);
      for (const k of Object.keys(FS)) FS[k].items = FS[k].items.filter(x => !ids.has(x.id));
      for (const k of Object.keys(DUMMY_FS)) DUMMY_FS[k].items = DUMMY_FS[k].items.filter(x => !ids.has(x.id));
      clearSel();
      renderVault();
      if (targets.length > 1) {
        toast("已安全擦除", `${targets.length} 个项目 · 已向 2 台设备传播删除`, "danger");
        addAudit("保险箱", `批量安全擦除 ${targets.length} 个项目（加密擦除 + 介质兜底）`, "本机", true);
      } else {
        toast("已安全擦除", `${targets[0].name} · 已向 2 台设备传播删除`, "danger");
        addAudit("保险箱", `安全擦除 ${targets[0].name}（加密擦除 + 介质兜底）`, "本机", true);
      }
    }
  }, 220);
});
$("#btnWipeFileOk").disabled = true;

/* ============================================================
   设备
   ============================================================ */
function renderDevices() {
  const wrap = $("#devList");
  const list = state.dummy ? DEVICES.slice(0, 1) : [...DEVICES, ...(state.devBadge ? [NEARBY] : [])];
  wrap.innerHTML = list.map(d => `
    <div class="dev-row" data-dev="${d.id}">
      <div class="dev-ava"><svg><use href="#${d.icon}"/></svg><span class="dot ${d.id === "lumia" ? "nearby" : d.online ? "on" : "off"}"></span></div>
      <div class="dv-main"><b>${esc(d.name)} ${d.id === "local" ? '<span class="badge gold plain">本机</span>' : ""}</b><span class="small">${esc(d.sub)}</span></div>
      <div class="dv-side">
        ${d.id === "lumia" ? '<button class="btn sm gold" data-pair-accept>配对</button>' : `<span class="badge ${d.online ? "ok" : "mute"}">${d.online ? "在线" : "离线"}</span>`}
        <div class="small" style="margin-top:4px">${d.role} · ${d.id === "lumia" ? "请求配对" : "上次同步 " + d.last}</div>
      </div>
    </div>`).join("");
  $("#pairBannerWrap").innerHTML = (!state.dummy && state.devBadge) ? `
    <div class="pair-banner"><svg><use href="#i-warn"/></svg><span><b>配对请求</b> · 附近设备 LumiaDevice 请求与保险箱配对</span>
    <button class="btn sm" data-pair-accept>查看配对</button></div>` : "";
  $("#navDevBadge").style.display = state.devBadge && !state.dummy ? "" : "none";
  $("#devSub").textContent = state.dummy ? "伪空间中无法访问已配对设备" : `${list.length - 1} 台已配对设备 · ${list.filter(d => d.online).length - 1} 台在线`;
}
$("#pairBannerWrap").addEventListener("click", e => {
  if (e.target.closest("[data-pair-accept]")) openPair(2);
});
$("#devList").addEventListener("click", e => {
  if (e.target.closest("[data-pair-accept]")) { openPair(2); return; }
  const row = e.target.closest("[data-dev]"); if (!row) return;
  openDeviceInfo(row.dataset.dev);
});
function openDeviceInfo(id) {
  const d = [...DEVICES, NEARBY].find(x => x.id === id); if (!d || d.id === "lumia") return;
  state.curDevice = id;
  $("#dvName").textContent = d.name;
  $("#dvKv").innerHTML = `
    <dt>状态</dt><dd><span class="badge ${d.online ? "ok" : "mute"}">${d.online ? "在线" : "离线"}</span></dd>
    <dt>角色</dt><dd>${d.role}</dd>
    <dt>平台</dt><dd>${d.sub}</dd>
    <dt>上次同步</dt><dd class="num">${d.last}</dd>
    <dt>公钥指纹</dt><dd class="mono">${id === "local" ? "A9F3 0C5D 7B62 E4B1" : hex(8, id.length * 31).match(/.{4}/g).join(" ")}</dd>`;
  $("#dvChan").innerHTML = `
    <dt>路径</dt><dd>${d.chan}</dd>
    <dt>信令</dt><dd>HMAC-SHA256 + 单调序号（防重放）</dd>
    <dt>待执行指令</dt><dd>无</dd>`;
  openModal("ov-devinfo");
}
$("#btnDevSync").addEventListener("click", () => { closeModal($("#ov-devinfo")); go("sync"); toast("已发起同步", "增量比对中…", "info"); });
$("#btnDevUnpair").addEventListener("click", () => { const d = DEVICES.find(x => x.id === state.curDevice); $("#upName").textContent = `将解除与「${d ? d.name : ""}」的配对`; closeModal($("#ov-devinfo")); openModal("ov-unpair"); });
$("#btnUnpairOk").addEventListener("click", () => {
  const i = DEVICES.findIndex(x => x.id === state.curDevice);
  if (i > 0) { const [rm] = DEVICES.splice(i, 1); toast("已解除配对", rm.name + " 已从节点清单移除", "warn"); addAudit("设备", `解除配对 ${rm.name}`, "本机", true); }
  closeModal($("#ov-unpair")); renderDevices();
});
$("#btnDevRemoteWipe").addEventListener("click", () => {
  const d = DEVICES.find(x => x.id === state.curDevice);
  $("#rwName").textContent = `目标：${d ? d.name : ""}${d && !d.online ? "（当前离线 · 指令进入高优先级队列，上线后优先执行）" : ""}`;
  $("#inpRwPhrase").value = ""; $("#btnRwOk").disabled = true;
  closeModal($("#ov-devinfo"));
  openModal("ov-rwipe");
});
$("#inpRwPhrase").addEventListener("input", e => $("#btnRwOk").disabled = e.target.value.trim() !== "销毁");
$("#btnRwOk").addEventListener("click", () => { closeModal($("#ov-rwipe")); runWipeFly(`正在远程销毁 ${DEVICES.find(x => x.id === state.curDevice)?.name || "目标设备"}`, () => { toast("销毁指令已签发", "由本机设备密钥签名 · 对端上线即执行", "danger"); addAudit("安全", `签发远程销毁指令 → ${DEVICES.find(x => x.id === state.curDevice)?.name}`, "本机", false); }); });
$("#btnManualAdd").addEventListener("click", () => openPair(1, "manual"));
$("#btnPair").addEventListener("click", () => openPair(1));

/* ---------- 配对向导 ---------- */
let pairStep = 1;
function drawQR(box) {
  box.innerHTML = "";
  const N = 25, cells = [];
  let seed = 20260917;
  const rnd = () => (seed = (seed * 1103515245 + 12345) % 2147483648) / 2147483648;
  const finder = (r, c) => (r < 7 && c < 7) || (r < 7 && c >= N - 7) || (r >= N - 7 && c < 7);
  const finderDark = (r, c) => {
    const lr = r < 7 ? r : r - (N - 7), lc = c < 7 ? c : c - (N - 7);
    return lr === 0 || lr === 6 || lc === 0 || lc === 6 || (lr >= 2 && lr <= 4 && lc >= 2 && lc <= 4);
  };
  for (let r = 0; r < N; r++) for (let c = 0; c < N; c++) {
    const dark = finder(r, c) ? finderDark(r, c) : rnd() > 0.52;
    cells.push(`<i class="${dark ? "" : "o"}"></i>`);
  }
  box.innerHTML = cells.join("");
}
function openPair(step = 1, tab = "qr") {
  pairStep = step;
  openModal("ov-pair");
  switchPairTab(tab);
  drawQR($("#qrBox"));
  $("#qrPayload").textContent = "vs-pk:" + hex(10, 42).toLowerCase() + "…";
  $("#inviteCode").value = hex(4, 7).match(/.{2}/g).join("-") + "-" + hex(4, 13).match(/.{2}/g).join("-");
  $("#fprA").textContent = "A9 F3 0C 5D\n7B 62 E4 B1".replace("\n", " ");
  $("#fprB").textContent = "A9 F3 0C 5D\n7B 62 E4 B1".replace("\n", " ");
  applyPairStep();
}
function switchPairTab(t) {
  $$("#pairTabs button").forEach(b => b.classList.toggle("on", b.dataset.pt === t));
  $("#paneQr").classList.toggle("on", t === "qr");
  $("#paneInvite").classList.toggle("on", t === "invite");
  $("#paneManual").classList.toggle("on", t === "manual");
}
$("#pairTabs").addEventListener("click", e => { const b = e.target.closest("button"); if (b) switchPairTab(b.dataset.pt); });
function applyPairStep() {
  [1, 2, 3].forEach(i => {
    const s = $("#ps" + i);
    s.classList.toggle("cur", i === pairStep);
    s.classList.toggle("done", i < pairStep);
    if (i < pairStep) s.querySelector(".dot").innerHTML = '<svg style="width:11px;height:11px"><use href="#i-check"/></svg>';
    else s.querySelector(".dot").textContent = i;
    $("#pw" + i).classList.toggle("on", i === pairStep);
  });
  $("#btnPairNext").style.display = pairStep === 1 ? "" : "none";
  $("#btnPairConfirm").style.display = pairStep === 2 ? "" : "none";
  $("#btnPairDone").style.display = pairStep === 3 ? "" : "none";
}
$("#btnPairNext").addEventListener("click", () => { pairStep = 2; applyPairStep(); });
$("#btnPairConfirm").addEventListener("click", () => { pairStep = 3; applyPairStep(); });
$("#btnPairDone").addEventListener("click", () => {
  closeModal($("#ov-pair"));
  if (!DEVICES.find(d => d.id === "lumia")) {
    DEVICES.push({ id: "lumia", name: "LumiaDevice", sub: "Lumia · Linux", icon: "i-pc", online: true, role: "已配对", last: "刚刚", chan: "TCP 直连 · Noise 加密" });
  }
  state.devBadge = false;
  renderDevices();
  toast("配对完成", "LumiaDevice 已加入节点清单", "ok");
  addAudit("设备", "配对确认 · 双向认证通过（LumiaDevice）", "本机", true);
});

/* ============================================================
   同步
   ============================================================ */
function renderSync() {
  if (state.dummy) {
    $("#syncStats").innerHTML = `<div class="stat" style="grid-column:1/-1;justify-content:center;color:var(--text-3)"><svg class="ic"><use href="#i-eye-off"/></svg>伪空间中不提供真实同步数据</div>`;
    $("#xferSummary").textContent = "—";
    $("#xferActive").innerHTML = '<div class="small t3" style="padding:8px 4px">无</div>';
    $("#xferWaiting").innerHTML = '<div class="small t3" style="padding:8px 4px">无</div>';
    $("#xferConflict").innerHTML = '<div class="small t3" style="padding:8px 4px">无</div>';
    $("#navSyncBadge").style.display = "none";
    return;
  }
  const total = state.conflicts;
  $("#syncStats").innerHTML = `
    ${statCard("i-devices", "var(--blue-soft);color:var(--blue)", DEVICES.filter(d => d.online).length, "台设备在线")}
    ${statCard("i-up", "var(--gold-soft);color:var(--gold)", 12, "项待同步")}
    ${statCard("i-warn", "var(--danger-soft);color:var(--danger)", total, "项冲突待解决")}
    ${statCard("i-wifi", "var(--ok-soft);color:var(--ok)", '<span style="font-size:13px">↑ 2.1 MB/s · ↓ 800 KB/s</span>', "通道速率（Noise 加密）")}`;
  $("#xferSummary").textContent = "上传 2 项 · 下载 1 项";
  $("#xferActive").innerHTML = TRANSFERS.filter(t => !t.paused).map(xferHTML).join("");
  $("#xferWaiting").innerHTML = TRANSFERS.filter(t => t.paused).map(xferHTML).join("");
  $("#xferConflict").innerHTML = state.conflicts ? `
    <div class="xfer" data-conflict>
      <span class="fico" style="background:var(--danger-soft);color:var(--danger)"><svg><use href="#i-warn"/></svg></span>
      <div class="xm"><div class="l1"><b>document.docx</b></div><div class="l2"><span>本机与 WorkBook-X1 均有修改</span><span>向量时钟无法判定因果</span></div></div>
      <div class="xops"><button class="btn sm" data-conflict>查看 / 解决</button></div>
    </div>` : '<div class="small t3" style="padding:8px 4px">无冲突</div>';
  $("#navSyncBadge").style.display = state.conflicts ? "" : "none";
}
function statCard(ic, style, num, label) {
  return `<div class="stat"><div class="sic" style="${style}"><svg><use href="#${ic}"/></svg></div><div><b class="num">${num}</b><span class="small">${label}</span></div></div>`;
}
function xferHTML(t) {
  const pct = Math.round(t.pct);
  return `<div class="xfer" data-xfer="${t.id}">
    <span class="fico"><svg><use href="#${KIND_ICON[t.kind]}"/></svg></span>
    <div class="xm"><div class="l1"><b>${esc(t.name)}</b><span class="pct">${t.paused ? t.speed : pct + "%"}</span></div>
    <div class="pbar"><i style="width:${pct}%"></i></div>
    <div class="l2" style="margin-top:4px"><span>${t.dir === "up" ? "上传" : "下载"} · ${esc(t.meta)}</span><span>${t.paused ? "" : t.speed}</span></div></div>
    <div class="xops"><button class="iconbtn" data-tp="${t.id}" title="${t.paused ? "继续" : "暂停"}"><svg class="ic"><use href="#${t.paused ? "i-play" : "i-pause"}"/></svg></button></div>
  </div>`;
}
$("#page-sync").addEventListener("click", e => {
  const tp = e.target.closest("[data-tp]");
  if (tp) {
    const t = TRANSFERS.find(x => x.id === tp.dataset.tp);
    t.paused = !t.paused;
    if (!t.paused && t.pct === 0) t.speed = "1.4 MB/s";
    renderSync(); return;
  }
  if (e.target.closest("[data-conflict]")) { $("#cfName").textContent = "document.docx · 双方均有修改"; openModal("ov-conflict"); return; }
  const row = e.target.closest("[data-xfer]");
  if (row) openSyncInfo(TRANSFERS.find(x => x.id === row.dataset.xfer));
});
function openSyncInfo(t) {
  if (!t) return;
  $("#dsName").textContent = t.name;
  const total = 39, done = Math.round(t.pct / 100 * total);
  $("#dsKv").innerHTML = `
    <dt>方向</dt><dd>${t.dir === "up" ? "上传 → WorkBook-X1" : "下载 ← Phone-01"}</dd>
    <dt>进度</dt><dd class="num">${done} / ${total} 块（${t.pct}%）</dd>
    <dt>速度</dt><dd class="num">${t.speed}</dd>
    <dt>信道</dt><dd>NAT 直连 · Noise 加密</dd>
    <dt>块对齐</dt><dd>加密块 = 同步块 = 传输块</dd>`;
  let cells = "";
  const skip = 6;
  for (let i = 0; i < total; i++) {
    const cls = i < done ? "done" : i === done && !t.paused ? "cur" : i < done + 2 && t.dir === "up" ? "skip" : "";
    cells += `<i class="${cls}"></i>`;
  }
  $("#dsBlocks").innerHTML = cells;
  $("#dsResume").textContent = t.pct > 0 ? `断点位于第 ${done} 块（偏移 ${(t.pct * 33).toFixed(0)} KB）· 连接中断后自动续传，已到块经强哈希校验后复用` : "尚未开始传输";
  openModal("ov-syncinfo");
}
$("#btnSyncPause").addEventListener("click", e => {
  const paused = TRANSFERS.every(t => t.paused);
  TRANSFERS.forEach(t => t.paused = !paused);
  e.currentTarget.innerHTML = paused ? '<svg class="ic"><use href="#i-pause"/></svg>暂停同步' : '<svg class="ic"><use href="#i-play"/></svg>恢复同步';
  renderSync();
  toast(paused ? "同步已恢复" : "同步已暂停", paused ? "断点已记录，可续传" : "进行中的任务已挂起", "info");
});
$("#btnSyncNow").addEventListener("click", () => { TRANSFERS.forEach(t => { t.paused = false; if (t.pct === 0) t.speed = "1.4 MB/s"; }); $("#btnSyncPause").innerHTML = '<svg class="ic"><use href="#i-pause"/></svg>暂停同步'; renderSync(); toast("已发起同步", "与 2 台设备交换块哈希清单…", "info"); });
setInterval(() => {
  if (state.locked) return;
  let dirty = false;
  TRANSFERS.forEach(t => {
    if (!t.paused && t.pct < 100) { t.pct = Math.min(100, t.pct + 1.5); if (t.pct >= 100) { t.speed = "已完成"; } dirty = true; }
  });
  if (dirty && state.page === "sync") renderSync();
}, 900);
$("#btnCfBoth").addEventListener("click", () => resolveConflict("已保留两个版本", "document_v1.vault · document_v2.vault"));
$("#btnCfPick").addEventListener("click", () => resolveConflict("已使用所选版本", "另一版本保留为加密时间戳副本"));
function resolveConflict(t1, t2) {
  state.conflicts = 0;
  closeModal($("#ov-conflict"));
  renderSync();
  toast(t1, t2, "ok");
  addAudit("同步", `冲突解决 document.docx（${t1}）`, "本机", true);
}

/* ============================================================
   安全中心
   ============================================================ */
function renderDetect() {
  if (state.dummy) {
    $("#detectGrid").innerHTML = `<div class="detect" style="grid-column:1/-1"><div class="dic ok"><svg><use href="#i-shield"/></svg></div><div><b>伪空间</b><span class="small">真实安全事件与审计仅在主空间可见</span></div></div>`;
    return;
  }
  $("#detectGrid").innerHTML = `
    ${detect("i-lock", "ok", "暴力破解检测", "正常 · 计数已归零")}
    ${detect("i-devices", "warn", "异常设备", "注意 · 1 次未授权配对已拒绝（09-17 09:12）")}
    ${detect("i-shield", "ok", "文件完整性", "正常 · GCM 逐块校验通过")}
    ${detect("i-key", "ok", "内存泄漏扫描", "正常 · 密钥缓冲已清零")}
    ${detect("i-history", "ok", "审计链完整性", "正常 · 13 / 13 条校验通过")}
    ${detect("i-wifi", "ok", "信令防重放", "正常 · 序号连续")}`;
}
function detect(ic, tone, t, s) { return `<div class="detect"><div class="dic ${tone}"><svg><use href="#${ic}"/></svg></div><div><b>${t}</b><span class="small">${s}</span></div></div>`; }
function renderAudit() {
  if (state.dummy) {
    $("#auditChips").innerHTML = "";
    $("#auditBody").innerHTML = `<tr><td colspan="6" style="text-align:center;color:var(--text-3);padding:22px">伪空间不记录真实审计链</td></tr>`;
    return;
  }
  $("#auditChips").innerHTML = CATS.map(c => `<button class="chip ${c === auditFilter ? "on" : ""}" data-cat="${c}">${c}</button>`).join("");
  const rows = AUDIT.filter(a => auditFilter === "全部" || a.cat === auditFilter);
  $("#auditBody").innerHTML = rows.map((a, i) => `
    <tr data-audit="${AUDIT.indexOf(a)}">
      <td class="num t2">${a.t}</td>
      <td><span class="badge mute plain">${a.cat}</span></td>
      <td>${esc(a.act)}</td>
      <td class="t2">${a.dev}</td>
      <td>${a.ok ? '<span class="badge ok plain">✓ 正常</span>' : '<span class="badge warn plain">⚠ 告警</span>'}</td>
      <td class="mono t3">h(${AUDIT.indexOf(a)})=${hex(6, AUDIT.indexOf(a) + 3)}</td>
    </tr>`).join("");
}
$("#auditChips").addEventListener("click", e => { const b = e.target.closest("[data-cat]"); if (b) { auditFilter = b.dataset.cat; renderAudit(); } });
$("#auditBody").addEventListener("click", e => {
  const r = e.target.closest("[data-audit]"); if (!r) return;
  const a = AUDIT[+r.dataset.audit];
  $("#daKv").innerHTML = `
    <dt>时间</dt><dd class="num">${a.t}</dd>
    <dt>类别</dt><dd>${a.cat}</dd>
    <dt>动作</dt><dd>${esc(a.act)}</dd>
    <dt>来源设备</dt><dd>${a.dev}</dd>
    <dt>结果</dt><dd>${a.ok ? "成功" : "触发告警"}</dd>
    <dt>本条哈希</dt><dd class="mono">${hex(16, +r.dataset.audit + 3)}</dd>
    <dt>前链哈希</dt><dd class="mono">${+r.dataset.audit > 0 ? hex(16, +r.dataset.audit + 2) : "GENESIS"}</dd>
    <dt>链 ID</dt><dd class="mono">device:DESKTOP-VAULT</dd>`;
  openModal("ov-auditinfo");
});
$("#btnAuditVerify").addEventListener("click", e => {
  const b = e.currentTarget; b.disabled = true;
  b.innerHTML = '<svg class="ic"><use href="#i-sync"/></svg>校验中…';
  setTimeout(() => {
    b.disabled = false; b.innerHTML = '<svg class="ic"><use href="#i-check"/></svg>校验审计链';
    toast("审计链校验通过", `${AUDIT.length} / ${AUDIT.length} 条 · 各设备链头已交叉核对`, "ok");
  }, 1200);
});
$("#btnAuditExport").addEventListener("click", () => openModal("ov-audexp"));
$("#btnAudExpOk").addEventListener("click", () => { closeModal($("#ov-audexp")); toast("审计日志已导出", "vault_audit.vaudit · 含链头哈希 · 导出事件已记入审计", "ok"); });

/* ---------- 审计内部追加 ---------- */
function addAudit(cat, act, dev, ok) { AUDIT.unshift({ t: "09-17 " + new Date().toTimeString().slice(0, 5), cat, act, dev, ok }); if (state.page === "security") renderAudit(); }

/* ---------- 紧急销毁 ---------- */
$("#btnWipe").addEventListener("click", () => { $("#inpGwPhrase").value = ""; $("#btnGwGo").disabled = true; openModal("ov-gwipe"); });
$("#inpGwPhrase").addEventListener("input", e => $("#btnGwGo").disabled = e.target.value.trim() !== "销毁");
$("#btnGwGo").addEventListener("click", () => { closeModal($("#ov-gwipe")); runWipeFly("正在紧急销毁", () => lockAll()); });
function runWipeFly(title, done) {
  $("#wipeTitle").textContent = title;
  $("#wipefly").classList.add("on");
  const steps = ["销毁密钥槽 MK_wrap_pwd / MK_wrap_bio…", "摘除块清单 · 密文不可恢复…", "zeroize 内存与缩略图缓存…", "传播删除指令至已配对设备…", "写入最终审计记录…"];
  let p = 0, si = 0;
  const iv = setInterval(() => {
    p += 4; si = Math.min(steps.length - 1, Math.floor(p / 22));
    $("#wipeBar").style.width = p + "%";
    $("#wipeStat").textContent = steps[si];
    if (p >= 100) {
      clearInterval(iv);
      $("#wipeStat").textContent = "销毁完成";
      setTimeout(() => { $("#wipefly").classList.remove("on"); $("#wipeBar").style.width = "0"; done(); }, 700);
    }
  }, 130);
}

/* ---------- 隐写术 ---------- */
$("#btnStegoEnable").addEventListener("click", () => { if (!state.stegoOn) openModal("ov-stego"); else go("stego"); });
$("#btnStegoConfirm").addEventListener("click", () => {
  state.stegoOn = true;
  closeModal($("#ov-stego"));
  $("#navStego").classList.remove("hidden");
  $("#navStego").classList.add("reveal");
  $("#btnStegoEnable").textContent = "进入";
  $("#stegoStateTxt").textContent = "已启用 · 入口已显示于导航栏";
  addAudit("安全", "启用隐写术引擎", "本机", true);
  toast("隐写术已启用", "导航栏已出现「隐写术」入口", "ok");
});
(function renderCapWall() {
  const used = 14, partial = 3, total = 46;
  let h = "";
  for (let i = 0; i < total; i++) h += `<i class="${i < used ? "f" : i < used + partial ? "p" : ""}"></i>`;
  $("#capWall").innerHTML = h;
})();
function openEmbed(file) {
  openModal("ov-embed");
  $("#embFile").innerHTML = Object.keys(FS).flatMap(k => FS[k].items.filter(x => x.kind !== "folder").map(x => `<option ${file && x.id === file.id ? "selected" : ""}>${esc(x.name)} · ${x.size}</option>`)).join("");
  embStep(1); calcCap();
}
function embStep(n) {
  [1, 2, 3].forEach(i => {
    $("#es" + i).classList.toggle("cur", i === n); $("#es" + i).classList.toggle("done", i < n);
    $("#ew" + i).classList.toggle("on", i === n);
  });
  $("#btnEmbNext").style.display = n < 3 ? "" : "none";
  $("#btnEmbDone").style.display = n === 3 ? "" : "none";
}
function calcCap() {
  const [name, size] = $("#embFile").value.split(" · ");
  const capKB = +($("#embCarrier").selectedOptions[0]?.value || 760);
  const mb = parseFloat(size) * (size.includes("GB") ? 1024 : 1);
  const need = Math.max(1, Math.ceil(mb * 1024 / capKB));
  $("#embCap").innerHTML = `${esc(name)} 密文约 <b class="hl">${(mb * 1024 * 1.05).toFixed(0)} KB</b>（含头与完整性校验）→ 需要分割嵌入 <b class="hl">${need}</b> 张载体图片`;
}
$("#embFile").addEventListener("change", calcCap);
$("#embCarrier").addEventListener("change", calcCap);
$("#btnEmbNext").addEventListener("click", () => {
  embStep($("#ew1").classList.contains("on") ? 2 : 3);
});
$("#btnEmbDone").addEventListener("click", () => {
  if ($("#embDone").style.display !== "none") {
    closeModal($("#ov-embed"));
    toast("隐写嵌入完成", "载荷默认省略 Magic 标记，防自曝", "ok");
    addAudit("保险箱", "隐写嵌入 → holiday_2026.png", "本机", true);
    return;
  }
  embStep(3);
  $("#embProgWrap").style.display = ""; $("#embDone").style.display = "none";
  let p = 0;
  const iv = setInterval(() => {
    p += 6; $("#embBar").style.width = p + "%";
    if (p >= 100) {
      clearInterval(iv);
      $("#embProgWrap").style.display = "none";
      $("#embDone").style.display = "";
      $("#embDoneTxt").textContent = "holiday_2026.png 外观无可见变化 · 已存入「相册」";
    }
  }, 120);
});
$("#tileEmbed").addEventListener("click", () => openEmbed(null));
$("#tileExtract").addEventListener("click", () => {
  openModal("ov-extract");
  $("#xw1").classList.add("on"); $("#xw2").classList.remove("on");
  $("#btnExtGo").style.display = ""; $("#btnExtDone").style.display = "none";
  $("#extProgWrap").style.display = ""; $("#extDone").style.display = "none"; $("#extBar").style.width = "0";
});
$("#btnExtGo").addEventListener("click", () => {
  $("#xw1").classList.remove("on"); $("#xw2").classList.add("on");
  $("#btnExtGo").style.display = "none";
  let p = 0;
  const iv = setInterval(() => {
    p += 5; $("#extBar").style.width = p + "%";
    if (p >= 100) {
      clearInterval(iv);
      $("#extProgWrap").style.display = "none"; $("#extDone").style.display = "";
      $("#btnExtDone").style.display = "";
    }
  }, 110);
});
$("#btnExtDone").addEventListener("click", () => { closeModal($("#ov-extract")); toast("已还原 1 个文件", "合同草稿.pdf 已存入「工作」", "ok"); });

/* ============================================================
   设置
   ============================================================ */
$$(".acc-head").forEach(h => h.addEventListener("click", () => h.closest(".acc").classList.toggle("open")));
$("#btnChangePw").addEventListener("click", () => { $("#inpOldPw").value = $("#inpNewPw").value = $("#inpNewPw2").value = ""; $("#pwStrength").className = "strength"; openModal("ov-password"); });
$("#inpNewPw").addEventListener("input", e => {
  const v = e.target.value;
  let s = 0;
  if (v.length >= 8) s++;
  if (v.length >= 12 && /[A-Z]/.test(v) && /[a-z]/.test(v)) s++;
  if (/\d/.test(v) && /[^A-Za-z0-9]/.test(v)) s++;
  if (v.length >= 16 && s === 3) s = 4;
  $("#pwStrength").className = "strength" + (v ? " s" + Math.max(1, s) : "");
  $("#pwStrengthTxt").textContent = ["过短", "弱", "一般", "较强", "强"][v ? Math.max(1, s) : 0] + " · Argon2id 派生 KEK";
});
$("#btnPwOk").addEventListener("click", () => {
  if (!$("#inpOldPw").value || $("#inpNewPw").value.length < 8) { toast("请完整填写", "新密码至少 8 位", "warn"); return; }
  if ($("#inpNewPw").value !== $("#inpNewPw2").value) { toast("两次输入不一致", "", "warn"); return; }
  closeModal($("#ov-password"));
  toast("主密码已修改", "MK 重新包装完成 · 3ms · 数据零重加密", "ok");
  addAudit("会话", "修改主密码（重新包装 MK_wrap_pwd）", "本机", true);
});
$("#swBio").addEventListener("change", e => {
  const on = e.target.checked;
  $("#bioStateTxt").textContent = on ? "已绑定 · Windows Hello 指纹" : "未绑定 · 主密码始终可用";
  toast(on ? "生物识别已绑定" : "生物识别已解绑", on ? "已追加 MK_wrap_bio 副本至保险箱头部" : "MK_wrap_bio 已销毁，密码路径不受影响", on ? "ok" : "warn");
  addAudit("会话", (on ? "绑定生物识别（追加 MK_wrap_bio）" : "解绑生物识别"), "本机", true);
});
$("#btnRecoverCfg").addEventListener("click", () => { $("#chkRecover").checked = false; $("#btnRecoverGo").disabled = true; openModal("ov-recover"); });
$("#chkRecover").addEventListener("change", e => $("#btnRecoverGo").disabled = !e.target.checked);
$("#btnRecoverGo").addEventListener("click", () => { closeModal($("#ov-recover")); toast("授权请求已发送", "等待 WorkBook-X1 确认（演示环境不会真正执行）", "info"); });
$("#btnDummyCfg").addEventListener("click", () => { $("#inpDummyPw").value = ""; openModal("ov-dummy"); });
$("#btnDummySet").addEventListener("click", () => {
  const v = $("#inpDummyPw").value;
  if (v.length < 6) { toast("伪装密码至少 6 位", "", "warn"); return; }
  $("#dummyStateTxt").textContent = "已设置 · 锁屏输入即进入伪空间";
  closeModal($("#ov-dummy"));
  toast("伪装密码已保存", "伪空间使用独立 MK，与真空间互不可见", "ok");
  addAudit("会话", "配置伪装入口", "本机", true);
});
$("#btnDummyEnter").addEventListener("click", () => { closeModal($("#ov-dummy")); enterDummy(); });
$("#btnPolicyCfg").addEventListener("click", () => openModal("ov-policy"));
$("#btnPolicySave").addEventListener("click", () => {
  closeModal($("#ov-policy"));
  $("#policyStateTxt").textContent = "策略已更新 · 时段 09:00–18:00 · 上/下行 10 MB/s";
  toast("同步策略已保存", "选择性同步规则即时生效", "ok");
  addAudit("设备", "变更同步策略", "本机", true);
});
$("#selTheme").addEventListener("change", e => { document.body.dataset.theme = e.target.value; });

/* ============================================================
   通知
   ============================================================ */
function renderNotifs() {
  $("#notifList").innerHTML = NOTIFS.map((n, i) => `
    <div class="notif-item" data-nt="${i}">
      <div class="nic" style="background:${n.bg}"><svg><use href="#${n.ic}"/></svg></div>
      <div style="flex:1;min-width:0"><b>${esc(n.t)}</b><span class="small">${esc(n.s)}</span></div>
      <span class="ntime">${n.time}</span>
    </div>`).join("");
  $("#notifDot").style.display = state.notifRead ? "none" : "";
}
$("#btnNotif").addEventListener("click", () => $("#notifDD").classList.toggle("on"));
$("#btnNotifClear").addEventListener("click", () => { state.notifRead = true; renderNotifs(); });
$("#notifList").addEventListener("click", e => {
  const n = e.target.closest("[data-nt]"); if (!n) return;
  $("#notifDD").classList.remove("on");
  const item = NOTIFS[+n.dataset.nt];
  if (item.t === "配对请求") { go("devices"); openPair(2); }
  else if (item.t === "同步冲突") { go("sync"); }
  else if (item.t === "入侵检测") { go("security"); }
  else go("sync");
});

/* ============================================================
   初始化
   ============================================================ */
renderVault();
renderDevices();
renderSync();
renderDetect();
renderAudit();
renderNotifs();
