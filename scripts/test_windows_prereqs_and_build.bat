@echo off
setlocal
echo ==================================================================
echo Checking Windows build prerequisites and attempting local package
echo ==================================================================

rem Check for dotnet
where dotnet >nul 2>&1
if %ERRORLEVEL%==0 (
  echo [OK] dotnet found: & dotnet --version
  set HAS_DOTNET=1
) else (
  echo [WARN] dotnet not found in PATH
  set HAS_DOTNET=0
)

rem Check for cargo (Rust)
where cargo >nul 2>&1
if %ERRORLEVEL%==0 (
  echo [OK] cargo found: & cargo --version
  set HAS_CARGO=1
) else (
  echo [WARN] cargo not found in PATH
  set HAS_CARGO=0
)

rem Check for WIX env (legacy fallback)
set HAS_WIX=0
if defined WIX (
  if exist "%WIX%\bin\heat.exe" (
    echo [OK] WiX detected at %WIX%
    set HAS_WIX=1
  ) else (
    echo [WARN] WIX environment defined but heat.exe not found at %WIX%\bin\heat.exe
  )
) else (
  echo [INFO] WIX environment variable not set
)

rem Build the Rust release binary if cargo available
if "%HAS_CARGO%"=="1" (
  echo Building Rust release binary (cargo build --release)...
  cargo build --release
  if %ERRORLEVEL% NEQ 0 (
    echo [ERROR] cargo build failed.
    exit /b %ERRORLEVEL%
  )
) else (
  echo [SKIP] Skipping cargo build; cargo not installed.
)

rem Prefer dotnet-based WiX SDK flow
if "%HAS_DOTNET%"=="1" (
  echo Using dotnet / WixToolset.Sdk to build installer...
  dotnet --info
  dotnet restore wix\SSG.wixproj
  if %ERRORLEVEL% NEQ 0 (
    echo [ERROR] dotnet restore failed.
    exit /b %ERRORLEVEL%
  )

  dotnet build wix\SSG.wixproj -c Release
  if %ERRORLEVEL% NEQ 0 (
    echo [ERROR] dotnet build failed.
    exit /b %ERRORLEVEL%
  )

  rem Find the first MSI produced
  set MSI_PATH=
  for /f "delims=" %%I in ('dir /s /b *.msi 2^>nul') do set MSI_PATH=%%I
  if defined MSI_PATH (
    echo [OK] MSI built: %MSI_PATH%
    exit /b 0
  ) else (
    echo [WARN] No MSI found after dotnet build. Inspect wix project output.
    exit /b 1
  )
)

endlocal
