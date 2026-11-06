; PhotoCAT Windows 설치 프로그램 (Inno Setup 6). package-cat.ps1 이 /D 값을 넘겨 컴파일한다.
; 사용자별 설치라 관리자 권한이 필요 없고, 자체 업데이트가 같은 설치 프로그램을 조용히 실행한다.
; 환경설정은 %APPDATA%\Photocraft 에 있어 설치·업데이트·제거가 건드리지 않는다.

[Setup]
; AppId 는 바꾸지 않는다: 바꾸면 업데이트가 아니라 별도 앱으로 설치된다.
AppId={{7E2B6F4A-3C1D-4A8E-9B57-2D6C0F81A3E4}
AppName=PhotoCAT
AppVersion={#AppVersion}
AppVerName=PhotoCAT {#AppVersion}
AppPublisher=zzamjak-cloud
AppPublisherURL=https://github.com/zzamjak-cloud/PhotoCraft-CAT
DefaultDirName={autopf}\PhotoCAT
DefaultGroupName=PhotoCAT
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
WizardStyle=modern
Compression=lzma2
SolidCompression=yes
ChangesAssociations=yes
SetupIconFile={#RepoRoot}\assets\app-icon\photocraft.ico
UninstallDisplayIcon={app}\PhotoCAT.exe
OutputDir={#OutputDir}
OutputBaseFilename=photocraft-cat-{#AppVersion}-windows-x64-setup

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "korean"; MessagesFile: "compiler:Languages\Korean.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"

[Files]
Source: "{#StageDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs

[Icons]
Name: "{autoprograms}\PhotoCAT"; Filename: "{app}\PhotoCAT.exe"
Name: "{autodesktop}\PhotoCAT"; Filename: "{app}\PhotoCAT.exe"; Tasks: desktopicon

[Registry]
; 자체 형식(.pcraft)만 기본 앱으로 지정하고, 나머지는 "연결 프로그램" 목록에만 추가한다.
Root: HKA; Subkey: "Software\Classes\PhotoCAT.Document"; ValueType: string; ValueData: "PhotoCAT Document"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\PhotoCAT.Document\DefaultIcon"; ValueType: string; ValueData: "{app}\PhotoCAT.exe,0"
Root: HKA; Subkey: "Software\Classes\PhotoCAT.Document\shell\open\command"; ValueType: string; ValueData: """{app}\PhotoCAT.exe"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.pcraft"; ValueType: string; ValueData: "PhotoCAT.Document"; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.psd\OpenWithProgids"; ValueType: string; ValueName: "PhotoCAT.Document"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.psb\OpenWithProgids"; ValueType: string; ValueName: "PhotoCAT.Document"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.png\OpenWithProgids"; ValueType: string; ValueName: "PhotoCAT.Document"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.jpg\OpenWithProgids"; ValueType: string; ValueName: "PhotoCAT.Document"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.jpeg\OpenWithProgids"; ValueType: string; ValueName: "PhotoCAT.Document"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.webp\OpenWithProgids"; ValueType: string; ValueName: "PhotoCAT.Document"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.tif\OpenWithProgids"; ValueType: string; ValueName: "PhotoCAT.Document"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.tiff\OpenWithProgids"; ValueType: string; ValueName: "PhotoCAT.Document"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.bmp\OpenWithProgids"; ValueType: string; ValueName: "PhotoCAT.Document"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.gif\OpenWithProgids"; ValueType: string; ValueName: "PhotoCAT.Document"; ValueData: ""; Flags: uninsdeletevalue

[Run]
Filename: "{app}\PhotoCAT.exe"; Description: "{cm:LaunchProgram,PhotoCAT}"; Flags: nowait postinstall skipifsilent
