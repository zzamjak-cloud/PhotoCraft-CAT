# PhotoCAT Windows update helper (win_update.rs writes it to a temp folder and runs it).
# ASCII only: Windows PowerShell 5.1 reads BOM-less scripts in the system code page.
#
# Waits for the app to exit, then runs the verified installer silently and starts the app again.
# With -PortableDir it first converts a portable copy: settings go to %APPDATA%\Photocraft and
# autosaves to %LOCALAPPDATA%\Photocraft\Recovery, where an installed copy reads them.
# A failure is written to $Log (the app reports it on its next run); the old copy keeps working and
# %APPDATA%\Photocraft goes back to what it was.
param(
    [Parameter(Mandatory = $true)][int]$AppPid,
    [Parameter(Mandatory = $true)][string]$Setup,
    [Parameter(Mandatory = $true)][string]$Exe,
    [Parameter(Mandatory = $true)][string]$Log,
    [string]$PortableDir = '',
    [string]$InstallDir = '',
    [string]$OldExe = ''
)
$ErrorActionPreference = 'Stop'
$Restart = if ($OldExe) { $OldExe } else { $Exe }
$Settings = Join-Path $env:APPDATA 'Photocraft'
$Copied = $false
$Backup = ''

function Copy-Contents([string]$From, [string]$To, [string[]]$Skip = @()) {
    New-Item -ItemType Directory -Force -Path $To | Out-Null
    Get-ChildItem -LiteralPath $From -Force | Where-Object { $Skip -notcontains $_.Name } | ForEach-Object {
        Copy-Item -LiteralPath $_.FullName -Destination $To -Recurse -Force
    }
}

try {
    # Unsaved work was handled before the app closed.
    Wait-Process -Id $AppPid -ErrorAction SilentlyContinue
    Start-Sleep -Milliseconds 500

    $Marker = Join-Path $Settings '.migrated-from-portable'
    $Data = if ($PortableDir) { Join-Path $PortableDir 'PhotoCraftData' } else { '' }
    # The marker stops a leftover portable copy from overwriting newer settings.
    if ($Data -and (Test-Path -LiteralPath $Data) -and -not (Test-Path -LiteralPath $Marker)) {
        if (Test-Path -LiteralPath $Settings) {
            # Settings of an earlier install are kept beside the new ones, never overwritten.
            $Backup = "$Settings.before-install-$(Get-Date -Format 'yyyyMMdd-HHmmss')"
            Rename-Item -LiteralPath $Settings -NewName (Split-Path -Leaf $Backup)
        }
        $Copied = $true
        Copy-Contents $Data $Settings @('Recovery')
        $Recovery = Join-Path $Data 'Recovery'
        if (Test-Path -LiteralPath $Recovery) {
            Copy-Contents $Recovery (Join-Path $env:LOCALAPPDATA 'Photocraft\Recovery')
        }
    }

    $SetupArgs = @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/SP-')
    if ($InstallDir) { $SetupArgs += "/DIR=`"$InstallDir`"" }
    $Process = Start-Process -FilePath $Setup -ArgumentList $SetupArgs -Wait -PassThru
    if ($Process.ExitCode -ne 0) { throw "the installer failed with exit code $($Process.ExitCode)" }
    Remove-Item -LiteralPath $Setup -Force -ErrorAction SilentlyContinue
    $Restart = $Exe

    if ($PortableDir) {
        New-Item -ItemType File -Force -Path $Marker | Out-Null
        # Only what the portable package placed goes. PhotoCraftData stays as a backup.
        foreach ($Name in @('photocraft-cat.exe', 'portable.txt', 'PhotoCraft.portable', 'README.md', 'LICENSE', 'LICENSE-MIT', 'LICENSE-APACHE', 'NOTICE', 'ATTRIBUTION.md')) {
            Remove-Item -LiteralPath (Join-Path $PortableDir $Name) -Force -ErrorAction SilentlyContinue
        }
        Get-ChildItem -LiteralPath $PortableDir -Filter 'OFL-*.txt' -ErrorAction SilentlyContinue | Remove-Item -Force -ErrorAction SilentlyContinue
    }
} catch {
    "$($_.Exception.Message)" | Out-File -LiteralPath $Log -Encoding utf8
    # Not installed: the portable copy still owns its settings.
    if ($Copied -and $Restart -ne $Exe) {
        try {
            Remove-Item -LiteralPath $Settings -Recurse -Force -ErrorAction SilentlyContinue
            if ($Backup) { Rename-Item -LiteralPath $Backup -NewName 'Photocraft' }
        } catch {
            "$($_.Exception.Message)" | Out-File -LiteralPath $Log -Encoding utf8 -Append
        }
    }
}
Start-Process -FilePath $Restart -WorkingDirectory (Split-Path -Parent $Restart)
