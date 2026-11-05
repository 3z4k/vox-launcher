package dev.vox.client;

import com.google.gson.Gson;
import com.google.gson.GsonBuilder;
import net.fabricmc.loader.api.FabricLoader;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;

/** Settings, stored as JSON in the game's config folder (config/vox-client.json). Edit the file to change them. */
public final class Config {
    private static final Gson GSON = new GsonBuilder().setPrettyPrinting().create();
    private static final Path FILE = FabricLoader.getInstance().getConfigDir().resolve("vox-client.json");

    public boolean hud = true;
    public boolean showFps = true;
    public boolean showCps = true;
    public boolean showKeys = true;
    public boolean showCoords = false;
    /** HUD scale, 0.5 to 2.0. */
    public float scale = 1.0f;
    public int x = 6;
    public int y = 6;
    /** Field of view while the zoom key is held (30 to 110). */
    public int zoomFov = 30;

    private static Config instance = new Config();

    public static Config get() {
        return instance;
    }

    public static void load() {
        try {
            if (Files.exists(FILE)) {
                Config c = GSON.fromJson(Files.readString(FILE), Config.class);
                if (c != null) {
                    instance = c;
                }
            }
        } catch (IOException | RuntimeException e) {
            VoxClient.LOGGER.warn("Could not read {}, using defaults", FILE, e);
        }
        instance.scale = Math.max(0.5f, Math.min(2.0f, instance.scale));
        instance.zoomFov = Math.max(30, Math.min(110, instance.zoomFov));
        save();
    }

    public static void save() {
        try {
            Files.writeString(FILE, GSON.toJson(instance));
        } catch (IOException e) {
            VoxClient.LOGGER.warn("Could not write {}", FILE, e);
        }
    }
}
