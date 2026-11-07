// Library: all instances, grouped, with a "continue playing" hero.
import { api } from '../bridge';
import {
  attempt, avatar, colorFor, confirmDialog, debounce, emptyState, fmtDuration, h, icon, iconButton, INSTANCE_ICONS, logo, openModal, dialogHead, relativeTime, select, toast, type Modal,
} from '../dom';
import {
  launch, loadVersions, navigate, progressText, refreshInstances, runTask, scheduleRender, state, stopGame,
} from '../state';
import type { InstanceConfig, Loader } from '../types';
import { importLauncherDialog } from './importer';

let query = '';
let sort: 'recent' | 'name' | 'played' = 'recent';

export function playButton(inst: InstanceConfig, big = false): HTMLElement {
  const prog = state.progress.get(inst.id);
  const cls = big ? 'btn big' : 'btn small';
  if (state.running.has(inst.id)) {
    return h('button', { class: `${cls} danger`, type: 'button', onClick: (e: Event) => { e.stopPropagation(); stopGame(inst.id); } }, icon('stop', 15), 'Stop');
  }
  if (prog) return h('button', { class: cls, type: 'button', disabled: true }, h('span', { class: 'spin' }), 'Starting');
  return h('button', { class: `${cls} primary`, type: 'button', onClick: (e: Event) => { e.stopPropagation(); void launch(inst.id); } }, icon('play', big ? 18 : 14), 'Play');
}

export function progressBar(id: string): HTMLElement | null {
  const p = state.progress.get(id);
  if (!p) return null;
  const value = p.total ? Math.round((p.current / p.total) * 100) : 0;
  return h('div', { class: 'progress-wrap' },
    h('div', { class: `pbar${p.total ? '' : ' indeterminate'}`, 'data-pbar': id, role: 'progressbar', 'aria-valuenow': String(value) }, h('div', { 'data-pfill': id, style: { width: `${value}%` } })),
    h('div', { class: 'sub', 'data-plabel': id, style: { marginTop: '4px' } }, progressText(p)));
}

function loaderLabel(i: InstanceConfig): string {
  return i.loader === 'vanilla' ? 'Vanilla' : i.loader.charAt(0).toUpperCase() + i.loader.slice(1);
}

function card(inst: InstanceConfig): HTMLElement {
  const running = state.running.has(inst.id);
  return h('div', { class: 'inst-card', role: 'button', tabIndex: 0, onClick: () => navigate({ page: 'instance', id: inst.id, tab: 'overview' }),
    onKeydown: (e: KeyboardEvent) => { if (e.key === 'Enter') navigate({ page: 'instance', id: inst.id, tab: 'overview' }); } },
    h('div', { class: 'quick' },
      iconButton('copy', `Duplicate ${inst.name}`, async (e) => {
        e.stopPropagation();
        if (await attempt(() => api.duplicateInstance(inst.id))) { await refreshInstances(); scheduleRender(); toast(`Duplicated “${inst.name}”.`); }
      }),
      iconButton('trash', `Delete ${inst.name}`, async (e) => {
        e.stopPropagation();
        if (state.running.has(inst.id)) return toast('Stop the game before deleting this instance.', true);
        if (!(await confirmDialog(`Delete “${inst.name}” and all of its worlds, mods and settings? This cannot be undone.`, 'Delete'))) return;
        if (await attempt(() => api.deleteInstance(inst.id).then(() => true))) { await refreshInstances(); scheduleRender(); toast(`Deleted “${inst.name}”.`); }
      }, true)),
    h('div', { class: 'inst-top' }, avatar(inst, 46),
      h('div', { style: { minWidth: '0' } },
        h('div', { class: 'inst-name' }, running ? h('span', { class: 'run-dot' }) : null, inst.name),
        h('div', { class: 'sub' }, `${inst.mcVersion} · ${loaderLabel(inst)}`))),
    progressBar(inst.id),
    h('div', { class: 'inst-meta' }, h('span', { class: 'sub' }, inst.playedMs > 0 ? `${relativeTime(inst.lastPlayed)} · ${fmtDuration(inst.playedMs)}` : relativeTime(inst.lastPlayed)), playButton(inst)));
}

export async function newInstanceDialog(defaults: Partial<{ name: string; group: string }> = {}): Promise<void> {
  let m: Modal | null = null;
  let chosenIcon = 'cube';
  const versions = await loadVersions();
  if (!versions) return;
  const list = versions.filter((v) => (state.settings.showSnapshots ? v.type !== 'old_alpha' && v.type !== 'old_beta' : v.type === 'release'));
  const name = h('input', { type: 'text', placeholder: 'My new world', value: defaults.name ?? '' });
  const ver = select(list.map((v) => ({ value: v.id, label: v.type === 'release' ? v.id : `${v.id} (${v.type})` })), list[0]?.id ?? '', () => void loadLoaders());
  const loader = select<Loader>([
    { value: 'vanilla', label: 'Vanilla' }, { value: 'fabric', label: 'Fabric' }, { value: 'quilt', label: 'Quilt' },
    { value: 'forge', label: 'Forge' }, { value: 'neoforge', label: 'NeoForge' },
  ], 'vanilla', () => void loadLoaders());
  const lver = select<string>([], '', () => undefined);
  const lwrap = h('div', { class: 'field' }, h('label', {}, 'Loader version'), lver);
  lwrap.style.display = 'none';
  const group = h('input', { type: 'text', placeholder: 'Optional, e.g. Modded', value: defaults.group ?? '' });
  const iconRow = h('div', { class: 'icon-pick' });
  const paintIcons = () => {
    iconRow.replaceChildren(...INSTANCE_ICONS.map((n) => h('button', { type: 'button', class: n === chosenIcon ? 'active' : '', 'aria-label': n, onClick: () => { chosenIcon = n; paintIcons(); } }, icon(n, 18))));
  };
  paintIcons();

  let token = 0;
  async function loadLoaders(): Promise<void> {
    const l = loader.value as Loader;
    lwrap.style.display = l === 'vanilla' ? 'none' : '';
    if (l === 'vanilla') return;
    const mine = ++token;
    lver.replaceChildren(h('option', {}, 'Loading…'));
    const vs = await attempt(() => api.listLoaderVersions(l, ver.value));
    if (mine !== token) return;
    lver.replaceChildren(...(vs ?? []).map((v) => h('option', { value: v }, v)));
    if (vs && vs.length === 0) lver.replaceChildren(h('option', { value: '' }, `No ${l} build for ${ver.value}`));
  }

  const create = h('button', { class: 'btn primary', type: 'button', onClick: async () => {
    const nm = name.value.trim();
    if (!nm) return toast('Give the instance a name.', true);
    const l = loader.value as Loader;
    if (l !== 'vanilla' && !lver.value) return toast('Pick a loader version first.', true);
    create.disabled = true;
    const made = await attempt(() => api.createInstance({ name: nm, mcVersion: ver.value, loader: l, loaderVersion: l === 'vanilla' ? '' : lver.value, group: group.value.trim(), icon: chosenIcon }));
    create.disabled = false;
    if (!made) return;
    m?.close();
    await refreshInstances();
    navigate({ page: 'instance', id: made.id, tab: 'overview' });
  } }, 'Create instance');

  const f = (label: string, c: HTMLElement) => h('div', { class: 'field' }, h('label', {}, label), c);
  m = openModal(h('div', { class: 'dlg' }, dialogHead('New instance', () => m!),
    f('Name', name), h('div', { class: 'field-row' }, f('Minecraft version', ver), f('Mod loader', loader)), lwrap, f('Group', group),
    h('div', { class: 'field' }, h('label', {}, 'Icon'), iconRow),
    h('div', { class: 'dlg-actions' }, h('button', { class: 'btn', type: 'button', onClick: () => m!.close() }, 'Cancel'), create)));
  name.focus();
}

export async function importDialog(): Promise<void> {
  const path = await attempt(() => api.pickFile('import'));
  if (!path) return;
  const made = await runTask('Importing pack', (taskId) => api.importFile(path, taskId));
  if (!made) return;
  await refreshInstances();
  toast(`Imported “${made.name}”.`);
  navigate({ page: 'instance', id: made.id, tab: 'overview' });
}

function greeting(): string {
  const hr = new Date().getHours();
  const part = hr < 5 ? 'Good evening' : hr < 12 ? 'Good morning' : hr < 18 ? 'Good afternoon' : 'Good evening';
  const who = state.accounts.find((a) => a.active)?.name;
  return who ? `${part}, ${who}` : part;
}

const recentOf = (): InstanceConfig | undefined =>
  [...state.instances].filter((i) => i.lastPlayed).sort((a, b) => (b.lastPlayed ?? 0) - (a.lastPlayed ?? 0))[0] ?? state.instances[0];

/** Gradient used as an instance "cover": its own colour fading to dark. */
const coverBg = (i: InstanceConfig): string => `linear-gradient(135deg, ${colorFor(i.id)}, ${colorFor(i.id)}55 62%, #0d0f12)`;

function deleteInstance(inst: InstanceConfig): Promise<void> {
  return (async () => {
    if (state.running.has(inst.id)) return void toast('Stop the game before deleting this instance.', true);
    if (!(await confirmDialog(`Delete “${inst.name}” and all of its worlds, mods and settings? This cannot be undone.`, 'Delete'))) return;
    if (await attempt(() => api.deleteInstance(inst.id).then(() => true))) {
      await refreshInstances();
      scheduleRender();
      toast(`Deleted “${inst.name}”.`);
    }
  })();
}

function quickActions(inst: InstanceConfig): HTMLElement {
  return h('div', { class: 'quick' },
    iconButton('copy', `Duplicate ${inst.name}`, async (e) => {
      e.stopPropagation();
      if (await attempt(() => api.duplicateInstance(inst.id))) { await refreshInstances(); scheduleRender(); toast(`Duplicated “${inst.name}”.`); }
    }),
    iconButton('trash', `Delete ${inst.name}`, (e) => { e.stopPropagation(); void deleteInstance(inst); }, true));
}

const open = (i: InstanceConfig, tab = 'overview') => navigate({ page: 'instance', id: i.id, tab });

/** Tile with a coloured cover (Cover and Glass layouts). */
function coverCard(inst: InstanceConfig, glass = false): HTMLElement {
  const running = state.running.has(inst.id);
  return h('div', { class: `cover-card${glass ? ' glass-card' : ''}`, role: 'button', tabIndex: 0, style: glass ? {} : { background: coverBg(inst) }, onClick: () => open(inst),
    onKeydown: (e: KeyboardEvent) => { if (e.key === 'Enter') open(inst); } },
    quickActions(inst),
    glass ? h('div', { class: 'glass-ic', style: { background: colorFor(inst.id) } }, icon((INSTANCE_ICONS as string[]).includes(inst.icon) ? (inst.icon as never) : 'cube', 20)) : h('div', { class: 'cover-ic' }, icon((INSTANCE_ICONS as string[]).includes(inst.icon) ? (inst.icon as never) : 'cube', 38)),
    h('div', { class: 'cover-info' },
      h('div', { class: 'inst-name' }, running ? h('span', { class: 'run-dot' }) : null, inst.name),
      h('div', { class: 'sub' }, `${inst.mcVersion} · ${loaderLabel(inst)}`),
      progressBar(inst.id)),
    h('div', { class: 'cover-play' }, playButton(inst)));
}

function groupedGrid(items: InstanceConfig[], make: (i: InstanceConfig) => HTMLElement, gridClass: string): HTMLElement[] {
  const groups = new Map<string, InstanceConfig[]>();
  for (const i of items) groups.set(i.group || '', [...(groups.get(i.group || '') ?? []), i]);
  const names = [...groups.keys()].sort((a, b) => (a === '' ? -1 : b === '' ? 1 : a.localeCompare(b)));
  return names.flatMap((g) => {
    const parts: HTMLElement[] = [];
    if (groups.size > 1 || g) parts.push(h('div', { class: 'group-title' }, g || 'Ungrouped'));
    parts.push(h('div', { class: gridClass }, groups.get(g)!.map(make)));
    return parts;
  });
}

function heroBanner(inst: InstanceConfig): HTMLElement {
  return h('div', { class: 'hero-cover', style: { background: coverBg(inst) } },
    h('div', { class: 'hero-cover-text' },
      h('div', { class: 'eyebrow' }, inst.lastPlayed ? 'Continue playing' : 'Ready to play'),
      h('h1', {}, inst.name),
      h('div', { class: 'hero-sub' }, `Minecraft ${inst.mcVersion} · ${loaderLabel(inst)}${inst.playedMs > 0 ? ` · ${fmtDuration(inst.playedMs)} played` : ''}`),
      progressBar(inst.id)),
    h('div', { class: 'hero-cover-actions' }, h('button', { class: 'btn ghost-light', type: 'button', onClick: () => open(inst) }, 'Open'), playButton(inst, true)));
}

let selectedId: string | null = null;

function denseView(items: InstanceConfig[]): HTMLElement {
  const sel = items.find((i) => i.id === selectedId) ?? items[0];
  selectedId = sel?.id ?? null;
  const table = h('table', { class: 'dense-table' },
    h('thead', {}, h('tr', {}, ['Name', 'Version', 'Loader', 'Played', 'Last played'].map((c) => h('th', {}, c)))),
    h('tbody', {}, items.map((i) => h('tr', { class: i.id === sel.id ? 'sel' : '', tabIndex: 0,
      onClick: () => { selectedId = i.id; scheduleRender(); }, onDblclick: () => open(i), onKeydown: (e: KeyboardEvent) => { if (e.key === 'Enter') open(i); } },
      h('td', {}, h('span', { class: 'row-dot', style: { background: colorFor(i.id) } }), state.running.has(i.id) ? h('span', { class: 'run-dot' }) : null, i.name),
      h('td', {}, i.mcVersion), h('td', {}, h('span', { class: 'chip' }, loaderLabel(i))),
      h('td', {}, i.playedMs > 0 ? fmtDuration(i.playedMs) : '—'), h('td', { class: 'sub' }, relativeTime(i.lastPlayed).replace('Played ', ''))))));
  const detail = h('aside', { class: 'dense-detail' },
    h('div', { class: 'detail-cover', style: { background: coverBg(sel) } }, icon((INSTANCE_ICONS as string[]).includes(sel.icon) ? (sel.icon as never) : 'cube', 44)),
    h('h2', {}, sel.name), h('div', { class: 'sub' }, `Minecraft ${sel.mcVersion} · ${loaderLabel(sel)}`),
    progressBar(sel.id), h('div', { class: 'detail-actions' }, playButton(sel, true),
      ...[['Mods', 'mods'], ['Worlds', 'worlds'], ['Settings', 'settings']].map(([l, t]) => h('button', { class: 'btn', type: 'button', onClick: () => open(sel, t) }, l)),
      h('button', { class: 'btn', type: 'button', onClick: () => void attempt(() => api.openPath(sel.id)) }, icon('folder', 16), 'Open folder'),
      h('button', { class: 'btn danger', type: 'button', onClick: () => void deleteInstance(sel) }, icon('trash', 16), 'Delete')));
  return h('div', { class: 'dense-layout' }, h('div', { class: 'dense-table-wrap' }, table), detail);
}

function glassView(items: InstanceConfig[]): HTMLElement[] {
  const feat = recentOf()!;
  const total = state.instances.reduce((n, i) => n + i.playedMs, 0);
  const most = [...state.instances].sort((a, b) => b.playedMs - a.playedMs)[0];
  const stat = (v: string, l: string) => h('div', { class: 'glass-card stat-tile' }, h('b', {}, v), h('span', {}, l));
  const others = items.filter((i) => i.id !== feat.id);
  return [
    h('div', { class: 'glass-top' },
      h('div', { class: 'glass-feature glass-card', style: { backgroundImage: `linear-gradient(0deg, #0009, #0000), ${coverBg(feat)}` } },
        h('div', { class: 'eyebrow' }, feat.lastPlayed ? 'Continue playing' : 'Ready to play'), h('h2', {}, feat.name),
        h('div', { class: 'hero-sub' }, `${feat.mcVersion} · ${loaderLabel(feat)}${feat.playedMs > 0 ? ` · ${fmtDuration(feat.playedMs)}` : ''}`), progressBar(feat.id),
        h('div', { class: 'head-actions', style: { marginTop: '14px' } }, playButton(feat, true), h('button', { class: 'btn ghost-light', type: 'button', onClick: () => open(feat) }, 'Open'))),
      h('div', { class: 'glass-side' },
        h('div', { class: 'stat-row' }, stat(total > 0 ? fmtDuration(total) : '0 min', 'Total playtime'), stat(String(state.instances.length), 'Instances'), stat(most && most.playedMs > 0 ? most.name : '—', 'Most played')),
        h('div', { class: 'glass-mini' }, others.slice(0, 4).map((i) => coverCard(i, true))))),
    ...(others.length > 4 ? [h('div', { class: 'group-title' }, 'All instances'), h('div', { class: 'grid' }, others.slice(4).map((i) => coverCard(i, true)))] : []),
  ];
}

/** Updates every mod in every instance, then shows what happened. */
async function updateAllFlow(): Promise<void> {
  if (!(await confirmDialog('Look for newer versions of the mods in all your instances and install them? A snapshot is saved in each instance first, so you can go back.', 'Update all', false, 'Update all mods'))) return;
  const rows = await runTask('Updating all mods', (taskId) => api.updateAllMods(taskId));
  if (!rows) return;
  await refreshInstances();
  const updated = rows.reduce((n, r) => n + r.updated.length, 0);
  const failed = rows.reduce((n, r) => n + r.failed.length, 0);
  const m = openModal(h('div', { class: 'dlg' },
    h('h2', {}, updated ? `${updated} mod${updated === 1 ? '' : 's'} updated` : 'Everything is up to date'),
    h('p', { class: 'dlg-text' }, updated || failed ? `${failed ? `${failed} could not be updated. ` : ''}A snapshot was saved in each changed instance.` : 'None of your instances had mods with newer versions.'),
    rows.length ? h('div', { class: 'conflict-list' }, rows.map((r) => h('div', { class: 'conflict' + (r.failed.length ? ' error' : '') },
      h('div', {}, h('b', {}, r.name),
        r.skipped ? h('div', { class: 'sub' }, r.skipped) : null,
        ...r.updated.map((u) => h('div', { class: 'sub' }, `${u.title}: ${u.from || '?'} → ${u.to}`)),
        ...r.failed.map((f) => h('div', { class: 'sub' }, `Could not update ${f}`)))))) : null,
    h('div', { class: 'dlg-actions' }, h('button', { class: 'btn primary', type: 'button', onClick: () => m.close() }, 'Done'))), true);
}

export function renderLibrary(): HTMLElement {
  const layout = state.settings.layout;
  const page = h('div', { class: 'page library-page' });
  const actions = h('div', { class: 'head-actions' },
    state.instances.length ? h('button', { class: 'btn', type: 'button', onClick: () => void updateAllFlow() }, icon('refresh', 16), 'Update all mods') : null,
    h('button', { class: 'btn', type: 'button', onClick: () => void importDialog() }, icon('upload', 16), 'Import'),
    h('button', { class: 'btn', type: 'button', onClick: () => void importLauncherDialog() }, icon('download', 16), 'From another launcher'),
    h('button', { class: 'btn primary', type: 'button', onClick: () => void newInstanceDialog() }, icon('plus', 16), 'New instance'));
  const title = layout === 'glass' ? greeting() : 'Library';
  const sub = layout === 'glass' ? `${state.instances.length} instance${state.instances.length === 1 ? '' : 's'}` : `${state.instances.length} instance${state.instances.length === 1 ? '' : 's'}`;
  page.appendChild(h('div', { class: 'page-head' }, h('div', {}, h('h1', {}, title), h('div', { class: 'sub' }, sub)), actions));

  if (state.instances.length === 0) {
    const empty = emptyState('cube', 'Your library is empty', 'Create an instance, or import a Modrinth modpack (.mrpack) or a Vox export (.zip).',
      h('button', { class: 'btn primary', type: 'button', onClick: () => void newInstanceDialog() }, icon('plus', 16), 'New instance'),
      h('button', { class: 'btn', type: 'button', onClick: () => navigate({ page: 'discover', kind: 'modpacks', instanceId: null }) }, icon('compass', 16), 'Browse modpacks'));
    empty.querySelector('.empty-icon')?.replaceChildren(logo(52));
    page.appendChild(empty);
    return page;
  }

  const recent = recentOf()!;
  if (layout === 'cover') page.appendChild(heroBanner(recent));
  if (layout === 'classic' && recent.lastPlayed) {
    page.appendChild(h('div', { class: 'hero' }, avatar(recent, 64),
      h('div', { class: 'hero-text' }, h('div', { class: 'eyebrow' }, 'Continue playing'), h('h2', {}, recent.name),
        h('div', { class: 'sub' }, `${recent.mcVersion} · ${loaderLabel(recent)} · ${relativeTime(recent.lastPlayed)}`), progressBar(recent.id)),
      playButton(recent, true)));
  }

  const list = h('div', {});
  const paint = () => {
    const q = query.trim().toLowerCase();
    let items = state.instances.filter((i) => !q || `${i.name} ${i.mcVersion} ${i.loader} ${i.group}`.toLowerCase().includes(q));
    items = [...items].sort((a, b) => sort === 'name' ? a.name.localeCompare(b.name) : sort === 'played' ? b.playedMs - a.playedMs : (b.lastPlayed ?? b.created) - (a.lastPlayed ?? a.created));
    if (items.length === 0) return list.replaceChildren(emptyState('search', 'No matches', 'Nothing in your library matches that search.'));
    if (layout === 'dense') return list.replaceChildren(denseView(items));
    if (layout === 'glass' && !q) return list.replaceChildren(...glassView(items));
    list.replaceChildren(...groupedGrid(items, layout === 'classic' ? card : (i) => coverCard(i, layout === 'glass'), layout === 'classic' ? 'grid' : 'grid cover-grid'));
  };
  const search = h('input', { type: 'search', placeholder: 'Search instances', 'aria-label': 'Search instances', value: query, onInput: debounce(() => { query = search.value; paint(); }, 120) });
  const bar = h('div', { class: 'toolbar' }, h('div', { class: 'search-box' }, icon('search', 16), search),
    select([{ value: 'recent', label: 'Recently played' }, { value: 'name', label: 'Name' }, { value: 'played', label: 'Most played' }], sort, (v) => { sort = v; paint(); }));
  if (layout !== 'glass') page.appendChild(bar);
  else page.appendChild(bar);
  paint();
  page.appendChild(list);
  return page;
}
