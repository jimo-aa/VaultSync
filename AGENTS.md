# AGENTS.md — VaultSync 工作区指南

VaultSync：端到端加密的多设备文件同步系统。当前仓库处于**骨架阶段**，由三部分组成：

- `docs/` — 设计文档 V0.2（01–04 整体设计、05-0X 六大模块详细设计、06–08 横切专项：威胁模型 / 密钥轮换 / 中继协议）。入口为 `docs/README.md`，内含统一术语表（Vault、MK、KEK、FSK、FSKey、Chunk、Engine、Relay 等）——**改动任何文档前先读术语表，保持跨文档一致**。
- `prototype/` — 桌面端高保真 HTML+CSS+JS 原型，纯静态零依赖，双击 `index.html` 即可打开。演示口令：`1234`（真实保险箱）、`88888888`（伪装伪空间）。全部为 Mock 数据，不含真实加密逻辑。其 SVG 图标库已提取至 `app/assets/icons/`（53 枚，i- 前缀已去除）。
- `app/` — Flutter 桌面客户端（Windows 已验证）。分层：`lib/core`（主题 Token / 图标）、`lib/state`（Riverpod）、`lib/service`（编排 + 引擎服务，后台 Isolate 调 FFI）、`lib/ffi`（dart:ffi 绑定）、`lib/presentation`（router / pages / shell）。锁屏已对接真实引擎（首启输入即创建保险箱，演示口令已废弃）。
- `native/` — Rust Workspace 七 crate（P3 起新增 `vault-relay` 自建中继服务端 bin）。`vault-crypto` 为唯一密码学原语层（AES-256-GCM / Argon2id / HKDF / CSPRNG / Zeroize）；`vault-core` 为唯一 FFI 门面（keystore VSVB v2 头部、会话、暴力破解防护、keyring 平台安全存储）；`vault-p2p` 承载设备身份 / Noise 信道（snow）/ 配对 / 增量同步 / 冲突 / 远程销毁（P3）。
- FFI 链路：`flutter build windows` 时 CMake POST_BUILD 自动 `cargo build --release` 并把 `vault_core.dll` 拷进产物（`app/windows/runner/CMakeLists.txt`）。P1+P2+P3 冒烟：`cd app && dart run tool/ffi_check.dart`（61 项断言：解锁/冷却/改密/生物识别/导入导出/检索/擦除/分享 + 双设备配对/增量同步/删除同步/误删保护副本/远程销毁）。
- **FFI 约定**：状态码 0=OK/1=密码错误(wait ms)/2=冷却(ms)/3=IO/4=安全存储不可用/5=未绑定生物/6=格式/7=参数/8=内部；会话句柄为不透明指针地址（int），MK 永不出引擎；**每个导出的参数个数不同，typedef 严禁复用**（unlock=5 参、unlock_bio=3 参）。

## 常用命令

```
cd app    && flutter analyze && flutter test && flutter build windows --debug
cd app    && dart run tool/ffi_check.dart          # P1+P2+P3 端到端冒烟（需先 build windows）
cd native && cargo fmt --check && cargo clippy --all-targets && cargo test
cd native && cargo test -p vault-core -- --ignored --nocapture   # 生物识别本机联测（写 Credential Manager）
```

- 严格性由各 crate 源码级 `#![deny(warnings)]` 保证；CI 的 clippy 不带 `-D warnings`（命令行标志会压过源码内 allow，测试代码无法豁免）。
- 提交钩子：仓库已配置 `git config core.hooksPath .githooks`（clone 后需重新执行一次），commit-msg 校验 Conventional Commits（docs/10）。
- CI：`.github/workflows/ci.yml`（flutter / rust 双流水线 + Windows 集成构建）。

另见：

- `docs/09-开发任务面板.md` — 分阶段的开发计划与任务面板。
- `LOG.md` — 开发日志（问题、隐患、决策记录），**新条目一律追加在文件末尾**，附日期。
- `docs/10-Git提交与版本规范.md` — 提交信息格式、分支模型与版本号规则；**所有提交与发版必须遵循**。

## 安全设计红线（改任何代码前必读）

- **零信任**：明文与密钥只允许存在于 Rust 核心引擎层；Dart（展示 / 状态 / 服务层）一律不触碰明文或密钥。
- **密钥体系**：MK 由 CSPRNG 生成且永不变更；密码 / 生物识别只是两把 KEK，用于解封 MK 的 wrapped 副本；文件用 per-file FSKey 加密。详见 `docs/06`、`docs/07`，不要在代码里绕开此体系。
- **加密块 = 同步块 = 传输块**（CDC 分块），局部修改不得触发全文件重加密。
- 全部敏感操作必须写入链式哈希审计日志（`vault-audit`）。
- 中继（Relay）仅转发密文，不落盘、不解密（协议见 `docs/08`）。

## 架构边界

- 层次与调用方向：展示层 → Riverpod 状态层 → Dart 服务层 → FFI 桥 → Rust 核心引擎（`vault-core` / `vault-crypto` / `vault-vault` / `vault-p2p` / `vault-stego` / `vault-audit`）。
- Rust Workspace 依赖单向无环：`vault-crypto`、`vault-audit` 为最底层；`vault-core` 是唯一 FFI 门面（P3 起 `vault-p2p` 亦依赖 `vault-vault` 做同步编排，仍无环，偏差见 LOG）。
- 未来的代码目录规划、里程碑与任务拆分见 `docs/09-开发任务面板.md`。

## 文档与原型约定

- **文档禁止使用 mermaid 图**，一律用文字 / ASCII 描述架构与流程。
- 文档有版本号（当前 V0.2）；重大修订须同步更新 `docs/README.md` 索引表与各文档头部版本行。
- 原型是纯静态、无构建、无依赖的 HTML/CSS/JS，不要引入框架或包管理器；新增交互需与 `docs/03`、`docs/04` 的设计 Token（暗色 `#10131A` + 香槟金 `#D9B25F`）保持一致。
- 原型文件列表交互约定：单击选中、双击打开、右键菜单；所有二级界面用**弹窗（modal），不用滑入抽屉**。
- 原型说明更新后，同步维护 `prototype/README.md` 的页面与交互清单。

## 本地验证

- 原型：直接用浏览器打开 `prototype/index.html`，或 `python -m http.server` 后访问（无 Lint / 测试 / 构建命令）。
- 文档：为 Markdown，无构建；检查项为与 `docs/README.md` 索引及术语表的一致性。
