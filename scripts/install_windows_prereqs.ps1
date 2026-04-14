<#
.SYNOPSIS
    Checks and installs prerequisites required to develop, build, and package
    the Windows version of SSG Tether Capture.

.DESCRIPTION
    Installs missing prerequisites using winget (Windows Package Manager).
    Requires Windows 10 1709 or later / Windows 11.

    Prerequisites checked and installed:
      - Rust toolchain (rustup)               — required for cargo build
      - VS 2022 Build Tools + VCTools         — required MSVC linker for Rust
      - .NET 8 SDK                            — required for WiX MSI packaging
      - Git                                   — required for source control + cargo-release
      - Node.js LTS                           — required for Docusaurus docs site
      - Windows 10/11 SDK (optional)          — required for signtool code signing

    After installation you MUST open a new terminal session for PATH changes to
    take effect before running build scripts.

.EXAMPLE
    .\scripts\install_windows_prereqs.ps1

.NOTES
    Run from the repo root or the scripts\ folder. Elevation (Administrator)
    is recommended so winget can install machine-scope packages without prompts.
#>

#Requires -Version 5.1
[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

# ─── helpers (mirrors test_windows_prereqs_and_build.ps1 style) ──────────────

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

function Install-WithWinget {
    param(
        [string]$DisplayName,
        [string]$WingetId,
        [string[]]$ExtraArgs = @()
    )
    Write-Host "       Installing $DisplayName via winget..." -ForegroundColor DarkCyan
    $wingetArgs = @('install', '--id', $WingetId, '--accept-package-agreements', '--accept-source-agreements') + $ExtraArgs
    & winget @wingetArgs
    if ($LASTEXITCODE -ne 0) {
        Write-Fail "winget install for $DisplayName failed (exit $LASTEXITCODE). Check https://github.com/microsoft/winget-cli/issues."
        exit $LASTEXITCODE
    }
    Write-Check "$DisplayName installed"
}

# ─── check: winget ──────────────────────────────────────────────────────────

Write-Step "Checking winget availability"

$winget = Get-Command winget -ErrorAction SilentlyContinue
if (-not $winget) {
    Write-Fail "winget (Windows Package Manager) not found. It ships with Windows 10 1809+ / Windows 11."
    Write-Fail "Install it from https://aka.ms/getwinget or update via the Microsoft Store (App Installer)."
    exit 1
}
Write-Check "winget" $winget.Source

# ─── 1. Rust ────────────────────────────────────────────────────────────────

Write-Step "Checking Rust toolchain (cargo)"

$cargo = Get-Command cargo -ErrorAction SilentlyContinue
if ($cargo) {
    $cargoVersion = & cargo --version 2>&1
    Write-Check "cargo" $cargoVersion
} else {
    Install-WithWinget -DisplayName "Rust (rustup)" -WingetId "Rustlang.Rustup"
    Write-Warn "Rust installed. Open a new terminal and run: rustup default stable"
}

# ─── 2. Visual Studio 2022 Build Tools (MSVC linker) ────────────────────────
# Rust on Windows requires the MSVC linker. The VCTools workload provides it.

Write-Step "Checking Visual Studio 2022 Build Tools (MSVC linker)"

# vswhere ships with VS installer; check for it as a proxy for VS build tools
$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
if (Test-Path $vswhere) {
    $vsInfo = & "$vswhere" -latest -products * -requires Microsoft.VisualCpp.Tools.HostX64.TargetX64 -property displayName 2>&1
    if ($vsInfo) {
        Write-Check "VS C++ Build Tools" $vsInfo
    } else {
        # VS is installed but VCTools workload may be missing
        Write-Warn "Visual Studio found but Microsoft.VisualCpp.Tools.HostX64.TargetX64 component not detected."
        Write-Warn "If cargo link errors occur, install the 'Desktop development with C++' workload via the VS Installer."
        Install-WithWinget `
            -DisplayName "VS 2022 Build Tools + VCTools workload" `
            -WingetId "Microsoft.VisualStudio.2022.BuildTools" `
            -ExtraArgs @('--override', '--add Microsoft.VisualStudio.Workload.VCTools --includeRecommended --passive --wait')
    }
} else {
    Install-WithWinget `
        -DisplayName "VS 2022 Build Tools + VCTools workload" `
        -WingetId "Microsoft.VisualStudio.2022.BuildTools" `
        -ExtraArgs @('--override', '--add Microsoft.VisualStudio.Workload.VCTools --includeRecommended --passive --wait')
}

# ─── 3. .NET 8 SDK ──────────────────────────────────────────────────────────

Write-Step "Checking .NET 8 SDK (required for WiX MSI packaging)"

$dotnet = Get-Command dotnet -ErrorAction SilentlyContinue
if ($dotnet) {
    $dotnetVersion = & dotnet --version 2>&1
    # Confirm .NET 8 specifically (non-CLI sdks may be different)
    $sdk8Installed = & dotnet sdk check 2>&1 | Select-String "^8\." -Quiet
    if ($sdk8Installed) {
        Write-Check "dotnet (.NET 8 SDK)" $dotnetVersion
    } else {
        Write-Warn "dotnet found ($dotnetVersion) but .NET 8 SDK not confirmed. Installing .NET 8 SDK..."
        Install-WithWinget -DisplayName ".NET 8 SDK" -WingetId "Microsoft.DotNet.SDK.8"
    }
} else {
    Install-WithWinget -DisplayName ".NET 8 SDK" -WingetId "Microsoft.DotNet.SDK.8"
}

# ─── 4. Git ─────────────────────────────────────────────────────────────────

Write-Step "Checking Git"

$git = Get-Command git -ErrorAction SilentlyContinue
if ($git) {
    $gitVersion = & git --version 2>&1
    Write-Check "git" $gitVersion
} else {
    Install-WithWinget -DisplayName "Git" -WingetId "Git.Git"
}

# ─── 5. Node.js LTS ─────────────────────────────────────────────────────────

Write-Step "Checking Node.js LTS (required for Docusaurus docs site)"

$node = Get-Command node -ErrorAction SilentlyContinue
if ($node) {
    $nodeVersion = & node --version 2>&1
    Write-Check "node" $nodeVersion
} else {
    Install-WithWinget -DisplayName "Node.js LTS" -WingetId "OpenJS.NodeJS.LTS"
}

# ─── 6. Windows SDK (optional — needed for signtool code signing) ────────────

Write-Step "Checking Windows SDK (optional — enables MSI code signing)"

$signtool = Get-Command signtool.exe -ErrorAction SilentlyContinue
if ($signtool) {
    Write-Check "signtool.exe" $signtool.Source
} else {
    Write-Warn "signtool.exe not found. The Windows 10/11 SDK provides it."
    Write-Warn "Code signing is optional for local builds. To install:"
    Write-Warn "  winget install --id Microsoft.WindowsSDK.10.0.22621"
    Write-Warn "Or install the 'Windows 10 SDK' component via the VS Installer."
}

# ─── summary ─────────────────────────────────────────────────────────────────

""
Write-Host "================================================================" -ForegroundColor Green
Write-Host "  PREREQUISITES CHECK COMPLETE" -ForegroundColor Green
Write-Host "================================================================" -ForegroundColor Green
""
Write-Warn "If any tools were just installed, OPEN A NEW TERMINAL before"
Write-Warn "running build scripts. PATH changes only take effect in new shells."
""
Write-Host "  Next step: .\scripts\test_windows_prereqs_and_build.ps1" -ForegroundColor Cyan
