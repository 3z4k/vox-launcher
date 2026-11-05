# Vox Client (optional mod)

A small client-side Fabric mod that Vox Launcher can install into one instance: FPS and CPS counters,
keystrokes, optional coordinates and a zoom key. It sends nothing over the network and changes no gameplay.

- **Keys:** Right Shift toggles the HUD, C zooms (both can be rebound in Controls → Vox Client).
- **Settings:** edit `config/vox-client.json` in the instance's game folder (positions, scale, which lines show, zoom FOV).
- **Install / remove:** Instance → Overview → Vox Client in Vox Launcher. Removing deletes the one jar.

Built by the `client-mod` job in `.github/workflows/build-windows.yml` and attached to each tagged release as
`vox-client-<version>+mc<minecraft>.jar`. To support another Minecraft version, change `gradle.properties`
(values from https://fabricmc.net/develop) and add a build for it.

Status: written but not yet compiled or run in-game by its author. Expect to fix small API differences on the first CI run.
Cosmetics (capes, etc.) are not included: other players cannot see client-only cosmetics, and the hooks are version-specific.
