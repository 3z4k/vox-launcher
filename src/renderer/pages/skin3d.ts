// A rotating 3D player model drawn with plain CSS 3D transforms (no libraries, works offline).
import { h } from '../dom';

let savedAngle = 25; // the next viewer carries on from here, so a page refresh does not snap the model back
const P = 9; // screen pixels per skin texel (the model is 32 texels tall)

interface Tex {
  src: string;
  /** Pixels of the source image per 1/64 of its width, so HD skins (128x128, ...) map correctly. */
  w: number;
  h: number;
}

function loadTex(src: string): Promise<Tex | null> {
  return new Promise((resolve) => {
    const img = new Image();
    img.onload = () => resolve({ src, w: img.naturalWidth, h: img.naturalHeight });
    img.onerror = () => resolve(null);
    img.src = src;
  });
}

interface Rect { u: number; v: number; w: number; h: number }

/** One flat face showing part of the texture. Sizes and offsets are in 64-wide texture units. */
function face(tex: Tex, r: Rect, cls: string, transform: string, flip = false): HTMLElement {
  const ratio = tex.h / tex.w;
  const el = h('div', { class: `s3-face ${cls}` });
  Object.assign(el.style, {
    width: `${r.w * P}px`,
    height: `${r.h * P}px`,
    backgroundImage: `url("${tex.src}")`,
    backgroundSize: `${64 * P}px ${64 * P * ratio}px`,
    backgroundPosition: `${-r.u * P}px ${-r.v * P}px`,
    transform: flip ? `${transform} scaleX(-1)` : transform,
  });
  return el;
}

interface BoxOpts {
  /** Texture origin of the box. */
  u: number;
  v: number;
  /** Grow the box on every side (the outer skin layer sits slightly outside the base layer). */
  inflate?: number;
  /** Old 64x32 skins reuse the right limb for the left one, mirrored. */
  mirror?: boolean;
}

/** A box w x h x d (texture units) built from six textured faces, centred on its own origin. */
function box(tex: Tex, w: number, hh: number, d: number, o: BoxOpts): HTMLElement {
  const g = o.inflate ?? 0;
  const [W, H, D] = [w + g * 2, hh + g * 2, d + g * 2];
  const { u, v } = o;
  const el = h('div', { class: 's3-box' });
  Object.assign(el.style, { width: `${W * P}px`, height: `${H * P}px`, left: '50%', top: '50%', marginLeft: `${(-W * P) / 2}px`, marginTop: `${(-H * P) / 2}px` });
  const m = o.mirror ?? false;
  // In the texture: right side, front, left side, back run left to right; top and bottom sit above them.
  const right: Rect = { u, v: v + d, w: d, h: hh };
  const front: Rect = { u: u + d, v: v + d, w, h: hh };
  const left: Rect = { u: u + d + w, v: v + d, w: d, h: hh };
  const back: Rect = { u: u + 2 * d + w, v: v + d, w, h: hh };
  const top: Rect = { u: u + d, v, w, h: d };
  const bottom: Rect = { u: u + d + w, v, w, h: d };
  const side = (rect: Rect, cls: string, tf: string, fw: number, fh: number, flip: boolean) => {
    const f = face({ ...tex }, rect, cls, tf, flip);
    f.style.width = `${fw * P}px`;
    f.style.height = `${fh * P}px`;
    // keep the texture scale of the unscaled rect even though inflated faces are a little bigger
    f.style.backgroundSize = `${64 * P * (fw / rect.w)}px ${64 * P * (tex.h / tex.w) * (fh / rect.h)}px`;
    f.style.backgroundPosition = `${-rect.u * P * (fw / rect.w)}px ${-rect.v * P * (fh / rect.h)}px`;
    el.appendChild(f);
  };
  // a mirrored limb swaps its two sides and flips the faces that show the texture sideways
  side(front, 'front', `translateZ(${(D * P) / 2}px)`, W, H, m);
  side(back, 'back', `rotateY(180deg) translateZ(${(D * P) / 2}px)`, W, H, m);
  side(m ? left : right, 'right', `rotateY(-90deg) translateZ(${(W * P) / 2}px)`, D, H, false);
  side(m ? right : left, 'left', `rotateY(90deg) translateZ(${(W * P) / 2}px)`, D, H, false);
  side(top, 'top', `rotateX(90deg) translateZ(${(H * P) / 2}px)`, W, D, m);
  side(bottom, 'bottom', `rotateX(-90deg) translateZ(${(H * P) / 2}px) rotate(180deg)`, W, D, m);
  // faces of boxes narrower/shallower than the box need centring
  el.querySelectorAll<HTMLElement>('.s3-face').forEach((f) => {
    f.style.left = `${(W * P - parseFloat(f.style.width)) / 2}px`;
    f.style.top = `${(H * P - parseFloat(f.style.height)) / 2}px`;
  });
  return el;
}

/** A group w x h (texture units) that holds boxes stacked on the same centre (base layer and outer layer). */
function group(w: number, hh: number, ...kids: HTMLElement[]): HTMLElement {
  const g = h('div', { class: 's3-limb' }, ...kids);
  Object.assign(g.style, { width: `${w * P}px`, height: `${hh * P}px` });
  return g;
}

/** Positions a group so its centre is (cx, cy) texture units from the model's top centre; the scene turns around the model's middle. */
function place(el: HTMLElement, cx: number, cy: number, w: number, hh: number): HTMLElement {
  Object.assign(el.style, { left: `${(cx - w / 2) * P}px`, top: `${(cy - 16 - hh / 2) * P}px` });
  return el;
}

export interface SkinViewer {
  el: HTMLElement;
  setCape: (src: string | null) => void;
  /** Turn the model to a given angle (degrees) and tilt. */
  setView: (angle: number, tilt?: number) => void;
  /** Spin and swing on (true) or hold still (false). */
  setAuto: (on: boolean) => void;
}

/** Builds the viewer. `skin` and `cape` are image URLs (data: URLs work). */
export function skinViewer(skin: string, slim: boolean, cape: string | null, reduceMotion: boolean): SkinViewer {
  const stage = h('div', { class: 's3-stage', role: 'img', 'aria-label': '3D skin preview. Drag to rotate.' });
  const scene = h('div', { class: 's3-scene' });
  const world = h('div', { class: 's3-world' }, scene);
  stage.appendChild(world);
  stage.appendChild(h('div', { class: 's3-hint' }, 'Drag to rotate'));

  const limbs: { el: HTMLElement; dir: number }[] = [];
  let capeEl: HTMLElement | null = null;
  let angle = savedAngle;
  let tilt = -8;
  let dragging = false;
  let auto = !reduceMotion;

  const build = async (): Promise<void> => {
    const tex = await loadTex(skin);
    if (!tex) {
      stage.replaceChildren(h('div', { class: 'sub' }, 'Could not draw this skin.'));
      return;
    }
    const legacy = tex.h <= tex.w / 2 + 1; // 64x32
    const arm = slim ? 3 : 4;
    const mk = (w: number, hh: number, d: number, base: [number, number], over: [number, number] | null, mirror: boolean) => {
      const g = group(w, hh, box(tex, w, hh, d, { u: base[0], v: base[1], mirror }));
      if (over && !legacy) g.appendChild(box(tex, w, hh, d, { u: over[0], v: over[1], inflate: 0.25 }));
      return g;
    };
    const head = group(8, 8, box(tex, 8, 8, 8, { u: 0, v: 0 }), box(tex, 8, 8, 8, { u: 32, v: 0, inflate: 0.5 }));
    const body = mk(8, 12, 4, [16, 16], [16, 32], false);
    const rArm = mk(arm, 12, 4, [40, 16], [40, 32], false);
    const lArm = legacy ? mk(arm, 12, 4, [40, 16], null, true) : mk(arm, 12, 4, [32, 48], [48, 48], false);
    const rLeg = mk(4, 12, 4, [0, 16], [0, 32], false);
    const lLeg = legacy ? mk(4, 12, 4, [0, 16], null, true) : mk(4, 12, 4, [16, 48], [0, 48], false);

    scene.append(
      place(head, 0, 4, 8, 8),
      place(body, 0, 14, 8, 12),
      place(rArm, -(4 + arm / 2), 14, arm, 12),
      place(lArm, 4 + arm / 2, 14, arm, 12),
      place(rLeg, -2, 26, 4, 12),
      place(lLeg, 2, 26, 4, 12),
    );
    // arms and legs swing from the shoulder / hip
    for (const [el, dir] of [[rArm, 1], [lArm, -1], [rLeg, -1], [lLeg, 1]] as const) {
      el.style.transformOrigin = '50% 0';
      limbs.push({ el, dir });
    }
    capeEl = h('div', { class: 's3-cape' });
    scene.appendChild(capeEl);
    await applyCape(cape);
    frame();
  };

  const applyCape = async (src: string | null): Promise<void> => {
    if (!capeEl) return;
    capeEl.replaceChildren();
    if (!src) return;
    const t = await loadTex(src);
    if (!t) return;
    // outside of the cape = texture (1,1); the side against the back = (12,1)
    const b = box(t, 10, 16, 1, { u: 0, v: 0 });
    b.style.transform = 'rotateY(180deg)'; // its outside faces away from the body
    const g = group(10, 16, b);
    place(g, 0, 16, 10, 16);
    g.style.transformOrigin = '50% 0';
    g.style.transform = `translateZ(${-2.7 * P}px) rotateX(6deg)`;
    capeEl.appendChild(g);
  };

  let t0 = performance.now();
  const frame = (): void => {
    if (!stage.isConnected && performance.now() - t0 > 2000) return; // the page was re-rendered: stop quietly
    const t = (performance.now() - t0) / 1000;
    if (auto && !dragging) angle += 0.6;
    savedAngle = angle;
    scene.style.transform = `rotateX(${tilt}deg) rotateY(${angle}deg)`;
    const swing = auto && !dragging ? Math.sin(t * 2) * 14 : 0;
    for (const l of limbs) l.el.style.transform = `rotateX(${swing * l.dir}deg)`;
    requestAnimationFrame(frame);
  };

  // drag to turn the model
  let lastX = 0;
  let lastY = 0;
  stage.addEventListener('pointerdown', (e) => {
    dragging = true;
    lastX = e.clientX;
    lastY = e.clientY;
    stage.setPointerCapture(e.pointerId);
    stage.classList.add('grabbing');
  });
  stage.addEventListener('pointermove', (e) => {
    if (!dragging) return;
    angle += (e.clientX - lastX) * 0.9;
    tilt = Math.max(-40, Math.min(35, tilt - (e.clientY - lastY) * 0.4));
    lastX = e.clientX;
    lastY = e.clientY;
    if (reduceMotion) scene.style.transform = `rotateX(${tilt}deg) rotateY(${angle}deg)`;
  });
  const end = (): void => { dragging = false; stage.classList.remove('grabbing'); };
  stage.addEventListener('pointerup', end);
  stage.addEventListener('pointercancel', end);
  stage.addEventListener('dblclick', () => { auto = !auto; });

  void build().then(() => { if (reduceMotion) scene.style.transform = `rotateX(${tilt}deg) rotateY(${angle}deg)`; });
  return {
    el: stage,
    setCape: (src) => void applyCape(src),
    setAuto: (on) => { auto = on && !reduceMotion; },
    setView: (a, t = -8) => { angle = a; tilt = t; if (reduceMotion || !auto) scene.style.transform = `rotateX(${tilt}deg) rotateY(${angle}deg)`; },
  };
}
