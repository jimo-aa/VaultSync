# VaultSync 开发日志（LOG.md）

> 记录开发过程中的日志、问题、隐患与关键决策。**新条目一律追加到文件末尾**，每条带日期；当前活跃隐患在解决后标注 `[已解决 日期]`。

---

## 2026-09-16 ～ 2026-09-17 · 设计与原型阶段

- 完成设计文档 V0.2（01–08），修复 V0.1 评审发现的密钥层级缺陷：MK 不再由密码派生，改为 CSPRNG 生成 + 双 KEK 包装（密码 / 生物识别），永不依赖硬件安全区。
- 完成桌面端高保真原型（`prototype/`），作为后续 Flutter 实现的交互与视觉验收基准。
- 遗留隐患：原型中"同步详情抽屉"与既定"只用弹窗、不用滑入抽屉"的 UI 约定冲突，Flutter 实现阶段须改为弹窗形式。
- 遗留隐患：`docs/03` 设计 Token 与原型 CSS 之间为手工同步，未做自动化校验，实现阶段需以 `docs/03` 为准逐项核对。
- 决策：采用官方公共中继（用户可自建），中继仅转发密文；详见 `docs/08`。
- 决策：桌面端（Windows）优先，移动端随后。

## 2026-09-17 · Flutter 工程骨架搭建（P0-3 Flutter 侧）

- 完成 `app/` 分层骨架：`core/`（Design Token 主题 + SVG 图标封装）、`state/`（Riverpod 会话状态）、`service/`（编排层占位）、`ffi/`（FFI 桥占位）、`presentation/`（go_router + 锁屏 + 主壳导航 + 六模块占位页）。`flutter analyze` 无告警，2 个测试通过，Windows debug 构建成功。
- 从 `prototype/index.html` 的 SVG symbol 库提取 53 枚图标到 `app/assets/icons/`（i- 前缀去除，如 `#i-vault` → `vault.svg`），随主题着色。
- 隐患：锁定守卫当前仅在 Dart 层内存中生效，演示口令（1234 / 88888888）为前端硬编码桩——**不具备任何安全性**，P1 对接真实引擎后必须移除。
- 隐患：riverpod 已装到 3.x，网上部分教程/示例仍为 2.x API（`StateNotifier` 等），写状态层时以 3.x `Notifier` 为准。
- 隐患：pubspec 中 intl 用 `any` 以兼容 flutter_localizations（SDK 固定 intl 0.20.2），后续升级 intl 时须与 Flutter SDK 版本联动。
- 版本号按《Git 提交与版本规范》设为 0.1.0+1（M0 里程碑目标 v0.1.0）。

## 2026-09-17 · P0 完成：Rust Workspace + FFI 打通 + CI + 提交钩子

- `native/` Rust Workspace 六 crate 就位（vault-core/crypto/vault/p2p/stego/audit），依赖边按 docs/01 单向无环；P0 为纯 std 骨架，每 crate 带自检测试。`cargo fmt/clippy/test` 全绿。
- FFI（P0-4）：手写 dart:ffi（弃用 flutter_rust_bridge——P0 仅需 2 个自检接口，FRB 的代码生成链路收益为负，P1 接口变复杂后可再评估）。`vault_core_hello()` 返回 0、`vault_core_version()` 返回 "0.1.0"，`tool/ffi_check.dart` 独立验证通过。
- 踩坑记录（CMake×cargo×MSBuild 三连）：
  1. `add_custom_command(TARGET app POST_BUILD)` 必须写在 runner/CMakeLists.txt（app 目标的定义目录），写在顶层报 "TARGET 'app' was not created in this directory"；
  2. VS 生成器对自定义目标忽略 `WORKING_DIRECTORY`、对 POST_BUILD 会生成 `cd` 但路径算错时静默跑错目录——最终以正确变量 + POST_BUILD `WORKING_DIRECTORY` 落定；
  3. `NATIVE_WORKSPACE_DIR` 一度定义在 `add_subdirectory(runner)` 之后，runner 作用域取到空值，衍生三种表象各异的失败。教训：**跨目录 CMake 变量必须在 add_subdirectory 之前定义**。
- 隐患：FFI 桥在原生库加载失败时静默回退 Stub（为 CI 无 DLL 场景保留），P1 起必须在生产路径改为硬失败，防止"看似在用引擎实为桩"。
- 隐患：`vault_core_version` 返回的 C 字符串需调用方 free，Dart 侧已用 try/finally 保证；后续新增返回指针的接口必须逐一配对释放函数。
- 隐患：CI 的 windows-desktop job 尚未实际运行过（push 后首跑才可验证），GitHub Actions 的 cargo/MSBuild 环境与本机差异可能暴露新问题。
- P0-5/P0-6：CI 双流水线 + Windows 集成构建已写入 `.github/workflows/ci.yml`；`.githooks/commit-msg` 已启用并通过正/反用例自测。
- 决策：P0-6 钩子用 POSIX sh 实现（commitlint 需引入 Node 工具链，对本仓库收益为负）； `.mimosa/`、`native/target/` 已加入根 `.gitignore`。
