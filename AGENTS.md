# AGENTS.md — VaultSync 工作区指南

VaultSync：端到端加密的多设备文件同步系统。当前实现进度见 `LOG.md` 末尾条目与 git tag（M4–M7 已交付）；设计基线已从 **V1.0（归档）** → **V2.0** 推进到 **V3.0（现行·增量基线）**。仓库由三部分组成：

- `docs/` — 设计文档，**按代际分目录**。入口 `docs/README.md`（版本索引 + 统一术语表：Vault、MK、KEK、FSK、FSKey、Chunk、Engine、Relay 等）——**改动任何文档前先读术语表，保持跨文档一致**。
  - `docs/v1.0/` — **已冻结归档**（01–04 整体设计、05-0X 六大模块、06–08 横切专项、09 任务面板历史快照、11 威胁模型复核）。**只读，不得就地修改**；文中「未实现 / 待做」表述保持原文供对照取证。
  - `docs/v2.0/` — **被继承基线**（同编号同骨架，可左右对照）：多端生产可用 + 把 V1.0 复核出的 12 项「文档声称 > 实现」缺口逐条收口。V3.0 未收录的章节（`01`–`04`、`05-01…05-06`、`06`–`08`、`12`、`13`）一律沿用本目录；`docs/v2.0/09-开发任务面板.md` 上的 **P8/P9 未完成项继续在此勾选，不因 V3.0 而冻结**。
  - `docs/v3.0/` — **现行 · 增量基线**：**只收录本代新增 / 修订的文档**，其余章节沿用 `docs/v2.0/`（这一点与 V1.0/V2.0 的「全量基线」不同，读 `docs/v3.0/` 时遇到 `01`/`02`/`06` 这类编号指的都是 `v2.0/` 下的同名文件）。本代只做一项能力：**透明加密文件系统**（把保险箱挂载成透明解密的目录树供 IDEA / git 等外部应用读写）。四份文档：`README.md`（增量说明 + 四类落点 + **登记回填门禁**）、`05-07-透明加密文件系统.md`（**唯一设计依据**）、`09-开发任务面板.md`（**现行活面板** P10–P12）、`11-威胁模型复核.md`（七档判定，含 `⏸ 待前置`）。
  - `docs/10-Git提交与版本规范.md` — 仓库级协作规范，**不随设计基线归档**。
- `prototype/` — 多端高保真 HTML+CSS+JS 原型，纯静态零依赖，双击 `index.html` 即可打开（版本索引页）。**按设计基线分目录**：`prototype/v1.0/` 已冻结只读（对应 `docs/v1.0/`）；`prototype/v2.0/` 为**现行 · 桌面端（≥1280 档）**；`prototype/mobile/` 为**现行 · 移动端（<720 档）**。两代/两端均为 `assets/` 分层样式 + `js/` 内核·原语·数据·按域视图 + 外壳路由，**刻意不用 ES Module**（否则 `file://` 双击即失效）。`mobile/` **不复制**内核，直接引用 `../v2.0/js/{core,ui,data}.js` —— 即产品原则「一套内核、受管外壳」在原型工程上的投影，保证两端共享同一份错误码文案出口与 Design Token。演示口令：`1234`（真实保险箱）、`88888888`（伪装伪空间）。全部为 Mock 数据，不含真实加密逻辑。其 SVG 图标库已提取至 `app/assets/icons/`（53 枚，i- 前缀已去除）。
- `app/` — Flutter 桌面客户端（Windows 已验证）。分层：`lib/core`（主题 Token / 图标 / `security` 强度策略 / `l10n` 取词桥）、`lib/state`（Riverpod）、`lib/service`（编排 + 引擎服务，后台 Isolate 调 FFI）、`lib/ffi`（dart:ffi 绑定）、`lib/presentation`（router / pages / shell / widgets，自绘无边框标题栏）。锁屏已对接真实引擎（首启输入即创建保险箱，演示口令已废弃）。
- **界面文案（P4 起强制）**：任何用户可见字符串都必须走 `AppLocalizations`（`app/lib/l10n/app_zh.arb` 为模板语言、`app_en.arb` 为译文，改 ARB 后 `flutter gen-l10n`）。状态层 / 服务层不在组件树内，用 `core/l10n/vs_l10n.dart` 的 `VsL10n.orNull?.xxx ?? '中文兜底'`。无障碍与动效同理：图标按钮给语义标签，动效时长用 `core/theme/vs_motion.dart` 的 `VsMotion.ms(context, n)`。
- `native/` — Rust Workspace **九 crate**（含 `vault-relay` 自建中继服务端 bin，**VSR2**：token 挑战-应答 + 三类会话上限 + 匿名计量，配置经 stdin 读入）。`vault-crypto` 为唯一密码学原语层（AES-256-GCM / Argon2id / HKDF / CSPRNG / Zeroize）；`vault-store` 为分段持久化层（`VSSG v1` 段 / 原子提交 / 恢复日志 / 单写者租约 / 压实，**已落地**，最底层之一）；`vault-audit` 为链式哈希审计日志（分段 `VSAU v1`，失败不阻断业务）；`vault-net` 为传输抽象层（优先级队列 / `.part` 续传 / 填充帧层 / mDNS 发现 / 打洞原语 / 路径降级，**已落地**）；`vault-stego` 为 PNG LSB 隐写（只搬位不做密码学）；`vault-vault` 为容器 / 索引 / 原地编辑 / 擦除 / 检索；`vault-core` 为唯一 FFI 门面（keystore `VSVB v3` 头部、会话、暴力破解防护、keyring 平台安全存储、审计与隐写编排）；`vault-p2p` 承载设备身份 / Noise 信道（snow）/ 配对 / 增量同步 / 冲突与块级合并 / 选择性同步策略 / 远程销毁 / 轮换传播 / **VSR2 中继客户端** / **阅后即焚（`burn`：一次性票据 + 自动擦除）**。**V3.0 计划新增 `vault-vfs`（透明加密文件系统：FUSE 语义内核 + 单模块 Windows 宿主绑定）**——见 `docs/v3.0/05-07` §三 与 `docs/v2.0/02` §三。
- FFI 链路：`flutter build windows` 时 CMake POST_BUILD 自动 `cargo build --release` 并把 `vault_core.dll` 拷进产物（`app/windows/runner/CMakeLists.txt`）。端到端冒烟：`cd app && dart run tool/ffi_check.dart`（**366 项断言 / 94 个 `vault_core_*` 导出**：解锁/冷却/改密/生物识别/导入导出/**原地编辑（P8-8：rev 递增 + 同一 id 可导）**/检索 V2/擦除强度分级/分享/文件夹重命名·标签·递归擦除/审计链写入·校验·加密导出/隐写往返/**跨图分割与容量协商（P8-10：plan fits/不足如实拒绝/乱序提取/缺片失败）**/密钥轮换（文件·文件夹·MK 全库·轮换会话三件套·传播）/本机紧急销毁/更新清单验签·文件摘要/迁移三件套/缩略图/任务与事件流 + 双设备配对/增量同步/删除同步/误删保护副本/远程销毁·远程锁定/解绑 + **P8 中继 VSR2 端到端**（spawn `vault-relay.exe`，含 `[observe]` UDP 观测端点）+ **选择性同步策略（P8-7：读取默认/非法原子拒绝/往返保真/上下文上报）** + **阅后即焚（P8-9：能力位 bit13 / 票据 16 字符 / 非法码拒 / 单次性 / 直连投递 / 打开内容一致 / 超时自动擦除 / 显式擦除幂等）** + **打洞（P8-4：观测端点就绪 / 能力位 bit6 / 双向证实打通 / 候选非空 / 非对称 / 状态机 `punched` / 不泄露地址 / `path` 仍如实为 relay / **两端探针 `ok=true` 且角色如实**）** + **中继收尾（P8-5：`/stats.json` 5 min 聚合与 401 门禁 / `/healthz` uptimeS / 带宽桶 R13 降速不丢字节 / 中继设置原子拒绝与往返保真 / 禁用官方中继候选不拨号且 `relay_unavailable` / 自建条目候选顺序同步端到端）**））。**打洞探针另有命令行入口**：`cd app && dart run tool/nat_probe.dart --relay <host:port> --room <room> --token <tok> --code <shared> --role initiator|responder`（NAT 矩阵每格的执行入口，见 `docs/v2.0/14` §七）。
- **FFI 约定**：状态码 0=OK/1=密码错误(wait ms)/2=冷却(ms)/3=IO/4=安全存储不可用/5=未绑定生物/6=格式/7=参数/8=内部；会话句柄为不透明指针地址（int），MK 永不出引擎；**每个导出的参数个数或类型不同，typedef 严禁复用**（unlock=5 参、unlock_bio=3 参）。唯一例外：`vault_core_vault_delete_folder` 成功返回删除的文件数（≥0），失败返回**负**状态码。**V2.0 扩展码** 9=租约被占（只读降级）/10=维护态（业务写入冻结）/11=需要恢复/12=已取消/13=能力不支持/14=未授权（能力存在但许可证不含该 `feat_*`，与 13 严格区分）；**V3.0 计划扩展码** 15=挂载失败/16=挂载卷未解锁/17=文件操作不受支持（宁可拒绝，不静默损坏）。**注意中继侧有独立错误空间 `R1–R17`，必须显式映射，不得原样返回**（`docs/v2.0/02` §六.6）。

## 常用命令

```
cd app    && flutter analyze && flutter test && flutter build windows --debug
cd app    && dart run tool/ffi_check.dart          # P1–P7 端到端冒烟（需先 build windows；不覆盖挂载，理由见 docs/v3.0/11 §五）
cd native && cargo fmt --check && cargo clippy --all-targets && cargo test
cd native && cargo test -p vault-core -- --ignored --nocapture   # 生物识别本机联测（写 Credential Manager）
```

- 严格性由各 crate 源码级 `#![deny(warnings)]` 保证（**现状：8/8 crate 已覆盖**，P6-7 收口）；CI 的 clippy 不带 `-D warnings`（命令行标志会压过源码内 allow，测试代码无法豁免）。`unsafe` 目前只出现在 `vault-core/src/ffi.rs`（`vault-core` 是唯一未加 `#![forbid(unsafe_code)]` 的 crate）；**V3.0 新增 `vault-vfs` 时，其 `unsafe` 只允许出现在 `src/host/windows.rs` 单模块**（`docs/v3.0/05-07` §3.1）。
- 提交钩子：仓库已配置 `git config core.hooksPath .githooks`（clone 后需重新执行一次），commit-msg 校验 Conventional Commits（docs/10）。
- CI：`.github/workflows/ci.yml`（flutter / rust 双流水线 + Windows 集成构建）。**CI 无法承载挂载类验收**（托管 runner 装不了内核驱动），因此 CI 绿不构成挂载可用的证据（`docs/v3.0/11` §五）。

另见：

- `docs/v3.0/09-开发任务面板.md` — **现行活面板**：P10–P12（透明加密文件系统 = 挂载）。**本代只做这一项能力。**
- `docs/v3.0/05-07-透明加密文件系统.md` — 本代唯一设计依据；含四项要素索引（§15.1）与 **26 项登记回填点**（§15.2，其中 8 组为交付门禁）。
- `docs/v2.0/09-开发任务面板.md` — P6–P9（P8/P9 未完成项**继续在此勾选**，不因 V3.0 而冻结）。
- `docs/v2.0/14-打洞NAT矩阵验收手册.md` — P8-4 打洞 NAT 矩阵的**运维/验收手册**（判定算法、环境方案与保真度边界、netns 实验室与自检、探针驱动、矩阵表模板、回填清单）。**可判定矩阵至少需两个公网 IP**；单机 netns 只能做流程预演，**不得**据此宣布达标。
- `LOG.md` — 开发日志（问题、隐患、决策记录），**新条目一律追加在文件末尾**，附日期。
- `docs/10-Git提交与版本规范.md` — 提交信息格式、分支模型与版本号规则；**所有提交与发版必须遵循**。
- `docs/v2.0/11-威胁模型复核.md` — 上一代复核口径与待复核清单；**任务勾选前必须回填证据**（见其 §二 纪律）。本代的复核在 `docs/v3.0/11`。

## 安全设计红线（改任何代码前必读）

- **零信任**：明文与密钥只允许存在于 Rust 核心引擎层；Dart（展示 / 状态 / 服务层）一律不触碰明文或密钥。**V3.0 修订（必读）**：引入挂载后，明文还允许存在于**已解锁挂载卷的 I/O 路径**上——即挂载解锁窗口内，本机同账户的任意进程都能读到被挂载子树的明文。这是**有意的能力扩张**，条件与证据见 `docs/v3.0/05-07` §11.1（A1 修订）与 §11.3（E13–E18）；`v2.0/06` 的旧措辞「引擎进程内存是唯一明文边界」**不再成立**（回填前不得声称）。**P8-9 阅后即焚的同类例外（已登记）**：对端「打开」会在本机写出**受控临时明文** `<data_dir>/burn/tmp/<id>.part`（Unix `0600`，Windows 未做 ACL 收紧），窗口 = `openTimeoutSecs`，到期自动擦除（临时明文 + 容器 + 内存密钥）。**P8-4 打洞的边界（已登记）**：候选（`ip:port`，≤ 8）经 **Noise 信道**与已认证对端交换；中继的 UDP 观测端点**只回请求方自己的地址**，非观测请求一律丢弃；`path_status` 的 `punch` 段**只有计数与原因、无 IP**；打通后数据**仍走本会话承载**（可靠 datagram 层未落地），故 `path` 不谎报 `hole_punch`。
- **密钥体系**：MK 由 CSPRNG 生成且永不变更；密码 / 生物识别只是两把 KEK，用于解封 MK 的 wrapped 副本；文件用 per-file FSKey 加密。详见 `docs/v2.0/05-01`、`docs/v2.0/07`（上一代对照：`docs/v1.0/05-01`、`docs/v1.0/07`），不要在代码里绕开此体系。**V2.0 起新增从属密钥层**（索引/搜索/分享/订单/隐写载荷/链键/审计导出/发现指纹盐，独立包装于 `VSVB v3` 头部）——**不得**再从 MK 直接派生这些功能密钥。
- **加密块 = 同步块 = 传输块**（CDC 分块），局部修改不得触发全文件重加密。
- 全部敏感操作必须写入链式哈希审计日志（`vault-audit`）：落点在 `vault-core` 的 FFI 导出层（唯一持 MK 的层），新增导出务必同步补 `audit_op` / `session.audit`。**审计写入失败不阻断业务**（事后取证 vs 可用性的取舍，理由见 LOG）。
- 中继（Relay）仅转发密文，不落盘、不解密（协议见 `docs/v2.0/08`）。
- **证据先行（V2.0 原则 7）**：任何对外声称的安全能力必须同时有运行时接口与可复现证据；没有接口就在 UI 上如实说没有，**禁止「设计保证」式的对外承诺**。V1.0 在此吃过亏（`docs/v1.0/11` 与 `docs/v2.0/11` §三 已逐条登记）。

## 架构边界

- 层次与调用方向：展示层 → Riverpod 状态层 → Dart 服务层 → FFI 桥 → Rust 核心引擎（`vault-core` / `vault-crypto` / `vault-store` / `vault-vault` / `vault-p2p` / `vault-stego` / `vault-audit`）。
- Rust Workspace 依赖单向无环：`vault-crypto` 与 `vault-store` 为最底层且互不依赖；`vault-core` 是唯一 FFI 门面（`vault-p2p` 亦依赖 `vault-vault` 做同步编排，仍无环，偏差见 LOG）。**现状**：`vault-store` 已落地并被 `vault-audit` / `vault-vault` / `vault-core` 依赖；V2.0 计划中的 `vault-net`（传输细节下沉）**尚未落地**。**V3.0 计划**：新增 `vault-vfs`（`vault-vfs → vault-vault / vault-store / vault-audit / vault-crypto`，矩阵保持严格下三角）——见 `docs/v3.0/05-07` §三 与 `docs/v2.0/02` §三。
- 未来的代码目录规划、里程碑与任务拆分见 `docs/v3.0/09-开发任务面板.md`（本代）与 `docs/v2.0/09`（P8/P9 未完成项）；V1.0 的历史面板冻结在 `docs/v1.0/09`。

## 文档与原型约定

- **文档禁止使用 mermaid 图**，一律用文字 / ASCII 描述架构与流程。
- 文档**按设计基线分目录**并各有版本号（现行 **V3.0**〔增量基线，只收录新增/修订文档〕、被继承 **V2.0**、归档 **V1.0**）；重大修订须同步更新 `docs/README.md` §一/§三/§四 与相应版本 `README` 的版本记录。**`docs/v1.0/` 只读**——修订既有声明落到 `docs/v2.0/`，本代新增落到 `docs/v3.0/`。
- **增量基线的回填纪律（V3.0 起）**：在 `docs/v3.0/` 写清新设计**不等于**完成——凡修订 V2.0 的既有声明（如明文边界 A1、新增不变量与边界声明），必须按 `docs/v3.0/05-07` §15.2 回填到被继承文档，否则任务不得勾选（`docs/v3.0/README.md` §三）。
- 文档内引用约定：V3.0 语境下**引用被继承章节一律写全称**（`v2.0/01`、`v2.0/06`、`v2.0/05-02`）——因为 `05-02` 在本代指 V2.0 的那份文件，简写会引起歧义；本代自有文档写 `05-07` 或 `v3.0/05-07`；仓库级写 `docs/10`、`LOG.md`、`AGENTS.md`。**不得出现裸的 `docs/06` 形式**。
- 新增能力必须写清四项要素（接口 / 数据格式 / 失败语义 / 证据）；本代的索引表在 `docs/v3.0/05-07` §15.1，上一代的在 `docs/v2.0/01` §六.13。
- 原型是纯静态、无构建、无依赖的 HTML/CSS/JS，不要引入框架或包管理器，**也不要使用 ES Module**（`file://` 下会被 CORS 拦截，双击即失效）；新增交互需与设计 Token 保持一致——沿用代（`prototype/v1.0/`）为暗色 `#10131A` + 香槟金 `#D9B25F`，现行代（`prototype/v2.0/`）以 `docs/v2.0/03` §五 的 Token V2 为准（亮色 `color.accent = #B98F2E`，并含 `maintenance` / `erase` / `quarantine` / `stale` 四语义色）。
- 原型文件列表交互约定：**桌面端**单击选中、Ctrl/Shift 多选、双击打开、右键菜单；**移动端**单击打开、**长按 = 动作面板**、左滑为等价降级。
- 二级界面不变式：**桌面端一律居中弹窗（modal），不用滑入抽屉**；**移动端一律底部面板（bottom sheet），不用居中弹窗**（即 `prototype/mobile/` 下禁用 `UI.modal`）。移动端详情为**全屏路由**，审计日志为**卡片流**，底部 Tab 固定 5 项（隐写不占 Tab），触控目标 ≥ 44×44。
- 减配如实声明（`docs/v2.0/04` §1.2）：每端被减掉的能力**必须对用户可见**，不得让用户以为「桌面有的手机也有」。移动端清单与呈现方式见 `prototype/mobile/README.md` §五。
- 原型说明更新后，同步维护 `prototype/README.md`（版本索引）与各端的 `prototype/v2.0/README.md`、`prototype/mobile/README.md` 页面与交互清单。

## 本地验证

- 原型：直接用浏览器打开 `prototype/index.html`（版本索引），或 `prototype/v2.0/index.html`（桌面端）、`prototype/mobile/index.html`（移动端）；或 `python -m http.server --directory prototype` 后访问（无 Lint / 测试 / 构建命令）。**原型目录按代际/端分目录**：`prototype/v1.0/` 只读，新增与修订一律落到 `prototype/v2.0/` 或 `prototype/mobile/`。
- 文档：为 Markdown，无构建；检查项为与 `docs/README.md` 索引及术语表的一致性。

## Agent skills

### Issue tracker

Issue 存放在本仓库的 GitHub Issues（`jimo-aa/VaultSync`），一律用 `gh` CLI 操作。See `docs/agents/issue-tracker.md`.

### Triage labels

五个 canonical triage 角色沿用默认标签名（`needs-triage` / `needs-info` / `ready-for-agent` / `ready-for-human` / `wontfix`），类别标签用 GitHub 默认的 `bug` / `enhancement`。See `docs/agents/triage-labels.md`.

### Domain docs

single-context：仓库根目录一份 `CONTEXT.md`，架构决策放 `docs/adr/`。See `docs/agents/domain.md`.
