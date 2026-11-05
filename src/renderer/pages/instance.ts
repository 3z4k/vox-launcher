// One instance: overview, content tabs, worlds, screenshots, servers, logs and per-instance settings.
import { api } from '../bridge';
import {
  attempt, avatar, confirmDialog, debounce, emptyState, field, fmtBytes, fmtDate, fmtDuration, formDialog, h, icon, iconButton, INSTANCE_ICONS,
  relativeTime, row, segmented, select, spinner, toast, toggle, card,
} from '../dom';
import { instanceById, navigate, patchInstance, refreshInstances, runTask, scheduleRender, state } from '../state';
import type { ContentKind, FileEntry, InstanceConfig, LogFile, Screenshot, Server, UpdateInfo, World } from '../types';
import { colorLine } from './console';
import { showCrashHelper } from './crash';
import { playButton, progressBar } from './library';

const TABS: { id: string; label: string; icon: Parameters<typeof icon>[0] }[] = [
  { id: 'overview', label: 'Overview', icon: 'home' },
  { id: 'mods', label: 'Mods', icon: 'cube' },
  { id: 'worlds', label: 'Worlds', icon: 'globe' },
  { id: 'screenshots', label: 'Screenshots', icon: 'image' },
  { id: 'packs', label: 'Packs', icon: 'layers' },
  { id: 'servers', label: 'Servers', icon: 'server' },
  { id: 'logs', label: 'Logs', icon: 'file' },
  { id: 'settings', label: 'Settings', icon: 'sliders' },
];

export function renderInstance(id: string, tab: string): HTMLElement {
  const inst = instanceById(id);
  const page = h('div', { class: 'page' });
  if (!inst) {
    page.appendChild(emptyState('cube', 'Instance not found', 'It may have been deleted.', h('button', { class: 'btn', type: 'button', onClick: () => navigate({ page: 'library' }) }, 'Back to library')));
    return page;
  }
  page.appendChild(h('button', { class: 'back', type: 'button', onClick: () => navigate({ page: 'library' }) }, icon('back', 16), 'Library'));
  page.appendChild(h('div', { class: 'inst-head' }, avatar(inst, 64),
    h('div', { class: 'grow' }, h('h1', {}, inst.name),
      h('div', { class: 'sub' }, `Minecraft ${inst.mcVersion} · ${inst.loader === 'vanilla' ? 'Vanilla' : `${inst.loader} ${inst.loaderVersion}`}`), progressBar(inst.id)),
    h('div', { class: 'head-actions' },
      state.logs.has(inst.id) || state.running.has(inst.id) ? h('button', { class: 'btn', type: 'button', onClick: () => document.dispatchEvent(new CustomEvent('vox-console', { detail: { id: inst.id, toggle: true } })) }, icon('terminal', 16), 'Console') : null,
      iconButton('folder', 'Open folder', () => void attempt(() => api.openPath(inst.id))),
      playButton(inst, true))));
  page.appendChild(h('div', { class: 'tabs', role: 'tablist' }, TABS.map((t) =>
    h('button', { class: `tab${t.id === tab ? ' active' : ''}`, type: 'button', role: 'tab', 'aria-selected': String(t.id === tab), onClick: () => navigate({ page: 'instance', id, tab: t.id }) }, icon(t.icon, 16), t.label))));
  const body = h('div', {});
  page.appendChild(body);
  switch (tab) {
    case 'mods': body.appendChild(contentTab(inst, 'mods')); break;
    case 'worlds': body.appendChild(worldsTab(inst)); break;
    case 'screenshots': body.appendChild(screenshotsTab(inst)); break;
    case 'packs': body.appendChild(packsTab(inst)); break;
    case 'servers': body.appendChild(serversTab(inst)); break;
    case 'logs': body.appendChild(logsTab(inst)); break;
    case 'settings': body.appendChild(settingsTab(inst)); break;
    default: body.appendChild(overviewTab(inst));
  }
  return page;
}

// ---- overview -----------------------------------------------------------------------------------

function overviewTab(inst: InstanceConfig): HTMLElement {
  const wrap = h('div', {});
  wrap.appendChild(h('div', { class: 'stat-grid' },
    h('div', { class: 'stat' }, h('span', {}, 'Play time'), h('b', {}, fmtDuration(inst.playedMs))),
    h('div', { class: 'stat' }, h('span', {}, 'Last played'), h('b', {}, inst.lastPlayed ? relativeTime(inst.lastPlayed).replace('Played ', '') : 'Never')),
    h('div', { class: 'stat' }, h('span', {}, 'Created'), h('b', {}, fmtDate(inst.created))),
    h('div', { class: 'stat' }, h('span', {}, 'Version'), h('b', {}, inst.mcVersion))));
  const notes = h('textarea', { rows: 5, placeholder: 'Notes about this instance…', 'aria-label': 'Notes' });
  notes.value = inst.notes;
  notes.addEventListener('input', debounce(() => void patchInstance(inst.id, { notes: notes.value }), 600));
  wrap.appendChild(clientCard(inst));
  wrap.appendChild(card('Notes', null, notes));
  wrap.appendChild(card('Manage', null,
    row('Duplicate', 'Copy this instance with its mods, worlds and settings.', h('button', { class: 'btn', type: 'button', onClick: async () => {
      const copy = await attempt(() => api.duplicateInstance(inst.id));
      if (!copy) return;
      await refreshInstances();
      navigate({ page: 'instance', id: copy.id, tab: 'overview' });
    } }, icon('copy', 16), 'Duplicate')),
    row('Export', 'Save a .zip you can import into another Vox Launcher.', h('button', { class: 'btn', type: 'button', onClick: () => void exportFlow(inst) }, icon('download', 16), 'Export…')),
    row('Verify files on next launch', 'Re-checks and re-downloads game files, libraries and assets. Use this if the game will not start.',
      toggle(Boolean(inst.verifyNext), (v) => void patchInstance(inst.id, { verifyNext: v }))),
    row('Delete instance', 'Removes the instance and everything in it, including worlds.', h('button', { class: 'btn danger', type: 'button', onClick: async () => {
      if (!(await confirmDialog(`Delete “${inst.name}” and all of its worlds, mods and settings? This cannot be undone.`, 'Delete'))) return;
      if (await attempt(() => api.deleteInstance(inst.id).then(() => true))) {
        await refreshInstances();
        navigate({ page: 'library' });
      }
    } }, icon('trash', 16), 'Delete'))));
  return wrap;
}

async function exportFlow(inst: InstanceConfig): Promise<void> {
  const opts = await formDialog('Export instance', [{ key: 'worlds', label: 'Include worlds', type: 'checkbox', value: false }], 'Choose file…', 'Mods, packs and settings are always included. Logs and crash reports are skipped.');
  if (!opts) return;
  const path = await attempt(() => api.pickSave(`${inst.id}.zip`));
  if (!path) return;
  const ok = await runTask(`Exporting ${inst.name}`, (taskId) => api.exportInstance(inst.id, path, Boolean(opts.worlds), taskId).then(() => true));
  if (ok) toast('Export finished.');
}

// ---- mods / resource packs / shaders -------------------------------------------------------------

function contentTab(inst: InstanceConfig, kind: ContentKind): HTMLElement {
  const wrap = h('div', {});
  const list = h('div', {}, spinner());
  let files: FileEntry[] = [];
  let updates: UpdateInfo[] = [];
  let q = '';
  const noun = kind === 'mods' ? 'mod' : kind === 'resourcepacks' ? 'resource pack' : 'shader pack';

  const paint = () => {
    const items = files.filter((f) => !q || `${f.name} ${f.title}`.toLowerCase().includes(q.toLowerCase()));
    if (files.length === 0) return list.replaceChildren(emptyState('layers', `No ${noun}s yet`, `Find ${noun}s on Modrinth, or add files you already have.`,
      h('button', { class: 'btn primary', type: 'button', onClick: () => navigate({ page: 'discover', kind, instanceId: inst.id }) }, icon('compass', 16), 'Browse Modrinth')));
    list.replaceChildren(h('div', { class: 'list' }, items.map((f) => {
      const upd = updates.find((u) => u.filename === f.name);
      return h('div', { class: `item${f.enabled ? '' : ' off'}` },
        f.iconUrl ? h('img', { class: 'item-icon', src: f.iconUrl, alt: '', loading: 'lazy' }) : h('div', { class: 'item-icon' }, icon(f.isDir ? 'folder' : 'file', 18)),
        h('div', { class: 'grow' }, h('div', { class: 'title' }, f.title || f.name), h('div', { class: 'sub' }, `${f.version ? `${f.version} · ` : ''}${f.name} · ${fmtBytes(f.size)}`)),
        upd ? h('button', { class: 'btn small primary', type: 'button', title: `Update to ${upd.newVersion.versionNumber}`, onClick: async () => {
          const ok = await runTask(`Updating ${upd.title}`, () => api.applyUpdate(inst.id, kind, upd.filename, upd.newVersion.id).then(() => true));
          if (ok) { toast(`Updated ${upd.title}.`); void load(); }
        } }, icon('refresh', 14), upd.newVersion.versionNumber) : null,
        h('div', { class: 'item-actions' },
          toggle(f.enabled, async (v) => { await attempt(() => api.setFileEnabled(inst.id, kind, f.name, v)); void load(); }, `Enable ${f.name}`),
          iconButton('trash', `Delete ${f.name}`, async () => {
            if (!(await confirmDialog(`Delete ${f.title || f.name}?`, 'Delete'))) return;
            await attempt(() => api.deleteFile(inst.id, kind, f.name));
            void load();
          }, true)));
    })));
  };
  async function load(): Promise<void> {
    files = (await attempt(() => api.listFiles(inst.id, kind))) ?? [];
    updates = updates.filter((u) => files.some((f) => f.name === u.filename));
    paint();
  }

  const search = h('input', { type: 'search', placeholder: `Search ${noun}s`, 'aria-label': 'Search', onInput: () => { q = search.value; paint(); } });
  const checkBtn = h('button', { class: 'btn', type: 'button', onClick: async () => {
    checkBtn.disabled = true;
    const r = await attempt(() => api.checkUpdates(inst.id, kind));
    checkBtn.disabled = false;
    if (!r) return;
    updates = r;
    toast(r.length ? `${r.length} update${r.length === 1 ? '' : 's'} available.` : `All ${noun}s are up to date.`);
    paint();
  } }, icon('refresh', 16), 'Check for updates');
  wrap.appendChild(h('div', { class: 'toolbar' }, h('div', { class: 'search-box' }, icon('search', 16), search), checkBtn,
    h('button', { class: 'btn', type: 'button', onClick: async () => {
      const p = await attempt(() => api.pickFile('content'));
      if (!p) return;
      if (await attempt(() => api.addFile(inst.id, kind, p))) void load();
    } }, icon('plus', 16), 'Add file'),
    h('button', { class: 'btn', type: 'button', onClick: () => void attempt(() => api.openPath(inst.id, kind)) }, icon('folder', 16), 'Folder'),
    h('button', { class: 'btn primary', type: 'button', onClick: () => navigate({ page: 'discover', kind, instanceId: inst.id }) }, icon('compass', 16), 'Browse Modrinth')));
  wrap.appendChild(list);
  if (kind === 'mods' && inst.loader === 'vanilla') wrap.prepend(h('div', { class: 'panel' }, h('b', {}, 'This instance is vanilla. '), 'Mods need a loader such as Fabric. Create a new instance with a loader, or duplicate this one first.'));
  void load();
  return wrap;
}

function packsTab(inst: InstanceConfig): HTMLElement {
  const wrap = h('div', {});
  const body = h('div', {});
  const paint = (k: ContentKind) => {
    body.replaceChildren(contentTab(inst, k));
  };
  wrap.appendChild(h('div', { style: { marginBottom: '16px' } }, segmented<ContentKind>([{ value: 'resourcepacks', label: 'Resource packs' }, { value: 'shaderpacks', label: 'Shader packs' }], packKind, (v) => { packKind = v; navigate({ page: 'instance', id: inst.id, tab: 'packs' }); })));
  wrap.appendChild(body);
  paint(packKind);
  return wrap;
}
let packKind: ContentKind = 'resourcepacks';

// ---- worlds ---------------------------------------------------------------------------------------

function worldsTab(inst: InstanceConfig): HTMLElement {
  const list = h('div', {}, spinner());
  async function load(): Promise<void> {
    const ws: World[] = (await attempt(() => api.listWorlds(inst.id))) ?? [];
    if (ws.length === 0) return list.replaceChildren(emptyState('globe', 'No worlds yet', 'Worlds appear here after you create one in the game.'));
    list.replaceChildren(h('div', { class: 'list' }, ws.map((w) => h('div', { class: 'item' },
      w.iconPath ? h('img', { class: 'item-icon', src: api.fileUrl(w.iconPath), alt: '' }) : h('div', { class: 'item-icon' }, icon('globe', 18)),
      h('div', { class: 'grow' }, h('div', { class: 'title' }, w.name), h('div', { class: 'sub' }, `${w.gameType} · ${relativeTime(w.lastPlayed || null)} · ${fmtBytes(w.size)}`)),
      h('div', { class: 'item-actions' },
        h('button', { class: 'btn small', type: 'button', onClick: async () => {
          const p = await runTask(`Backing up ${w.name}`, () => api.backupWorld(inst.id, w.folder));
          if (p) toast(`Backup saved to ${p}`);
        } }, icon('save', 14), 'Back up'),
        iconButton('trash', `Delete ${w.name}`, async () => {
          if (!(await confirmDialog(`Delete the world “${w.name}” permanently? Back it up first if you might want it again.`, 'Delete world'))) return;
          await attempt(() => api.deleteWorld(inst.id, w.folder));
          void load();
        }, true))))));
  }
  void load();
  return h('div', {}, h('div', { class: 'toolbar' }, h('button', { class: 'btn', type: 'button', onClick: () => void attempt(() => api.openPath(inst.id, 'saves')) }, icon('folder', 16), 'Open saves folder')), list);
}

// ---- screenshots ------------------------------------------------------------------------------------

function screenshotsTab(inst: InstanceConfig): HTMLElement {
  const grid = h('div', {}, spinner());
  async function load(): Promise<void> {
    const shots: Screenshot[] = (await attempt(() => api.listScreenshots(inst.id))) ?? [];
    if (shots.length === 0) return grid.replaceChildren(emptyState('image', 'No screenshots', 'Press F2 in the game to take one.'));
    grid.replaceChildren(h('div', { class: 'shots' }, shots.map((s) => h('button', { class: 'shot', type: 'button', 'aria-label': s.name, onClick: () => view(s) },
      h('img', { src: api.fileUrl(s.path), alt: s.name, loading: 'lazy' })))));
  }
  function view(s: Screenshot): void {
    const box = h('div', { class: 'lightbox', role: 'dialog', 'aria-label': s.name, onClick: () => box.remove() },
      h('img', { src: api.fileUrl(s.path), alt: s.name }),
      h('div', { class: 'bar' },
        h('button', { class: 'btn danger', type: 'button', onClick: async (e: Event) => {
          e.stopPropagation();
          if (!(await confirmDialog(`Delete ${s.name}?`, 'Delete'))) return;
          await attempt(() => api.deleteScreenshot(inst.id, s.name));
          box.remove();
          void load();
        } }, icon('trash', 16), 'Delete'),
        h('button', { class: 'btn', type: 'button', onClick: () => box.remove() }, 'Close')));
    const esc = (e: KeyboardEvent) => { if (e.key === 'Escape') { box.remove(); document.removeEventListener('keydown', esc); } };
    document.addEventListener('keydown', esc);
    document.body.appendChild(box);
  }
  void load();
  return h('div', {}, h('div', { class: 'toolbar' }, h('button', { class: 'btn', type: 'button', onClick: () => void attempt(() => api.openPath(inst.id, 'screenshots')) }, icon('folder', 16), 'Open folder')), grid);
}

// ---- servers -----------------------------------------------------------------------------------------

function serversTab(inst: InstanceConfig): HTMLElement {
  const wrap = h('div', {});
  let servers: Server[] = [];
  const list = h('div', {}, spinner());
  const paint = () => {
    if (servers.length === 0) return list.replaceChildren(emptyState('server', 'No servers saved', 'Add one here and it shows up in the game’s multiplayer list.'));
    list.replaceChildren(h('div', { class: 'list' }, servers.map((s, i) => h('div', { class: 'item' },
      s.icon ? h('img', { class: 'item-icon', src: `data:image/png;base64,${s.icon}`, alt: '' }) : h('div', { class: 'item-icon' }, icon('server', 18)),
      h('div', { class: 'grow' }, h('div', { class: 'title' }, s.name), h('div', { class: 'sub mono' }, s.ip)),
      h('div', { class: 'item-actions' },
        iconButton('edit', `Edit ${s.name}`, async () => {
          const r = await formDialog('Edit server', [{ key: 'name', label: 'Name', value: s.name }, { key: 'ip', label: 'Address', value: s.ip }], 'Save');
          if (!r || !String(r.ip).trim()) return;
          servers[i] = { ...s, name: String(r.name).trim() || String(r.ip), ip: String(r.ip).trim() };
          await save();
        }),
        iconButton('trash', `Remove ${s.name}`, async () => { servers.splice(i, 1); await save(); }, true))))));
  };
  async function save(): Promise<void> {
    if (await attempt(() => api.saveServers(inst.id, servers).then(() => true))) paint();
  }
  void attempt(() => api.listServers(inst.id)).then((s) => { servers = s ?? []; paint(); });
  wrap.appendChild(h('div', { class: 'toolbar' }, h('button', { class: 'btn primary', type: 'button', onClick: async () => {
    const r = await formDialog('Add server', [{ key: 'name', label: 'Name', placeholder: 'My server' }, { key: 'ip', label: 'Address', placeholder: 'play.example.com' }], 'Add');
    if (!r || !String(r.ip).trim()) return;
    servers.push({ name: String(r.name).trim() || String(r.ip).trim(), ip: String(r.ip).trim(), icon: null });
    await save();
  } }, icon('plus', 16), 'Add server')));
  wrap.appendChild(list);
  return wrap;
}

// ---- logs ----------------------------------------------------------------------------------------------

function logsTab(inst: InstanceConfig): HTMLElement {
  const wrap = h('div', {});
  const box = h('div', { class: 'console', tabIndex: 0, 'aria-label': 'Log contents' });
  const picker = h('div', { style: { minWidth: '260px' } });
  let text = '';
  async function open(p: string): Promise<void> {
    box.replaceChildren(spinner());
    text = (await attempt(() => api.readLog(inst.id, p))) ?? '';
    const lines = text.split('\n');
    const shown = lines.length > 4000 ? lines.slice(-4000) : lines;
    box.replaceChildren(...(lines.length > 4000 ? [colorLine(`… ${lines.length - 4000} earlier lines hidden. Copy to get the whole log.`)] : []), ...shown.map(colorLine));
    box.scrollTop = box.scrollHeight;
  }
  void attempt(() => api.listLogs(inst.id)).then((logs: LogFile[] | undefined) => {
    if (!logs || logs.length === 0) return wrap.replaceChildren(emptyState('file', 'No logs yet', 'Logs and crash reports show up after the game has run.'));
    picker.replaceChildren(select(logs.map((l) => ({ value: l.path, label: `${l.path} (${fmtBytes(l.size)})` })), logs[0].path, (v) => void open(v)));
    void open(logs[0].path);
  });
  wrap.appendChild(h('div', { class: 'toolbar' }, picker,
    h('button', { class: 'btn', type: 'button', onClick: async () => { await navigator.clipboard.writeText(text).catch(() => undefined); toast('Copied the log.'); } }, icon('copy', 16), 'Copy'),
    h('button', { class: 'btn primary', type: 'button', onClick: () => void showCrashHelper(inst.id, null) }, icon('sparkle', 16), 'Analyze with crash helper'),
    h('button', { class: 'btn', type: 'button', onClick: () => void attempt(() => api.openPath(inst.id, 'logs')) }, icon('folder', 16), 'Open folder')));
  wrap.appendChild(box);
  return wrap;
}

// ---- per-instance settings -------------------------------------------------------------------------------

function settingsTab(inst: InstanceConfig): HTMLElement {
  const save = (patch: Partial<InstanceConfig>) => void patchInstance(inst.id, patch);
  const text = (value: string, key: keyof InstanceConfig, ph = '', area = false) => {
    const el = area ? h('textarea', { rows: 3, placeholder: ph }) : h('input', { type: 'text', placeholder: ph });
    el.value = value;
    el.addEventListener('change', () => save({ [key]: el.value } as Partial<InstanceConfig>));
    return el;
  };
  const num = (value: number | null, key: keyof InstanceConfig, ph: string) => {
    const el = h('input', { type: 'number', class: 'num-input', min: 0, placeholder: ph, value: value ?? '' });
    el.addEventListener('change', () => save({ [key]: el.value === '' ? null : Math.max(0, Number(el.value)) } as Partial<InstanceConfig>));
    return el;
  };
  const wrap = h('div', {});

  const nameEl = text(inst.name, 'name');
  const groupEl = text(inst.group, 'group', 'Optional');
  const iconRow = h('div', { class: 'icon-pick' });
  const paintIcons = () => iconRow.replaceChildren(...INSTANCE_ICONS.map((n) => h('button', { type: 'button', class: n === inst.icon ? 'active' : '', 'aria-label': n, onClick: async () => { await patchInstance(inst.id, { icon: n }); scheduleRender(); } }, icon(n, 18))));
  paintIcons();
  wrap.appendChild(card('General', null, field('Name', nameEl), field('Group', groupEl), h('div', { class: 'field' }, h('label', {}, 'Icon'), iconRow)));

  const fsOptions = [{ value: 'default', label: 'Launcher default' }, { value: 'on', label: 'Fullscreen' }, { value: 'off', label: 'Windowed' }];
  wrap.appendChild(card('Java and memory', 'Empty fields use the launcher defaults.',
    row('Java path', 'Leave empty to use the launcher’s Java (downloaded automatically when needed).', h('div', { style: { display: 'flex', gap: '8px' } }, text(inst.javaPath, 'javaPath', 'Automatic'),
      h('button', { class: 'btn', type: 'button', onClick: async () => { const p = await attempt(() => api.pickFile('java')); if (p) { await patchInstance(inst.id, { javaPath: p }); scheduleRender(); } } }, 'Browse'))),
    row('Maximum memory (MB)', `Launcher default: ${state.settings.defaultMemoryMb} MB`, num(inst.memoryMb, 'memoryMb', String(state.settings.defaultMemoryMb))),
    row('Minimum memory (MB)', `Launcher default: ${state.settings.defaultMinMemoryMb} MB`, num(inst.minMemoryMb, 'minMemoryMb', String(state.settings.defaultMinMemoryMb))),
    field('Extra JVM arguments', text(inst.jvmArgs, 'jvmArgs', 'Added after the launcher’s global arguments', true))));

  wrap.appendChild(card('Game window', null,
    row('Display mode', null, select(fsOptions, inst.fullscreen === null ? 'default' : inst.fullscreen ? 'on' : 'off', (v) => save({ fullscreen: v === 'default' ? null : v === 'on' }))),
    row('Window width', null, num(inst.width, 'width', 'Default')), row('Window height', null, num(inst.height, 'height', 'Default')),
    field('Extra game arguments', text(inst.extraGameArgs, 'extraGameArgs', 'e.g. --quickPlayMultiplayer play.example.com', true))));

  wrap.appendChild(card('Custom commands', 'Variables available: INST_ID, INST_NAME, INST_DIR, INST_MC_DIR, INST_MC_VER, INST_LOADER.',
    field('Wrapper command', text(inst.wrapper, 'wrapper', 'e.g. gamemoderun'), 'Placed in front of the java command.'),
    field('Before launch', text(inst.preLaunch, 'preLaunch', 'Runs before the game starts'), 'A failing command stops the launch.'),
    field('After exit', text(inst.postExit, 'postExit', 'Runs after the game closes')),
    field('Environment variables', text(inst.envVars, 'envVars', 'One KEY=value per line', true))));

  const others = state.instances.filter((i) => i.id !== inst.id);
  const sources = [{ value: '', label: 'Off' }, ...(state.hasVanillaDir ? [{ value: 'mc', label: 'Vanilla .minecraft folder' }] : []), ...others.map((i) => ({ value: i.id, label: `Instance: ${i.name}` }))];
  const files = [{ f: 'options.txt', l: 'Game options and keybinds (options.txt)' }, { f: 'optionsof.txt', l: 'OptiFine options' }, { f: 'optionsshaders.txt', l: 'Shader options' }, { f: 'servers.dat', l: 'Server list (manual only)' }];
  const picked = new Set(['options.txt', 'optionsof.txt', 'optionsshaders.txt']);
  const syncCard = card('Sync settings', 'Copy options and keybinds from the regular game or another instance.',
    row('Sync automatically at launch', 'Before each launch, options.txt, optionsof.txt and optionsshaders.txt are copied from this source.',
      select(sources, sources.some((s) => s.value === inst.syncOptionsFrom) ? inst.syncOptionsFrom : '', (v) => save({ syncOptionsFrom: v }))));
  const manualSource = select(sources.filter((s) => s.value), sources.find((s) => s.value)?.value ?? '', () => undefined);
  if (sources.length > 1) {
    syncCard.appendChild(h('div', { class: 'field' }, h('label', {}, 'Copy now from'), manualSource,
      ...files.map((x) => h('label', { class: 'check' }, h('input', { type: 'checkbox', checked: picked.has(x.f), onChange: (e: Event) => { if ((e.target as HTMLInputElement).checked) picked.add(x.f); else picked.delete(x.f); } }), x.l)),
      h('div', {}, h('button', { class: 'btn', type: 'button', onClick: async () => {
        if (!manualSource.value) return;
        if (state.running.has(inst.id)) return toast('Close the game first.', true);
        const copied = await attempt(() => api.syncOptions(inst.id, manualSource.value, [...picked]));
        if (copied) toast(copied.length ? `Copied ${copied.join(', ')}.` : 'None of the selected files exist in the source.');
      } }, icon('download', 16), 'Copy now'))));
  }
  wrap.appendChild(syncCard);
  return wrap;
}

// ---- Vox Client (optional mod) ------------------------------------------------------------------------

function clientCard(inst: InstanceConfig): HTMLElement {
  const modded = inst.loader === 'fabric' || inst.loader === 'quilt';
  const control = h('div', { class: 'row-inline' }, spinner(''));
  const panel = card('Vox Client', 'An optional Fabric mod: FPS and CPS counters, keystrokes, a clean HUD and local cosmetics. Installing adds one file to this instance’s mods folder (plus Fabric API if it is missing). Removing deletes that file.',
    row('Vox Client mod', modded ? null : 'Needs a Fabric or Quilt instance.', control));
  if (!modded) { control.replaceChildren(h('span', { class: 'chip' }, 'Not available')); return panel; }
  const paint = (status: { installed: string | null; supported: string[]; sourceKnown: boolean }) => {
    const sup = status.supported.includes(inst.mcVersion);
    if (status.installed) {
      control.replaceChildren(h('span', { class: 'chip ok' }, 'Installed'),
        h('button', { class: 'btn small', type: 'button', onClick: async () => {
          if (!(await confirmDialog('Remove Vox Client from this instance? Your other mods are not touched.', 'Remove', true, 'Remove Vox Client?'))) return;
          if (await attempt(() => api.clientRemove(inst.id).then(() => true))) { toast('Vox Client removed.'); void load(); }
        } }, 'Remove'));
    } else if (!status.sourceKnown) {
      control.replaceChildren(h('span', { class: 'sub' }, 'No GitHub repository set'),
        h('button', { class: 'btn small primary', type: 'button', onClick: () => navigate({ page: 'settings', section: 'general' }) }, 'Set it in Settings'));
    } else {
      control.replaceChildren(sup || status.supported.length === 0
        ? h('button', { class: 'btn small primary', type: 'button', onClick: async () => {
            const done = await runTask('Installing Vox Client', (taskId) => api.clientInstall(inst.id, taskId));
            if (done) { toast('Vox Client installed.'); void load(); }
          } }, icon('download', 15), 'Install')
        : h('span', { class: 'chip warn' }, `Not available for ${inst.mcVersion} yet`));
    }
  };
  const load = async () => { const s = await attempt(() => api.clientStatus(inst.id)); if (s) paint(s); else control.replaceChildren(h('span', { class: 'chip' }, 'Unavailable')); };
  void load();
  return panel;
}
