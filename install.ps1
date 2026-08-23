#!/usr/bin/env pwsh
# Installs ghx to Program Files on Windows.
#
# Layout:
#   C:\Program Files\ghx\bin\ghx.exe   the binary
#   C:\Program Files\ghx\lib\          reserved for future support files
#   %APPDATA%\ghx\                     per-user config (unchanged — this
#                                       is where ghx already reads/writes it)
#
# Requires an elevated (Administrator) PowerShell to install, since
# Program Files is only writable by Administrators by default. To let
# `ghx --update`/`--uninstall` work afterward WITHOUT elevation, this
# installer creates a local group "_GHXmaintenance" and grants it Modify
# rights on the install directory specifically (not Program Files as a
# whole) — the same "owner has full control, group can update, everyone
# else read+execute only" model as install.sh's Unix group, just via an
# NTFS ACL instead of POSIX permission bits. Add another user to that
# group (Computer Management > Local Users and Groups, or
# `Add-LocalGroupMember -Group _GHXmaintenance -Member <user>`) to let
# them update/uninstall without an admin prompt too.
#
# Params:
#   -Channel        stable (default) | beta | dev
#   -LocalBinary    path to an already-built ghx.exe to install instead of
#                   downloading a release (for building from source)
#   -GrantTo        user (DOMAIN\name or name) to add to _GHXmaintenance.
#                   Defaults to whoever is running the installer — but when
#                   this runs as NT AUTHORITY\SYSTEM (a deployment tool or
#                   scheduled task), that default is useless, since nobody
#                   logs in as SYSTEM. In that case, pass the real target
#                   user explicitly, e.g. -GrantTo "CONTOSO\jsmith".

param(
    [string]$Channel = "stable",
    [string]$LocalBinary = "",
    [string]$GrantTo = ""
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

$GroupName = "_GHXmaintenance"
if (-not (Get-LocalGroup -Name $GroupName -ErrorAction SilentlyContinue)) {
    New-LocalGroup -Name $GroupName -Description "Can update/uninstall ghx without elevation" | Out-Null
}

$grantUser = $GrantTo
if (-not $grantUser) {
    if ("$env:USERNAME" -eq "SYSTEM") {
        # Running as NT AUTHORITY\SYSTEM (a deployment tool or scheduled
        # task) with no -GrantTo given — fall back to whoever owns the
        # explorer.exe process, i.e. the actual interactive user, if one
        # is logged on.
        $explorer = Get-CimInstance Win32_Process -Filter "Name = 'explorer.exe'" -ErrorAction SilentlyContinue | Select-Object -First 1
        if ($explorer) {
            $owner = Invoke-CimMethod -InputObject $explorer -MethodName GetOwner
            if ($owner.ReturnValue -eq 0) {
                $grantUser = "$($owner.Domain)\$($owner.User)"
            }
        }
    } else {
        $grantUser = "$env:USERDOMAIN\$env:USERNAME"
    }
}

if ($grantUser) {
    if (-not (Get-LocalGroupMember -Group $GroupName -Member $grantUser -ErrorAction SilentlyContinue)) {
        Add-LocalGroupMember -Group $GroupName -Member $grantUser
    }
} else {
    Write-Host "==> Running as SYSTEM with no logged-on user found and no -GrantTo given." -ForegroundColor Yellow
    Write-Host "    Nobody was added to '$GroupName' — add the intended user yourself:" -ForegroundColor Yellow
    Write-Host "        Add-LocalGroupMember -Group $GroupName -Member <DOMAIN\user>" -ForegroundColor Yellow
}

$acl = Get-Acl $GhxHome
$rule = New-Object System.Security.AccessControl.FileSystemAccessRule(
    $GroupName, "Modify", "ContainerInherit,ObjectInherit", "None", "Allow"
)
$acl.AddAccessRule($rule)
Set-Acl $GhxHome $acl

Write-Host "==> $GhxHome is group-writable by '$GroupName'." -ForegroundColor Cyan
if ($grantUser) {
    Write-Host "    $grantUser was added to it." -ForegroundColor Cyan
}
Write-Host "==> Installed: $TargetExe" -ForegroundColor Green
