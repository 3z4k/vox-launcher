// The custom theme: a handful of colours the player picks, from which every other token is derived.

export const COLOR_KEYS = ['bg', 'side', 'panel', 'text', 'muted', 'accent', 'border'] as const;
export type ColorKey = (typeof COLOR_KEYS)[number];

export const COLOR_LABELS: Record<ColorKey, { label: string; hint: string }> = {
  bg: { label: 'Background', hint: 'Behind everything' },
  side: { label: 'Sidebar and bars', hint: 'The navigation area' },
  panel: { label: 'Cards', hint: 'Panels, lists and dialogs' },
  text: { label: 'Text', hint: 'Main text colour' },
  muted: { label: 'Quiet text', hint: 'Descriptions and hints' },
  accent: { label: 'Accent', hint: 'Buttons and highlights' },
  border: { label: 'Lines', hint: 'Borders and dividers' },
};

export const CUSTOM_DEFAULT: Record<string, string> = {
  bg: '#101216', side: '#0b0c0f', panel: '#171a20', text: '#eceff3', muted: '#8d97a5', accent: '#ff6b3d', border: '#2a3039', radius: '14',
};

export const PRESETS: { name: string; colors: Record<string, string> }[] = [
  { name: 'Forest', colors: { bg: '#0f1512', side: '#0a100d', panel: '#16201b', text: '#e8f1ea', muted: '#8aa393', accent: '#46d19a', border: '#243329', radius: '14' } },
  { name: 'Ocean', colors: { bg: '#0a1118', side: '#070d13', panel: '#111c27', text: '#e6f0fa', muted: '#82a0bb', accent: '#38bdf8', border: '#1f3345', radius: '16' } },
  { name: 'Rose', colors: { bg: '#160f14', side: '#100a0e', panel: '#201419', text: '#f8eaf0', muted: '#b08c9b', accent: '#f472b6', border: '#35212b', radius: '18' } },
  { name: 'Paper', colors: { bg: '#f4f1ea', side: '#ebe6db', panel: '#fffdf8', text: '#24221d', muted: '#746f63', accent: '#2f6f5e', border: '#d8d1c2', radius: '10' } },
  { name: 'Midnight', colors: { bg: '#05060a', side: '#000000', panel: '#0d0f18', text: '#e9ecff', muted: '#7d84a8', accent: '#a78bfa', border: '#1a1e33', radius: '8' } },
];

const HEX = /^#[0-9a-f]{6}$/i;

function rgb(hex: string): [number, number, number] {
  const n = parseInt(hex.slice(1), 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

function toHex(c: number[]): string {
  return `#${c.map((v) => Math.max(0, Math.min(255, Math.round(v))).toString(16).padStart(2, '0')).join('')}`;
}

/** `amount` of `b` blended into `a`. */
export function mix(a: string, b: string, amount: number): string {
  const [x, y] = [rgb(a), rgb(b)];
  return toHex(x.map((v, i) => v + (y[i]! - v) * amount));
}

export function luminance(hex: string): number {
  const [r, g, b] = rgb(hex).map((v) => {
    const c = v / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  }) as [number, number, number];
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

/** Contrast ratio between two colours (1 to 21). */
export function contrast(a: string, b: string): number {
  const [la, lb] = [luminance(a), luminance(b)];
  return (Math.max(la, lb) + 0.05) / (Math.min(la, lb) + 0.05);
}

export function complete(custom: Record<string, string> | undefined): Record<string, string> {
  custom = custom ?? {};
  const out = { ...CUSTOM_DEFAULT };
  for (const k of [...COLOR_KEYS]) if (HEX.test(custom[k] ?? '')) out[k] = custom[k]!;
  const r = Number(custom.radius);
  if (Number.isFinite(r) && r >= 0 && r <= 28) out.radius = String(Math.round(r));
  return out;
}

/** Every CSS variable the app needs, worked out from the picked colours. */
export function customVars(custom: Record<string, string> | undefined): Record<string, string> {
  const c = complete(custom);
  const light = luminance(c.bg!) > 0.45;
  const [a, ar] = [c.accent!, rgb(c.accent!)];
  const radius = Number(c.radius);
  return {
    '--bg': c.bg!, '--bg-side': c.side!, '--panel': c.panel!, '--text': c.text!, '--muted': c.muted!, '--border': c.border!,
    '--panel-2': mix(c.panel!, c.text!, 0.05), '--hover': mix(c.panel!, c.text!, 0.1),
    '--accent': a, '--accent-text': light ? mix(a, '#000000', 0.15) : mix(a, '#ffffff', 0.12),
    '--on-accent': luminance(a) > 0.4 ? '#0e1116' : '#ffffff',
    '--accent-soft': `rgba(${ar[0]}, ${ar[1]}, ${ar[2]}, ${light ? 0.1 : 0.15})`,
    '--hero': `linear-gradient(135deg, rgba(${ar[0]}, ${ar[1]}, ${ar[2]}, .22), rgba(${ar[0]}, ${ar[1]}, ${ar[2]}, .04) 55%), ${c.panel!}`,
    '--radius': `${radius}px`, '--radius-sm': `${Math.max(2, Math.round(radius * 0.65))}px`,
    '--shadow': light ? '0 8px 26px rgba(30, 30, 40, .12)' : '0 10px 34px rgba(0, 0, 0, .45)',
    '--danger': light ? '#c93a37' : '#f0605d', '--ok': light ? '#1f9c6b' : '#46d19a', '--warn': light ? '#b9801b' : '#f2b84b',
  };
}

export const CUSTOM_VAR_NAMES = Object.keys(customVars({}));

export function isLight(custom: Record<string, string> | undefined): boolean {
  return luminance(complete(custom).bg!) > 0.45;
}

// ---- files ----------------------------------------------------------------------------------

export function themeFile(name: string, custom: Record<string, string>): string {
  return JSON.stringify({ 'vox-theme': 1, name, ...complete(custom) }, null, 2);
}

/** Reads a theme file. Throws a message fit to show the player. */
export function parseThemeFile(text: string): { name: string; colors: Record<string, string> } {
  let v: unknown;
  try {
    v = JSON.parse(text);
  } catch {
    throw new Error('That file is not a Vox theme.');
  }
  if (!v || typeof v !== 'object' || (v as Record<string, unknown>)['vox-theme'] !== 1) throw new Error('That file is not a Vox theme.');
  const o = v as Record<string, unknown>;
  const colors: Record<string, string> = {};
  for (const k of COLOR_KEYS) {
    const val = o[k] ?? (o.colors as Record<string, unknown> | undefined)?.[k];
    if (typeof val === 'string' && HEX.test(val)) colors[k] = val.toLowerCase();
  }
  if (Object.keys(colors).length === 0) throw new Error('That theme file has no colours in it.');
  const r = Number(o.radius);
  if (Number.isFinite(r) && r >= 0 && r <= 28) colors.radius = String(Math.round(r));
  return { name: typeof o.name === 'string' && o.name.trim() ? o.name.trim().slice(0, 40) : 'Imported theme', colors: complete(colors) };
}
