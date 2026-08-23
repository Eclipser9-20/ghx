#!/usr/bin/env pwsh
# Builds ghx in release mode on Windows and code-signs it if a certificate
# is configured. Signing is fully optional — if no cert env vars are set,
# this just builds and tells you signing was skipped.
#
# Codesigning env vars (set these to enable signing):
#   GHX_SIGN_PFX       Path to a .pfx code-signing certificate
#   GHX_SIGN_PFX_PASS  Password for that .pfx (leave unset for a passwordless cert)
#   GHX_SIGN_TIMESTAMP Timestamp server URL (default: http://timestamp.digicert.com)

$ErrorActionPreference = "Stop"

Write-Host "==> cargo build --release" -ForegroundColor Cyan
cargo build --release
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$binary = Join-Path $PSScriptRoot "target\release\ghx.exe"
if (-not (Test-Path $binary)) {
    Write-Error "Build succeeded but $binary was not found."
    exit 1
}

if (-not $env:GHX_SIGN_PFX) {
    Write-Host "==> Skipping codesign (GHX_SIGN_PFX not set). Binary: $binary" -ForegroundColor Yellow
    exit 0
}

if (-not (Test-Path $env:GHX_SIGN_PFX)) {
    Write-Error "GHX_SIGN_PFX is set to '$($env:GHX_SIGN_PFX)' but that file does not exist."
    exit 1
}

$signtool = Get-Command signtool.exe -ErrorAction SilentlyContinue
if (-not $signtool) {
    # Common Windows SDK install location as a fallback.
    $candidates = Get-ChildItem "C:\Program Files (x86)\Windows Kits\10\bin\*\x64\signtool.exe" -ErrorAction SilentlyContinue
    if ($candidates) { $signtool = $candidates[-1].FullName } else {
        Write-Error "signtool.exe not found. Install the Windows SDK, or add it to PATH."
        exit 1
    }
} else {
    $signtool = $signtool.Source
}

$timestampUrl = if ($env:GHX_SIGN_TIMESTAMP) { $env:GHX_SIGN_TIMESTAMP } else { "http://timestamp.digicert.com" }

Write-Host "==> Codesigning $binary" -ForegroundColor Cyan
$signArgs = @(
    "sign", "/fd", "SHA256",
    "/f", $env:GHX_SIGN_PFX,
    "/tr", $timestampUrl, "/td", "SHA256"
)
if ($env:GHX_SIGN_PFX_PASS) {
    $signArgs += @("/p", $env:GHX_SIGN_PFX_PASS)
}
$signArgs += $binary

& $signtool @signArgs
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "==> Verifying signature" -ForegroundColor Cyan
& $signtool verify /pa $binary
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "==> Done: $binary (signed)" -ForegroundColor Green
