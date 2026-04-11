---
sidebar_position: 2
---

# Prerequisites

Before building or running SSG Tether Capture, make sure the following are installed.

## Rust toolchain

Install Rust via [rustup](https://rustup.rs/):

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

The project targets **stable Rust**. Check `Cargo.toml` for the minimum supported version.

## System dependencies

Bevy has a few platform-level dependencies depending on your OS.

### macOS

```bash
xcode-select --install
```

### Linux (Debian/Ubuntu)

```bash
sudo apt-get install -y \
  libasound2-dev \
  libudev-dev \
  libwayland-dev \
  libxkbcommon-dev
```

Full dependency list: [Bevy Linux dependencies](https://github.com/bevyengine/bevy/blob/main/docs/linux_dependencies.md)

### Windows

No additional runtime dependencies are required for running the app, but building and bundling a Windows MSI installer uses the WiX Toolset and a few Windows build tools. To test the build/bundle process locally, install the following:

- **Visual Studio C++ Build Tools**: required to build native dependencies and for some packaging steps. Install via the Visual Studio installer: https://visualstudio.microsoft.com/visual-cpp-build-tools/
- **WiX Toolset (v3.11.x)**: the repository's WiX project uses the classic `heat.exe`, `candle.exe` and `light.exe` workflow (see `wix/main.wxs`). Download and install WiX Toolset 3.11.2 from https://wixtoolset.org/releases/ (the script expects the `WIX` environment variable to point to the WiX install directory).
- **Windows 10/11 SDK (optional but recommended)**: provides `signtool.exe` and other packaging utilities if you plan to sign the MSI. Install via the Windows SDK installer or Visual Studio components.

Install notes and quick checks:

- After installing WiX, set the `WIX` environment variable to the WiX installation folder (for example `C:\Program Files (x86)\WiX v3.11`). The packaging script `scripts/run_windows_release_workflow.ps1` expects `$env:WIX\bin\heat.exe` to exist.

  PowerShell example (run as Administrator to set machine environment variable):

  ```powershell
  setx WIX "C:\Program Files (x86)\WiX v3.11" /M
  ```

- Verify the WiX binaries are reachable:

  ```powershell
  Test-Path "$env:WIX\bin\heat.exe"
  Get-ChildItem "$env:WIX\bin" -Filter "*.exe" | Select-Object -ExpandProperty Name
  ```

- Alternative install: you can also install WiX via Chocolatey:

  ```powershell
  choco install wixtoolset --version=3.11.2
  ```

Notes:

- The build scripts in `scripts/` use `heat.exe`, `candle.exe` and `light.exe` (WiX v3-style). If you install WiX v4, ensure the same executables are available or adjust the script accordingly.
- If you plan to sign the MSI for distribution, obtain an Authenticode certificate and use `signtool.exe` from the Windows SDK.

### WiX v7 migration (what changed and how to migrate)

WiX has evolved significantly since v3. If you plan to migrate the project's packaging to WiX v7 (recommended long-term), here are the key points and a checklist to guide the transition:

- **Tooling change**: WiX v6+ and v7 use an SDK-style workflow delivered via NuGet (`WixToolset.Sdk`) and a new CLI (`wix`) rather than the classic `heat.exe`, `candle.exe` and `light.exe` binaries. Heat has been removed in v7.
- **Local prerequisites for v7**:
  - Install the .NET SDK (we recommend `dotnet 8.x` or newer).
  - Use the `WixToolset.Sdk` package in an SDK-style project (see `wix/SSG.wixproj`).
- **Authoring changes**: replace `heat`-generated fragments with `Files`/`Payloads` authoring or include a modern `.wixproj` that references `main_v7.wix` (see `wix/main_v7.wix`). The SDK approach lets you build with `dotnet build`.
- **CI friendliness**: SDK-based builds are preferred for CI — they only require `actions/setup-dotnet` and `dotnet build` on a Windows runner instead of installing WiX system-wide. We've already updated the repository CI to use `dotnet build` for packaging.
- **EULA / OSMF**: WiX v6+ introduced the Open Source Maintenance Fee (OSMF) EULA. Review the WiX release notes and OSMF terms before using WiX v6/v7 for commercial distribution.
- **Compatibility option**: if you need minimal changes, continue using WiX v3.11 (legacy `heat/candle/light`) — the repo still supports that flow as a fallback in `scripts/run_windows_release_workflow.ps1` when `dotnet` is not available.
- **Code signing**: signing is unchanged — use `signtool.exe` from the Windows SDK and inject certificates through CI secrets for automated signing.

Quick local test commands (from repo root):

```powershell
dotnet restore wix/SSG.wixproj
dotnet build wix/SSG.wixproj -c Release
# resulting MSI will be under wix/bin/Release (or similar output path)
```

Files added/updated in this repo to support v7:

- [wix/SSG.wixproj](wix/SSG.wixproj) — SDK-style WiX project for `dotnet build`
- [wix/main_v7.wix](wix/main_v7.wix) — modern `Files`/`Payloads` authoring example
- [`scripts/run_windows_release_workflow.ps1`](scripts/run_windows_release_workflow.ps1) — now prefers `dotnet build` and falls back to legacy WiX v3 tools

Migration checklist (high level):

1. Convert any `heat`-dependent logic to `Files`/`Payloads` authoring or use an SDK target that harvests assets.
2. Add `wix/SSG.wixproj` (done) and ensure `main_v7.wix` authoring covers the files you need.
3. Update CI to use `actions/setup-dotnet` and `dotnet build` (done in workflows).
4. Add code-signing steps in CI (optional) using `signtool.exe` and secrets for certificates.
5. Validate the produced MSI on a clean Windows VM and test upgrade/uninstall behavior.


## Assets

The app loads KTX2-compressed textures and font files at runtime from the `assets/` directory. These are checked into the repository and do not require separate installation:

| Asset | Path |
|---|---|
| Earth texture (8K) | `assets/textures/earth_8192x4096_uastc.ktx2` |
| Star field (8K) | `assets/textures/8k_stars_uastc.ktx2` |
| HDR cubemap | `assets/textures/hdr-cubemap-2048x2048.ktx2` |
| UI font | `assets/fonts/FiraMono-Medium.ttf` |

KTX2/Basis Universal decoding is handled automatically by Bevy's asset pipeline.
