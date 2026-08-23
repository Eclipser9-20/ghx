#!/usr/bin/env pwsh
# Removes a ghx install done by install.ps1. This is the standalone
# counterpart to `ghx --uninstall` — use it when you can't (or don't
# want to) run the ghx binary itself to remove it, e.g. a broken install,
# remote/unattended cleanup, or removing it for another user.
#
# Run elevated (Administrator) to remove a system-wide install:
#   C:\Program Files\ghx\          removed, and dropped from the machine PATH
# Run un-elevated to remove a per-user install:
#   %LOCALAPPDATA%\ghx\            removed, and dropped from the user PATH
#
# Also removes:
#   %APPDATA%\ghx\                 config for the CURRENT user only
#                                   (pass -AllUserProfiles, elevated only,
#                                   to sweep every profile on the machine)
#
# Leaves the "_GHXmaintenance" local group in place, since removing a
# group can strand its membership on other machines/tools that reference
# it by name; delete it yourself with Remove-LocalGroup if you're sure
# nothing else depends on it.
#
# Params:
#   -AllUserProfiles   (elevated only) also remove %APPDATA%\ghx from
#                      every local user profile, not just the one running
#                      this script

param(
    [switch]$AllUserProfiles
)

$ErrorActionPreference = "Stop"

$isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)

if ($isAdmin) {
    $GhxHome = "C:\Program Files\ghx"
    $PathScope = "Machine"
} else {
    $GhxHome = Join-Path $env:LOCALAPPDATA "ghx"
    $PathScope = "User"
    if ($AllUserProfiles) {
        Write-Error "-AllUserProfiles needs an elevated (Administrator) PowerShell to read other users' profiles."
        exit 1
    }
}
$BinDir = Join-Path $GhxHome "bin"

if (Test-Path $GhxHome) {
    Remove-Item -Recurse -Force $GhxHome
    Write-Host "==> Removed $GhxHome" -ForegroundColor Cyan
} else {
    Write-Host "==> $GhxHome not found, nothing to remove there." -ForegroundColor Yellow
    if ($isAdmin) {
        Write-Host "    (run without elevation to remove a per-user install instead)" -ForegroundColor DarkGray
    } else {
        Write-Host "    (run elevated to remove a system-wide install instead)" -ForegroundColor DarkGray
    }
}

$existingPath = [Environment]::GetEnvironmentVariable("Path", $PathScope)
$pathEntries = $existingPath -split ';' | Where-Object { $_ -and $_ -ne $BinDir }
if (($existingPath -split ';') -contains $BinDir) {
    [Environment]::SetEnvironmentVariable("Path", ($pathEntries -join ';'), $PathScope)
    Write-Host "==> Removed $BinDir from the $PathScope PATH." -ForegroundColor Cyan
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
