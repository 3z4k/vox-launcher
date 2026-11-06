<h1 align="center">🛠️ Developing Vox Launcher</h1>

<p align="center">
  Notes for people building or changing the launcher.<br>
  <sub>Just want to use it? See the <a href="README.md">main README</a>.</sub>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/engine-Rust-DEA584?style=for-the-badge&logo=rust&logoColor=black" alt="Rust">
  <img src="https://img.shields.io/badge/framework-Tauri%202-24C8DB?style=for-the-badge&logo=tauri&logoColor=white" alt="Tauri 2">
  <img src="https://img.shields.io/badge/UI-HTML%20%2F%20CSS%20%2F%20TS-7c5cff?style=for-the-badge" alt="HTML/CSS/TS">
  <img src="https://img.shields.io/badge/platform-Windows-0078D6?style=for-the-badge&logo=windows&logoColor=white" alt="Windows">
</p>

<p align="center">
  <a href="#-overview">Overview</a> •
  <a href="#-features">Features</a> •
  <a href="#-get-the-windows-installer">Installer</a> •
  <a href="#-settings-you-may-want-to-change">Settings</a> •
  <a href="#-why-it-is-fast">Performance</a> •
  <a href="#-develop-locally">Develop</a> •
  <a href="#-project-layout">Layout</a>
</p>

---

## 🧭 Overview

A fast Minecraft: Java Edition launcher. **Rust engine (Tauri 2)** with a custom HTML/CSS UI.

| | |
|---|---|
| 🎨 **Themes** | Four themes, **Voxel** is the default |
| 🧱 **Layouts** *(alpha)* | **Cover** (top bar and banner), **Dense** (icon rail, table and detail panel), **Glass** (frosted cards and a floating dock) and **Classic** (sidebar) |
| ⚙️ **Where to change them** | Settings → Appearance |

---

## 🧩 Features

<details open>
<summary><b>🗂️ Instances</b></summary>

<br>

Vanilla, Fabric, Quilt, Forge and NeoForge; groups, icons, notes, play time, duplicate, export/import (`.zip`), per-instance Java, memory, JVM/game arguments, window size, wrapper command, pre-launch / post-exit commands and environment variables.

</details>

<details open>
<summary><b>🔧 Mods and modpacks</b></summary>

<br>

Search Modrinth, install with required dependencies, enable/disable, update checks, install or import `.mrpack` modpacks.

> [!NOTE]
> CurseForge packs are not supported.

</details>

<details open>
<summary><b>📑 Content tabs per instance</b></summary>

<br>

Mods, resource packs, shader packs, worlds (backup/delete), screenshots, servers (edits the in-game list), logs and crash reports.

</details>

<details open>
<summary><b>👤 Accounts and skins</b></summary>

<br>

Microsoft sign-in, offline accounts, a saved-skin library (wear, rename, delete), front/back preview, skin upload/reset, cape switching.

</details>

<details open>
<summary><b>🔄 Sync settings</b></summary>

<br>

Copy `options.txt` (keybinds), OptiFine and shader options, and the server list from your regular `.minecraft` or another instance, manually or automatically at launch.

</details>

<details open>
<summary><b>🛟 Crash helper</b></summary>

<br>

When the game stops unexpectedly, reads the crash report and log and explains the likely cause in plain English (out of memory, wrong Java, missing mod dependency, graphics driver, ...) with one-click fixes.

</details>

<details open>
<summary><b>🖥️ Console pop-up</b></summary>

<br>

The game console opens as a pop-up on launch (toggle in Settings) and can be reopened from the corner button or an instance's header.

</details>

<details open>
<summary><b>🔔 Updates, uninstall and notifications</b></summary>

<br>

- **Updates:** checks GitHub Releases on start and offers to install the new version.
- **Uninstall:** **Settings → Data and about → Uninstall** starts the uninstaller (keeping or deleting your data).
- **Notifications:** desktop alerts when downloads, installs or updates finish, or the game crashes.

</details>

<details open>
<summary><b>💬 Discord</b></summary>

<br>

Rich presence with an optional "show the server I am on" switch (off by default).

</details>

<details open>
<summary><b>🎯 Vox Client (optional)</b></summary>

<br>

A small Fabric mod (FPS, CPS, keystrokes, zoom) you can install into or remove from any Fabric/Quilt instance. Source in `vox-client-mod/`; see its README for its status.

</details>

<details open>
<summary><b>⚙️ Settings</b></summary>

<br>

Java detection and download, memory, proxy, download threads, console, launcher behaviour, Discord presence, themes, accent colour, interface size, compact layout, reduced motion.

</details>

---

## 📦 Get the Windows installer

*No tools needed on your PC.*

1. 📤 **Create a free GitHub repository** and upload everything in this folder (drag and drop works on github.com, or `git push`).
2. ▶️ Open the repository's **Actions** tab. The **Build Windows installer** workflow starts by itself (or press **Run workflow**).
3. ✅ When it turns green (about 10 minutes the first time, faster after), open the run and download **Vox-Launcher-Windows-Installer** at the bottom. Unzip it and run the `.exe`.

> [!IMPORTANT]
> Keep the `.github` folder. Some browsers hide it, so check that `.github/workflows/build-windows.yml` shows up in the repository after uploading.

🏷️ To publish a release instead, push a tag such as `v0.3.0`; the installer is attached to the release.

🔁 Running the installer on a PC that already has Vox Launcher offers to reinstall/upgrade it in place.

---

## 🔗 Updates and the Vox Client mod need your GitHub repository

The workflow passes your repository name to the launcher (`VOX_UPDATE_REPO`), so update checks and the Vox Client download work as soon as you publish a release: push a tag such as `v0.3.1` and both the installer and the mod are attached to it.

> [!NOTE]
> Until a release exists, the launcher just reports "No releases have been published yet".

---

## 🎛️ Settings you may want to change

Both IDs are build-time values in `src-tauri/src/config.rs`:

| Value | Purpose |
|---|---|
| `MS_CLIENT_ID` | Azure application (client) ID for Microsoft sign-in |
| `DISCORD_APP_ID` | Discord application ID for Rich Presence |

> [!WARNING]
> New Azure apps must be approved by Mojang before Microsoft sign-in works ([review form](https://aka.ms/mce-reviewappid)). Until then use **Accounts → Add offline account** (singleplayer, LAN and offline-mode servers).

---

## ⚡ Why it is fast

- 🦀 **Native Rust process and the system WebView** (no bundled browser), so it opens quickly and uses little memory.
- 🔥 **Warm launches touch no network and hash no files.** Existing files are trusted by size, a per-version "ready" stamp skips the asset folder entirely, natives are unpacked only when they change, and detected Javas are cached.
- 🌐 **Parallel downloads** over HTTP/2 connections, hashed while they stream.

> [!TIP]
> **Instance → Overview → Verify files on next launch** forces a full hash check.

---

## 📁 Your data

Data lives in:

```
%APPDATA%\Vox Launcher\data
```

If you used the earlier Electron version, its folder (`%APPDATA%\vox-launcher\data`) is reused so your instances carry over.

> [!NOTE]
> Microsoft sign-ins from the older version cannot be read by this one, so sign in again once (offline accounts carry over).

---

## 💻 Develop locally

Needs **Node 20+**, **Rust (stable)** and the [Tauri prerequisites](https://tauri.app/start/prerequisites/).

```bash
npm install
npm run dev                                          # runs the app
cargo test --manifest-path src-tauri/Cargo.toml      # unit + end-to-end tests
```

`src-tauri/tests/e2e.rs` starts a fake Mojang server and launches a real JVM through the whole pipeline (skipped when `javac` is not installed).

---

## 🗺️ Project layout

### 🦀 `src-tauri/src/` (Rust engine)

| File | Role |
|---|---|
| `core.rs` | Version, rule and argument logic |
| `mojang.rs` | Mojang endpoints |
| `java.rs` | Java detection and download |
| `loaders.rs` | Mod loaders |
| `auth.rs` | Sign-in |
| `launch.rs` | Game launch |
| `instances.rs` | Instance management |
| `modrinth.rs` | Modrinth integration |
| `packs.rs` | Modpacks |
| `content.rs` | Per-instance content |
| `skins.rs` | Skins and capes |
| `nbt.rs` | NBT reading and writing |
| `discord.rs` | Rich Presence |
| `lib.rs` | Tauri commands |

### 🖼️ `src/renderer/` (UI)

| Path | Role |
|---|---|
| `bridge.ts` | Connects the UI to the Rust commands |
| `pages/` | One module per page |
| `state.ts` | Shared state and events |
| `dom.ts` | The widgets |

### 🏗️ Build

`scripts/build-web.js` type-checks the UI with `tsc` and bundles it with `esbuild` into `dist-web/`.

---

<p align="center">
  <sub>Back to the <a href="README.md">main README</a></sub>
</p>
