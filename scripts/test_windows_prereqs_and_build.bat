@echo off
setlocal EnableDelayedExpansion

rem ================================================================
rem  test_windows_prereqs_and_build.bat
rem
rem  Mirrors release.yml / packaging-test.yml on windows-latest:
rem    1. Check prerequisites: cargo, dotnet
rem    2. Build the Rust release binary (cargo build --release)
rem    3. Build the MSI (dotnet build wix\SSG.wixproj -c Release)
rem       AcceptEula=wix7 is already in SSG.wixproj; the /p flag
rem       here is belt-and-suspenders for clarity.
rem    4. Report MSI location and BUILD SUCCESS / FAILURE
rem
rem  Optional argument: version tag for output file name.
rem    e.g.  test_windows_prereqs_and_build.bat v0.3.0
rem  If omitted the script reads the version from Cargo.toml.
rem ================================================================

echo ==================================================================
echo  SSG Tether Capture -- Windows Local Build
echo ==================================================================

rem ── resolve version tag ────────────────────────────────────────────
set TAG=%~1
if "!TAG!"=="" (
    for /f "tokens=*" %%L in ('findstr /R "^version" ..\Cargo.toml 2^>nul') do (
        for /f "tokens=3 delims= ^"" %%V in ("%%L") do (
            if "!TAG!"=="" set TAG=v%%V
        )
    )
)
if "!TAG!"=="" set TAG=v0.0.0-local
echo [INFO] Version tag: !TAG!

rem ── derive MSI-safe PackageVersion (strip leading 'v' and pre-release suffix) ─
rem   MSI versions must be Major.Minor.Build with no labels (e.g. "-beta.1").
set PKGVER=!TAG:~1!
for /f "tokens=1 delims=-" %%V in ("!PKGVER!") do set PKGVER=%%V
echo [INFO] Package version: !PKGVER!

rem ── check: cargo ───────────────────────────────────────────────────
where cargo >nul 2>&1
if %ERRORLEVEL% NEQ 0 (
    echo [FAIL] cargo not found in PATH. Install Rust via https://rustup.rs/
    exit /b 1
)
for /f "tokens=*" %%V in ('cargo --version 2^>^&1') do echo [OK]   cargo: %%V

rem ── check: dotnet ──────────────────────────────────────────────────
where dotnet >nul 2>&1
if %ERRORLEVEL% NEQ 0 (
    echo [FAIL] dotnet not found in PATH.
    echo        Install .NET 8 SDK from https://dotnet.microsoft.com/download
    exit /b 1
)
for /f "tokens=*" %%V in ('dotnet --version 2^>^&1') do echo [OK]   dotnet: %%V

rem ── check: signtool (optional) ─────────────────────────────────────
where signtool.exe >nul 2>&1
if %ERRORLEVEL%==0 (
    echo [OK]   signtool.exe found
) else (
    echo [WARN] signtool.exe not found (optional^). Install Windows 10/11 SDK for code signing.
)

rem ── build: Rust release binary ─────────────────────────────────────
echo.
echo ^>^> Building Rust release binary (cargo build --release)
pushd "%~dp0.."
cargo build --release
if %ERRORLEVEL% NEQ 0 (
    echo [FAIL] cargo build --release failed.
    popd
    exit /b %ERRORLEVEL%
)
echo [OK]   Rust binary: target\release\ssg_tether_capture.exe

rem ── package: dotnet / WixToolset.Sdk v7 ────────────────────────────
echo.
echo ^>^> Restoring WiX SDK project (dotnet restore)
dotnet restore wix\SSG.wixproj
if %ERRORLEVEL% NEQ 0 (
    echo [FAIL] dotnet restore failed.
    popd
    exit /b %ERRORLEVEL%
)

echo.
echo ^>^> Building MSI (dotnet build wix\SSG.wixproj -c Release)
dotnet build wix\SSG.wixproj -c Release /p:AcceptEula=wix7 /p:PackageVersion=!PKGVER!
if %ERRORLEVEL% NEQ 0 (
    echo [FAIL] dotnet build failed.
    popd
    exit /b %ERRORLEVEL%
)

rem ── locate the produced MSI ────────────────────────────────────────
set MSI_PATH=
for /f "delims=" %%I in ('dir /s /b *.msi 2^>nul') do (
    if "!MSI_PATH!"=="" set MSI_PATH=%%I
)

if "!MSI_PATH!"=="" (
    echo [FAIL] dotnet build succeeded but no .msi file was found.
    echo        Check wix\SSG.wixproj output configuration.
    popd
    exit /b 1
)

rem ── copy with release-convention name ─────────────────────────────
set "DEST_DIR=%~dp0.."
set "DEST_MSI=!DEST_DIR!\ssg-tether-capture-!TAG!-windows-x86_64.msi"
copy /Y "!MSI_PATH!" "!DEST_MSI!" >nul
echo [OK]   MSI built: !DEST_MSI!

popd

echo.
echo ==================================================================
echo   BUILD SUCCESS
echo   Tag: !TAG!
echo ==================================================================
endlocal
exit /b 0
