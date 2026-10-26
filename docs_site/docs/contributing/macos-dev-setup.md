---
sidebar_position: 0
---

# macOS Development Setup (Apple Silicon)

This is a from-scratch checklist for setting up an Apple Silicon (M-series) Mac to build, test, profile, document, and release SSG Tether Capture. Work through it in order. Each step ends with a **Verify** block. Don't move on until it passes.

:::tip Already set up?
If you're coming back to the project after a break, skip the installs. Run the **Verify** commands in each step and fix only what fails. Then go to [Step 11 — Refresh an existing setup](#11--refresh-an-existing-setup).
:::

## Overview

| Step | What | Needed for |
|---|---|---|
| 1 | System checks | Everything |
| 2 | Xcode Command Line Tools | Everything (compiler, linker, `git`, `codesign`, `hdiutil`) |
| 3 | Homebrew | Installing the tools below |
| 4 | Git identity | Committing |
| 5 | Rust toolchain (rustup) | Building and running the app |
| 6 | VS Code + extensions | Editing and debugging |
| 7 | First build and test | Confirming the toolchain works |
| 8 | Node.js | Docs site (`docs_site/`) |
| 9 | Release tooling | `cargo bundle`, `.dmg` packaging |
| 10 | Tracy profiler | `--features bevy/trace_tracy` |

---

## 1 — System checks

```bash
uname -m          # must print: arm64
sw_vers           # macOS version
df -h ~           # free disk space
```

- **Architecture:** must be `arm64`. If it prints `x86_64`, your terminal is running under Rosetta. Fix that before going further (in Finder → Applications → Terminal → Get Info, uncheck *Open using Rosetta*), or every tool you install will be the wrong architecture.
- **macOS:** a current release is recommended. The release pipeline builds on `macos-latest`.
- **Disk:** have at least **20 GB free**. A single `target/` directory with debug and release builds of Bevy can easily reach 10–15 GB.
- **RAM:** 16 GB or more is recommended. Release builds use `lto = "fat"` and `codegen-units = 1` and are memory-heavy.

Apply any pending macOS updates first, since they often include Command Line Tools updates:

```bash
softwareupdate --list
```

---

## 2 — Xcode Command Line Tools

These provide `clang`, the system linker (`ld`), `git`, `codesign`, and the macOS SDK that Rust links against. **Full Xcode is not required.**

```bash
xcode-select --install
```

If you get "command line tools are already installed", check for updates with `softwareupdate --list` and install any **Command Line Tools** entries.

**Verify:**

```bash
xcode-select -p                   # /Library/Developer/CommandLineTools (or an Xcode.app path)
clang --version                   # should succeed
pkgutil --pkg-info=com.apple.pkg.CLTools_Executables | grep version
```

:::note
After a major macOS upgrade, the Command Line Tools sometimes break silently. A symptom is `xcrun: error: invalid active developer path`. Re-run `xcode-select --install` to fix it.
:::

---

## 3 — Homebrew

On Apple Silicon, Homebrew installs to `/opt/homebrew`.

```bash
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
```

Follow the installer's final instructions to add Homebrew to your shell. For zsh:

```bash
echo 'eval "$(/opt/homebrew/bin/brew shellenv)"' >> ~/.zprofile
eval "$(/opt/homebrew/bin/brew shellenv)"
```

If Homebrew is already installed, bring it up to date:

```bash
brew update && brew upgrade
```

**Verify:**

```bash
which brew        # /opt/homebrew/bin/brew  (NOT /usr/local/bin/brew — that's the Intel prefix)
brew doctor       # "Your system is ready to brew." (warnings are usually OK)
```

---

## 4 — Git identity

The repo's remote uses HTTPS. GitHub authentication is out of scope here. Use whatever you already have: credential helper, `gh auth login`, or SSH.

Set the name and email your commits will be attributed to. Use an email associated with your GitHub account:

```bash
git config --global user.name  "Your Name"
git config --global user.email "you@example.com"
```

Optional, but useful for a shared `main` branch:

```bash
git config --global pull.rebase true
git config --global fetch.prune true
```

**Verify:**

```bash
git config --global user.name
git config --global user.email
git -C /path/to/ssg-tether-capture fetch --tags   # confirms remote access
```

:::info Git LFS is not required
The CI workflows check out with `lfs: true`, but the repository has no `.gitattributes`. All assets in `assets/` (including the ~66 MB HDR cubemap) are regular git objects, so you don't need to install `git-lfs`. You can confirm with `head -c 4 assets/textures/earth_8192x4096_uastc.ktx2`. It should print binary `«KTX`, not `version https://git-lfs...`.
:::

---

## 5 — Rust toolchain

Install via **rustup**, the official installer. Avoid `brew install rust`, which doesn't support toolchain or component management.

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Accept the defaults: the `stable` toolchain, the `default` profile, and the host `aarch64-apple-darwin`. Then load it into your current shell:

```bash
source "$HOME/.cargo/env"
```

The installer adds this to `~/.zshenv` / `~/.zprofile` for new shells.

Make sure the linting and editor components are present. The `default` profile includes them, but older installs may not:

```bash
rustup component add rustfmt clippy rust-src
```

**Verify:**

```bash
rustup show                  # host: aarch64-apple-darwin, active toolchain: stable-aarch64-apple-darwin
rustc --version              # must be ≥ 1.85 (Cargo.toml uses edition = "2024")
cargo --version
cargo clippy --version
cargo fmt --version
```

:::note Toolchain version
`rust-toolchain.toml` pins the exact Rust version the project builds and lints with. rustup installs it automatically the first time you run `cargo` in the repository, so you do not need `rustup update` for this repo. The pin is raised by hand when Bevy or another dependency needs a newer compiler; if a build fails with an error like `package X requires rustc 1.NN`, check whether the pin has moved and run `cargo` again to pick it up.
:::

---

## 6 — VS Code

1. Install [VS Code](https://code.visualstudio.com/), or run `brew install --cask visual-studio-code`. Choose the **Apple Silicon** build.
2. Install the `code` shell command: open the Command Palette (`⇧⌘P`) and run **Shell Command: Install 'code' command in PATH**.
3. Install the recommended extensions:

```bash
code --install-extension rust-lang.rust-analyzer      # language server (bundles its own rust-analyzer binary)
code --install-extension vadimcn.vscode-lldb          # CodeLLDB — native debugger for Rust on macOS
code --install-extension tamasfe.even-better-toml     # Cargo.toml support
code --install-extension fill-labs.dependi            # inline crate version hints for Cargo.toml
```

Optional, for docs work:

```bash
code --install-extension dbaeumer.vscode-eslint
code --install-extension unifiedjs.vscode-mdx
```

4. Recommended user settings. Open `⇧⌘P` → **Preferences: Open User Settings (JSON)**:

```jsonc
{
  "rust-analyzer.check.command": "clippy",
  "[rust]": { "editor.formatOnSave": true }
}
```

:::tip Avoid lock contention with rust-analyzer
rust-analyzer and your terminal `cargo` compete for the same `target/` lock, which shows up as `Blocking waiting for file lock on build directory`. If this gets annoying, give rust-analyzer its own target directory. It costs extra disk space but removes the stalls:

```jsonc
"rust-analyzer.cargo.targetDir": "target/rust-analyzer"
```

The path is relative to the workspace root. Keeping it inside `target/` means `/target/` in `.gitignore` already covers it, and `cargo clean` removes it too.
:::

**Verify:** open the repo folder in VS Code (`code .` from the repo root), open `src/main.rs`, and wait for the status bar to show that rust-analyzer finished indexing. The first index of Bevy takes a few minutes. Hovering a Bevy type should show its docs.

---

## 7 — First build and test

From the repository root:

```bash
cargo build
```

The first build downloads the crates.io dependencies **and** the project's forked Bevy (`Conjunction-Crew/ssg-bevy`, branch `ssg-bevy-0.18.0`, configured via `[patch.crates-io]` in `Cargo.toml`). It then compiles everything, which takes **5–15 minutes** on an M-series Mac. Later builds are incremental.

Then run what CI runs:

```bash
cargo test
cargo run          # always run from the repo root so assets/ resolves
```

**Verify:**

- `cargo test` passes.
- `cargo run` opens the app window and shows the Select Working Directory screen. Rendering uses Metal through wgpu, so there are no GPU drivers to install.
- macOS may ask for permission the first time the app opens a file dialog (from `rfd`) or uses the clipboard (from `arboard`). Allow it.

Before opening a PR, also run:

```bash
cargo fmt --check
cargo clippy --all-targets
```

### Optional: faster iteration builds

Bevy's `dynamic_linking` feature greatly reduces link time on incremental builds. Use it only for local development:

```bash
cargo run --features bevy/dynamic_linking
```

The macOS system linker (ld-prime, which ships with the Command Line Tools) is already fast, so you don't need `lld` or `mold` on Apple Silicon.

If you add a local `.cargo/config.toml` for personal build tweaks, keep it out of commits. `.gitignore` doesn't cover it, so add it to your local exclude file:

```bash
echo ".cargo/" >> .git/info/exclude
```

---

## 8 — Node.js (docs site)

The Docusaurus site in `docs_site/` requires **Node ≥ 20**. Install the latest **Long Term Support (LTS)** release of Node.js with a version manager. [fnm](https://github.com/Schniz/fnm) is quick to set up:

```bash
brew install fnm
echo 'eval "$(fnm env --use-on-cd --shell zsh)"' >> ~/.zshrc
source ~/.zshrc
fnm install --lts
fnm default lts-latest
```

**Verify:**

```bash
node --version     # must be ≥ v20
npm --version
```

:::note CI uses the latest LTS too
CI builds the docs with `node-version: lts/*` (see `.github/workflows/pr.yaml`), so `fnm install --lts` matches it. If CI's Node ever moves ahead of yours, run `fnm install --lts` again.
:::

Install the dependencies and run the site locally:

```bash
cd docs_site
npm ci
npm start          # live-reload dev server at http://localhost:3000/ssg-tether-capture/
```

**Verify** that the same build CI runs succeeds:

```bash
cp ../CHANGELOG.md docs/release-notes.md   # CI syncs this before building
npm run build
```

:::caution
`docs/release-notes.md` is generated by CI. Don't commit your local copy. Run `git status` and discard it if needed.
:::

---

## 9 — Release tooling

Releases are cut from GitHub Actions, so you don't need any release tooling to contribute. See [Release Workflow](./release-workflow.md) for the full process. The only extra tool you need locally is `cargo bundle`, and only if you want to reproduce the macOS package:

```bash
cargo install cargo-bundle --version 0.6.1 --locked   # pinned to match build-packages.yml
```

**Verify:**

```bash
cargo bundle --version                         # must report 0.6.1
sh scripts/next_version.sh 0.3.0 minor stable  # prints 0.4.0: the helper the Release workflow uses
```

To reproduce the macOS release artifact locally, follow the same steps as `build-packages.yml`:

```bash
cargo bundle --release
APP="target/release/bundle/osx/SSG Tether Capture.app"
codesign --force --deep --sign - --options runtime "$APP/Contents/MacOS/ssg_tether_capture"
codesign --force --deep --sign - --options runtime "$APP"
codesign --verify --verbose "$APP"
hdiutil create -volname "SSG Tether Capture" -srcfolder "$APP" -ov -format UDZO ssg-tether-capture-local-macos-arm64.dmg
```

The release profile uses fat LTO with a single codegen unit, so expect `cargo bundle --release` to take considerably longer than a debug build.

:::note
`codesign` with `-` creates an **ad-hoc** signature. No Apple Developer account or certificate is needed. `codesign` and `hdiutil` come with macOS and the Command Line Tools.
:::

---

## 10 — Tracy profiler

The app supports [Tracy](https://github.com/wolfpld/tracy) via Bevy's `trace_tracy` feature:

```bash
cargo run --release --features bevy/trace_tracy
```

You also need the **Tracy GUI (server)** to connect to the running app. Its version **must match the protocol** of the `tracy-client` crate that Bevy pulls in, or it will refuse to connect.

1. Find the client version the project uses:

   ```bash
   cargo tree -i tracy-client --features bevy/trace_tracy
   ```

2. Look up the matching Tracy GUI version in the compatibility table in [Bevy's profiling guide](https://github.com/bevyengine/bevy/blob/main/docs/profiling.md#backend-tracy) or the [`tracy-client` README](https://github.com/nagisa/rust_tracy_client#version-support-table).

3. Install the GUI. Homebrew provides a native arm64 build:

   ```bash
   brew install tracy
   brew info tracy | head -1   # check installed version
   ```

   If Homebrew's version doesn't match the table, build the matching tagged release from source instead (see the Tracy repo's build instructions, which use CMake: `brew install cmake`).

**Verify:** start `tracy-profiler`, then launch the app with the command above. The app should appear in Tracy's connection list. Click **Connect**.

---

## 11 — Refresh an existing setup

Coming back after time away? Run these from the repo root:

```bash
softwareupdate --list                 # OS / Command Line Tools updates
brew update && brew upgrade           # Homebrew packages (fnm, tracy, …)
rustup self update && rustup update   # newest stable Rust
git checkout main && git pull --tags  # latest code + tags
cargo clean                           # optional: drop stale artifacts built with an old toolchain
cargo build && cargo test
(cd docs_site && npm ci && npm run build)
```

`cargo clean` isn't required, but it frees a lot of disk space. Artifacts from an older `rustc` are never reused anyway.

## Troubleshooting

| Symptom | Likely cause / fix |
|---|---|
| `xcrun: error: invalid active developer path` | Command Line Tools missing after an OS upgrade. Run `xcode-select --install`. |
| `brew` installs to `/usr/local` or tools are `x86_64` | Terminal running under Rosetta. Disable it in Get Info and reinstall Homebrew under `/opt/homebrew`. |
| `requires rustc 1.NN or newer` | Run `rustup update`. |
| `failed to load source for dependency bevy` / git fetch errors | The forked Bevy is fetched from GitHub. Check your network and GitHub access. Try `CARGO_NET_GIT_FETCH_WITH_CLI=true cargo build` to use your system git credentials. |
| `Blocking waiting for file lock` | rust-analyzer is building. Wait, or set `rust-analyzer.cargo.targetDir` (see Step 6). |
| App window opens but assets are missing | You ran from the wrong directory. Run from the repo root. |
| Tracy GUI won't connect | GUI and `tracy-client` protocol versions differ. See Step 10. |
| The `Changelog entry` CI check fails | Add a bullet under `## [Unreleased]` in `CHANGELOG.md`, or add the `no-changelog` label. Don't bump the version by hand. See [Release Workflow](./release-workflow.md). |
| The `cargo fmt` CI check fails | Run `cargo fmt` and commit the result. |
