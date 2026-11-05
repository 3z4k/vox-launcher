package dev.vox.client;

import java.util.ArrayDeque;

/** Clicks per second: counts presses over the last second. Fed once per rendered frame. */
public final class ClickCounter {
    private final ArrayDeque<Long> presses = new ArrayDeque<>();
    private boolean down;

    public void update(boolean pressed, long nowMs) {
        if (pressed && !down) {
            presses.addLast(nowMs);
        }
        down = pressed;
        while (!presses.isEmpty() && nowMs - presses.peekFirst() > 1000) {
            presses.removeFirst();
        }
    }

    public int cps() {
        return presses.size();
    }

    public boolean isDown() {
        return down;
    }
}
