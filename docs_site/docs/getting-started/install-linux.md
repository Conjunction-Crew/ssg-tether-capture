---
sidebar_position: 4
---

# Installing on Linux

This guide walks through installing SSG Tether Capture on Linux.

:::caution Not yet tested on a real Linux install

Unlike the Windows and macOS guides, this guide has not been verified step by step on a Linux machine, and it has no screenshots. It is written from how the release is built and packaged. The package names and troubleshooting tips below may need adjusting for your distribution. If you find something that is wrong or missing, please [open an issue](https://github.com/Conjunction-Crew/ssg-tether-capture/issues).

:::

## Requirements

- A 64-bit **x86_64** Linux PC (ARM-based Linux devices are not supported)
- A desktop session running **X11 or Wayland**
- A graphics card and driver that support **Vulkan**
- A recent distribution. The release is built on GitHub's `ubuntu-latest` runner, so older distributions may fail to start with an error about a missing `GLIBC_2.xx` version.
- At least 8GB of RAM (16GB or more recommended for optimal performance)
- At least 500MB of free disk space for the app itself, plus additional space for captured data

### Runtime libraries

The app needs a few system libraries for audio, input devices, windowing, and graphics. Most desktop distributions already have these. If you are unsure, install them with your package manager.

**Debian / Ubuntu:**

```bash
sudo apt-get install -y \
  libasound2 \
  libudev1 \
  libwayland-client0 \
  libxkbcommon0 \
  libvulkan1 \
  mesa-vulkan-drivers
```

:::note
On Ubuntu 24.04 and later, the audio library package is named `libasound2t64`. `apt` usually picks the right one for you.
:::

**Fedora:**

```bash
sudo dnf install -y alsa-lib systemd-libs wayland libxkbcommon vulkan-loader mesa-vulkan-drivers
```

`mesa-vulkan-drivers` covers AMD and Intel graphics. If you use an NVIDIA card, install NVIDIA's proprietary driver, which includes Vulkan support.

## Download

1. Visit the [SSG Tether Capture releases page](https://github.com/Conjunction-Crew/ssg-tether-capture/releases) and navigate to the latest release.
2. Download the Linux version of the app, which will be a `.tar.gz`.
   - The file will be named something like `ssg-tether-capture-vx.y.z-linux-x86_64.tar.gz`, where `x.y.z` is the version number.

## Installation

The archive contains the `ssg_tether_capture` program and an `assets/` folder next to it. It does **not** contain a top-level folder, so extract it into a new folder of your own. Otherwise the files are scattered across your current directory.

1. Open a terminal and create a folder for the app. Anywhere you have write access works, for example:

   ```bash
   mkdir -p ~/ssg-tether-capture
   ```

2. Extract the archive into that folder, replacing the filename with the one you downloaded:

   ```bash
   tar -xzf ~/Downloads/ssg-tether-capture-vx.y.z-linux-x86_64.tar.gz -C ~/ssg-tether-capture
   ```

3. Confirm that the program and the assets folder are both there:

   ```bash
   ls ~/ssg-tether-capture
   # assets  ssg_tether_capture
   ```

4. If the program is not executable, make it so:

   ```bash
   chmod +x ~/ssg-tether-capture/ssg_tether_capture
   ```

## First Launch

:::warning Run the app from its own folder

The app looks for its `assets/` folder in the **current directory** when it starts. Always change into the install folder first. If you start it from somewhere else, it will not find its textures and fonts.

:::

1. From a terminal:

   ```bash
   cd ~/ssg-tether-capture
   ./ssg_tether_capture
   ```

2. The app window opens, and log output is printed in the terminal. Leave the terminal open while you use the app.
3. The first time the app starts, it asks you to **Select Working Directory**. This is the folder where your capture plans are stored. Click **Browse** to pick a folder, or edit the path directly, then click **Select**.
4. You can change the working directory later from the home screen. See [Running the App](./running-the-app.md#first-launch) for what to do next.

## Updating and Uninstalling

- **Updating:** download the newer `.tar.gz` and extract it into a new folder, or over the existing one. Your capture plans stay in your working directory.
- **Uninstalling:** delete the install folder, for example `rm -rf ~/ssg-tether-capture`. Your working directory and the capture plans in it are not deleted.

## Troubleshooting

| Problem | What to try |
|---|---|
| `error while loading shared libraries: lib...so: cannot open shared object file` | A runtime library is missing. Install the packages listed under [Runtime libraries](#runtime-libraries). |
| `version 'GLIBC_2.xx' not found` | Your distribution is older than the one the release was built on. Update your distribution, or [build the app from source](./prerequisites.md). |
| `Permission denied` when running `./ssg_tether_capture` | Run `chmod +x ssg_tether_capture`. |
| The app closes immediately or the window stays black | Check that your graphics driver supports Vulkan. If `vulkaninfo` is installed, run `vulkaninfo --summary` to check. Update your GPU driver, then try again. The terminal output usually shows the reason. |
| Textures or fonts are missing, or the app reports missing files | You started the app from the wrong directory. `cd` into the install folder and run `./ssg_tether_capture` again. |
| The **Browse** button does nothing | The folder picker needs a desktop portal service (such as `xdg-desktop-portal` with a backend for your desktop). Install it, or type the working directory path directly into the text field. |
