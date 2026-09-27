/* VaultSync 官网脚本 — 零依赖，非 ES Module（file:// 双击可开）
   购买 / 许可证均为 Mock 数据：仅前端演示，不发起任何真实网络请求。 */
(function () {
  "use strict";

  /* ---------------- Mock 商品目录（价格与额度为示例数据，非官方定价） ---------------- */
  var PRODUCTS = {
    pro: {
      name: "Pro 个人版",
      mode: "一次性买断（含 1 年快速更新通道）",
      base: 198,
      unit: "",
      features: { seats: 1, updates: "快速通道 + LTS（1 年）" }
    },
    relay: {
      name: "Pro 中继包",
      mode: "按年订阅官方中继高额度",
      base: 60,
      unit: "/年",
      features: { quota: "高额度 · 高速率", updates: "不改变更新通道" }
    },
    team: {
      name: "Team 团队版",
      mode: "按席位 / 年",
      base: 120,
      unit: "/席位/年",
      minSeats: 2,
      features: { seats: "席位池（每席位 3 台设备）", updates: "快速通道 + LTS" }
    },
    enterprise: {
      name: "企业自建部署",
      mode: "年度授权",
      base: 0, // 0 = 需联系
      unit: "",
      features: { headless: "无头 / CLI / NAS / SDK", support: "审计报告 · SBOM · 支持 SLA" }
    }
  };

  /* ---------------- Toast ---------------- */
  var toastTimer = null;
  function toast(msg) {
    var el = document.getElementById("toast");
    if (!el) return;
    el.textContent = msg;
    el.classList.add("show");
    clearTimeout(toastTimer);
    toastTimer = setTimeout(function () { el.classList.remove("show"); }, 2600);
  }
  window.vsToast = toast;

  /* ---------------- 购买弹窗（Mock） ---------------- */
  var modal = null;
  var current = null;

  function openPurchase(key) {
    current = PRODUCTS[key];
    if (!current) return;
    var mask = document.getElementById("purchase-mask");
    if (!mask) return;
    // 重置视图
    mask.querySelector(".form-view").style.display = "";
    mask.querySelector(".success-view").style.display = "none";

    document.getElementById("pm-title").textContent = "购买 · " + current.name;
    document.getElementById("pm-mode").textContent = current.mode;

    var seatsWrap = document.getElementById("pm-seats-wrap");
    if (key === "team") {
      seatsWrap.style.display = "";
      document.getElementById("pm-seats").value = Math.max(2, current.minSeats || 2);
    } else {
      seatsWrap.style.display = "none";
    }

    var qtyLabel = document.getElementById("pm-years-label");
    qtyLabel.textContent = key === "pro" ? "快速更新通道续期（年）" : "购买年限";
    document.getElementById("pm-years").value = 1;
    document.getElementById("pm-years-wrap").style.display =
      (key === "enterprise") ? "none" : "";

    updateSummary();
    mask.classList.add("open");
  }

  function seatCount() {
    if (!current || current !== PRODUCTS.team) return 1;
    var v = parseInt(document.getElementById("pm-seats").value, 10);
    if (isNaN(v) || v < 2) v = 2;
    return v;
  }

  function totalPrice() {
    if (!current) return 0;
    if (current.base === 0) return 0;
    var years = parseInt(document.getElementById("pm-years").value, 10) || 1;
    return current.base * seatCount() * years;
  }

  function updateSummary() {
    if (!current) return;
    var parts = [];
    if (current === PRODUCTS.team) parts.push(seatCount() + " 席位");
    var years = parseInt(document.getElementById("pm-years").value, 10) || 1;
    if (current !== PRODUCTS.enterprise && years > 1) parts.push(years + " 年");
    document.getElementById("pm-desc").textContent = parts.length ? current.name + " · " + parts.join(" · ") : current.name;
    var total = totalPrice();
    document.getElementById("pm-total").textContent = current.base === 0 ? "联系获取报价" : "¥ " + total;
  }

  function buildMockLicense() {
    var now = Date.now();
    var years = parseInt(document.getElementById("pm-years").value, 10) || 1;
    var key = Object.keys(PRODUCTS).filter(function (k) { return PRODUCTS[k] === current; })[0];
    var tier = key === "relay" ? "pro" : key; // 中继包属 Pro 档增值服务
    var featByTier = {
      pro: ["feat_history", "feat_stego_multi", "feat_audit_archive", "feat_sel_sync_full", "feat_burn_share"],
      team: ["feat_history", "feat_stego_multi", "feat_audit_archive", "feat_sel_sync_full",
        "feat_burn_share", "feat_team_manage", "feat_headless"],
      enterprise: ["feat_history", "feat_stego_multi", "feat_audit_archive", "feat_sel_sync_full",
        "feat_burn_share", "feat_team_manage", "feat_headless"]
    };
    var licensee = (document.getElementById("pm-email").value || "演示用户").trim();
    var payload = {
      schema: 1,
      licenseId: mockHex(16),
      tier: tier,
      seats: seatCount(),
      features: featByTier[tier] || [],
      relayQuota: {
        bytesPerMonth: key === "relay" || tier !== "free" ? 53687091200 : 5368709120,
        rateBps: key === "relay" ? 20971520 : 4194304,
        graceBytes: 1073741824
      },
      issuedMs: now,
      expiresMs: current.base === 0 ? 0 : 0, // 本地功能门永久（expiresMs=0）
      updatesUntilMs: now + years * 365 * 24 * 3600 * 1000,
      licensee: licensee,
      issuerKeyId: mockHex(8)
    };
    return payload;
  }

  function mockHex(bytes) {
    var s = "";
    for (var i = 0; i < bytes * 2; i++) s += "0123456789abcdef"[Math.floor(Math.random() * 16)];
    return s;
  }

  function completePurchase() {
    var email = document.getElementById("pm-email").value.trim();
    if (!email || email.indexOf("@") < 0) { toast("请填写有效的接收邮箱（演示环境不会真的发送）"); return; }

    var payload = buildMockLicense();
    // VSL1 封套说明：真实许可证为二进制封套 + Ed25519 签名；此处为演示 JSON。
    var demo = "// VaultSync 许可证（演示数据 · 未签名 · 不可用于真实客户端）\n// 真实格式为 VSL1 二进制封套：magic(\"VSL1\") + payload(JSON) + Ed25519 签名\n" +
      JSON.stringify(payload, null, 2);

    var mask = document.getElementById("purchase-mask");
    mask.querySelector(".form-view").style.display = "none";
    var sv = mask.querySelector(".success-view");
    sv.style.display = "";
    document.getElementById("pm-license").textContent = demo;

    var blob = new Blob([demo], { type: "application/json" });
    var a = document.getElementById("pm-download");
    a.href = URL.createObjectURL(blob);
    a.download = "license.vsl.demo.json";
  }

  function bindPurchase() {
    modal = document.getElementById("purchase-mask");
    if (!modal) return;
    document.querySelectorAll("[data-buy]").forEach(function (btn) {
      btn.addEventListener("click", function () { openPurchase(btn.getAttribute("data-buy")); });
    });
    modal.addEventListener("click", function (e) { if (e.target === modal) modal.classList.remove("open"); });
    document.querySelectorAll("[data-close]").forEach(function (el) {
      el.addEventListener("click", function () { modal.classList.remove("open"); });
    });
    ["pm-seats", "pm-years"].forEach(function (id) {
      var el = document.getElementById(id);
      if (el) el.addEventListener("input", updateSummary);
    });
    var pay = document.getElementById("pm-pay");
    if (pay) pay.addEventListener("click", completePurchase);
  }

  /* ---------------- Mock 下载按钮 ---------------- */
  function bindDownloads() {
    document.querySelectorAll("[data-download]").forEach(function (el) {
      el.addEventListener("click", function (e) {
        e.preventDefault();
        toast("演示站点：安装包尚未发布（首个正式版规划为 v2.0.0）。请前往 GitHub 仓库自行构建。");
      });
    });
  }

  /* ---------------- SVG 图标 sprite（内联注入，file:// 可用，颜色随 currentColor） ----------------
     图标源：app/assets/icons/*.svg（仓库原型提取库）；win/apple 为品牌示意形。 */
  var SPRITE =
    '<svg xmlns="http://www.w3.org/2000/svg" style="display:none" aria-hidden="true">' +
    /* 仓库图标库 */
    '<symbol id="i-vault" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="4" width="18" height="16" rx="2.5"/><circle cx="11" cy="12" r="3.4"/><path d="M11 10.4v1.6l1.2 1.2M18.5 9v6"/></symbol>' +
    '<symbol id="i-key" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><circle cx="8" cy="14" r="4.2"/><path d="m11.2 10.8 8.3-8.3M16 6l2.6 2.6M13.5 8.5l2.3 2.3"/></symbol>' +
    '<symbol id="i-sync" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><path d="M19.5 12a7.5 7.5 0 0 1-13 5.1M4.5 12a7.5 7.5 0 0 1 13-5.1"/><path d="M17.5 3.5v3.4h-3.4M6.5 20.5v-3.4h3.4"/></symbol>' +
    '<symbol id="i-search" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><circle cx="11" cy="11" r="6.5"/><path d="m20 20-4.4-4.4"/></symbol>' +
    '<symbol id="i-stego" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><rect x="3.5" y="4.5" width="17" height="15" rx="2"/><circle cx="9" cy="10" r="1.6"/><path d="m5 18 5-5 3.5 3.5L16.5 13l4 4.5"/><path d="M13 6.5h5M13 9h3.5" opacity=".6"/></symbol>' +
    '<symbol id="i-doc" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><path d="M6 3.5h8L19 8.5v12H6v-17z"/><path d="M9 12h6M9 15.5h6M9 8.5h2"/></symbol>' +
    '<symbol id="i-eye-off" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><path d="M4 4.5 20 19.5M9.9 6.2A9.6 9.6 0 0 1 12 5.8c6 0 9.5 6.2 9.5 6.2a17 17 0 0 1-3.3 3.9M6 8A16 16 0 0 0 2.5 12S6 18.2 12 18.2c1.2 0 2.3-.25 3.3-.66"/><path d="M9.9 9.9a2.9 2.9 0 0 0 4.1 4.1"/></symbol>' +
    '<symbol id="i-flame" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3s5.5 4.2 5.5 9.5a5.5 5.5 0 0 1-11 0c0-2 1-3.9 2.2-5.4.3 1.2 1 2.2 2 2.4C10.2 7.6 10.8 5 12 3z"/><path d="M12 21a5.5 5.5 0 0 1-3.2-1"/></symbol>' +
    '<symbol id="i-devices" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="4.5" width="13" height="9.5" rx="1.8"/><path d="M7.5 18h4M9.5 14v4"/><rect x="17" y="9" width="4.5" height="9.5" rx="1.4"/></symbol>' +
    '<symbol id="i-check" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><path d="m5 12.5 4.5 4.5L19 7.5"/></symbol>' +
    '<symbol id="i-warn" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3.5 22 20H2L12 3.5z"/><path d="M12 9.5v5M12 17.2v.3"/></symbol>' +
    '<symbol id="i-pc" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><rect x="3.5" y="4.5" width="17" height="11" rx="1.8"/><path d="M8.5 19.5h7M12 15.5v4"/></symbol>' +
    '<symbol id="i-phone" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><rect x="7" y="3" width="10" height="18" rx="2.2"/><path d="M11 17.8h2"/></symbol>' +
    '<symbol id="i-clock" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="8.5"/><path d="M12 7.5V12l3 2"/></symbol>' +
    /* 平台品牌示意形 */
    '<symbol id="i-win" viewBox="0 0 24 24"><path fill="currentColor" d="M3 5.6l7.6-1.06v6.86H3zM12.6 4.4 21 3.25v8.15h-8.4zM3 12.6h7.6v6.86L3 18.4zM12.6 12.6H21v8.15l-8.4-1.15z"/></symbol>' +
    '<symbol id="i-apple" viewBox="0 0 24 24"><path fill="currentColor" d="M16.7 12.9c0-2.4 2-3.6 2.1-3.7-1.1-1.7-2.9-1.9-3.5-1.9-1.5-.2-2.9.9-3.7.9-.8 0-1.9-.9-3.2-.86-1.6.02-3.1 1-4 2.4-1.7 3-.4 7.4 1.2 9.8.8 1.2 1.8 2.5 3.1 2.4 1.2-.05 1.7-.8 3.2-.8s1.9.8 3.2.77c1.3-.02 2.2-1.2 3-2.4.9-1.4 1.3-2.7 1.3-2.8-.03-.01-2.6-1-2.7-3.8zM14.4 5.6c.7-.8 1.1-1.9 1-3.1-1 .04-2.2.66-2.9 1.5-.6.7-1.2 1.9-1 3 1.1.1 2.2-.6 2.9-1.4z"/></symbol>' +
    '<symbol id="i-linux" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="4.5" width="18" height="15" rx="2"/><path d="m7 9.5 3 2.5-3 2.5M12.5 14.5H17"/></symbol>' +
    '<symbol id="i-android" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><path d="M5 13.5a7 7 0 0 1 14 0v.5H5zM7.6 5.2 8.9 7.3M16.4 5.2 15.1 7.3"/><path d="M8.5 10.2v.3M15.5 10.2v.3" stroke-width="2.2"/></symbol>' +
    '<symbol id="i-logo" viewBox="0 0 48 48" fill="none" stroke="currentColor" stroke-width="2.8" stroke-linecap="round" stroke-linejoin="round"><circle cx="24" cy="24" r="11.5"/><circle cx="24" cy="24" r="1.8" fill="currentColor" stroke="none"/><path d="M24 21v-6M21.4 25.5l-5.2 3M26.6 25.5l5.2 3"/><path d="M10.1 16A16 16 0 0 1 35.3 12.7M37.6 14.2 36.6 10.7M37.6 14.2 34.1 13.2"/><path d="M37.9 35.3A16 16 0 0 1 12.7 35.3M10.4 33.8l1 3.5M10.4 33.8l3.5-1"/></symbol>' +
    '</svg>';

  function injectSprite() {
    var holder = document.createElement("div");
    holder.innerHTML = SPRITE;
    document.body.insertBefore(holder.firstChild, document.body.firstChild);
  }

  /* ---------------- 初始化 ---------------- */

  /* 滚动入场：自动给常见块加 .reveal，同组内做阶梯延迟 */
  function initReveal() {
    var groups = [
      ".grid > .card", ".tier-grid > .tier", ".grid > .dl-card",
      ".redline", ".sec-head", ".stats-grid > .stat", ".card.license-card"
    ];
    groups.forEach(function (sel) {
      document.querySelectorAll(sel).forEach(function (el, i) {
        if (el.classList.contains("reveal")) return;
        el.classList.add("reveal");
        el.style.setProperty("--d", (i % 4) * 0.09 + "s");
      });
    });
    if (!("IntersectionObserver" in window)) {
      document.querySelectorAll(".reveal").forEach(function (el) { el.classList.add("in"); });
      return;
    }
    var io = new IntersectionObserver(function (entries) {
      entries.forEach(function (e) {
        if (e.isIntersecting) { e.target.classList.add("in"); io.unobserve(e.target); }
      });
    }, { threshold: 0.05, rootMargin: "800px 0px -6% 0px" });
    document.querySelectorAll(".reveal").forEach(function (el) { io.observe(el); });
  }

  /* 导航滚动状态 + 返回顶部 */
  function initScrollUi() {
    var nav = document.querySelector(".nav");
    var top = document.getElementById("backtop");
    if (!top) {
      top = document.createElement("button");
      top.id = "backtop";
      top.setAttribute("aria-label", "返回顶部");
      top.textContent = "↑";
      document.body.appendChild(top);
    }
    top.addEventListener("click", function () { window.scrollTo({ top: 0, behavior: "smooth" }); });
    var ticking = false;
    function onScroll() {
      if (ticking) return;
      ticking = true;
      requestAnimationFrame(function () {
        if (nav) nav.classList.toggle("scrolled", window.scrollY > 10);
        top.classList.toggle("show", window.scrollY > 480);
        ticking = false;
      });
    }
    window.addEventListener("scroll", onScroll, { passive: true });
    onScroll();
  }

  /* 数字滚动统计 */
  function initCounters() {
    var els = document.querySelectorAll("[data-count]");
    if (!els.length) return;
    function animate(el) {
      var target = parseInt(el.getAttribute("data-count"), 10) || 0;
      var dur = 1400, t0 = null;
      function step(t) {
        if (!t0) t0 = t;
        var p = Math.min((t - t0) / dur, 1);
        p = 1 - Math.pow(1 - p, 3); // easeOutCubic
        el.textContent = Math.round(target * p).toLocaleString();
        if (p < 1) requestAnimationFrame(step);
      }
      requestAnimationFrame(step);
    }
    if (!("IntersectionObserver" in window)) {
      els.forEach(function (el) { el.textContent = el.getAttribute("data-count"); });
      return;
    }
    var io = new IntersectionObserver(function (entries) {
      entries.forEach(function (e) {
        if (e.isIntersecting) { animate(e.target); io.unobserve(e.target); }
      });
    }, { threshold: 0.5 });
    els.forEach(function (el) { io.observe(el); });
  }

  /* 卡片跟随鼠标的辉光 */
  function initSpotlight() {
    if (window.matchMedia("(hover: none)").matches) return;
    document.querySelectorAll(".card, .tier, .dl-card").forEach(function (el) {
      el.addEventListener("mousemove", function (e) {
        var r = el.getBoundingClientRect();
        el.style.setProperty("--mx", (e.clientX - r.left) + "px");
        el.style.setProperty("--my", (e.clientY - r.top) + "px");
      });
    });
  }

  document.addEventListener("DOMContentLoaded", function () {
    injectSprite();
    bindPurchase();
    bindDownloads();
    initReveal();
    initScrollUi();
    initCounters();
    initSpotlight();
  });
})();
