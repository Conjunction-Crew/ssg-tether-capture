---
sidebar_position: 2
---

import useBaseUrl from '@docusaurus/useBaseUrl';

# Installing on Windows

This guide walks through installing SSG Tether Capture on Windows.

## Requirements

- A Windows device with the following specifications:
  - Running Windows 11 or later
  - Which has a processor with an x64 architecture (ARM-based Windows devices are not supported)
  - A graphics card and up-to-date driver that supports Vulkan or DirectX 12
  - At least 8GB of RAM (16 GB or more recommended for optimal performance)
  - At least 500MB of free disk space for the app itself, plus additional space for captured data

## Download

1. Visit the [SSG Tether Capture releases page](https://github.com/Conjunction-Crew/ssg-tether-capture/releases) and navigate to the latest release.

<div className="screenshot-frame">
  <img src={useBaseUrl('/img/getting-started/install-guides-release-page-screenshot.png')} alt="GitHub releases page" className="screenshot" />
</div>

2. Download the Windows version of the app, which will be a `.msi`
  - The file will be named something like `ssg-tether-capture-vx.y.z-windows-x86_64.msi`, where `x.y.z` is the version number.

## Installation

:::note

The screenshots in this section were taken on a Windows device running Windows 11, but the installation process should be similar on other recent versions of Windows. The installer that was used was downloaded from the GitHub releases page, and is from release v0.3.0-rc.2.

:::

1. Once the `.msi` file has finished downloading, locate it in your Downloads folder and double-click to open it.
2. Windows will probably show a **"Windows protected your PC"** message, because the installer is not signed by a recognised publisher. This is expected. Click **More info**.

<img src={useBaseUrl('/img/getting-started/install-windows/windows-install-guide-screenshot-1.png')} alt="Windows SmartScreen warning" className="screenshot--small" />

3. The publisher is shown as "Unknown publisher". Click **Run anyway** to start the installer.

<img src={useBaseUrl('/img/getting-started/install-windows/windows-install-guide-screenshot-2.png')} alt="Windows SmartScreen with the Run anyway button" className="screenshot--small" />

4. The Setup Wizard opens. Click **Next** on the welcome screen.

<img src={useBaseUrl('/img/getting-started/install-windows/windows-install-guide-screenshot-3.png')} alt="Setup Wizard welcome screen" className="screenshot--small" />

5. Read the license agreement, tick **I accept the terms in the License Agreement**, and click **Next**.

<img src={useBaseUrl('/img/getting-started/install-windows/windows-install-guide-screenshot-4.png')} alt="End-user license agreement" className="screenshot--small" />

6. Choose where to install the app, then click **Next**:
  - **Install just for you** (default) installs to your user folder, `%LocalAppData%\Programs\SSG Tether Capture`. No administrator rights are needed.
  - **Install for all users of this machine** installs to `C:\Program Files\SSG Tether Capture`. You will be asked for administrator approval when the installation starts.

<img src={useBaseUrl('/img/getting-started/install-windows/windows-install-guide-screenshot-5.png')} alt="Installation scope selection" className="screenshot--small" />

7. Click **Install**.

<img src={useBaseUrl('/img/getting-started/install-windows/windows-install-guide-screenshot-6.png')} alt="Ready to install screen" className="screenshot--small" />

8. Wait for the progress bar to finish. This takes a short while because the app includes large texture files.

<img src={useBaseUrl('/img/getting-started/install-windows/windows-install-guide-screenshot-7.png')} alt="Installation progress" className="screenshot--small" />

9. Click **Finish** to close the wizard.

<img src={useBaseUrl('/img/getting-started/install-windows/windows-install-guide-screenshot-8.png')} alt="Setup Wizard completed" className="screenshot--small" />

The installer adds **SSG Tether Capture** to the Start menu and puts a shortcut on your Desktop.

## First Launch

1. Open **SSG Tether Capture** from the Start menu or the Desktop shortcut.
2. A console window opens alongside the app, showing log output such as your graphics card and the rendering backend in use. This is normal. Leave it open while you use the app, because closing it also closes the app.

<div className="screenshot-frame">
  <img src={useBaseUrl('/img/getting-started/install-windows/windows-install-guide-screenshot-9.png')} alt="Console window shown when the app starts" className="screenshot" />
</div>

3. The first time the app starts, it asks you to **Select Working Directory**. This is the folder where your capture plans are stored. Click **Browse** to pick a folder, or edit the path directly, then click **Select**.

<div className="screenshot-frame">
  <img src={useBaseUrl('/img/getting-started/install-windows/windows-install-guide-screenshot-10.png')} alt="Select Working Directory screen" className="screenshot" />
</div>

4. You can change the working directory later from the home screen. See [Running the App](./running-the-app.md#first-launch) for what to do next.

## Updating and Uninstalling

- **Updating:** download the newer `.msi` and run it. It replaces the installed version automatically, and your capture plans stay in your working directory.
- **Uninstalling:** open **Settings → Apps → Installed apps**, find **SSG Tether Capture**, and choose **Uninstall**. Your working directory and the capture plans in it are not deleted.

## Troubleshooting

| Problem | What to try |
|---|---|
| "Windows protected your PC" with no **Run anyway** button | Click **More info** first. The button only appears after that. |
| The installer says a newer version is already installed | Uninstall the newer version from **Settings → Apps**, then run the older installer. |
| The app closes immediately or the window stays black | Update your graphics driver from your GPU maker (NVIDIA, AMD, or Intel). The app needs Vulkan or DirectX 12. The console window usually shows the reason. |
| The app opens but shows no Earth or the textures are missing | The installation may be incomplete. Uninstall, then run the installer again. |
| You closed the console window and the app disappeared | This is expected. Closing the console window closes the app. Start the app again. |
