@echo off
setlocal EnableDelayedExpansion

rem ================================================================
rem  install_windows_prereqs.bat
rem
rem  Checks and installs prerequisites required to develop, build,
rem  and package the Windows version of SSG Tether Capture.
rem
rem  Prerequisites checked and installed:
rem    1. Rust toolchain (rustup)        -- required for cargo build
rem    2. VS 2022 Build Tools + VCTools  -- required MSVC linker for Rust
rem    3. .NET 8 SDK                     -- required for WiX MSI packaging
rem    4. Git                            -- required for source control
rem    5. Node.js LTS                    -- required for Docusaurus docs
rem    6. Windows SDK (optional)         -- required for signtool signing
rem
rem  Uses winget (Windows Package Manager). Requires Windows 10 1709+
rem  or Windows 11.
rem
rem  IMPORTANT: After installation, open a new terminal session before
rem  running build scripts so that PATH changes take effect.
rem ================================================================

echo ==================================================================
echo  SSG Tether Capture -- Windows Prerequisites Installer
echo ==================================================================
echo.

rem ── check: winget ──────────────────────────────────────────────────
where winget >nul 2>&1
if %ERRORLEVEL% NEQ 0 (
    echo [FAIL] winget ^(Windows Package Manager^) not found.
    echo        winget ships with Windows 10 1809+ / Windows 11.
    echo        Install from: https://aka.ms/getwinget
    echo        or update App Installer via the Microsoft Store.
    exit /b 1
)
for /f "tokens=*" %%V in ('winget --version 2^>^&1') do echo [OK]   winget: %%V

rem ── 1. Rust toolchain ──────────────────────────────────────────────
echo.
echo ^>^> Checking Rust toolchain ^(cargo^)
where cargo >nul 2>&1
if %ERRORLEVEL% EQU 0 (
    for /f "tokens=*" %%V in ('cargo --version 2^>^&1') do echo [OK]   cargo: %%V
) else (
    echo [INFO] cargo not found. Installing Rust via winget...
    winget install --id Rustlang.Rustup --accept-package-agreements --accept-source-agreements
    if %ERRORLEVEL% NEQ 0 (
        echo [FAIL] Failed to install Rust. Visit https://rustup.rs/ to install manually.
        exit /b %ERRORLEVEL%
    )
    echo [OK]   Rust ^(rustup^) installed.
    echo [WARN] Run: rustup default stable   ^(in a NEW terminal^)
)

rem ── 2. VS 2022 Build Tools + VCTools workload (MSVC linker) ────────
echo.
echo ^>^> Checking Visual Studio 2022 Build Tools ^(MSVC linker^)
set VSWHERE="%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe"
if exist !VSWHERE! (
    for /f "tokens=*" %%V in ('!VSWHERE! -latest -products * -requires Microsoft.VisualCpp.Tools.HostX64.TargetX64 -property displayName 2^>nul') do (
        if not "%%V"=="" (
            echo [OK]   VS C++ Build Tools: %%V
            goto :vs_done
        )
    )
)
echo [INFO] VS 2022 Build Tools ^(VCTools^) not detected. Installing via winget...
winget install --id Microsoft.VisualStudio.2022.BuildTools ^
    --accept-package-agreements --accept-source-agreements ^
    --override "--add Microsoft.VisualStudio.Workload.VCTools --includeRecommended --passive --wait"
if %ERRORLEVEL% NEQ 0 (
    echo [FAIL] Failed to install VS Build Tools.
    echo        Install manually at https://visualstudio.microsoft.com/downloads/
    exit /b %ERRORLEVEL%
)
echo [OK]   VS 2022 Build Tools + VCTools installed.
:vs_done

rem ── 3. .NET 8 SDK ──────────────────────────────────────────────────
echo.
echo ^>^> Checking .NET 8 SDK ^(required for WiX MSI packaging^)
where dotnet >nul 2>&1
if %ERRORLEVEL% EQU 0 (
    for /f "tokens=*" %%V in ('dotnet --version 2^>^&1') do echo [OK]   dotnet: %%V
) else (
    echo [INFO] dotnet not found. Installing .NET 8 SDK via winget...
    winget install --id Microsoft.DotNet.SDK.8 --accept-package-agreements --accept-source-agreements
    if %ERRORLEVEL% NEQ 0 (
        echo [FAIL] Failed to install .NET 8 SDK.
        echo        Download manually: https://dotnet.microsoft.com/download/dotnet/8.0
        exit /b %ERRORLEVEL%
    )
    echo [OK]   .NET 8 SDK installed.
)

rem ── 4. Git ────────────────────────────────────────────────────────
echo.
echo ^>^> Checking Git
where git >nul 2>&1
if %ERRORLEVEL% EQU 0 (
    for /f "tokens=*" %%V in ('git --version 2^>^&1') do echo [OK]   git: %%V
) else (
    echo [INFO] git not found. Installing Git via winget...
    winget install --id Git.Git --accept-package-agreements --accept-source-agreements
    if %ERRORLEVEL% NEQ 0 (
        echo [FAIL] Failed to install Git. Visit https://git-scm.com/ to install manually.
        exit /b %ERRORLEVEL%
    )
    echo [OK]   Git installed.
)

rem ── 5. Node.js LTS ────────────────────────────────────────────────
echo.
echo ^>^> Checking Node.js LTS ^(required for Docusaurus docs site^)
where node >nul 2>&1
if %ERRORLEVEL% EQU 0 (
    for /f "tokens=*" %%V in ('node --version 2^>^&1') do echo [OK]   node: %%V
) else (
    echo [INFO] node not found. Installing Node.js LTS via winget...
    winget install --id OpenJS.NodeJS.LTS --accept-package-agreements --accept-source-agreements
    if %ERRORLEVEL% NEQ 0 (
        echo [FAIL] Failed to install Node.js. Visit https://nodejs.org/ to install manually.
        exit /b %ERRORLEVEL%
    )
    echo [OK]   Node.js LTS installed.
)

rem ── 6. Windows SDK (optional, for signtool code signing) ──────────
echo.
echo ^>^> Checking Windows SDK ^(optional -- enables MSI code signing^)
where signtool.exe >nul 2>&1
if %ERRORLEVEL% EQU 0 (
    echo [OK]   signtool.exe found
) else (
    echo [WARN] signtool.exe not found. Code signing is optional for local builds.
    echo [WARN] To install: winget install --id Microsoft.WindowsSDK.10.0.22621
    echo [WARN] Or add the 'Windows 10 SDK' component via the Visual Studio Installer.
)

rem ── summary ──────────────────────────────────────────────────────
echo.
echo ==================================================================
echo  PREREQUISITES CHECK COMPLETE
echo ==================================================================
echo.
echo [WARN] If any tools were just installed, OPEN A NEW TERMINAL before
echo [WARN] running build scripts. PATH changes only take effect in new shells.
echo.
echo  Next step: scripts\test_windows_prereqs_and_build.bat
echo.
