package dev.vox.client;

import net.fabricmc.api.ClientModInitializer;
import net.fabricmc.fabric.api.client.event.lifecycle.v1.ClientTickEvents;
import net.fabricmc.fabric.api.client.keybinding.v1.KeyBindingHelper;
import net.fabricmc.fabric.api.client.rendering.v1.HudRenderCallback;
import net.minecraft.client.MinecraftClient;
import net.minecraft.client.option.KeyBinding;
import net.minecraft.client.util.InputUtil;
import org.lwjgl.glfw.GLFW;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

/**
 * Vox Client: a small, client-side-only HUD. It never touches the network and changes no gameplay.
 * Everything here is optional: delete the jar from the mods folder (or use Remove in Vox Launcher) to get rid of it.
 */
public class VoxClient implements ClientModInitializer {
    public static final Logger LOGGER = LoggerFactory.getLogger("vox_client");

    private static KeyBinding toggleHud;
    private static KeyBinding zoom;
    private static Integer savedFov = null;

    @Override
    public void onInitializeClient() {
        Config.load();
        toggleHud = KeyBindingHelper.registerKeyBinding(new KeyBinding("key.vox_client.toggle_hud", InputUtil.Type.KEYSYM, GLFW.GLFW_KEY_RIGHT_SHIFT, "key.categories.vox_client"));
        zoom = KeyBindingHelper.registerKeyBinding(new KeyBinding("key.vox_client.zoom", InputUtil.Type.KEYSYM, GLFW.GLFW_KEY_C, "key.categories.vox_client"));

        HudRenderCallback.EVENT.register((context, tickCounter) -> Hud.render(context));
        ClientTickEvents.END_CLIENT_TICK.register(VoxClient::tick);
        LOGGER.info("Vox Client loaded");
    }

    private static void tick(MinecraftClient client) {
        while (toggleHud.wasPressed()) {
            Config.get().hud = !Config.get().hud;
            Config.save();
        }
        applyZoom(client);
    }

    /** Zoom works by lowering the FOV option while the key is held, and putting the player's own value back after. */
    private static void applyZoom(MinecraftClient client) {
        if (client.options == null) {
            return;
        }
        boolean wanted = zoom.isPressed() && client.currentScreen == null;
        if (wanted && savedFov == null) {
            savedFov = client.options.getFov().getValue();
            client.options.getFov().setValue(Config.get().zoomFov);
        } else if (!wanted && savedFov != null) {
            client.options.getFov().setValue(savedFov);
            savedFov = null;
        }
    }
}
