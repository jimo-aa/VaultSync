; VaultSync Windows 安装包脚本（Inno Setup 6）
;
; 用法：
;   1) flutter build windows --release           ; 产出 build\windows\x64\runner\Release\
;   2) iscc packaging\windows\VaultSync.iss      ; 产出 VaultSync-Setup-<版本>.exe
;
; 说明：本仓库**不**把安装包产物纳入版本控制（见 .gitignore）；此脚本只固化打包配方，
; 便于在任何装有 Inno Setup 6 的机器上得到可复现的安装包。

#define AppName "VaultSync"
; 版本号与 app/pubspec.yaml 的 version 保持一致（发版时同步递增）
#define AppVersion "0.5.0"
#define AppPublisher "VaultSync"
#define AppExeName "app.exe"
#define ReleaseDir "..\..\app\build\windows\x64\runner\Release"

[Setup]
AppId={{8F3C1A24-7B6E-4C51-9E7A-2D1F5B9C4A18}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher={#AppPublisher}
DefaultDirName={autopf}\{#AppName}
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
; 桌面端应用按用户安装，避免要求管理员权限
PrivilegesRequired=lowest
OutputBaseFilename={#AppName}-Setup-{#AppVersion}
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
; 与 app/windows/runner 的窗口尺寸下限保持一致（1024×680）
MinVersion=10.0

[Languages]
Name: "chinese"; MessagesFile: "compiler:Default.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}";

[Files]
Source: "{#ReleaseDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\{#AppName}"; Filename: "{app}\{#AppExeName}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExeName}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#AppExeName}"; Description: "{cm:LaunchProgram,{#AppName}}"; Flags: nowait postinstall skipifsilent

[UninstallDelete]
; 卸载即删除程序文件；**不动用户保险箱数据**（数据位于 %APPDATA%\VaultSync，
; 由应用自身管理，需经应用内「紧急销毁 / 安全擦除」流程处理——见 docs/05-06 §五）
Type: filesandordirs; Name: "{app}"
