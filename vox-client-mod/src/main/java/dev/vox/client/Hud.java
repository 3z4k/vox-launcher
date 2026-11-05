package dev.vox.client;

import net.minecraft.client.MinecraftClient;
import net.minecraft.client.font.TextRenderer;
import net.minecraft.client.gui.DrawContext;
import net.minecraft.client.util.InputUtil;
import org.lwjgl.glfw.GLFW;

import java.util.ArrayList;
import java.util.List;

/** Draws the FPS / CPS / coordinates lines and the keystroke boxes. */
final class Hud {
    private static final ClickCounter LEFT = new ClickCounter();
    private static final ClickCounter RIGHT = new ClickCounter();

    private static final int ON = 0xAAFFFFFF;
    private static final int OFF = 0x66000000;
    private static final int TEXT = 0xFFFFFFFF;
    private static final int TEXT_DARK = 0xFF111111;

    private Hud() {
    }

    static void render(DrawContext ctx) {
        MinecraftClient mc = MinecraftClient.getInstance();
        Config cfg = Config.get();
        if (mc.player == null || mc.options.hudHidden) {
            return;
        }
        long window = mc.getWindow().getHandle();
        long now = System.currentTimeMillis();
        // Poll every frame so quick clicks are not missed between game ticks.
        LEFT.update(GLFW.glfwGetMouseButton(window, GLFW.GLFW_MOUSE_BUTTON_LEFT) == GLFW.GLFW_PRESS, now);
        RIGHT.update(GLFW.glfwGetMouseButton(window, GLFW.GLFW_MOUSE_BUTTON_RIGHT) == GLFW.GLFW_PRESS, now);
        if (!cfg.hud) {
            return;
        }

        TextRenderer font = mc.textRenderer;
        ctx.getMatrices().push();
        ctx.getMatrices().scale(cfg.scale, cfg.scale, 1f);
        int x = Math.round(cfg.x / cfg.scale);
        int y = Math.round(cfg.y / cfg.scale);

        List<String> lines = new ArrayList<>();
        if (cfg.showFps) {
            lines.add(mc.getCurrentFps() + " FPS");
        }
        if (cfg.showCps) {
            lines.add(LEFT.cps() + " | " + RIGHT.cps() + " CPS");
        }
        if (cfg.showCoords) {
            lines.add(String.format("XYZ %.0f %.0f %.0f", mc.player.getX(), mc.player.getY(), mc.player.getZ()));
        }
        for (String line : lines) {
            ctx.fill(x - 2, y - 2, x + font.getWidth(line) + 2, y + 10, OFF);
            ctx.drawTextWithShadow(font, line, x, y, TEXT);
            y += 14;
        }

        if (cfg.showKeys && mc.currentScreen == null) {
            y += 4;
            int s = 22;
            key(ctx, font, window, x + s + 2, y, s, "W", GLFW.GLFW_KEY_W);
            y += s + 2;
            key(ctx, font, window, x, y, s, "A", GLFW.GLFW_KEY_A);
            key(ctx, font, window, x + s + 2, y, s, "S", GLFW.GLFW_KEY_S);
            key(ctx, font, window, x + 2 * (s + 2), y, s, "D", GLFW.GLFW_KEY_D);
            y += s + 2;
            box(ctx, font, x, y, s + 10, s, "LMB " + LEFT.cps(), LEFT.isDown());
            box(ctx, font, x + s + 12, y, s + 10, s, "RMB " + RIGHT.cps(), RIGHT.isDown());
            y += s + 2;
            key(ctx, font, window, x, y, 3 * s + 4, "SPACE", GLFW.GLFW_KEY_SPACE);
        }
        ctx.getMatrices().pop();
    }

    private static void key(DrawContext ctx, TextRenderer font, long window, int x, int y, int w, String label, int code) {
        box(ctx, font, x, y, w, 22, label, InputUtil.isKeyPressed(window, code));
    }

    private static void box(DrawContext ctx, TextRenderer font, int x, int y, int w, int h, String label, boolean down) {
        ctx.fill(x, y, x + w, y + h, down ? ON : OFF);
        int tw = font.getWidth(label);
        ctx.drawText(font, label, x + (w - tw) / 2, y + (h - 8) / 2, down ? TEXT_DARK : TEXT, false);
    }
}
