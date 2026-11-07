// Launcher settings. Every change saves immediately.
import { api } from '../bridge';
import { attempt, card, confirmDialog, field, formDialog, h, icon, logo, row, select, toast, toggle, type IconName } from '../dom';
import { checkForUpdate } from './update';
import { applyAppearance, navigate, state } from '../state';
import { COLOR_KEYS, COLOR_LABELS, complete, contrast, CUSTOM_DEFAULT, parseThemeFile, PRESETS, themeFile } from '../theme';
import type { JavaInstall, Layout, Settings, Theme } from '../types';

export const APP_VERSION = '0.3.1';

const SECTIONS: { id: string; label: string; icon: IconName }[] = [
  { id: 'general', label: 'General', icon: 'gear' },
  { id: 'appearance', label: 'Appearance', icon: 'sparkle' },
  { id: 'java', label: 'Java', icon: 'cpu' },
  { id: 'game', label: 'Minecraft', icon: 'cube' },
  { id: 'launch', label: 'Launch', icon: 'play' },
  { id: 'network', label: 'Network', icon: 'globe' },
  { id: 'discord', label: 'Discord', icon: 'chat' },
  { id: 'about', label: 'Data and about', icon: 'info' },
];

const THEMES: { id: Theme; label: string; desc: string; colors: string[] }[] = [
  { id: 'voxel', label: 'Voxel', desc: 'Dark with orange', colors: ['#0a0b0d', '#161a1f', '#ff6b3d'] },
  { id: 'sleek', label: 'Sleek', desc: 'Dark, soft and blue', colors: ['#080a12', '#121627', '#7c8cff'] },
  { id: 'bold', label: 'Bold', desc: 'High contrast, sharp', colors: ['#000000', '#131316', '#ffd400'] },
  { id: 'minimal', label: 'Minimal', desc: 'Light and quiet', colors: ['#efeeea', '#ffffff', '#d9481c'] },
];
const LAYOUTS: { id: Layout; label: string; desc: string }[] = [
  { id: 'cover', label: 'Cover', desc: 'Top bar, big banner, cover tiles' },
  { id: 'dense', label: 'Dense', desc: 'Icon rail, table and detail panel' },
  { id: 'glass', label: 'Glass', desc: 'Frosted cards and a floating dock' },
  { id: 'classic', label: 'Classic', desc: 'Sidebar and card grid' },
  { id: 'compact', label: 'Compact', desc: 'Slim sidebar, tighter spacing' },
  { id: 'right', label: 'Right rail', desc: 'Sidebar on the right-hand side' },
  { id: 'bar', label: 'Bottom bar', desc: 'Tabs along the bottom, like a phone' },
  { id: 'zen', label: 'Zen', desc: 'Centred, quiet, no borders' },
];
const ACCENTS = ['#ff6b3d', '#f2b84b', '#46d19a', '#38bdf8', '#7c8cff', '#c084fc', '#f472b6', '#ef4444'];

let saving: Promise<unknown> = Promise.resolve();

/** Applies a change locally right away, then saves it (saves are queued so they cannot overtake each other). */
function change(patch: Partial<Settings>): void {
  state.settings = { ...state.settings, ...patch };
  applyAppearance(state.settings);
  const snapshot = state.settings;
  saving = saving.then(() => api.saveSettings(snapshot).then((s) => { state.settings = s; }).catch((e) => toast(String(e instanceof Error ? e.message : e), true)));
}

const text = (key: keyof Settings, ph = '', area = false) => {
  const el = area ? h('textarea', { rows: 3, placeholder: ph }) : h('input', { type: 'text', placeholder: ph });
  el.value = String(state.settings[key]);
  el.addEventListener('change', () => change({ [key]: el.value } as Partial<Settings>));
  return el;
};
const num = (key: keyof Settings, min: number, max: number) => {
  const el = h('input', { type: 'number', class: 'num-input', min, max, value: Number(state.settings[key]) });
  el.addEventListener('change', () => {
    const v = Math.min(max, Math.max(min, Math.round(Number(el.value) || 0)));
    el.value = String(v);
    change({ [key]: v } as Partial<Settings>);
  });
  return el;
};
const flag = (key: keyof Settings) => toggle(Boolean(state.settings[key]), (v) => change({ [key]: v } as Partial<Settings>), String(key));

function general(): HTMLElement[] {
  return [
    card('Launcher', null,
      row('Check for launcher updates', 'On start, look for a newer Vox Launcher and offer to install it.', flag('checkLauncherUpdates')),
      row('GitHub repository', 'Where updates and the Vox Client mod are downloaded from, as owner/name (for example yourname/vox-launcher). Leave empty to use the one this build was made with.', text('updateRepo', 'owner/name')),
      row('Desktop notifications', 'Tell me when a download, install or update finishes, or the game crashes, while Vox is in the background.', h('div', { class: 'row-inline' },
        h('button', { class: 'btn small', type: 'button', onClick: () => void attempt(() => api.notify('Vox Launcher', 'Notifications are working.', true)) }, 'Test'), flag('notifications')))),
    card('Updates and content', null,
      row('Check for mod updates', 'Show an update count on each instance’s Mods tab when it opens.', flag('checkModUpdates')),
      row('Warn about mod problems', 'Before launching, check for duplicate mods, mods made for another loader or Minecraft version, and missing required mods.', flag('warnModConflicts')),
      row('Show snapshots', 'Include Minecraft snapshot versions when creating an instance.', flag('showSnapshots')),
      row('Parallel downloads', 'Connections used when downloading game files (1–32).', num('downloadThreads', 1, 32))),
    card('When the game starts', null,
      row('Launcher behaviour', null, select([{ value: 'keep', label: 'Keep the launcher open' }, { value: 'minimize', label: 'Minimize while playing' }], state.settings.launcherAction, (v) => change({ launcherAction: v }))),
      row('Console pop-up', 'Show the game console in a pop-up when a game launches. You can always reopen it from the corner button.', flag('showConsoleOnLaunch')),
      row('Console history', 'Lines kept per game (100–20000).', num('consoleMaxLines', 100, 20000))),
  ];
}

function appearance(rerender: () => void): HTMLElement[] {
  const s = state.settings;
  const themes = h('div', { class: 'theme-grid' }, [...THEMES, { id: 'custom' as Theme, label: 'Custom', desc: 'Your own colours', colors: [complete(s.customTheme).bg!, complete(s.customTheme).panel!, complete(s.customTheme).accent!] }].map((t) => h('button', { class: `theme-card${s.theme === t.id ? ' active' : ''}`, type: 'button', onClick: () => { change({ theme: t.id }); rerender(); } },
    h('div', { class: 'theme-swatch' }, t.colors.map((c) => h('i', { style: { background: c } }))), h('b', {}, t.label), h('span', { class: 'sub' }, t.desc))));
  const custom = h('input', { type: 'color', value: /^#[0-9a-f]{6}$/i.test(s.accent) ? s.accent : '#ff6b3d', 'aria-label': 'Custom accent colour', onInput: () => { change({ accent: custom.value }); } });
  const scale = h('input', { type: 'range', min: 80, max: 130, step: 5, value: s.uiScale, 'aria-label': 'Interface size' });
  const scaleLabel = h('span', { class: 'sub', style: { width: '44px' } }, `${s.uiScale}%`);
  scale.addEventListener('input', () => { scaleLabel.textContent = `${scale.value}%`; });
  scale.addEventListener('change', () => change({ uiScale: Number(scale.value) }));
  const layouts = h('div', { class: 'theme-grid' }, LAYOUTS.map((l) => h('button', { class: `theme-card${s.layout === l.id ? ' active' : ''}`, type: 'button', onClick: () => { change({ layout: l.id }); rerender(); } },
    h('div', { class: `layout-prev lp-${l.id}` }, h('i', {}), h('i', {}), h('i', {})), h('b', {}, l.label), h('span', { class: 'sub' }, l.desc))));
  return [
    card('Layout', 'How the launcher is arranged. Works with every theme.', layouts),
    card('Theme', null, themes),
    customThemeCard(rerender),
    card('Colour and size', null,
      row('Accent colour', 'Overrides the theme’s accent.', h('div', { class: 'accent-row' },
        ...ACCENTS.map((c) => h('button', { class: `swatch${s.accent.toLowerCase() === c ? ' active' : ''}`, type: 'button', 'aria-label': `Accent ${c}`, style: { background: c }, onClick: () => { change({ accent: c }); rerender(); } })),
        custom, h('button', { class: 'btn small ghost', type: 'button', onClick: () => { change({ accent: '' }); rerender(); } }, 'Theme default'))),
      row('Interface size', null, h('div', { style: { display: 'flex', gap: '10px', alignItems: 'center', width: '240px' } }, scale, scaleLabel)),
      row('Compact layout', 'Tighter spacing, more on screen.', toggle(s.compact, (v) => change({ compact: v }), 'Compact')),
      row('Reduce motion', 'Turns off animations.', toggle(s.reduceMotion, (v) => change({ reduceMotion: v }), 'Reduce motion'))),
  ];
}

/** Applies the custom theme while it is being edited; `save` also stores it. */
function setCustom(colors: Record<string, string>, save: boolean): void {
  state.settings = { ...state.settings, theme: 'custom', customTheme: colors };
  applyAppearance(state.settings);
  if (save) change({ theme: 'custom', customTheme: colors });
}

function customThemeCard(rerender: () => void): HTMLElement {
  const s = state.settings;
  const cur = complete(s.customTheme);
  const active = s.theme === 'custom';
  const warn = h('div', { class: 'warn-box', hidden: true });
  const checkContrast = (c: Record<string, string>): void => {
    const low = contrast(c.text!, c.bg!) < 4.5 || contrast(c.text!, c.panel!) < 4.5;
    warn.hidden = !low;
    warn.textContent = low ? 'The text colour is hard to read on the background or cards. Pick a lighter or darker text colour.' : '';
  };
  checkContrast(cur);
  const inputs = COLOR_KEYS.map((k) => {
    const el = h('input', { type: 'color', value: cur[k]!, 'aria-label': COLOR_LABELS[k].label });
    el.addEventListener('input', () => { cur[k] = el.value; setCustom({ ...cur }, false); checkContrast(cur); });
    el.addEventListener('change', () => { cur[k] = el.value; setCustom({ ...cur }, true); rerender(); });
    return h('label', { class: 'ct-field' }, el, h('span', {}, h('b', {}, COLOR_LABELS[k].label), h('span', { class: 'sub' }, COLOR_LABELS[k].hint)));
  });
  const radius = h('input', { type: 'range', min: 0, max: 28, step: 1, value: Number(cur.radius), 'aria-label': 'Corner roundness' });
  radius.addEventListener('input', () => { cur.radius = radius.value; setCustom({ ...cur }, false); });
  radius.addEventListener('change', () => { cur.radius = radius.value; setCustom({ ...cur }, true); });
  const presets = h('div', { class: 'ct-presets' }, PRESETS.map((p) => h('button', { class: 'ct-preset', type: 'button', title: p.name, onClick: () => { setCustom(complete(p.colors), true); rerender(); } },
    h('span', { class: 'theme-swatch' }, [p.colors.bg!, p.colors.panel!, p.colors.accent!].map((c) => h('i', { style: { background: c } }))), p.name)));
  return card('Custom theme', 'Pick your own colours. Everything else is worked out from them.',
    active ? null : h('p', { class: 'sub', style: { marginBottom: '10px' } }, 'Changing a colour below switches to your custom theme.'),
    h('div', { class: 'ct-grid' }, inputs),
    row('Roundness', 'How rounded cards and buttons are.', h('div', { style: { width: '220px' } }, radius)),
    warn,
    h('div', { class: 'ct-label' }, 'Start from a preset'), presets,
    h('div', { class: 'ct-actions' },
      h('button', { class: 'btn', type: 'button', onClick: async () => {
        const name = (await formDialog('Export theme', [{ key: 'name', label: 'Theme name', value: 'My theme' }], 'Choose file…', 'Saves your colours to a file you can share or load on another PC.'));
        if (!name) return;
        const nm = String(name.name || 'My theme').trim().slice(0, 40) || 'My theme';
        const ok = await attempt(() => api.exportTheme(themeFile(nm, cur), `${nm.replace(/[^\w .-]/g, '').trim() || 'theme'}.voxtheme`));
        if (ok) toast('Theme saved.');
      } }, icon('download', 16), 'Export…'),
      h('button', { class: 'btn', type: 'button', onClick: async () => {
        const text = await attempt(() => api.importTheme());
        if (!text) return;
        try {
          const t = parseThemeFile(text);
          setCustom(t.colors, true);
          toast(`Loaded “${t.name}”.`);
          rerender();
        } catch (e) { toast(e instanceof Error ? e.message : String(e), true); }
      } }, icon('upload', 16), 'Import…'),
      h('button', { class: 'btn ghost', type: 'button', onClick: () => { setCustom({ ...CUSTOM_DEFAULT }, true); rerender(); } }, 'Reset')));
}

function java(): HTMLElement[] {
  const listHost = h('div', { class: 'sub' }, 'Not scanned yet.');
  const pathInput = text('javaPath', 'Automatic');
  const result = h('span', { class: 'sub' });
  return [
    card('Java runtime', 'Vox downloads the right Java for each Minecraft version when you leave this empty.',
      row('Download Java automatically', 'Fetches Temurin builds when no suitable Java is found.', flag('autoDownloadJava')),
      row('Java path', 'Used for every instance without its own path.', h('div', { style: { display: 'flex', gap: '8px' } }, pathInput,
        h('button', { class: 'btn', type: 'button', onClick: async () => { const p = await attempt(() => api.pickFile('java')); if (p) { pathInput.value = p; change({ javaPath: p }); } } }, 'Browse'),
        h('button', { class: 'btn', type: 'button', onClick: async () => {
          result.textContent = 'Testing…';
          try { result.textContent = `Java ${await api.testJava(pathInput.value.trim())} works.`; } catch (e) { result.textContent = String(e instanceof Error ? e.message : e); }
        } }, 'Test'))),
      h('div', { style: { textAlign: 'right' } }, result),
      row('Detected installations', null, h('button', { class: 'btn', type: 'button', onClick: async () => {
        const found: JavaInstall[] | undefined = await attempt(() => api.detectJavas());
        listHost.replaceChildren(...(found?.length ? found.map((j) => h('div', {}, h('button', { class: 'btn small ghost', type: 'button', onClick: () => { pathInput.value = j.path; change({ javaPath: j.path }); } }, `Java ${j.major}`), ` ${j.path}`)) : ['No Java found. Vox will download one.']));
      } }, icon('search', 16), 'Scan')),
      listHost),
    card('Memory and arguments', null,
      row('Maximum memory (MB)', 'Default for all instances.', num('defaultMemoryMb', 512, 65536)),
      row('Minimum memory (MB)', null, num('defaultMinMemoryMb', 128, 65536)),
      field('JVM arguments', text('jvmArgs', 'e.g. -XX:+UseZGC', true), 'Added to every launch. Instances can add more.')),
  ];
}

function game(): HTMLElement[] {
  return [card('Game window', 'Defaults for every instance.',
    row('Start fullscreen', null, flag('defaultFullscreen')),
    row('Window width', '0 lets the game decide.', num('defaultWidth', 0, 16000)),
    row('Window height', null, num('defaultHeight', 0, 16000)))];
}

function launch(): HTMLElement[] {
  return [card('Custom commands', 'Defaults for every instance. Variables: INST_ID, INST_NAME, INST_DIR, INST_MC_DIR, INST_MC_VER, INST_LOADER.',
    field('Wrapper command', text('wrapper', 'e.g. gamemoderun'), 'Placed in front of the java command.'),
    field('Before launch', text('preLaunch', 'Runs before the game starts'), 'A failing command stops the launch (5 minute limit).'),
    field('After exit', text('postExit', 'Runs after the game closes')),
    field('Environment variables', text('envVars', 'One KEY=value per line', true)))];
}

function network(): HTMLElement[] {
  return [card('Proxy', 'Used for downloads and Modrinth. Restart the launcher after changing it.',
    field('Proxy URL', text('proxyUrl', 'http://127.0.0.1:8080 or socks5://127.0.0.1:1080'), 'Leave empty for a direct connection.'))];
}

function discord(): HTMLElement[] {
  return [card('Rich presence', 'Show what you are playing on Discord.',
    row('Enable Discord presence', null, flag('discordEnabled')),
    row('Show instance name', null, flag('discordShowInstance')),
    row('Show elapsed time', null, flag('discordShowTime')),
    row('Show the server I am on', 'Shows the server address while you are connected. Off by default because it tells people where you play. Detected from the game log, so it can lag a little behind.', flag('discordShowServer')))];
}

function about(): HTMLElement[] {
  const ver = h('div', { class: 'sub' }, `Version ${APP_VERSION}`);
  void api.appVersion().then((v) => { ver.textContent = `Version ${v}`; }).catch(() => undefined);
  const portable = h('div', { class: 'portable-note', hidden: true }, 'Portable mode: everything is stored in the “data” folder next to the program, so you can move the whole folder to a USB stick.');
  const uninstall = card('Uninstall', 'Removes the launcher from this PC.',
    row('Uninstall Vox Launcher', 'Opens the uninstaller. You can keep or delete your instances and settings.', h('button', { class: 'btn danger', type: 'button', onClick: () => void uninstallFlow() }, icon('trash', 16), 'Uninstall…')));
  void api.isPortable().then((p) => { if (p) { portable.hidden = false; uninstall.hidden = true; } }).catch(() => undefined);
  return [
    portable,
    card('Data', 'Instances, accounts and settings live in one folder.',
      row('Data folder', null, h('button', { class: 'btn', type: 'button', onClick: () => void attempt(() => api.openDataDir()) }, icon('folder', 16), 'Open'))),
    card('Updates', null, row('Check now', 'Looks for a newer version on GitHub.', h('button', { class: 'btn', type: 'button', onClick: () => void checkForUpdate(true) }, icon('refresh', 16), 'Check for updates'))),
    uninstall,
    card(null, null, h('div', { class: 'about-logo' }, logo(72), h('div', {}, h('h2', {}, 'Vox Launcher'), ver,
      h('div', { class: 'sub' }, 'Not affiliated with Mojang or Microsoft.')))),
  ];
}

async function uninstallFlow(): Promise<void> {
  const dir = await attempt(() => api.dataDir());
  const r = await formDialog('Uninstall Vox Launcher?', [
    { key: 'wipe', label: `Also delete my instances, worlds and accounts (${dir ?? 'data folder'})`, type: 'checkbox', value: false },
  ], 'Uninstall', 'Vox Launcher will close and its uninstaller will start. By default your instances, worlds and settings are kept, so a later reinstall finds them.');
  if (!r) return;
  if (r.wipe && !(await confirmDialog(`This permanently deletes ALL instances, worlds, screenshots and accounts in ${dir ?? 'the data folder'}. This cannot be undone.`, 'Delete everything and uninstall', true, 'Delete your data?'))) return;
  await attempt(() => api.uninstallApp(Boolean(r.wipe)));
}

export function renderSettings(section: string): HTMLElement {
  const page = h('div', { class: 'page' });
  const current = SECTIONS.some((s) => s.id === section) ? section : 'general';
  const rerender = () => navigate({ page: 'settings', section: current });
  const body = {
    general, appearance: () => appearance(rerender), java, game, launch, network, discord, about,
  }[current as 'general']!;
  page.appendChild(h('div', { class: 'page-head' }, h('div', {}, h('h1', {}, 'Settings'), h('div', { class: 'sub' }, 'Changes save automatically'))));
  page.appendChild(h('div', { class: 'settings-layout' },
    h('div', { class: 'settings-nav' }, SECTIONS.map((s) => h('button', { class: `snav-item${s.id === current ? ' active' : ''}`, type: 'button', onClick: () => navigate({ page: 'settings', section: s.id }) }, icon(s.icon, 18), s.label))),
    h('div', {}, ...body())));
  return page;
}
