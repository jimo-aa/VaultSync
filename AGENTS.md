# AGENTS.md — VaultSync 工作区指南

VaultSync：端到端加密的多设备文件同步系统。当前实现进度 **v0.5.0**（里程碑 M4，P0–P4 全部 + P5 大部分落地）；设计基线已从 **V1.0（归档）** 推进到 **V2.0（现行）**。仓库由三部分组成：

- `docs/` — 设计文档，**按代际分目录**。入口 `docs/README.md`（版本索引 + 统一术语表：Vault、MK、KEK、FSK、FSKey、Chunk、Engine、Relay 等）——**改动任何文档前先读术语表，保持跨文档一致**。
  - `docs/v1.0/` — **已冻结归档**（01–04 整体设计、05-0X 六大模块、06–08 横切专项、09 任务面板历史快照、11 威胁模型复核）。**只读，不得就地修改**；文中「未实现 / 待做」表述保持原文供对照取证。
  - `docs/v2.0/` — **现行基线**（同编号同骨架，可左右对照）：多端生产可用 + 把 V1.0 复核出的 12 项「文档声称 > 实现」缺口逐条收口；`docs/v2.0/09-开发任务面板.md` 是**现行活面板**（P6–P9）。
  - `docs/10-Git提交与版本规范.md` — 仓库级协作规范，**不随设计基线归档**。
- `prototype/` — 桌面端高保真 HTML+CSS+JS 原型，纯静态零依赖，双击 `index.html` 即可打开。演示口令：`1234`（真实保险箱）、`88888888`（伪装伪空间）。全部为 Mock 数据，不含真实加密逻辑。其 SVG 图标库已提取至 `app/assets/icons/`（53 枚，i- 前缀已去除）。
- `app/` — Flutter 桌面客户端（Windows 已验证）。分层：`lib/core`（主题 Token / 图标 / `security` 强度策略 / `l10n` 取词桥）、`lib/state`（Riverpod）、`lib/service`（编排 + 引擎服务，后台 Isolate 调 FFI）、`lib/ffi`（dart:ffi 绑定）、`lib/presentation`（router / pages / shell / widgets，自绘无边框标题栏）。锁屏已对接真实引擎（首启输入即创建保险箱，演示口令已废弃）。
- **界面文案（P4 起强制）**：任何用户可见字符串都必须走 `AppLocalizations`（`app/lib/l10n/app_zh.arb` 为模板语言、`app_en.arb` 为译文，改 ARB 后 `flutter gen-l10n`）。状态层 / 服务层不在组件树内，用 `core/l10n/vs_l10n.dart` 的 `VsL10n.orNull?.xxx ?? '中文兜底'`。无障碍与动效同理：图标按钮给语义标签，动效时长用 `core/theme/vs_motion.dart` 的 `VsMotion.ms(context, n)`。
- `native/` — Rust Workspace **七 crate**（含 `vault-relay` 自建中继服务端 bin）。`vault-crypto` 为唯一密码学原语层（AES-256-GCM / Argon2id / HKDF / CSPRNG / Zeroize）；`vault-audit` 为链式哈希审计日志（P5-1，失败不阻断业务）；`vault-stego` 为 PNG LSB 隐写（P5-3，只搬位不做密码学）；`vault-core` 为唯一 FFI 门面（keystore VSVB v2 头部、会话、暴力破解防护、keyring 平台安全存储、审计与隐写编排）；`vault-p2p` 承载设备身份 / Noise 信道（snow）/ 配对 / 增量同步 / 冲突 / 远程销毁（P3）。**V2.0 计划新增 `vault-store`（分段持久化 / 原子提交 / 租约 / 恢复日志）与 `vault-net`（发现 / 打洞 / 路径状态机 / 传输队列）共九成员**——见 `docs/v2.0/02` §三。
- FFI 链路：`flutter build windows` 时 CMake POST_BUILD 自动 `cargo build --release` 并把 `vault_core.dll` 拷进产物（`app/windows/runner/CMakeLists.txt`）。端到端冒烟：`cd app && dart run tool/ffi_check.dart`（**114 项断言 / 54 个 FFI 导出**：解锁/冷却/改密/生物识别/导入导出/检索/擦除/分享/文件夹重命名·标签·递归擦除/审计链写入·校验·加密导出/隐写往返/密钥轮换（文件·文件夹·MK 全库）/本机紧急销毁/更新清单验签·文件摘要 + 双设备配对/增量同步/删除同步/误删保护副本/远程销毁/解绑）。
- **FFI 约定**：状态码 0=OK/1=密码错误(wait ms)/2=冷却(ms)/3=IO/4=安全存储不可用/5=未绑定生物/6=格式/7=参数/8=内部；会话句柄为不透明指针地址（int），MK 永不出引擎；**每个导出的参数个数不同，typedef 严禁复用**（unlock=5 参、unlock_bio=3 参）。唯一例外：`vault_core_vault_delete_folder` 成功返回删除的文件数（≥0），失败返回**负**状态码。**V2.0 扩展码** 9=租约被占（只读降级）/10=维护态（业务写入冻结）/11=需要恢复/12=已取消/13=能力不支持；**注意中继侧有独立错误空间 `R1–R17`，必须显式映射，不得原样返回**（`docs/v2.0/02` §六.6）。

## 常用命令

```
cd app    && flutter analyze && flutter test && flutter build windows --debug
cd app    && dart run tool/ffi_check.dart          # P1+P2+P3 端到端冒烟（需先 build windows）
cd native && cargo fmt --check && cargo clippy --all-targets && cargo test
cd native && cargo test -p vault-core -- --ignored --nocapture   # 生物识别本机联测（写 Credential Manager）
```

- 严格性由各 crate 源码级 `#![deny(warnings)]` 保证；CI 的 clippy 不带 `-D warnings`（命令行标志会压过源码内 allow，测试代码无法豁免）。**已知现状**：`deny(warnings)` 目前只覆盖 4/7 crate（缺 `vault-p2p` / `vault-relay` / `vault-stego`），`unsafe` 只出现在 `vault-core/src/ffi.rs` 但无 `forbid` 约束，`ed25519-dalek` 尚有两个大版本并存——三项均由 P6-7 收口（`docs/v2.0/09`）。
- 提交钩子：仓库已配置 `git config core.hooksPath .githooks`（clone 后需重新执行一次），commit-msg 校验 Conventional Commits（docs/10）。
- CI：`.github/workflows/ci.yml`（flutter / rust 双流水线 + Windows 集成构建）。

另见：

- `docs/v2.0/09-开发任务面板.md` — **现行**开发计划与任务面板（P6–P9，含 V1.0 遗留承接）。
- `LOG.md` — 开发日志（问题、隐患、决策记录），**新条目一律追加在文件末尾**，附日期。
- `docs/10-Git提交与版本规范.md` — 提交信息格式、分支模型与版本号规则；**所有提交与发版必须遵循**。
- `docs/v2.0/11-威胁模型复核.md` — 复核口径与待复核清单；**任务勾选前必须回填证据**（见其 §二 纪律）。

## 安全设计红线（改任何代码前必读）

- **零信任**：明文与密钥只允许存在于 Rust 核心引擎层；Dart（展示 / 状态 / 服务层）一律不触碰明文或密钥。
- **密钥体系**：MK 由 CSPRNG 生成且永不变更；密码 / 生物识别只是两把 KEK，用于解封 MK 的 wrapped 副本；文件用 per-file FSKey 加密。详见 `docs/v2.0/05-01`、`docs/v2.0/07`（上一代对照：`docs/v1.0/05-01`、`docs/v1.0/07`），不要在代码里绕开此体系。**V2.0 起新增从属密钥层**（索引/搜索/分享/订单/隐写载荷/链键/审计导出/发现指纹盐，独立包装于 `VSVB v3` 头部）——**不得**再从 MK 直接派生这些功能密钥。
- **加密块 = 同步块 = 传输块**（CDC 分块），局部修改不得触发全文件重加密。
- 全部敏感操作必须写入链式哈希审计日志（`vault-audit`）：落点在 `vault-core` 的 FFI 导出层（唯一持 MK 的层），新增导出务必同步补 `audit_op` / `session.audit`。**审计写入失败不阻断业务**（事后取证 vs 可用性的取舍，理由见 LOG）。
- 中继（Relay）仅转发密文，不落盘、不解密（协议见 `docs/v2.0/08`）。
- **证据先行（V2.0 原则 7）**：任何对外声称的安全能力必须同时有运行时接口与可复现证据；没有接口就在 UI 上如实说没有，**禁止「设计保证」式的对外承诺**。V1.0 在此吃过亏（`docs/v1.0/11` 与 `docs/v2.0/11` §三 已逐条登记）。

## 架构边界

- 层次与调用方向：展示层 → Riverpod 状态层 → Dart 服务层 → FFI 桥 → Rust 核心引擎（`vault-core` / `vault-crypto` / `vault-vault` / `vault-p2p` / `vault-stego` / `vault-audit`）。
- Rust Workspace 依赖单向无环：`vault-crypto`、`vault-audit` 为最底层；`vault-core` 是唯一 FFI 门面（P3 起 `vault-p2p` 亦依赖 `vault-vault` 做同步编排，仍无环，偏差见 LOG）。**V2.0 计划**：`vault-audit` 改为依赖新的 `vault-store`，`vault-p2p` 的传输细节下沉到 `vault-net`（`docs/v2.0/02` §三）。
- 未来的代码目录规划、里程碑与任务拆分见 `docs/v2.0/09-开发任务面板.md`（V1.0 的历史面板冻结在 `docs/v1.0/09`）。

## 文档与原型约定

- **文档禁止使用 mermaid 图**，一律用文字 / ASCII 描述架构与流程。
- 文档**按设计基线分目录**并各有版本号（现行 **V2.0**，归档 **V1.0**）；重大修订须同步更新 `docs/README.md` §一/§三/§四 与相应版本 `README` 的版本记录。**`docs/v1.0/` 只读**——设计修订一律落到 `docs/v2.0/`。
- 文档内引用约定（`docs/v2.0/README` §九）：同版本内写 `` `05-02` `` 或 `v2.0/05-02`；引用上一代写 `v1.0/05-02`；仓库级写 `docs/10`、`LOG.md`、`AGENTS.md`。**不得出现裸的 `docs/06` 形式**。
- 新增能力必须写清四项要素（接口 / 数据格式 / 失败语义 / 证据），并在 `docs/v2.0/01` §六.13 索引表登记。
- 原型是纯静态、无构建、无依赖的 HTML/CSS/JS，不要引入框架或包管理器；新增交互需与 `docs/v1.0/03`、`docs/v1.0/04` 的设计 Token（暗色 `#10131A` + 香槟金 `#D9B25F`）保持一致（Flutter 端的新 Token 见 `docs/v2.0/03` §五）。
- 原型文件列表交互约定：单击选中、双击打开、右键菜单；所有二级界面用**弹窗（modal），不用滑入抽屉**。
- 原型说明更新后，同步维护 `prototype/README.md` 的页面与交互清单。

## 本地验证

- 原型：直接用浏览器打开 `prototype/index.html`，或 `python -m http.server` 后访问（无 Lint / 测试 / 构建命令）。
- 文档：为 Markdown，无构建；检查项为与 `docs/README.md` 索引及术语表的一致性。
