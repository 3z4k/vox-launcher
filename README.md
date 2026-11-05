<p align="center">
  <img src="src-tauri/icons/icon.png" alt="Vox Launcher" width="120">
</p>

<h1 align="center">Vox Launcher</h1>

<p align="center">
  A fast, good-looking launcher for Minecraft: Java Edition.<br>
  Manage as many instances as you like, install mods and modpacks in a click, and get playing quickly.
</p>

<p align="center">
  <a href="https://github.com/sylphiras/vox-launcher/releases/latest"><b>Download for Windows</b></a>
</p>

---

## Why Vox?

- **Quick to open, light on memory.** It uses your system's built-in web view instead of bundling a whole browser, so it starts fast and stays small.
- **Quick to launch.** Once a game is set up, launching it doesn't re-download or re-check anything.
- **Looks the way you want.** Four themes, four layouts, and your own accent colour.
- **Everything in one place.** Instances, mods, worlds, screenshots, skins and logs, all inside the launcher.

## Features

### Instances
- Vanilla, **Fabric**, **Quilt**, **Forge** and **NeoForge**
- Groups, icons, notes and play time for every instance
- Duplicate, delete, export and import instances (`.zip`)
- Per-instance Java, memory, game window size, launch arguments, and commands that run before and after the game

### Mods and modpacks
- Search **Modrinth** from inside the launcher
- One-click install, with required dependencies added for you
- Turn mods on or off, delete them, and check for updates
- Install or import **Modrinth modpacks** (`.mrpack`)
- Resource packs and shader packs get the same treatment

### Worlds, screenshots and servers
- Back up and delete worlds
- Browse and delete screenshots
- Edit your multiplayer server list without starting the game
- Read game logs and crash reports

### Crash helper
When the game stops unexpectedly, Vox reads the crash report and explains in plain English what probably went wrong: out of memory, the wrong Java, a missing mod, a graphics driver problem, a damaged file. Where it can, it offers a button that fixes it.

### Accounts and skins
- Sign in with your Microsoft account, or add an offline profile for singleplayer
- **Skin library:** save skins, switch between them in one click, preview them from the front and back
- Upload a skin, reset it, or change your cape

### Sync your settings
Copy your keybinds and options from your regular `.minecraft` folder (or another instance) into any instance, once or automatically every time you launch.

### Make it yours
- **Themes:** Voxel (the default), Sleek, Bold and Minimal
- **Layouts:** Cover, Dense, Glass and Classic
- Accent colour, interface size, compact mode and reduced motion
- Console pop-up that shows game output when you launch (optional)
- Desktop notifications when downloads and installs finish
- Discord Rich Presence, with an optional "show the server I'm on" switch

### Stay up to date
Vox checks for new versions when it starts and can install them for you. If you ever want to remove it, there's an **Uninstall** button in Settings that lets you keep or delete your data.

### Vox Client (optional)
A small Fabric mod you can add to any Fabric or Quilt instance with one click: FPS and CPS counters, keystrokes and a zoom key. It's client-side only, and you can remove it just as easily. *(Currently for Minecraft 1.21.4.)*

## Getting started

1. Go to the [**Releases**](https://github.com/sylphiras/vox-launcher/releases/latest) page and download the Windows installer.
2. Run it. No administrator rights are needed.
3. Open Vox Launcher, choose **New instance**, pick a Minecraft version and a mod loader, and press **Play**.
4. Want mods? Open **Discover** to find them.

To play online, add your Microsoft account under **Accounts**. Offline profiles work for singleplayer and LAN.

## Your data

Everything lives in `%APPDATA%\Vox Launcher\data`. Open it from **Settings → Data and about**. Uninstalling keeps it unless you choose otherwise.

## Help and feedback

Found a bug or have an idea? [Open an issue](https://github.com/sylphiras/vox-launcher/issues). If the game crashed, use **Copy report** in the crash helper and paste it into your issue.

## Building it yourself

See [DEVELOPING.md](DEVELOPING.md).

---

<sub>Copyright © 2026 sylphiras. All rights reserved, see [LICENSE](LICENSE). Vox Launcher is not affiliated with Mojang Studios or Microsoft. Minecraft is a trademark of Mojang Studios.</sub>
