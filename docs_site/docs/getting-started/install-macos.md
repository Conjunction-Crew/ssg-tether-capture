---
sidebar_position: 5
---

# Installing on macOS

This guide walks through installing SSG Tether Capture on macOS.

## Requirements

- A macOS device with the following specifications:
  - Running version 11 (Big Sur) or later
  - Which has an ARM based Apple Silicon processor (the app is not compatible with Intel-based Macs)
  - At least 8GB of RAM (16GB or more recommended for optimal performance)
  - At least 500MB of free disk space for the app itself, plus additional space for captured data

## Download

1. Visit the [SSG Tether Capture releases page](https://github.com/Conjunction-Crew/ssg-tether-capture/releases) and navigate to the latest release.

![GitHub releases page](/img/getting-started/install-guides-release-page-screenshot.png)

2. Download the macOS version of the app, which will be a `.dmg`
  - The file will be named something like `ssg-tether-capture-x.y.z-macos.dmg`, where `x.y.z` is the version number.

## Installation

:::note

The screenshots in this section were taken on a Mac running macOS Tahoe version 26.4.1, but the installation process should be similar on other recent versions of macOS. The installer that was used was downloaded from the GitHub releases page, and is from release v0.2.0.

:::

1. Once the `.dmg` file has finished downloading, locate it in your Downloads folder and double-click to open it.
2. The `.dmg` will mount and a new window will appear showing the SSG Tether Capture app icon

![Application DMG mounted](/img/getting-started/install-macos/macos-install-guide-screenshot-1.png)

3. Drag the SSG Tether Capture app icon into your Applications folder shortcut in the same window

![Drag app to Applications folder](/img/getting-started/install-macos/macos-install-guide-screenshot-2.png)

  - Depending on how your machine is configured, ytou may be prompted to enter your administrator password to allow the app to be copied to the Applications folder. Enter your password and click "OK" to proceed.

  ![Password prompt during installation](/img/getting-started/install-macos/macos-install-guide-screenshot-3.png)

4. Once the app has finished copying, you can eject the mounted `.dmg` by right-clicking on it in the Finder sidebar and selecting "Eject".

## First Launch

1. Open your Applications folder and locate the SSG Tether Capture app. You can also use Spotlight Search (Cmd + Space) and type "SSG Tether Capture" to find it quickly.
2. Double-click the app to launch it for the first time.
3. The first time you launch the app, you may see a security warning since it is not from an identified developer.

![Security warning on first launch](/img/getting-started/install-macos/macos-install-guide-screenshot-4.png)

4. To bypass this, click "Done" on the warning dialog. Do not click "Move to Trash" from this dialog.
5. Next, you need to allow the app to run from the Security & Privacy settings:
6. Open System Preferences and go to "Security & Privacy" and scroll all the way down to the bottom of the menu.

![View Warning in Security & Privacy settings](/img/getting-started/install-macos/macos-install-guide-screenshot-5.png)

7. Under the "Security" section, you should see a message "SSG Tether Capture.app was blocked to protect your Mac". Click "Open Anyway".
8. Confirm that you want to open the app in the next dialog by entering your administrator username and password.

![Enter your admin credentials to allow the app to run](/img/getting-started/install-macos/macos-install-guide-screenshot-6.png)

9. After entering your credentials, you will be prompted one more time to confirm that you want to open the app. Click "Open Anyway" to proceed.

![Final confirmation to open the app](/img/getting-started/install-macos/macos-install-guide-screenshot-7.png)

10. The app should now launch successfully and you will see the main interface. You will not be prompted with the security warning again unless you upgrade to a new version of the app in the future.
