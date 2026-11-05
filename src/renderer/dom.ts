// DOM helpers shared by every page. Text is always set through text nodes, never innerHTML.
import type { InstanceConfig } from './types';

export type Child = Node | string | number | null | false | undefined | Child[];

function append(el: Node, child: Child): void {
  if (child === null || child === false || child === undefined) return;
  if (Array.isArray(child)) child.forEach((c) => append(el, c));
  else el.appendChild(typeof child === 'string' || typeof child === 'number' ? document.createTextNode(String(child)) : child);
}

export function h<K extends keyof HTMLElementTagNameMap>(tag: K, props: Record<string, unknown> = {}, ...kids: Child[]): HTMLElementTagNameMap[K] {
  const el = document.createElement(tag);
  for (const [k, v] of Object.entries(props)) {
    if (v === null || v === undefined || v === false) continue;
    if (k === 'class') el.className = String(v);
    else if (k === 'style') Object.assign(el.style, v as object);
    else if (k.startsWith('on')) el.addEventListener(k.slice(2).toLowerCase(), v as EventListener);
    else if (k === 'dataset') Object.assign(el.dataset, v as object);
    else if (k in el) (el as unknown as Record<string, unknown>)[k] = v;
    else el.setAttribute(k, String(v));
  }
  kids.forEach((c) => append(el, c));
  return el;
}

// ---------------------------------------------------------------------------------------------
// Icons
// ---------------------------------------------------------------------------------------------
type Shape = string | { c: [number, number, number] } | { r: [number, number, number, number, number] };

export const ICONS = {
  play: ['M7 4.5v15l12-7.5z'],
  stop: [{ r: [6, 6, 12, 12, 2] }],
  grid: [{ r: [4, 4, 7, 7, 1.5] }, { r: [13, 4, 7, 7, 1.5] }, { r: [4, 13, 7, 7, 1.5] }, { r: [13, 13, 7, 7, 1.5] }],
  user: [{ c: [12, 8, 4] }, 'M4 20c0-4 3.6-6 8-6s8 2 8 6'],
  terminal: [{ r: [3, 5, 18, 14, 2.5] }, 'M7 10l3 2-3 2M12.5 15H17'],
  gear: [{ c: [12, 12, 3] }, 'M12 2.5v3M12 18.5v3M4.5 4.5l2.1 2.1M17.4 17.4l2.1 2.1M2.5 12h3M18.5 12h3M4.5 19.5l2.1-2.1M17.4 6.6l2.1-2.1'],
  folder: ['M3 7.5A2.5 2.5 0 0 1 5.5 5H9l2 2.2h7.5A2.5 2.5 0 0 1 21 9.7v7.8a2.5 2.5 0 0 1-2.5 2.5h-13A2.5 2.5 0 0 1 3 17.5z'],
  copy: [{ r: [9, 9, 11, 11, 2] }, 'M5 15V6a2 2 0 0 1 2-2h9'],
  trash: ['M4 7h16M9 7V4.5h6V7M6.5 7l1 12.5h9l1-12.5'],
  edit: ['M4 20l1-4.5L16.5 4a2 2 0 0 1 3 0l.5.5a2 2 0 0 1 0 3L8.5 19z'],
  plus: ['M12 5v14M5 12h14'],
  search: [{ c: [11, 11, 6.5] }, 'M20 20l-4.2-4.2'],
  close: ['M6 6l12 12M18 6L6 18'],
  bolt: ['M13 3L5 13.5h6L10 21l8-10.5h-6z'],
  home: ['M3 11l9-7 9 7M5 10v10h5v-6h4v6h5V10'],
  compass: [{ c: [12, 12, 9] }, 'M15.5 8.5l-2 5-5 2 2-5z'],
  download: ['M12 4v11M7.5 11L12 15.5 16.5 11M5 20h14'],
  upload: ['M12 15V4M7.5 8L12 3.5 16.5 8M5 20h14'],
  cube: ['M12 3l8 4.5v9L12 21l-8-4.5v-9z', 'M4 7.5l8 4.5 8-4.5M12 12v9'],
  globe: [{ c: [12, 12, 9] }, 'M3 12h18M12 3c3 3 3 15 0 18M12 3c-3 3-3 15 0 18'],
  image: [{ r: [3, 4, 18, 16, 2.5] }, { c: [9, 10, 1.8] }, 'M4 17l5-5 4 4 3-3 4 4'],
  server: [{ r: [3, 4, 18, 6, 2] }, { r: [3, 14, 18, 6, 2] }, 'M7 7h.01M7 17h.01'],
  file: ['M6 3h8l4 4v14H6z', 'M14 3v4h4'],
  refresh: ['M20 11a8 8 0 1 0-2.3 5.7M20 4v7h-7'],
  check: ['M5 12.5l4.5 4.5L19 7.5'],
  external: ['M14 4h6v6M20 4l-9 9M18 14v5a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1h5'],
  chevronDown: ['M6 9l6 6 6-6'],
  chevronRight: ['M9 6l6 6-6 6'],
  back: ['M15 5l-7 7 7 7'],
  star: ['M12 3.5l2.7 5.6 6.1.9-4.4 4.3 1 6.1L12 17.5 6.6 20.4l1-6.1L3.2 10l6.1-.9z'],
  clock: [{ c: [12, 12, 9] }, 'M12 7v5l3 2'],
  keyboard: [{ r: [3, 6, 18, 12, 2.5] }, 'M7 10h.01M11 10h.01M15 10h.01M8 14h8'],
  sliders: ['M4 7h9M17 7h3M4 17h3M11 17h9', { c: [15, 7, 2] }, { c: [9, 17, 2] }],
  sparkle: ['M12 3l1.8 5.2L19 10l-5.2 1.8L12 17l-1.8-5.2L5 10l5.2-1.8z'],
  cpu: [{ r: [6, 6, 12, 12, 2] }, 'M9 3v3M15 3v3M9 18v3M15 18v3M3 9h3M3 15h3M18 9h3M18 15h3'],
  chat: ['M4 5h16v11H9l-5 4z'],
  info: [{ c: [12, 12, 9] }, 'M12 11v5M12 8h.01'],
  more: ['M5 12h.01M12 12h.01M19 12h.01'],
  sword: ['M14 4l6-1-1 6-9 9-3-3z', 'M7 14l-3 3M9 19l-2 2'],
  pickaxe: ['M4 8c4-4 12-4 16 0', 'M12 7v13M9 20h6'],
  heart: ['M12 20s-7-4.4-7-10a4 4 0 0 1 7-2.5A4 4 0 0 1 19 10c0 5.6-7 10-7 10z'],
  flame: ['M12 3c1 4 5 5.5 5 10a5 5 0 0 1-10 0c0-2 1-3 2-4 .3 1.2 1 2 2 2-1-3 0-5 1-8z'],
  gem: ['M6 4h12l3 5-9 11L3 9z', 'M3 9h18M9 4l-1 5 4 11 4-11-1-5'],
  pack: ['M4 8h16v12H4z', 'M4 8l2-4h12l2 4M12 8v12'],
  potion: ['M9 3h6M10 3v5l-4.5 8a3 3 0 0 0 2.6 4.5h7.8a3 3 0 0 0 2.6-4.5L14 8V3'],
  power: ['M12 3v8M7 6.5a7 7 0 1 0 10 0'],
  shield: ['M12 3l8 3v6c0 4.5-3.2 7.8-8 9-4.8-1.2-8-4.5-8-9V6z'],
  list: ['M8 6h12M8 12h12M8 18h12M4 6h.01M4 12h.01M4 18h.01'],
  layers: ['M12 3l9 5-9 5-9-5z', 'M3 13l9 5 9-5'],
  save: ['M5 4h11l3 3v13H5z', 'M8 4v5h7V4M8 20v-6h8v6'],
} satisfies Record<string, Shape[]>;

export type IconName = keyof typeof ICONS;

/** Icons offered when choosing an instance icon. */
export const INSTANCE_ICONS: IconName[] = ['cube', 'pickaxe', 'sword', 'heart', 'flame', 'gem', 'star', 'pack', 'potion', 'bolt', 'globe', 'shield'];

const NS = 'http://www.w3.org/2000/svg';

export function icon(name: IconName, size = 20): SVGSVGElement {
  const svg = document.createElementNS(NS, 'svg');
  const attrs: Record<string, string> = {
    viewBox: '0 0 24 24', width: String(size), height: String(size), fill: 'none', stroke: 'currentColor',
    'stroke-width': '1.8', 'stroke-linecap': 'round', 'stroke-linejoin': 'round', 'aria-hidden': 'true',
  };
  for (const [k, v] of Object.entries(attrs)) svg.setAttribute(k, v);
  for (const shape of ICONS[name] as Shape[]) {
    if (typeof shape === 'string') {
      const p = document.createElementNS(NS, 'path');
      p.setAttribute('d', shape);
      svg.appendChild(p);
    } else if ('c' in shape) {
      const c = document.createElementNS(NS, 'circle');
      c.setAttribute('cx', String(shape.c[0]));
      c.setAttribute('cy', String(shape.c[1]));
      c.setAttribute('r', String(shape.c[2]));
      svg.appendChild(c);
    } else {
      const r = document.createElementNS(NS, 'rect');
      const [x, y, w, hh, rx] = shape.r;
      for (const [k, v] of [['x', x], ['y', y], ['width', w], ['height', hh], ['rx', rx]] as const) r.setAttribute(k, String(v));
      svg.appendChild(r);
    }
  }
  return svg;
}

let logoCounter = 0;
/** The Vox cube mark. */
export function logo(size = 40): SVGSVGElement {
  const n = ++logoCounter;
  const svg = document.createElementNS(NS, 'svg');
  svg.setAttribute('viewBox', '0 0 512 512');
  svg.setAttribute('width', String(size));
  svg.setAttribute('height', String(size));
  svg.setAttribute('aria-hidden', 'true');
  const defs = document.createElementNS(NS, 'defs');
  const grad = (id: string, a: string, b: string, diag = false) => {
    const g = document.createElementNS(NS, 'linearGradient');
    g.setAttribute('id', id);
    g.setAttribute('x1', '0'); g.setAttribute('y1', '0'); g.setAttribute('x2', diag ? '1' : '0'); g.setAttribute('y2', '1');
    for (const [o, c] of [[0, a], [1, b]] as const) {
      const s = document.createElementNS(NS, 'stop');
      s.setAttribute('offset', String(o)); s.setAttribute('stop-color', c);
      g.appendChild(s);
    }
    defs.appendChild(g);
  };
  grad(`gl${n}`, '#FF8A4C', '#E8481F');
  grad(`gr${n}`, '#FFB27A', '#FF6B3D');
  grad(`gt${n}`, '#FFFFFF', '#FFD9C2', true);
  svg.appendChild(defs);
  for (const [d, f] of [
    ['M109 171 L256 256 L256 426 L109 341 Z', `gl${n}`],
    ['M403 171 L256 256 L256 426 L403 341 Z', `gr${n}`],
    ['M256 62 L350 116 L256 170 L162 116 Z', `gt${n}`],
  ] as const) {
    const p = document.createElementNS(NS, 'path');
    p.setAttribute('d', d);
    p.setAttribute('fill', `url(#${f})`);
    svg.appendChild(p);
  }
  return svg;
}

// ---------------------------------------------------------------------------------------------
// Formatting
// ---------------------------------------------------------------------------------------------
export function fmtBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  const u = ['KB', 'MB', 'GB', 'TB'];
  let v = n / 1024;
  let i = 0;
  while (v >= 1024 && i < u.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v >= 100 ? v.toFixed(0) : v.toFixed(1)} ${u[i]}`;
}

export function fmtNum(n: number): string {
  if (n >= 1e9) return `${(n / 1e9).toFixed(1)}B`;
  if (n >= 1e6) return `${(n / 1e6).toFixed(1)}M`;
  if (n >= 1e3) return `${(n / 1e3).toFixed(1)}K`;
  return String(n);
}

export function fmtDuration(ms: number): string {
  const mins = Math.floor(ms / 60000);
  if (mins < 1) return ms > 0 ? '< 1 min' : '0 min';
  if (mins < 60) return `${mins} min`;
  const hrs = Math.floor(mins / 60);
  return hrs < 100 ? `${hrs} h ${mins % 60} min` : `${hrs} h`;
}

export function fmtDate(ts: number): string {
  return ts ? new Date(ts).toLocaleDateString(undefined, { year: 'numeric', month: 'short', day: 'numeric' }) : '—';
}

export function relativeTime(ts: number | null): string {
  if (!ts) return 'Never played';
  const mins = Math.round((Date.now() - ts) / 60000);
  if (mins < 1) return 'Played just now';
  if (mins < 60) return `Played ${mins} min ago`;
  const hours = Math.round(mins / 60);
  if (hours < 24) return `Played ${hours} h ago`;
  const days = Math.round(hours / 24);
  return days < 60 ? `Played ${days} d ago` : `Played ${fmtDate(ts)}`;
}

export function debounce<A extends unknown[]>(fn: (...a: A) => void, ms: number): (...a: A) => void {
  let t: number | undefined;
  return (...a: A) => {
    window.clearTimeout(t);
    t = window.setTimeout(() => fn(...a), ms);
  };
}

export function errMsg(e: unknown): string {
  const raw = e instanceof Error ? e.message : String(e);
  return raw.replace(/^Error invoking remote method '[^']+': (Error: )?/, '');
}

// ---------------------------------------------------------------------------------------------
// Toasts
// ---------------------------------------------------------------------------------------------
export function toast(message: string, isError = false): void {
  const host = document.getElementById('toasts');
  if (!host) return;
  const el = h('div', { class: `toast${isError ? ' error' : ''}` }, message);
  host.appendChild(el);
  setTimeout(() => el.remove(), isError ? 9000 : 3500);
}

export async function attempt<T>(fn: () => Promise<T>): Promise<T | undefined> {
  try {
    return await fn();
  } catch (e) {
    toast(errMsg(e), true);
    return undefined;
  }
}

// ---------------------------------------------------------------------------------------------
// Instance avatars
// ---------------------------------------------------------------------------------------------
const PALETTE = ['#ff8a63', '#6cc4f5', '#b7a0fa', '#4fd9a6', '#f5c451', '#f58fb8', '#8fd14f', '#e76f51'];

export function colorFor(seed: string): string {
  let n = 0;
  for (const c of seed) n = (n * 31 + c.charCodeAt(0)) >>> 0;
  return PALETTE[n % PALETTE.length];
}

export function avatar(inst: Pick<InstanceConfig, 'id' | 'name' | 'icon'>, size = 44): HTMLElement {
  const known = (INSTANCE_ICONS as string[]).includes(inst.icon);
  return h(
    'div',
    { class: 'avatar', 'aria-hidden': 'true', style: { background: colorFor(inst.id), width: `${size}px`, height: `${size}px`, borderRadius: `${Math.round(size * 0.28)}px`, fontSize: `${Math.round(size * 0.42)}px` } },
    known ? icon(inst.icon as IconName, Math.round(size * 0.55)) : inst.name.charAt(0).toUpperCase(),
  );
}

// ---------------------------------------------------------------------------------------------
// Controls
// ---------------------------------------------------------------------------------------------
export function toggle(checked: boolean, onChange: (v: boolean) => void, label = ''): HTMLElement {
  const input = h('input', { type: 'checkbox', checked, 'aria-label': label || 'toggle', onChange: () => onChange(input.checked) });
  return h('label', { class: 'switch' }, input, h('span', { class: 'track' }, h('span', { class: 'thumb' })));
}

export function segmented<T extends string>(options: { value: T; label: string }[], value: T, onChange: (v: T) => void): HTMLElement {
  const wrap = h('div', { class: 'segmented', role: 'tablist' });
  for (const o of options) {
    wrap.appendChild(
      h('button', { type: 'button', role: 'tab', class: `seg${o.value === value ? ' active' : ''}`, 'aria-selected': String(o.value === value), onClick: () => onChange(o.value) }, o.label),
    );
  }
  return wrap;
}

export function select<T extends string>(options: { value: T; label: string }[], value: T, onChange: (v: T) => void): HTMLSelectElement {
  const el = h('select', { onChange: () => onChange(el.value as T) }, options.map((o) => h('option', { value: o.value, selected: o.value === value }, o.label)));
  el.value = value;
  return el;
}

export function field(label: string, control: HTMLElement, hint?: string): HTMLElement {
  const id = `f-${Math.random().toString(36).slice(2, 8)}`;
  control.id = id;
  return h('div', { class: 'field' }, h('label', { for: id }, label), control, hint ? h('div', { class: 'hint' }, hint) : null);
}

/** A settings row: label + description on the left, control on the right. */
export function row(label: string, desc: string | null, control: Child): HTMLElement {
  return h('div', { class: 'row' }, h('div', { class: 'row-text' }, h('div', { class: 'row-label' }, label), desc ? h('div', { class: 'hint' }, desc) : null), h('div', { class: 'row-control' }, control));
}

export function card(title: string | null, subtitle: string | null, ...kids: Child[]): HTMLElement {
  return h('section', { class: 'panel' }, title ? h('div', { class: 'panel-head' }, h('h2', {}, title), subtitle ? h('div', { class: 'sub' }, subtitle) : null) : null, ...kids);
}

export function emptyState(ic: IconName, title: string, text: string, ...actions: Child[]): HTMLElement {
  return h('div', { class: 'empty' }, h('div', { class: 'empty-icon' }, icon(ic, 30)), h('h2', {}, title), h('p', { class: 'sub' }, text), actions.length ? h('div', { class: 'head-actions' }, ...actions) : null);
}

export function spinner(label = 'Loading…'): HTMLElement {
  return h('div', { class: 'loading' }, h('span', { class: 'spin' }), label);
}

export function iconButton(name: IconName, label: string, onClick: (e: MouseEvent) => void, danger = false): HTMLButtonElement {
  return h('button', { class: `icon-btn${danger ? ' danger' : ''}`, type: 'button', 'aria-label': label, title: label, onClick }, icon(name, 17));
}

// ---------------------------------------------------------------------------------------------
// Dialogs
// ---------------------------------------------------------------------------------------------
export interface Modal {
  el: HTMLDialogElement;
  close: () => void;
}

export function openModal(content: HTMLElement, wide = false): Modal {
  const dlg = h('dialog', { class: wide ? 'wide' : '' }, content);
  dlg.addEventListener('close', () => dlg.remove());
  document.body.appendChild(dlg);
  dlg.showModal();
  return { el: dlg, close: () => dlg.close() };
}

export function dialogHead(title: string, m: () => Modal): HTMLElement {
  return h('div', { class: 'dlg-head' }, h('h2', {}, title), iconButton('close', 'Close', () => m().close()));
}

export function confirmDialog(message: string, action: string, danger = true, title = 'Are you sure?'): Promise<boolean> {
  return new Promise((resolve) => {
    let answer = false;
    const m: Modal = openModal(
      h(
        'div',
        { class: 'dlg' },
        h('h2', {}, title),
        h('p', { class: 'dlg-text' }, message),
        h('div', { class: 'dlg-actions' },
          h('button', { class: 'btn', type: 'button', onClick: () => m.close() }, 'Cancel'),
          h('button', { class: `btn ${danger ? 'danger-solid' : 'primary'}`, type: 'button', onClick: () => { answer = true; m.close(); } }, action)),
      ),
    );
    m.el.addEventListener('close', () => resolve(answer));
  });
}

export interface FormField {
  key: string;
  label: string;
  type?: 'text' | 'number' | 'select' | 'checkbox' | 'textarea';
  value?: string | number | boolean;
  options?: { value: string; label: string }[];
  placeholder?: string;
  hint?: string;
  list?: string[]; // suggestions for text fields
}

/** Small form in a dialog. Resolves with the entered values, or null when cancelled. */
export function formDialog(title: string, fields: FormField[], submit: string, intro?: string): Promise<Record<string, string | number | boolean> | null> {
  return new Promise((resolve) => {
    let result: Record<string, string | number | boolean> | null = null;
    const controls = new Map<string, HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement>();
    const datalists: HTMLElement[] = [];
    const body = fields.map((f) => {
      let c: HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement;
      if (f.type === 'select') {
        c = h('select', {}, (f.options ?? []).map((o) => h('option', { value: o.value }, o.label)));
        c.value = String(f.value ?? '');
      } else if (f.type === 'textarea') {
        c = h('textarea', { rows: 4, placeholder: f.placeholder ?? '' });
        c.value = String(f.value ?? '');
      } else if (f.type === 'checkbox') {
        c = h('input', { type: 'checkbox', checked: Boolean(f.value) });
      } else {
        c = h('input', { type: f.type ?? 'text', placeholder: f.placeholder ?? '' });
        c.value = String(f.value ?? '');
        if (f.list) {
          const id = `dl-${f.key}-${Math.random().toString(36).slice(2, 6)}`;
          c.setAttribute('list', id);
          datalists.push(h('datalist', { id }, f.list.map((v) => h('option', { value: v }))));
        }
      }
      controls.set(f.key, c);
      return f.type === 'checkbox'
        ? h('label', { class: 'check' }, c, f.label)
        : field(f.label, c, f.hint);
    });
    const m: Modal = openModal(
      h('div', { class: 'dlg' }, dialogHead(title, () => m), intro ? h('p', { class: 'dlg-text' }, intro) : null, ...body, ...datalists,
        h('div', { class: 'dlg-actions' },
          h('button', { class: 'btn', type: 'button', onClick: () => m.close() }, 'Cancel'),
          h('button', {
            class: 'btn primary', type: 'button',
            onClick: () => {
              const out: Record<string, string | number | boolean> = {};
              for (const f of fields) {
                const c = controls.get(f.key)!;
                out[f.key] = f.type === 'checkbox' ? (c as HTMLInputElement).checked : f.type === 'number' ? Number(c.value) : c.value;
              }
              result = out;
              m.close();
            },
          }, submit))),
    );
    m.el.addEventListener('close', () => resolve(result));
    (controls.get(fields[0]?.key ?? '') as HTMLElement | undefined)?.focus();
  });
}
