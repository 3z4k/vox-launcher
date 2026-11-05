# Developing Vox Launcher

Notes for people building or changing the launcher. Users: see the main README.

A fast Minecraft: Java Edition launcher. Rust engine (Tauri 2) with a custom HTML/CSS UI and four themes
(Voxel is the default) and four layouts: Cover (top bar and banner), Dense (icon rail, table and detail panel),
Glass (frosted cards and a floating dock) and Classic (sidebar). Both are chosen in Settings → Appearance.

## Features

- **Instances**: Vanilla, Fabric, Quilt, Forge and NeoForge; groups, icons, notes, play time, duplicate,
  export/import (`.zip`), per-instance Java, memory, JVM/game arguments, window size, wrapper command,
  pre-launch / post-exit commands and environment variables.
- **Mods and modpacks**: search Modrinth, install with required dependencies, enable/disable, update checks,
  install or import `.mrpack` modpacks. (CurseForge packs are not supported.)
- **Content tabs per instance**: mods, resource packs, shader packs, worlds (backup/delete), screenshots,
  servers (edits the in-game list), logs and crash reports.
- **Accounts and skins**: Microsoft sign-in, offline accounts, a saved-skin library (wear, rename, delete), front/back preview, skin upload/reset, cape switching.
- **Sync settings**: copy `options.txt` (keybinds), OptiFine and shader options, and the server list from your
  regular `.minecraft` or another instance, manually or automatically at launch.
- **Crash helper**: when the game stops unexpectedly, reads the crash report and log and explains the likely cause in
  plain English (out of memory, wrong Java, missing mod dependency, graphics driver, ...) with one-click fixes.
- **Console pop-up**: the game console opens as a pop-up on launch (toggle in Settings) and can be reopened from the
  corner button or an instance's header.
- **Updates and uninstall**: checks GitHub Releases on start and offers to install the new version; **Settings →
  Data and about → Uninstall** starts the uninstaller (keeping or deleting your data).
- **Notifications**: desktop alerts when downloads, installs or updates finish, or the game crashes.
- **Discord**: rich presence with an optional "show the server I am on" switch (off by default).
- **Vox Client (optional)**: a small Fabric mod (FPS, CPS, keystrokes, zoom) you can install into or remove from any
  Fabric/Quilt instance. Source in `vox-client-mod/`; see its README for its status.
- **Settings**: Java detection and download, memory, proxy, download threads, console, launcher behaviour,
  Discord presence, themes, accent colour, interface size, compact layout, reduced motion.

## Get the Windows installer (no tools needed on your PC)

1. Create a free GitHub repository and upload everything in this folder (drag and drop works on github.com,
   or `git push`). Keep the `.github` folder: some browsers hide it, so check that
   `.github/workflows/build-windows.yml` shows up in the repository after uploading.
2. Open the repository's **Actions** tab. The **Build Windows installer** workflow starts by itself
   (or press **Run workflow**).
3. When it turns green (about 10 minutes the first time, faster after), open the run and download
   **Vox-Launcher-Windows-Installer** at the bottom. Unzip it and run the `.exe`.

To publish a release instead, push a tag such as `v0.3.0`; the installer is attached to the release.

Running the installer on a PC that already has Vox Launcher offers to reinstall/upgrade it in place.

## Updates and the Vox Client mod need your GitHub repository

The workflow passes your repository name to the launcher (`VOX_UPDATE_REPO`), so update checks and the Vox Client
download work as soon as you publish a release: push a tag such as `v0.3.1` and both the installer and the mod are
attached to it. Until a release exists, the launcher just reports "No releases have been published yet".

## Settings you may want to change

Both IDs are build-time values in `src-tauri/src/config.rs`:

| Value | Purpose |
|---|---|
| `MS_CLIENT_ID` | Azure application (client) ID for Microsoft sign-in |
| `DISCORD_APP_ID` | Discord application ID for Rich Presence |

New Azure apps must be approved by Mojang before Microsoft sign-in works
(form: https://aka.ms/mce-reviewappid). Until then use **Accounts → Add offline account**
(singleplayer, LAN and offline-mode servers).

## Why it is fast

- Native Rust process and the system WebView (no bundled browser), so it opens quickly and uses little memory.
- A warm launch touches no network and hashes no files: existing files are trusted by size, a per-version
  "ready" stamp skips the asset folder entirely, natives are unpacked only when they change, and detected
  Javas are cached. **Instance → Overview → Verify files on next launch** forces a full hash check.
- Downloads run in parallel over HTTP/2 connections and are hashed while they stream.

## Your data

Data lives in `%APPDATA%\Vox Launcher\data`. If you used the earlier Electron version, its folder
(`%APPDATA%\vox-launcher\data`) is reused so your instances carry over. Microsoft sign-ins from the older
version cannot be read by this one, so sign in again once (offline accounts carry over).

## Develop locally

Needs Node 20+, Rust (stable) and the [Tauri prerequisites](https://tauri.app/start/prerequisites/).

```
npm install
npm run dev          # runs the app
cargo test --manifest-path src-tauri/Cargo.toml     # unit + end-to-end tests
```

`src-tauri/tests/e2e.rs` starts a fake Mojang server and launches a real JVM through the whole pipeline
(skipped when `javac` is not installed).

## Layout

- `src-tauri/src/` Rust engine: `core.rs` (version/rule/argument logic), `mojang.rs`, `java.rs`, `loaders.rs`,
  `auth.rs`, `launch.rs`, `instances.rs`, `modrinth.rs`, `packs.rs`, `content.rs`, `skins.rs`, `nbt.rs`,
  `discord.rs`, `lib.rs` (Tauri commands).
- `src/renderer/` the UI; `src/renderer/bridge.ts` connects it to the Rust commands.
- `src/renderer/pages/` one module per page; `state.ts` holds shared state and events; `dom.ts` the widgets.
- `scripts/build-web.js` type-checks the UI with tsc and bundles it with esbuild into `dist-web/`.
