<#
.SYNOPSIS
    Checks Windows build prerequisites and performs a local build + MSI package.

.DESCRIPTION
    - Checks for cargo (Rust toolchain), dotnet (.NET SDK), WIX env (legacy WiX v3), and signtool.
    - Builds the Rust release binary via cargo.
    - Prefers WixToolset.Sdk / dotnet build to produce the MSI.
    - Falls back to legacy heat/candle/light if dotnet is unavailable.
    - Exits with a non-zero code on first failure.

.EXAMPLE
    .\scripts\test_windows_prereqs_and_build.ps1
#>

#Requires -Version 5.1
[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

# ─── helpers ─────────────────────────────────────────────────────────────────

function Write-Check {
    param([string]$Label, [string]$Value)
    Write-Host "[OK] " -ForegroundColor Green -NoNewline
    Write-Host "$Label" -NoNewline
    if ($Value) { Write-Host ": $Value" } else { Write-Host }
}

function Write-Warn {
    param([string]$Message)
    Write-Host "[WARN] $Message" -ForegroundColor Yellow
}

function Write-Fail {
    param([string]$Message)
    Write-Host "[FAIL] $Message" -ForegroundColor Red
}

function Write-Step {
    param([string]$Message)
    ""
    Write-Host ">> $Message" -ForegroundColor Cyan
}

# ─── check: cargo ────────────────────────────────────────────────────────────

Write-Step "Checking prerequisites"

$cargo = Get-Command cargo -ErrorAction SilentlyContinue
if ($cargo) {
    $cargoVersion = & cargo --version 2>&1
    Write-Check "cargo" $cargoVersion
    $hasCargo = $true
} else {
    Write-Fail "cargo not found in PATH. Install Rust via https://rustup.rs/"
    $hasCargo = $false
}

# ─── check: dotnet ───────────────────────────────────────────────────────────

$dotnet = Get-Command dotnet -ErrorAction SilentlyContinue
if ($dotnet) {
    $dotnetVersion = & dotnet --version 2>&1
    Write-Check "dotnet" $dotnetVersion
    $hasDotnet = $true
} else {
    Write-Warn "dotnet not found in PATH. WiX SDK (v7) build will be unavailable; will fall back to legacy WiX v3."
    $hasDotnet = $false
}

# ─── check: WiX v3 (legacy) ──────────────────────────────────────────────────

$hasWixV3 = $false
if ($env:WIX) {
    $heatExe = Join-Path $env:WIX 'bin\heat.exe'
    if (Test-Path $heatExe) {
        Write-Check "WiX v3 (legacy)" $env:WIX
        $hasWixV3 = $true
    } else {
        Write-Warn "WIX env set to '$env:WIX' but heat.exe not found there."
    }
} else {
    Write-Warn "WIX environment variable not set. Legacy WiX v3 build unavailable."
}

# ─── check: signtool (optional) ──────────────────────────────────────────────

$signtool = Get-Command signtool.exe -ErrorAction SilentlyContinue
if ($signtool) {
    Write-Check "signtool" $signtool.Source
} else {
    Write-Warn "signtool.exe not found (optional). Install Windows 10/11 SDK to enable MSI code signing."
}

# ─── guard: must have at least one packaging path ───────────────────────────

if (-not $hasDotnet -and -not $hasWixV3) {
    Write-Fail "Neither dotnet (WiX SDK) nor WiX v3 (legacy) is available. Install at least one to package."
    exit 1
}

if (-not $hasCargo) {
    Write-Fail "cargo is required to build the Rust binary. Aborting."
    exit 1
}

# ─── build: Rust release binary ──────────────────────────────────────────────

Write-Step "Building Rust release binary (cargo build --release)"
& cargo build --release
if ($LASTEXITCODE -ne 0) {
    Write-Fail "cargo build --release failed (exit $LASTEXITCODE)."
    exit $LASTEXITCODE
}
Write-Check "Rust binary" "target\release\ssg_tether_capture.exe"

# ─── package: prefer dotnet / WixToolset.Sdk ────────────────────────────────

if ($hasDotnet) {
    Write-Step "Restoring WiX SDK project (dotnet restore)"
    & dotnet restore wix\SSG.wixproj
    if ($LASTEXITCODE -ne 0) {
        Write-Fail "dotnet restore failed (exit $LASTEXITCODE)."
        exit $LASTEXITCODE
    }

    Write-Step "Building MSI (dotnet build wix\SSG.wixproj -c Release)"
    & dotnet build wix\SSG.wixproj -c Release
    if ($LASTEXITCODE -ne 0) {
        Write-Fail "dotnet build failed (exit $LASTEXITCODE)."
        exit $LASTEXITCODE
    }

    $msiFile = Get-ChildItem -Path . -Recurse -Filter '*.msi' | Select-Object -First 1
    if ($msiFile) {
        Write-Check "MSI built" $msiFile.FullName
    } else {
        Write-Warn "dotnet build succeeded but no .msi file was found. Check wix\SSG.wixproj output configuration."
    }
    exit 0
}

# ─── fallback: legacy WiX v3 (heat / candle / light) ────────────────────────

Write-Step "Falling back to legacy WiX v3 packaging"

& "$env:WIX\bin\heat.exe" dir 'assets' `
    -out 'wix\assets_fragment.wxs' `
    -cg AssetsComponentGroup `
    -dr INSTALLDIR `
    -scom -sfrag -srd -sreg `
    -var 'var.AssetsDir'
if ($LASTEXITCODE -ne 0) { Write-Fail "heat.exe failed."; exit $LASTEXITCODE }

$tag     = 'v0.2.0-beta.1'
$version = $tag -replace '^v', '' -replace '-.*$', ''
$msi     = "ssg-tether-capture-${tag}-windows-x86_64.msi"

& "$env:WIX\bin\candle.exe" `
    -dVersion="$version" `
    -dSourceDir='target\release' `
    -dAssetsDir='assets' `
    -arch x64 `
    'wix\main.wxs' `
    'wix\assets_fragment.wxs' `
    -out 'wix\'
if ($LASTEXITCODE -ne 0) { Write-Fail "candle.exe failed."; exit $LASTEXITCODE }

& "$env:WIX\bin\light.exe" `
    -ext WixUIExtension `
    'wix\main.wixobj' `
    'wix\assets_fragment.wixobj' `
    -out $msi
if ($LASTEXITCODE -ne 0) { Write-Fail "light.exe failed."; exit $LASTEXITCODE }

Write-Check "MSI built (legacy)" $msi
