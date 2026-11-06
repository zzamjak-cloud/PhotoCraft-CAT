<#
.SYNOPSIS
  PhotoCAT 포크의 Windows(x64) 포터블 zip 을 만든다. 코드 서명은 하지 않는다.

    dist/release/photocraft-cat-<버전>-windows-x64-portable.zip
      photocraft-cat.exe + portable.txt + 라이선스 문서 (zip 루트에 바로 위치)

  portable.txt 가 exe 옆에 있으면 설정·프리셋·복구 파일을 같은 폴더의 PhotoCraftData 에 둔다.
  새 버전 zip 을 같은 폴더에 덮어 풀면 설정이 유지된다. 실행 파일 이름은 craft-update 의
  UPDATE_APP.id(photocraft-cat) 와 같아야 한다.

.PARAMETER Version
  릴리스 버전 (예: 0.19.1). 앱이 보고하는 버전도 이 값으로 빌드한다.
#>
param(
  [Parameter(Mandatory = $true)][string]$Version,
  [switch]$SkipBuild
)
$ErrorActionPreference = 'Stop'
$Root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$Target = 'x86_64-pc-windows-msvc'
$AppId = 'photocraft-cat'
$Dist = Join-Path $Root 'dist\release'
New-Item -ItemType Directory -Force $Dist | Out-Null

if (-not $SkipBuild) {
  # MSVC 런타임 정적 링크: VC++ 재배포 패키지 없이 실행된다.
  $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS = '-C target-feature=+crt-static'
  $env:PHOTOCRAFT_OVERRIDE_VERSION = $Version
  Push-Location $Root
  try {
    cargo build --release --locked -p photocraft --target $Target
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed ($LASTEXITCODE)" }
  } finally { Pop-Location }
}

$Exe = Join-Path $Root "target\$Target\release\photocraft.exe"
$Stage = Join-Path $Root "target\windows-package-cat\$AppId-$Version-windows-x64-portable"
if (Test-Path $Stage) { Remove-Item -Recurse -Force $Stage }
New-Item -ItemType Directory -Force $Stage | Out-Null
Copy-Item $Exe (Join-Path $Stage "$AppId.exe")
Copy-Item (Join-Path $PSScriptRoot 'portable.txt') $Stage
foreach ($f in 'README.md', 'LICENSE', 'LICENSE-MIT', 'LICENSE-APACHE', 'NOTICE', 'ATTRIBUTION.md') {
  $p = Join-Path $Root $f
  if (Test-Path $p) { Copy-Item $p $Stage }
}
if ($env:CRAFT_FONTS_DIR) {
  Get-ChildItem (Join-Path $env:CRAFT_FONTS_DIR 'fonts\*\OFL.txt') -ErrorAction SilentlyContinue | ForEach-Object {
    Copy-Item $_.FullName (Join-Path $Stage ("OFL-" + $_.Directory.Name + ".txt"))
  }
}

$Zip = Join-Path $Dist "$AppId-$Version-windows-x64-portable.zip"
if (Test-Path $Zip) { Remove-Item -Force $Zip }
# 폴더 자체가 아니라 내용물을 zip 루트에 넣는다 (자체 업데이트가 폴더 위에 그대로 풀어쓴다).
Compress-Archive -Path (Join-Path $Stage '*') -DestinationPath $Zip
Get-Item $Zip | Format-Table Name, Length
