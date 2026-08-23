#!/usr/bin/env pwsh
# Installs ghx to Program Files on Windows.
#
# Layout:
#   C:\Program Files\ghx\bin\ghx.exe   the binary
#   C:\Program Files\ghx\lib\          reserved for future support files
#   %APPDATA%\ghx\                     per-user config (unchanged — this
#                                       is where ghx already reads/writes it)
#
# Requires an elevated (Administrator) PowerShell, since Program Files is
# only writable by Administrators by default — the same reason a later
# `ghx --update` also needs to be run elevated.
#
# Params:
#   -Channel        stable (default) | beta | dev
#   -LocalBinary    path to an already-built ghx.exe to install instead of
#                   downloading a release (for building from source)

param(
    [string]$Channel = "stable",
    [string]$LocalBinary = ""
)

$ErrorActionPreference = "Stop"

$isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $isAdmin) {
    Write-Error "This installer needs to run elevated (Administrator), since it writes to Program Files. Re-run PowerShell as Administrator."
    exit 1
}

$Repo = "Eclipser9-20/ghx"
$GhxHome = "C:\Program Files\ghx"
$BinDir = Join-Path $GhxHome "bin"
$LibDir = Join-Path $GhxHome "lib"
$TargetExe = Join-Path $BinDir "ghx.exe"

Write-Host "==> Installing ghx ($Channel) to $GhxHome" -ForegroundColor Cyan

New-Item -ItemType Directory -Force -Path $BinDir | Out-Null
New-Item -ItemType Directory -Force -Path $LibDir | Out-Null

if ($LocalBinary) {
    Write-Host "==> Using local binary: $LocalBinary" -ForegroundColor Cyan
    Copy-Item $LocalBinary $TargetExe -Force
} else {
    $apiUrl = "https://api.github.com/repos/$Repo/releases"
    $release = switch ($Channel) {
        "stable" { Invoke-RestMethod "$apiUrl/latest" }
        "dev"    { Invoke-RestMethod "$apiUrl/tags/dev" }
        "beta"   {
            $all = Invoke-RestMethod $apiUrl
            $all | Where-Object { $_.tag_name -match "-beta\." } | Select-Object -First 1
        }
        default {
            Write-Error "Unknown channel '$Channel' (expected stable, beta, or dev)"
            exit 1
        }
    }

    if (-not $release) {
        Write-Error "No release found on the $Channel channel."
        exit 1
    }

    $asset = $release.assets | Where-Object { $_.name -eq "ghx-windows-x86_64.exe" } | Select-Object -First 1
    if (-not $asset) {
        Write-Error "Release $($release.tag_name) has no ghx-windows-x86_64.exe asset."
        exit 1
    }

    Write-Host "==> Downloading $($asset.browser_download_url)" -ForegroundColor Cyan
    Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $TargetExe
}

# Add the install dir to the machine PATH if it isn't already there.
$machinePath = [Environment]::GetEnvironmentVariable("Path", "Machine")
if (($machinePath -split ';') -notcontains $BinDir) {
    [Environment]::SetEnvironmentVariable("Path", "$machinePath;$BinDir", "Machine")
    Write-Host "==> Added $BinDir to the system PATH (restart your terminal to pick it up)." -ForegroundColor Yellow
}

Write-Host "==> Installed: $TargetExe" -ForegroundColor Green
