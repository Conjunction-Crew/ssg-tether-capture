<#
.SYNOPSIS
    Checks Windows build prerequisites and performs a local build + MSI package.

.DESCRIPTION
    Mirrors exactly what release.yml and packaging-test.yml do on windows-latest:
      1. Check prerequisites: cargo, dotnet
      2. Build the Rust release binary via cargo
      3. Build the MSI via dotnet / WixToolset.Sdk v7
         (AcceptEula=wix7 is already set in wix/SSG.wixproj; the explicit
          -p flag here is belt-and-suspenders for clarity)
      4. Report the MSI location

    The -Tag parameter overrides the version tag used in the output file name.
    When omitted the script reads the version from Cargo.toml automatically.

.PARAMETER Tag
    Optional. Version tag for the output MSI name, e.g. "v0.3.0".
    Defaults to "v<version>" parsed from Cargo.toml.

.EXAMPLE
    .\scripts\test_windows_prereqs_and_build.ps1
    .\scripts\test_windows_prereqs_and_build.ps1 -Tag v0.3.0
#>

#Requires -Version 5.1
[CmdletBinding()]
param(
    [string]$Tag = ""
)

$ErrorActionPreference = 'Stop'

# ─── helpers ─────────────────────────────────────────────────────────────────

function Write-Check {
    param([string]$Label, [string]$Value = "")
    Write-Host "[OK]   " -ForegroundColor Green -NoNewline
    Write-Host $Label -NoNewline
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

# ─── resolve tag / version ───────────────────────────────────────────────────

Write-Step "Resolving version tag"

if (-not $Tag) {
    $cargoToml = Join-Path $PSScriptRoot '..\Cargo.toml'
    if (Test-Path $cargoToml) {
        $versionLine = Select-String -Path $cargoToml -Pattern '^\s*version\s*=' |
                       Select-Object -First 1
        if ($versionLine -match '"([^"]+)"') {
            $Tag = "v$($Matches[1])"
        }
    }
    if (-not $Tag) {
        $Tag = "v0.0.0-local"
        Write-Warn "Could not read version from Cargo.toml; using fallback tag: $Tag"
    }
}

Write-Check "Version tag" $Tag

# Derive MSI-safe PackageVersion: strip leading 'v' and any pre-release suffix.
# MSI versions must be Major.Minor.Build with no labels (e.g. "-beta.1").
$PackageVersion = $Tag -replace '^v', '' -replace '-.*$', ''
Write-Check "Package version" $PackageVersion

# ─── check: cargo ────────────────────────────────────────────────────────────

Write-Step "Checking prerequisites"

$cargo = Get-Command cargo -ErrorAction SilentlyContinue
if ($cargo) {
    $cargoVersion = & cargo --version 2>&1
    Write-Check "cargo" $cargoVersion
    $hasCargo = $true
} else {
    Write-Fail "cargo not found in PATH. Install Rust via https://rustup.rs/"
    exit 1
}

# ─── check: dotnet ───────────────────────────────────────────────────────────

$dotnet = Get-Command dotnet -ErrorAction SilentlyContinue
if ($dotnet) {
    $dotnetVersion = & dotnet --version 2>&1
    Write-Check "dotnet" $dotnetVersion
} else {
    Write-Fail "dotnet not found in PATH. Install .NET 8 SDK from https://dotnet.microsoft.com/download"
    exit 1
}

# ─── check: signtool (optional) ──────────────────────────────────────────────

$signtool = Get-Command signtool.exe -ErrorAction SilentlyContinue
if ($signtool) {
    Write-Check "signtool" $signtool.Source
} else {
    Write-Warn "signtool.exe not found (optional). Install Windows 10/11 SDK to enable MSI code signing."
}

# ─── build: Rust release binary ──────────────────────────────────────────────

Write-Step "Building Rust release binary (cargo build --release)"

Push-Location (Join-Path $PSScriptRoot '..')
try {
    & cargo build --release
    if ($LASTEXITCODE -ne 0) {
        Write-Fail "cargo build --release failed (exit $LASTEXITCODE)."
        exit $LASTEXITCODE
    }
    Write-Check "Rust binary" "target\release\ssg_tether_capture.exe"
} finally {
    Pop-Location
}

# ─── package: dotnet / WixToolset.Sdk v7 ────────────────────────────────────

Write-Step "Restoring WiX SDK project (dotnet restore)"

Push-Location (Join-Path $PSScriptRoot '..')
try {
    & dotnet restore wix\SSG.wixproj
    if ($LASTEXITCODE -ne 0) {
        Write-Fail "dotnet restore failed (exit $LASTEXITCODE)."
        exit $LASTEXITCODE
    }

    Write-Step "Building MSI (dotnet build wix\SSG.wixproj -c Release)"
    # AcceptEula is already in SSG.wixproj; the -p flag is explicit for clarity.
    & dotnet build wix\SSG.wixproj -c Release -p:AcceptEula=wix7 -p:PackageVersion=$PackageVersion
    if ($LASTEXITCODE -ne 0) {
        Write-Fail "dotnet build failed (exit $LASTEXITCODE)."
        exit $LASTEXITCODE
    }

    $msiFile = Get-ChildItem -Path . -Recurse -Filter '*.msi' | Select-Object -First 1
    if ($msiFile) {
        # Rename to match release naming convention
        $destName = "ssg-tether-capture-${Tag}-windows-x86_64.msi"
        $destPath = Join-Path (Split-Path $msiFile.FullName) $destName
        Copy-Item -Path $msiFile.FullName -Destination $destPath -Force
        Write-Check "MSI built" $destPath
    } else {
        Write-Fail "dotnet build succeeded but no .msi file was found. Check wix\SSG.wixproj output."
        exit 1
    }
} finally {
    Pop-Location
}

# ─── summary ──────────────────────────────────────────────────────────────────

""
Write-Host "================================================================" -ForegroundColor Green
Write-Host "  BUILD SUCCESS" -ForegroundColor Green
Write-Host "  Tag    : $Tag" -ForegroundColor Green
Write-Host "================================================================" -ForegroundColor Green
