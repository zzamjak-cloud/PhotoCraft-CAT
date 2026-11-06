# Checks the installer and update helper with a stub app: portable conversion keeps settings, a later
# update keeps them, a failed update loses nothing, uninstall leaves settings. Changes the current
# user's PhotoCAT install and settings, so run it only on a throwaway machine such as a CI runner.
param(
    [Parameter(Mandatory = $true)][string]$Setup1,
    [Parameter(Mandatory = $true)][string]$Setup2,
    [Parameter(Mandatory = $true)][string]$Helper,
    [Parameter(Mandatory = $true)][string]$Stub
)
$ErrorActionPreference = 'Stop'
$Work = Join-Path $env:RUNNER_TEMP 'photocat-installer-test'
$Portable = Join-Path $Work 'portable folder'
$Log = Join-Path $Work 'failed.txt'
$Settings = Join-Path $env:APPDATA 'Photocraft'
$Recovery = Join-Path $env:LOCALAPPDATA 'Photocraft\Recovery'
$Install = Join-Path $env:LOCALAPPDATA 'Programs\PhotoCAT'
$Installed = Join-Path $Install 'PhotoCAT.exe'

function Assert-True([bool]$Condition, [string]$What) {
    if (-not $Condition) { throw "FAILED: $What" }
    Write-Host "ok: $What"
}
function Run-Helper([hashtable]$Params) {
    $Waited = Start-Process -FilePath 'ping.exe' -ArgumentList '-n', '2', '127.0.0.1' -PassThru -WindowStyle Hidden
    & powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File $Helper -AppPid $Waited.Id -Log $Log @Params
}
function Installed-Version {
    $Keys = Get-ChildItem 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall'
    ($Keys | ForEach-Object { Get-ItemProperty $_.PSPath } | Where-Object { $_.DisplayName -like 'PhotoCAT*' }).DisplayVersion
}

if (Test-Path $Work) { Remove-Item -Recurse -Force $Work }
New-Item -ItemType Directory -Force (Join-Path $Portable 'PhotoCraftData\cat'), (Join-Path $Portable 'PhotoCraftData\Recovery') | Out-Null
Copy-Item $Stub (Join-Path $Portable 'photocraft-cat.exe')
Set-Content (Join-Path $Portable 'portable.txt') ''
Set-Content (Join-Path $Portable 'mine.txt') 'user file'
Set-Content (Join-Path $Portable 'PhotoCraftData\prefs.json') 'portable prefs'
Set-Content (Join-Path $Portable 'PhotoCraftData\ui.ron') 'portable layout'
Set-Content (Join-Path $Portable 'PhotoCraftData\cat\settings.json') 'portable cat'
Set-Content (Join-Path $Portable 'PhotoCraftData\Recovery\a.bin') 'autosave'
New-Item -ItemType Directory -Force $Settings | Out-Null
Set-Content (Join-Path $Settings 'prefs.json') 'older install'

# A failed install leaves the portable copy working and writes the reason.
$Bad = Join-Path $Work 'bad-setup.exe'
Set-Content $Bad 'not an installer'
try { Run-Helper @{ Setup = $Bad; Exe = $Installed; PortableDir = $Portable; InstallDir = $Install; OldExe = (Join-Path $Portable 'photocraft-cat.exe') } } catch { Write-Host $_ }
Assert-True (Test-Path $Log) 'a failed install writes the reason'
Assert-True (Test-Path (Join-Path $Portable 'photocraft-cat.exe')) 'a failed install keeps the portable exe'
Assert-True (Test-Path (Join-Path $Portable 'portable.txt')) 'a failed install keeps portable mode'
Assert-True (-not (Test-Path (Join-Path $Settings '.migrated-from-portable'))) 'a failed install can be retried'
Remove-Item -Force $Log

# Portable to installed.
Run-Helper @{ Setup = $Setup1; Exe = $Installed; PortableDir = $Portable; InstallDir = $Install; OldExe = (Join-Path $Portable 'photocraft-cat.exe') }
Assert-True (-not (Test-Path $Log)) 'conversion reports no failure'
Assert-True (Test-Path $Installed) 'installed to the default folder'
Assert-True (-not (Test-Path (Join-Path $Install 'portable.txt'))) 'the install is not portable'
Assert-True ((Installed-Version) -eq '0.0.1') 'registered in Apps as 0.0.1'
Assert-True ((Get-Content (Join-Path $Settings 'prefs.json')) -eq 'portable prefs') 'prefs moved'
Assert-True ((Get-Content (Join-Path $Settings 'ui.ron')) -eq 'portable layout') 'layout moved'
Assert-True ((Get-Content (Join-Path $Settings 'cat\settings.json')) -eq 'portable cat') 'CAT settings moved'
Assert-True ((Get-Content (Join-Path $Recovery 'a.bin')) -eq 'autosave') 'autosaves moved'
Assert-True (-not (Test-Path (Join-Path $Settings 'Recovery'))) 'autosaves are not in the roaming folder'
$Backups = @(Get-ChildItem (Split-Path -Parent $Settings) -Filter 'Photocraft.before-install-*')
Assert-True ($Backups.Count -ge 1 -and (Get-Content (Join-Path $Backups[-1].FullName 'prefs.json')) -eq 'older install') 'older settings are backed up'
Assert-True (-not (Test-Path (Join-Path $Portable 'photocraft-cat.exe'))) 'the portable exe is gone'
Assert-True (-not (Test-Path (Join-Path $Portable 'portable.txt'))) 'the portable marker is gone'
Assert-True (Test-Path (Join-Path $Portable 'PhotoCraftData\prefs.json')) 'portable data stays as a backup'
Assert-True (Test-Path (Join-Path $Portable 'mine.txt')) 'user files stay'
Assert-True (Test-Path (Join-Path ([Environment]::GetFolderPath('Programs')) 'PhotoCAT.lnk')) 'Start menu shortcut'

# A leftover portable copy never overwrites newer settings.
Set-Content (Join-Path $Settings 'prefs.json') 'changed after install'
Copy-Item $Setup1 (Join-Path $Work 'again.exe')
Run-Helper @{ Setup = (Join-Path $Work 'again.exe'); Exe = $Installed; PortableDir = $Portable; InstallDir = $Install; OldExe = $Installed }
Assert-True ((Get-Content (Join-Path $Settings 'prefs.json')) -eq 'changed after install') 'a second conversion keeps newer settings'

# Installed update.
Run-Helper @{ Setup = $Setup2; Exe = $Installed }
Assert-True (-not (Test-Path $Log)) 'update reports no failure'
Assert-True ((Installed-Version) -eq '0.0.2') 'updated to 0.0.2'
Assert-True ((Get-Content (Join-Path $Settings 'prefs.json')) -eq 'changed after install') 'update keeps settings'
Assert-True ((Get-Content (Join-Path $Settings 'cat\settings.json')) -eq 'portable cat') 'update keeps CAT settings'

# Uninstall.
$Uninstall = Start-Process -FilePath (Join-Path $Install 'unins000.exe') -ArgumentList '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART' -Wait -PassThru
Assert-True ($Uninstall.ExitCode -eq 0) 'uninstall succeeds'
Start-Sleep -Seconds 3
Assert-True (-not (Test-Path $Installed)) 'uninstall removes the app'
Assert-True ((Get-Content (Join-Path $Settings 'prefs.json')) -eq 'changed after install') 'uninstall keeps settings'
Write-Host 'all installer checks passed'
