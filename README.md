# VaultSync

端到端加密的多设备文件同步系统 —— 你的文件，只有你能读。

VaultSync 把文件加密保险箱、P2P 增量同步、端到端加密通信、隐写术与安全中心整合为一个跨平台桌面/移动应用。全程密文流转（零信任）：中继只转发看不懂的密文，解密只发生在你自己的设备上。

> 当前状态：**设计文档 V0.2 定稿 · 高保真原型完成 · 引擎开发已完成 P0～P2（工程奠基 / 加密内核 / 保险箱与索引）**，下一阶段 P3（P2P 同步）。

---

## 核心特性

| 能力 | 说明 | 状态 |
| --- | --- | --- |
| 加密保险箱 | CDC 分块 AES-256-GCM 加密，每文件独立容器与密钥，导入/导出/重命名/文件夹树 | ✅ P2 |
| 密钥体系 | MK 随机生成永不变更，主密码（Argon2id）与生物识别（平台安全存储）双 KEK 包装，改密码零重加密 | ✅ P1 |
| 暴力破解防护 | 按域计数失败，3 次起 30s 指数冷却至 15min | ✅ P1 |
| 伪装空间 | 独立 MK 的第二保险箱，胁迫场景下保护真实数据 | ✅ P1 |
| 全文检索 | 密文侧可检索（HMAC 倒排 + CJK bigram），明文索引不落盘 | ✅ P2 |
| 安全擦除 | 密钥销毁 + 单次覆写兜底，密文与密钥同灭 | ✅ P2 |
| 阅后即焚分享 | 一次性令牌 + TTL + 次数限制 | ✅ P2 |
| P2P 设备同步 | libp2p 直连/中继、增量同步、断点续传、冲突解决 | 🚧 P3 |
| 端到端加密通信 / 隐写术（LSB）/ 安全中心 | 详见设计文档 05-04 / 05-05 / 05-06 | 🚧 P4-P5 |

## 架构

三层一桥（详见 [docs/01](docs/01-系统整体架构设计.md)）：

```
┌─────────────────────────────────────────────────┐
│  展示层  Flutter / Dart（页面 · 组件 · 主题）      │
├─────────────────────────────────────────────────┤
│  状态层  Riverpod        服务层  Dart（仅编排）    │
├─────────────────────────────────────────────────┤
│  FFI 桥（dart:ffi，会话句柄跨语言，MK 不出引擎）    │
├─────────────────────────────────────────────────┤
│  核心引擎层  Rust Workspace（唯一合法触碰密钥者）   │
│   vault-core  ← 唯一 FFI 门面                    │
│   vault-crypto（AES-GCM/Argon2id/HKDF/CSPRNG）   │
│   vault-vault（容器/CDC/索引）  vault-audit       │
│   vault-p2p  vault-stego                         │
├─────────────────────────────────────────────────┤
│  平台能力  安全存储 · 文件系统 · 网络 · 通知        │
└─────────────────────────────────────────────────┘
```

安全红线（全仓库适用）：明文与密钥只在 Rust 引擎层存在；Dart 各层只持有不透明会话句柄；块 nonce 确定性派生（prefix ∥ counter，杜绝 GCM 随机 nonce 误用）；全部敏感操作可审计（链式哈希日志，P5 落地）。

## 目录结构

```
VaultSync/
├── AGENTS.md          # AI/协作者工作区指南（安全红线、命令、约定）
├── LOG.md             # 开发日志：问题、隐患、决策（新条目追加在末尾）
├── docs/              # 设计文档 V0.2（01–08）+ 开发任务面板(09) + Git/版本规范(10)
├── prototype/         # 桌面端高保真 HTML 原型（交互与视觉验收基准）
├── app/               # Flutter 桌面客户端
│   ├── lib/core/      #   Design Token 主题（docs/03）+ SVG 图标（53 枚，源自原型）
│   ├── lib/state/     #   Riverpod 会话状态
│   ├── lib/service/   #   编排层（后台 Isolate 调 FFI，不冻结 UI）
│   ├── lib/ffi/       #   dart:ffi 绑定（唯一触碰原生指针的层）
│   ├── lib/presentation/  # 路由（锁屏守卫）/ 锁屏 / 保险箱 / 主壳导航
│   ├── assets/icons/  #   图标资产
│   └── tool/ffi_check.dart  # 引擎端到端冒烟（41 项断言）
└── native/            # Rust 核心引擎（Cargo Workspace，六 crate）
```

## 构建与运行

依赖：Flutter SDK（桌面端）+ Rust toolchain（cargo）。

```
# 运行桌面端（构建时自动 cargo 编译 vault_core.dll 并捆绑）
cd app
flutter pub get
flutter run -d windows

# 引擎端到端冒烟（41 项断言：解锁/冷却/改密/生物识别/导入导出/检索/擦除/分享）
cd app
dart run tool/ffi_check.dart

# 引擎单测与静态检查
cd native
cargo fmt && cargo clippy --all-targets && cargo test
```

首次运行：在锁屏输入的密码即成为主密码（≥4 位临时下限，完整强度策略随 P4-1 落地）。生物识别绑定后，KEK_bio 存于平台安全存储（Windows Credential Manager / macOS Keychain）；Linux 无后端时自动禁用该路径，主密码不受影响。

## 文档

| 文档 | 内容 |
| --- | --- |
| [docs/README.md](docs/README.md) | 文档索引 + 统一术语表（MK / KEK / FSK / FSKey / Chunk…） |
| docs/01–04 | 整体架构、核心引擎、前端设计与展示形式 |
| docs/05-01～05-06 | 六大功能模块详细设计（可含示例代码） |
| docs/06–08 | 威胁模型、密钥轮换与格式迁移、中继与信令协议 |
| [docs/09-开发任务面板.md](docs/09-开发任务面板.md) | 分阶段开发计划与任务状态（P0～P5） |
| [docs/10-Git提交与版本规范.md](docs/10-Git提交与版本规范.md) | 分支模型、Conventional Commits、SemVer |

## 开发流程

- 提交信息遵循 Conventional Commits（`feat/fix/docs/chore/security…`），仓库启用 `commit-msg` 钩子校验（新 clone 后执行 `git config core.hooksPath .githooks`）。
- 版本号 SemVer，里程碑对应：M0→v0.1.0（奠基）、M1→v0.2.0（加密内核）、M2→v0.3.0（保险箱可用）、M3→v0.4.0（双设备同步）、M4→v0.5.0（功能完整 Beta）、M5→v1.0.0（正式版）。
- CI（GitHub Actions）：Flutter analyze/test + Rust fmt/clippy/test 双流水线 + Windows 集成构建（校验 DLL 捆绑）。

## 原型

`prototype/index.html` 为纯静态高保真原型（零依赖，双击即开），是 UI 的交互与视觉验收基准；演示口令 `1234`（真实保险箱）/ `88888888`（伪装空间）。原型中的 SVG 图标库已提取为 Flutter 资产（`app/assets/icons/`）。

---

**私钥与数据安全提示**：主密码不可找回（无后门）；忘记密码 = 数据不可恢复（安全重置流程见 docs/07）。本项目当前处于开发阶段，请勿存放唯一副本的重要数据。
