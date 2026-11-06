# PhotoCAT Windows 자체 업데이트 도우미 (win_update.rs 가 임시 폴더에 써서 실행한다).
# 앱 프로세스가 끝나기를 기다린 뒤, 서명 확인을 마친 포터블 zip 을 설치 폴더에 덮어 풀고 앱을 다시 띄운다.
# 실패하면 이유를 $Log 에 남기고 기존 앱을 다시 띄운다 (다음 실행 때 앱이 알린다).
param(
    [Parameter(Mandatory = $true)][int]$AppPid,
    [Parameter(Mandatory = $true)][string]$Zip,
    [Parameter(Mandatory = $true)][string]$Dest,
    [Parameter(Mandatory = $true)][string]$Exe,
    [Parameter(Mandatory = $true)][string]$Log
)
$ErrorActionPreference = 'Stop'
try {
    # 사용자가 앱을 닫을 때까지 기다린다 (저장 안 한 작업이 있으면 앱이 먼저 묻는다).
    Wait-Process -Id $AppPid -ErrorAction SilentlyContinue
    $Stage = Join-Path (Split-Path -Parent $Zip) 'stage'
    if (Test-Path -LiteralPath $Stage) { Remove-Item -LiteralPath $Stage -Recurse -Force }
    Expand-Archive -LiteralPath $Zip -DestinationPath $Stage -Force
    # 종료 직후에는 exe 잠금이 잠시 남을 수 있어 재시도한다.
    for ($i = 0; $i -lt 40; $i++) {
        try {
            Copy-Item -Path (Join-Path $Stage '*') -Destination $Dest -Recurse -Force
            break
        } catch {
            if ($i -eq 39) { throw }
            Start-Sleep -Milliseconds 500
        }
    }
    Remove-Item -LiteralPath $Stage -Recurse -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $Zip -Force -ErrorAction SilentlyContinue
} catch {
    "$($_.Exception.Message)" | Out-File -LiteralPath $Log -Encoding utf8
}
Start-Process -FilePath $Exe -WorkingDirectory $Dest
