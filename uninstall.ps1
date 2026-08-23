#!/usr/bin/env pwsh
# Removes a system-wide ghx install done by install.ps1. This is the
# elevated counterpart to `ghx --uninstall` — use it when you can't (or
# don't want to) run the ghx binary itself to remove it, e.g. a broken
# install, remote/unattended cleanup, or removing it for another user.
#
# Removes:
#   C:\Program Files\ghx\          the install directory
#   The install dir from the machine PATH
#   %APPDATA%\ghx\                 config for the CURRENT user only
#                                   (pass -AllUserProfiles to sweep every
#                                   profile on the machine)
#
# Leaves the "_GHXmaintenance" local group in place, since removing a
# group can strand its membership on other machines/tools that reference
# it by name; delete it yourself with Remove-LocalGroup if you're sure
# nothing else depends on it.
#
# Params:
#   -AllUserProfiles   also remove %APPDATA%\ghx from every local user
#                       profile, not just the one running this script

param(
    [switch]$AllUserProfiles
)

$ErrorActionPreference = "Stop"

$isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $isAdmin) {
    Write-Error "This uninstaller needs to run elevated (Administrator), since it removes Program Files. Re-run PowerShell as Administrator."
    exit 1
}

$GhxHome = "C:\Program Files\ghx"
$BinDir = Join-Path $GhxHome "bin"

if (Test-Path $GhxHome) {
    Remove-Item -Recurse -Force $GhxHome
    Write-Host "==> Removed $GhxHome" -ForegroundColor Cyan
} else {
    Write-Host "==> $GhxHome not found, nothing to remove there." -ForegroundColor Yellow
}

$machinePath = [Environment]::GetEnvironmentVariable("Path", "Machine")
$pathEntries = $machinePath -split ';' | Where-Object { $_ -and $_ -ne $BinDir }
if (($machinePath -split ';') -contains $BinDir) {
    [Environment]::SetEnvironmentVariable("Path", ($pathEntries -join ';'), "Machine")
    Write-Host "==> Removed $BinDir from the system PATH." -ForegroundColor Cyan
}

function Remove-GhxConfig($profilePath) {
    $configDir = Join-Path $profilePath "AppData\Roaming\ghx"
    if (Test-Path $configDir) {
        Remove-Item -Recurse -Force $configDir
        Write-Host "==> Removed $configDir" -ForegroundColor Cyan
    }
}

if ($AllUserProfiles) {
    Get-CimInstance Win32_UserProfile | Where-Object { -not $_.Special } | ForEach-Object {
        Remove-GhxConfig $_.LocalPath
    }
} else {
    Remove-GhxConfig $env:USERPROFILE
}

Write-Host "==> ghx has been uninstalled. Restart open terminals to drop it from PATH." -ForegroundColor Green
