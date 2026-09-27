<div align="center">

# 🔐 VaultSync

**端到端加密的多设备文件同步系统 —— 你的文件，只有你能读。**

[![实现进度](https://img.shields.io/badge/实现-v0.5.0%20(M4)-D9B25F?style=flat-square)](#)
[![设计基线](https://img.shields.io/badge/基线-V2.0-D9B25F?style=flat-square)](#)
[![语言](https://img.shields.io/badge/引擎-Rust-000000?style=flat-square&logo=rust)](#)
[![语言](https://img.shields.io/badge/客户端-Flutter-02569B?style=flat-square&logo=flutter)](#)
[![平台](https://img.shields.io/badge/平台-Windows%20·%20macOS·Linux-blue?style=flat-square)](#)

VaultSync 把 **加密保险箱、P2P 增量同步、端到端加密通信、隐写术与安全中心** 整合为一个跨平台应用。全程密文流转（**零信任**）：中继只转发看不懂的密文，解密只发生在你自己的设备上。

> 里程碑：M4（P0–P4 全部 + P5 大部分落地）· 首个正式版规划为 `v2.0.0`

</div>

---

## ✨ 特性一览

| 能力 | 状态 |
| --- | --- |
| 🔒 **加密保险箱** — CDC 分块 + AES-256-GCM，每文件独立容器与密钥，导入/导出/重命名/文件夹树 | ✅ P2 |
| 🗝️ **双因子密钥体系** — MK 永不变更，主密码（Argon2id）与生物识别（平台安全存储）双 KEK 包装，改密零重加密 | ✅ P1 |
| 🛡️ **暴力破解防护** — 按域计数失败，3 次起 30s 指数冷却至 15min | ✅ P1 |
| 🎭 **伪装空间** — 独立 MK 的第二保险箱，胁迫场景保护真实数据 | ✅ P1 |
| 🔎 **密文侧全文检索** — HMAC 倒排 + CJK bigram，明文索引永不落盘 | ✅ P2 |
| 🧹 **安全擦除** — 密钥销毁 + 单次覆写兜底，密文与密钥同灭 | ✅ P2 |
| 📨 **限次分享 / 阅后即焚** — HMAC + TTL + 次数约束的分享令牌 | ⚠️ 一次性 P2P 语义待 P8-9 |
| 🔁 **P2P 设备同步** — 自研 Noise 帧协议（snow）+ 增量同步 + 冲突解决 + 自建中继 | ✅ P3 |
| 📜 **链式哈希审计日志** — 校验 + 加密导出，全部敏感操作可追溯 | ✅ P5-1 |
| 🖼️ **PNG LSB 隐写术** — 嵌入 / 提取 / 容量估算，载荷加密先行 | ✅ P5-3 |
| 🔄 **密钥轮换** — 单文件 / 文件夹 / MK 全库轮换 | ✅ P5-4 |
| ☠️ **紧急销毁** — 本机销毁 + 全设备联动离线指令队列 | ✅ P5-5 |
| 🌍 **多端与发布** — Linux / macOS / Android / iOS、签名安装包、更新基础设施 | 🚧 V2.0 (P9) |

## 🛡️ 安全模型（零信任）

**明文与密钥只存在于 Rust 核心引擎层**；Dart 展示 / 状态 / 服务层一律只触碰不透明的会话句柄，永不接触明文或密钥。

- **密钥分层**：MK 由 CSPRNG 生成且**永不变更**；密码 / 生物识别只是两把 KEK，用于解封 MK 的 wrapped 副本；文件用 per-file FSKey 加密。▸ V2.0 起新增从属密钥层（索引 / 搜索 / 分享 / 订单 / 隐写载荷），独立包装于 `VSVB v3` 头部，**不再从 MK 直接派生**。
- **加密块 = 同步块 = 传输块**（CDC 分块）：局部修改不触发全文件重加密。
- **中继只转发密文**：不落盘、不解密。
- **全程可审计**：安全敏感操作写入链式哈希审计日志，失败不阻断业务（事后取证优先）。
- **证据先行**：任何对外声称的安全能力都要求运行时接口 + 可复现证据；没有接口就在 UI 上如实说明，禁止「设计保证」式的承诺。

## 🏗️ 架构

三层一桥，调用方向：**展示层 → 状态层 → 服务层 → FFI 桥 → Rust 核心引擎**。

```text
┌───────────────────────────────────────────────────────┐
│  展示层   Flutter / Dart（页面 · 组件 · 主题）           │
├───────────────────────────────────────────────────────┤
│  状态层   Riverpod         服务层   Dart（仅编排）       │
├───────────────────────────────────────────────────────┤
│  FFI 桥（dart:ffi，会话句柄跨语言，MK 不出引擎）          │
├───────────────────────────────────────────────────────┤
│  核心引擎  Rust Workspace（唯一合法触碰密钥者）           │
│   vault-core    唯一 FFI 门面                           │
│   vault-crypto  AES-GCM / Argon2id / HKDF / CSPRNG     │
│   vault-vault   容器 / CDC / 索引                       │
│   vault-audit   vault-p2p   vault-stego   vault-relay  │
│   （V2.0 计划新增 vault-store / vault-net）              │
├───────────────────────────────────────────────────────┤
│  平台能力   安全存储 · 文件系统 · 网络 · 通知             │
└───────────────────────────────────────────────────────┘
```

Dart 层不持有密钥 —— 解封只发生在引擎内部，UI 拿到的只是一个不透明的会话句柄。

## 🚀 快速开始

### 环境依赖

- [Flutter SDK](https://flutter.dev)（桌面端）
- [Rust toolchain](https://rustup.rs)（`cargo`，构建引擎）

### 运行桌面端

```bash
cd app
flutter pub get
flutter run -d windows   # 构建时自动 cargo 编译 vault_core.dll 并捆绑
```

### 引擎端到端冒烟

```bash
cd app
dart run tool/ffi_check.dart   # 114 项断言 / 54 个 FFI 导出
```

覆盖：解锁 / 冷却 / 改密 / 生物识别 / 导入导出 / 检索 / 擦除 / 分享 / 审计链 / 隐写 / 密钥轮换 / 紧急销毁 / 更新验签 / 配对 / 增量同步 / 删除同步 / 冲突副本 / 远程销毁 / 解绑。

### 引擎单测与静态检查

```bash
cd native
cargo fmt && cargo clippy --all-targets && cargo test
```

> **首次运行**：锁屏输入的密码即成为主密码（≥8 位且强度达标）。生物识别绑定后 KEK_bio 存于平台安全存储（Windows Credential Manager / macOS Keychain）；Linux 无后端时自动禁用该路径，主密码不受影响。

## 📁 目录结构

```text
VaultSync/
├── AGENTS.md        # AI / 协作者工作区指南（安全红线、命令、约定）
├── LOG.md           # 开发日志：问题、隐患、决策（新条目追加在末尾）
├── docs/            # 设计文档（仅本地/私有版本控制，不随仓库上传）
├── prototype/       # 高保真 HTML 原型（v1.0 冻结 / v2.0 桌面端 / mobile 移动端）
├── packaging/       # 安装包配方（Windows Inno Setup）与未决决策清单
├── app/             # Flutter 桌面客户端
│   ├── lib/core/          # Design Token 主题 + SVG 图标 + l10n 取词桥
│   ├── lib/state/         # Riverpod 状态层
│   ├── lib/service/       # 编排层（后台 Isolate 调 FFI）
│   ├── lib/ffi/           # dart:ffi 绑定（唯一触碰原生指针的层）
│   ├── lib/presentation/  # 路由 / 页面 / 主壳导航 / 组件库
│   └── tool/ffi_check.dart  # 引擎端到端冒烟
└── native/          # Rust 核心引擎（Cargo Workspace，七 crate）
```

> `docs/` 通过 `.gitignore` 排除在云端仓库之外，仅保存在本地并走私有版本控制 —— 细节与完整规范、任务面板、威胁模型复核等文档均在此目录内，README 不再逐篇链接。

## 🧩 原型

`prototype/index.html` 为纯静态高保真原型版本索引（零依赖，双击即开），是 UI 的交互与视觉验收基准。原型**按设计基线与端分目录**：`prototype/v1.0/`（对应 `docs/v1.0/`，已冻结只读）、`prototype/v2.0/`（对应 `docs/v2.0/`，现行 · 桌面端 `≥1280` 档）与 `prototype/mobile/`（现行 · 移动端 `<720` 档）。移动端**不复制内核**，直接引用 `../v2.0/js/{core,ui,data}.js`，即产品原则「一套内核、受管外壳」在原型工程上的投影。演示口令：`1234`（真实保险箱 / `88888888` 伪装空间）。原型中的 SVG 图标库已提取为 Flutter 资产（`app/assets/icons/`）。各端的路由、弹窗与组件清单见 `prototype/v2.0/README.md` 与 `prototype/mobile/README.md`。

## 🤝 参与开发

- 提交信息遵循 **Conventional Commits**（`feat / fix / docs / chore / security`），仓库启用 `commit-msg` 钩子校验（新 clone 后执行 `git config core.hooksPath .githooks`）。
- 版本号 SemVer，里程碑：`M0`→v0.1.0 … `M4`→v0.5.0、`M5`→**v0.6.0**、…、`M9`→**v2.0.0（首个正式版）**。
- CI（GitHub Actions）：Flutter analyze/test + Rust fmt/clippy/test 双流水线 + Windows 集成构建。

## 📄 License

未定 —— 项目当前处于开发阶段（`v0.5.0`）。

---

**私钥与数据安全提示**：主密码不可找回（无后门），忘记密码 = 数据不可恢复。本项目当前处于开发阶段，**请勿存放唯一副本的重要数据**。