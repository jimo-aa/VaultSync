# VaultSync 打包分发（P5-7）

> 定位：固化打包配方，保证任何装有对应工具链的机器上可复现产物。
> 产物**不纳入版本控制**（见根 `.gitignore` 的 `packaging/out/`）。

## 一、Windows

```bash
# 1) 发布构建（同时经 CMake POST_BUILD 触发 cargo build --release 并捆绑 vault_core.dll）
cd app && flutter build windows --release

# 2) 安装包（需 Inno Setup 6，脚本见 packaging/windows/VaultSync.iss）
iscc packaging/windows/VaultSync.iss
# 产物：packaging/windows/Output/VaultSync-Setup-<版本>.exe
```

- 绿色版：直接分发 `app/build/windows/x64/runner/Release/` 整个目录（含 `data/`、各插件
  DLL 与 `vault_core.dll`），双击 `app.exe` 即可运行。
- 安装包脚本按用户级安装（`PrivilegesRequired=lowest`），不要求管理员权限；
  卸载**不触碰**用户保险箱数据（数据由应用自身管理，擦除须走应用内流程）。
- 版本号：`VaultSync.iss` 的 `AppVersion` 与 `app/pubspec.yaml` 的 `version` 必须同步递增
  （规则见 `docs/10-Git提交与版本规范.md` §三）。

## 二、macOS（**当前环境无法产出**）

```bash
cd app && flutter build macos --release
# 产物：build/macos/Build/Products/Release/app.app
codesign --deep --force --options runtime --sign "Developer ID Application: <团队>" <app>
xcrun notarytool submit <zip> --keychain-profile <profile> --wait   # 公证
hdiutil create -volname VaultSync -srcfolder <app> -ov VaultSync.dmg
```

未完成项：**本仓库当前无 macOS 构建机，`macos/` 目录从未编译验证过**（P0 起即如此）。
首次在 Mac 上构建需要：Xcode 命令行工具、Flutter macOS target、`window_manager` 的
macOS 插件（已声明）、以及把 `vault_core.dylib` 捆绑进 `app.app/Contents/Frameworks`
（当前 CMake 只在 Windows 侧做了 POST_BUILD 捆绑，macOS 需补一段等效脚本）。

## 三、自动更新通道（未实现 · 需基础设施决策）

`docs/09` 的 P5-7 含「自动更新通道」。当前**没有任何实现**，且它需要先定以下决策
（属产品/基础设施范围，不宜由实现方单方面决定）：

1. 更新源形态：自建静态文件服务（清单 JSON + 差分包）vs GitHub Releases；
2. 签名/校验：安装包签名链（Windows Authenticode / macOS Developer ID）由谁持有；
3. 更新语义：增量（bsdiff/按 CDC 块）还是整包替换；静默还是提示；
4. 与「零信任」的边界：更新器本身不接触保险箱数据，只需校验安装包签名——但**清单签名
   密钥的保管**需要与 `docs/07` 的密钥体系分开设计。

在上述决策落定前，本项目只发布**手动下载安装**的安装包。
