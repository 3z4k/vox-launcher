// Discover: search Modrinth and install into an instance (or create an instance from a modpack).
import { api } from '../bridge';
import {
  attempt, dialogHead, emptyState, fmtDate, fmtNum, h, icon, openModal, segmented, select, spinner, toast, type Modal,
} from '../dom';
import { instanceById, loadVersions, navigate, refreshInstances, runTask, state } from '../state';
import type { ContentKind, InstallPlan, InstanceConfig, ModVersion, SearchHit } from '../types';

type Kind = ContentKind | 'modpacks';
const KINDS: { value: Kind; label: string }[] = [
  { value: 'mods', label: 'Mods' }, { value: 'modpacks', label: 'Modpacks' }, { value: 'resourcepacks', label: 'Resource packs' }, { value: 'shaderpacks', label: 'Shaders' },
];
const SORTS = [
  { value: 'relevance', label: 'Relevance' }, { value: 'downloads', label: 'Downloads' }, { value: 'follows', label: 'Follows' },
  { value: 'newest', label: 'Newest' }, { value: 'updated', label: 'Recently updated' },
];

// Search state survives re-renders (a game event must not wipe the results).
const S = {
  query: '', sort: 'relevance',
  mc: 'auto' as string,            // 'auto' = the instance's version, 'any', or a version id
  loaders: null as string[] | null, // null = follow the instance
  cats: [] as string[], env: '',
  page: 1, perPage: 20,
  hits: [] as SearchHit[], total: 0, loading: false, loaded: false, key: '', kind: '',
};

const PER_PAGE = [5, 10, 15, 20, 50, 100];
const ALL_LOADERS = [{ id: 'fabric', label: 'Fabric' }, { id: 'quilt', label: 'Quilt' }, { id: 'forge', label: 'Forge' }, { id: 'neoforge', label: 'NeoForge' }];
const LOADER_COMPAT: Record<string, string[]> = { fabric: ['fabric'], quilt: ['quilt', 'fabric'], forge: ['forge'], neoforge: ['neoforge'], vanilla: [] };
const ENVS = [{ value: '', label: 'Any' }, { value: 'client', label: 'Plays on my PC' }, { value: 'server', label: 'Works on servers' }];

const CATS: Record<Kind, { id: string; label: string }[]> = {
  mods: [
    ['adventure', 'Adventure'], ['decoration', 'Decoration'], ['equipment', 'Equipment'], ['food', 'Food'], ['game-mechanics', 'Game mechanics'], ['library', 'Library'], ['magic', 'Magic'],
    ['management', 'Management'], ['mobs', 'Mobs'], ['optimization', 'Performance'], ['social', 'Social'], ['storage', 'Storage'], ['technology', 'Technology'], ['transportation', 'Transport'], ['utility', 'Utility'], ['worldgen', 'World generation'],
  ].map(([id, label]) => ({ id: id!, label: label! })),
  modpacks: [
    ['adventure', 'Adventure'], ['challenging', 'Challenging'], ['combat', 'Combat'], ['kitchen-sink', 'Kitchen sink'], ['lightweight', 'Lightweight'], ['magic', 'Magic'], ['multiplayer', 'Multiplayer'],
    ['optimization', 'Performance'], ['quests', 'Quests'], ['technology', 'Technology'],
  ].map(([id, label]) => ({ id: id!, label: label! })),
  resourcepacks: [
    ['cartoon', 'Cartoon'], ['combat', 'Combat'], ['decoration', 'Decoration'], ['fantasy', 'Fantasy'], ['modded', 'Modded'], ['realistic', 'Realistic'], ['simplistic', 'Simple'], ['vanilla-like', 'Vanilla-like'], ['utility', 'Utility'],
  ].map(([id, label]) => ({ id: id!, label: label! })),
  shaderpacks: [
    ['cartoon', 'Cartoon'], ['fantasy', 'Fantasy'], ['realistic', 'Realistic'], ['semi-realistic', 'Semi-realistic'], ['vanilla-like', 'Vanilla-like'], ['cursed', 'Cursed'],
  ].map(([id, label]) => ({ id: id!, label: label! })),
};

export function modrinthUrl(hit: Pick<SearchHit, 'projectType' | 'slug' | 'projectId'>): string {
  const t = ['mod', 'modpack', 'resourcepack', 'shader', 'plugin', 'datapack'].includes(hit.projectType) ? hit.projectType : 'project';
  return `https://modrinth.com/${t}/${encodeURIComponent(hit.slug || hit.projectId)}`;
}

export function renderDiscover(kind: Kind, instanceId: string | null): HTMLElement {
  const page = h('div', { class: 'page' });
  const target = kind === 'modpacks' ? undefined : instanceById(instanceId ?? '') ?? (kind === 'mods' ? state.instances.find((i) => i.loader !== 'vanilla') : undefined) ?? state.instances[0];
  if (S.kind !== kind) { S.kind = kind; S.cats = []; S.loaders = null; S.mc = 'auto'; S.env = ''; S.page = 1; }

  // What is actually being filtered on right now.
  const mc = S.mc === 'any' ? '' : S.mc === 'auto' ? (target?.mcVersion ?? '') : S.mc;
  const followLoader = kind === 'mods' && target && target.loader !== 'vanilla' ? LOADER_COMPAT[target.loader]! : [];
  const loaders = kind === 'mods' || kind === 'modpacks' ? (S.loaders ?? followLoader) : [];
  const instLoader = kind === 'mods' && target && target.loader !== 'vanilla' ? target.loader : null;
  const key = JSON.stringify([kind, target?.id ?? '', S.sort, mc, loaders, S.cats, S.env]);

  /** `page` is 1-based. Any change to the query or filters goes back to page 1. */
  async function search(page = 1): Promise<void> {
    S.page = page; S.loading = true; S.hits = [];
    paintResults();
    const r = await attempt(() => api.mrSearchFiltered({ query: S.query, kind, mc, loaders, categories: S.cats, environment: S.env, sort: S.sort, offset: (page - 1) * S.perPage, limit: S.perPage }));
    S.loading = false; S.loaded = true; S.key = key;
    if (r) { S.hits = r.hits; S.total = r.totalHits; }
    paintResults();
  }
  const refresh = (): void => { document.dispatchEvent(new CustomEvent('vox-rerender')); };

  const input = h('input', { type: 'search', placeholder: `Search ${KINDS.find((k) => k.value === kind)!.label.toLowerCase()} on Modrinth`, 'aria-label': 'Search Modrinth', value: S.query,
    onKeydown: (e: KeyboardEvent) => { if (e.key === 'Enter') { S.query = input.value.trim(); void search(1); } } });
  const results = h('div', {});
  const summary = h('div', { class: 'disc-summary sub' });

  const pages = (): number => Math.max(1, Math.ceil(S.total / S.perPage));
  const goto = (p: number): void => { void search(Math.min(Math.max(1, p), pages())).then(() => document.getElementById('main')?.scrollTo({ top: 0 })); };

  function pager(): HTMLElement {
    const n = pages();
    const nums: (number | '…')[] = [];
    for (let p = 1; p <= n; p++) if (p === 1 || p === n || Math.abs(p - S.page) <= 1) nums.push(p); else if (nums[nums.length - 1] !== '…') nums.push('…');
    return h('div', { class: 'pager' },
      h('div', { class: 'pager-nav' },
        h('button', { class: 'btn small', type: 'button', disabled: S.page <= 1, onClick: () => goto(S.page - 1) }, icon('back', 14), 'Previous'),
        ...nums.map((p) => p === '…' ? h('span', { class: 'pager-gap' }, '…') : h('button', { class: `pager-num${p === S.page ? ' on' : ''}`, type: 'button', 'aria-current': p === S.page ? 'page' : null, onClick: () => goto(p) }, String(p))),
        h('button', { class: 'btn small', type: 'button', disabled: S.page >= n, onClick: () => goto(S.page + 1) }, 'Next')),
      h('div', { class: 'pager-size sub' }, 'Per page', select(PER_PAGE.map((v) => ({ value: String(v), label: String(v) })), String(S.perPage), (v) => { S.perPage = Number(v); void search(1); })));
  }

  function paintResults(): void {
    summary.textContent = S.loading ? '' : S.loaded && S.total ? `${fmtNum(S.total)} result${S.total === 1 ? '' : 's'} · page ${S.page} of ${fmtNum(pages())}` : '';
    if (S.loading) return results.replaceChildren(spinner('Searching Modrinth…'));
    if (S.hits.length === 0) return results.replaceChildren(emptyState('search', 'No results', S.query || S.cats.length || S.env ? 'Try a different search, or loosen the filters.' : 'Nothing found. Try choosing “Any” for the Minecraft version.'));
    results.replaceChildren(h('div', { class: 'disc-grid' }, S.hits.map((hit) => hitCard(hit, kind, target))), pager());
  }

  page.appendChild(h('div', { class: 'page-head' },
    h('div', {}, h('h1', {}, 'Discover'), h('div', { class: 'sub' }, 'Mods, modpacks, resource packs and shaders from Modrinth')),
    h('div', { class: 'head-actions' }, segmented(KINDS, kind, (v) => { S.loaded = false; navigate({ page: 'discover', kind: v, instanceId: target?.id ?? instanceId }); }))));

  const bar = h('div', { class: 'toolbar' }, h('div', { class: 'search-box' }, icon('search', 16), input),
    select(SORTS, S.sort, (v) => { S.sort = v; void search(1); }),
    h('button', { class: 'btn primary', type: 'button', onClick: () => { S.query = input.value.trim(); void search(1); } }, 'Search'));
  if (kind !== 'modpacks') {
    if (state.instances.length === 0) {
      page.appendChild(emptyState('cube', 'Create an instance first', 'Content is installed into an instance.', h('button', { class: 'btn primary', type: 'button', onClick: () => navigate({ page: 'library' }) }, 'Go to library')));
      return page;
    }
    bar.insertBefore(select(state.instances.map((i) => ({ value: i.id, label: `Install to: ${i.name}` })), target!.id, (v) => { S.loaded = false; S.mc = 'auto'; S.loaders = null; navigate({ page: 'discover', kind, instanceId: v }); }), bar.children[1]);
  }
  page.appendChild(bar);
  if (kind === 'mods' && target && target.loader === 'vanilla') {
    page.appendChild(h('div', { class: 'panel' }, `“${target.name}” is vanilla, so mods will not load there. Pick an instance that uses Fabric, Quilt, Forge or NeoForge.`));
  }

  // ---- filters -------------------------------------------------------------------------
  const changed = (): void => { S.query = input.value.trim(); S.loaded = false; refresh(); };
  const versionSelect = select([{ value: 'any', label: 'Any version' }, ...(target ? [{ value: 'auto', label: `Same as ${target.name} (${target.mcVersion})` }] : []), ...(state.versions ?? []).filter((v) => v.type === 'release').slice(0, 70).map((v) => ({ value: v.id, label: v.id }))],
    S.mc === 'auto' && !target ? 'any' : S.mc, (v) => { S.mc = v; changed(); });
  if (!state.versions) void loadVersions().then((v) => { if (v) refresh(); });
  const chip = (label: string, on: boolean, click: () => void): HTMLElement => h('button', { class: `fchip${on ? ' on' : ''}`, type: 'button', 'aria-pressed': String(on), onClick: click }, label);
  const filters = h('aside', { class: 'disc-filters panel' },
    h('div', { class: 'fgroup' }, h('div', { class: 'flabel' }, 'Minecraft version'), versionSelect),
    kind === 'mods' || kind === 'modpacks' ? h('div', { class: 'fgroup' }, h('div', { class: 'flabel' }, 'Mod loader'),
      h('div', { class: 'fchips' }, ALL_LOADERS.map((l) => chip(l.label, loaders.includes(l.id), () => {
        S.loaders = loaders.includes(l.id) ? loaders.filter((x) => x !== l.id) : [...loaders, l.id];
        changed();
      })))) : null,
    h('div', { class: 'fgroup' }, h('div', { class: 'flabel' }, 'Category'),
      h('div', { class: 'fchips' }, CATS[kind].map((c) => chip(c.label, S.cats.includes(c.id), () => { S.cats = S.cats.includes(c.id) ? S.cats.filter((x) => x !== c.id) : [...S.cats, c.id]; changed(); })))),
    kind === 'mods' || kind === 'modpacks' ? h('div', { class: 'fgroup' }, h('div', { class: 'flabel' }, 'Where it runs'),
      h('div', { class: 'fchips' }, ENVS.map((e) => chip(e.label, S.env === e.value, () => { S.env = e.value; changed(); })))) : null,
    h('button', { class: 'btn small ghost', type: 'button', onClick: () => { S.mc = 'auto'; S.loaders = null; S.cats = []; S.env = ''; changed(); } }, 'Reset filters'));
  void instLoader;

  page.appendChild(h('div', { class: 'disc-layout' }, filters, h('div', { class: 'disc-main' }, summary, results)));
  if (!S.loaded || S.key !== key) void search(1); else paintResults();
  return page;
}

function hitCard(hit: SearchHit, kind: Kind, target: InstanceConfig | undefined): HTMLElement {
  const url = modrinthUrl(hit);
  const open = (e: Event): void => { e.preventDefault(); void attempt(() => api.openExternal(url)); };
  return h('div', { class: 'disc-card' },
    h('a', { href: url, class: 'disc-icon', title: 'Open on Modrinth', tabindex: -1, 'aria-hidden': 'true', onClick: open },
      hit.iconUrl ? h('img', { class: 'item-icon', src: hit.iconUrl, alt: '', loading: 'lazy' }) : h('div', { class: 'item-icon' }, icon('cube', 24))),
    h('div', { class: 'body' },
      h('div', { class: 'disc-titlebar' },
        h('a', { href: url, class: 'disc-title', title: 'Open the download page on Modrinth', onClick: open }, hit.title, icon('external', 12)),
        h('span', { class: 'sub' }, `by ${hit.author}`)),
      h('div', { class: 'desc' }, hit.description),
      hit.categories.length ? h('div', { class: 'disc-tags' }, hit.categories.filter((c) => !['fabric', 'quilt', 'forge', 'neoforge', 'minecraft'].includes(c)).slice(0, 3).map((c) => h('span', { class: 'chip' }, c.replace(/-/g, ' ')))) : null,
      h('div', { class: 'disc-foot' }, h('span', {}, icon('download', 12), ` ${fmtNum(hit.downloads)}`), h('span', {}, icon('heart', 12), ` ${fmtNum(hit.follows)}`),
        h('button', { class: 'btn small primary', type: 'button', onClick: () => void versionDialog(hit, kind, target) }, kind === 'modpacks' ? 'Install' : 'Add'))));
}

const COMPAT: Record<string, string[]> = { fabric: ['fabric'], quilt: ['quilt', 'fabric'], forge: ['forge'], neoforge: ['neoforge'], vanilla: [] };

/** Why a version may not work in `target` (empty when it looks fine). */
function mismatch(v: ModVersion, kind: Kind, target: InstanceConfig | undefined): string[] {
  if (!target || kind === 'modpacks') return [];
  const out: string[] = [];
  if (v.gameVersions.length && !v.gameVersions.includes(target.mcVersion)) out.push(`Made for Minecraft ${v.gameVersions.slice(0, 3).join(', ')}, not ${target.mcVersion}`);
  if (kind === 'mods' && target.loader !== 'vanilla' && v.loaders.length && !v.loaders.some((l) => COMPAT[target.loader].includes(l))) out.push(`Made for ${v.loaders.join(', ')}, not ${target.loader}`);
  return out;
}

/** Shows what will be installed (and any warnings). Resolves true when the person goes ahead. */
function planDialog(title: string, plan: InstallPlan, extra: string[]): Promise<boolean> {
  return new Promise((resolve) => {
    let go = false;
    const deps = plan.items.filter((i) => i.role === 'dependency');
    const warnings = [...extra, ...plan.warnings];
    const m: Modal = openModal(h('div', { class: 'dlg' },
      h('h2', {}, `Install ${title}`),
      deps.length ? h('p', { class: 'dlg-text' }, `${title} needs ${deps.length === 1 ? 'another mod' : `${deps.length} other mods`} to work. ${deps.length === 1 ? 'It' : 'They'} will be installed too:`) : h('p', { class: 'dlg-text' }, 'This will be installed:'),
      h('ul', { class: 'plan-list' }, ...plan.items.map((i) => h('li', {}, h('b', {}, i.title), h('span', { class: 'sub' }, ` ${i.versionNumber}${i.role === 'dependency' ? ' · required' : ''}`)))),
      plan.alreadyInstalled.length ? h('p', { class: 'sub' }, `Already installed, nothing to do: ${plan.alreadyInstalled.join(', ')}.`) : null,
      warnings.length ? h('div', { class: 'warn-box' }, h('b', {}, 'Heads up'), h('ul', {}, ...warnings.map((w) => h('li', {}, w)))) : null,
      h('div', { class: 'dlg-actions' },
        h('button', { class: 'btn', type: 'button', onClick: () => m.close() }, 'Cancel'),
        h('button', { class: 'btn primary', type: 'button', onClick: () => { go = true; m.close(); } }, warnings.length ? 'Install anyway' : 'Install'))));
    m.el.addEventListener('close', () => resolve(go));
  });
}

async function versionDialog(hit: SearchHit, kind: Kind, target: InstanceConfig | undefined): Promise<void> {
  let m: Modal | null = null;
  const list = h('div', { class: 'ver-list' }, spinner());
  const foot = h('div', { class: 'dlg-foot' });
  const loader = kind === 'mods' && target && target.loader !== 'vanilla' ? target.loader : null;
  const noLoader = kind === 'mods' && target?.loader === 'vanilla';
  m = openModal(h('div', { class: 'dlg' }, dialogHead(hit.title, () => m!),
    h('p', { class: 'dlg-text' }, kind === 'modpacks' ? 'Choose a version. A new instance is created from it.' : `Choose a version for ${target?.name ?? 'your instance'} (Minecraft ${target?.mcVersion}). Other mods it needs are listed before anything is installed.`),
    noLoader ? h('div', { class: 'warn-box' }, 'This instance has no mod loader (Fabric, Quilt, Forge or NeoForge), so mods will not load. Add a loader in the instance\'s settings first.') : null,
    list, foot), true);

  const install = async (v: ModVersion): Promise<void> => {
    if (kind === 'modpacks') {
      m!.close();
      const made = await runTask(`Installing ${hit.title}`, (taskId) => api.mrInstallModpack(v.id, taskId));
      if (!made) return;
      await refreshInstances();
      toast(`Created \u201c${made.name}\u201d.`);
      navigate({ page: 'instance', id: made.id, tab: 'overview' });
      return;
    }
    if (!target) return;
    const warn = mismatch(v, kind, target);
    let needsConfirm = warn.length > 0;
    let plan: InstallPlan | undefined;
    if (kind === 'mods') {
      plan = await attempt(() => api.mrPlan(target.id, kind as ContentKind, v.id, hit.title));
      if (plan) needsConfirm = needsConfirm || plan.items.length > 1 || plan.warnings.length > 0;
    }
    if (needsConfirm) {
      const shown = plan ?? { items: [{ projectId: v.projectId, versionId: v.id, title: hit.title, versionNumber: v.versionNumber, role: 'main' as const }], alreadyInstalled: [], warnings: [] };
      // the plan already repeats version problems, so only add ones it did not mention
      const extra = warn.filter((w) => !shown.warnings.some((x) => x.includes(w.slice(0, 20))));
      if (!(await planDialog(hit.title, shown, extra))) return;
    }
    m!.close();
    const files = await runTask(`Installing ${hit.title}`, (taskId) => api.mrInstall(target.id, kind as ContentKind, v.id, hit.title, hit.iconUrl, taskId));
    if (files) toast(files.length > 1 ? `Installed ${hit.title} and ${files.length - 1} dependenc${files.length === 2 ? 'y' : 'ies'}.` : `Installed ${hit.title}.`);
  };

  const show = (versions: ModVersion[], flagged: boolean): void => {
    list.replaceChildren(...versions.slice(0, 60).map((v) => {
      const bad = flagged ? mismatch(v, kind, target) : [];
      return h('div', { class: 'ver-row' },
        h('div', { style: { flex: '1', minWidth: '0' } }, h('b', {}, v.name || v.versionNumber),
          h('div', { class: 'sub' }, `${v.versionNumber} \u00b7 ${v.versionType} \u00b7 ${fmtDate(Date.parse(v.datePublished))} \u00b7 ${v.gameVersions.slice(0, 3).join(', ')}${v.gameVersions.length > 3 ? '\u2026' : ''}`),
          bad.length ? h('div', { class: 'sub warn' }, `\u26a0 ${bad.join('. ')}`) : null),
        h('button', { class: `btn small ${bad.length ? '' : 'primary'}`, type: 'button', onClick: () => void install(v) }, 'Install'));
    }));
  };

  const filtered: ModVersion[] | undefined = await attempt(() => api.mrVersions(hit.projectId, kind, target?.mcVersion ?? '', loader));
  if (!filtered) return m.close();
  const canShowAll = kind !== 'modpacks' && !!target;
  const showAll = async (): Promise<void> => {
    list.replaceChildren(spinner());
    const all = await attempt(() => api.mrVersions(hit.projectId, kind, '', null));
    if (!all) return m!.close();
    show(all, true);
    foot.replaceChildren(h('span', { class: 'sub' }, 'Showing every version. Ones that do not match this instance are marked.'));
  };
  if (filtered.length === 0) {
    list.replaceChildren(h('div', { class: 'loading' }, kind === 'modpacks' ? 'No versions found.' : `No version of this ${kind === 'mods' ? 'mod' : 'pack'} supports Minecraft ${target?.mcVersion}${loader ? ` on ${loader}` : ''}.`));
    if (canShowAll) foot.replaceChildren(h('button', { class: 'btn small', type: 'button', onClick: () => void showAll() }, 'See versions for other Minecraft versions or loaders'));
    return;
  }
  show(filtered, false);
  if (canShowAll) foot.replaceChildren(h('button', { class: 'btn small ghost', type: 'button', onClick: () => void showAll() }, 'Show versions for other Minecraft versions or loaders'));
}
