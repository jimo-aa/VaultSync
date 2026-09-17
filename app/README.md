# VaultSync 桌面端应用（Flutter）

桌面优先的 Flutter 客户端骨架（Windows 验证，macOS/Linux 随后），UI 依据 `docs/03`/`docs/04` 与 `prototype/` 原型。

## 运行

```
flutter pub get
flutter run -d windows
```

## 目录结构

```
lib/
├── core/
│   ├── icons/app_icon.dart      # 原型 SVG 图标资产封装（assets/icons/，53 枚）
│   └── theme/                   # Design Token（docs/03 §5）+ 亮/暗主题
├── state/                       # Riverpod 状态层（session_controller 等）
├── service/                     # 服务层（仅编排，禁止触碰明文/密钥）
├── ffi/                         # FFI 桥占位（P0-4 对接 vault-core）
└── presentation/
    ├── router/                  # go_router（锁屏守卫）
    ├── pages/lock_screen.dart   # 锁屏（演示口令 1234 / 88888888）
    └── shell/home_shell.dart    # 主壳导航 + 各模块占位页
```

## 依赖

flutter_riverpod、go_router、flutter_svg、window_manager、shared_preferences、path_provider、intl / flutter_localizations。

> 安全红线：Dart 各层不得触碰明文与密钥；加解密一律在 Rust 核心引擎（见 AGENTS.md、docs/01）。
