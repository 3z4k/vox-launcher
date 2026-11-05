// Discover: search Modrinth and install into an instance (or create an instance from a modpack).
import { api } from '../bridge';
import {
  attempt, dialogHead, emptyState, fmtDate, fmtNum, h, icon, openModal, segmented, select, spinner, toast, type Modal,
} from '../dom';
import { instanceById, navigate, refreshInstances, runTask, state } from '../state';
import type { ContentKind, InstanceConfig, ModVersion, SearchHit } from '../types';

type Kind = ContentKind | 'modpacks';
const KINDS: { value: Kind; label: string }[] = [
  { value: 'mods', label: 'Mods' }, { value: 'modpacks', label: 'Modpacks' }, { value: 'resourcepacks', label: 'Resource packs' }, { value: 'shaderpacks', label: 'Shaders' },
];
const SORTS = [
  { value: 'relevance', label: 'Relevance' }, { value: 'downloads', label: 'Downloads' }, { value: 'follows', label: 'Follows' },
  { value: 'newest', label: 'Newest' }, { value: 'updated', label: 'Recently updated' },
];

// Search state survives re-renders (a game event must not wipe the results).
const S = { query: '', sort: 'relevance', hits: [] as SearchHit[], total: 0, loading: false, loaded: false, key: '', loadingMore: false };

export function renderDiscover(kind: Kind, instanceId: string | null): HTMLElement {
  const page = h('div', { class: 'page' });
  const target = kind === 'modpacks' ? undefined : instanceById(instanceId ?? '') ?? (kind === 'mods' ? state.instances.find((i) => i.loader !== 'vanilla') : undefined) ?? state.instances[0];
  const mc = target?.mcVersion ?? '';
  const loader = kind === 'mods' && target && target.loader !== 'vanilla' ? target.loader : null;
  const key = `${kind}|${target?.id ?? ''}|${S.sort}`;

  async function search(more = false): Promise<void> {
    const offset = more ? S.hits.length : 0;
    if (more) S.loadingMore = true; else { S.loading = true; S.hits = []; }
    paintResults();
    const r = await attempt(() => api.mrSearch(S.query, kind, mc, loader, S.sort, offset));
    S.loading = false; S.loadingMore = false; S.loaded = true; S.key = key;
    if (r) { S.hits = more ? [...S.hits, ...r.hits] : r.hits; S.total = r.totalHits; }
    paintResults();
  }

  const input = h('input', { type: 'search', placeholder: `Search ${KINDS.find((k) => k.value === kind)!.label.toLowerCase()} on Modrinth`, 'aria-label': 'Search Modrinth', value: S.query,
    onKeydown: (e: KeyboardEvent) => { if (e.key === 'Enter') { S.query = input.value.trim(); void search(); } } });
  const results = h('div', {});

  function paintResults(): void {
    if (S.loading) return results.replaceChildren(spinner('Searching Modrinth…'));
    if (S.hits.length === 0) return results.replaceChildren(emptyState('search', 'No results', S.query ? 'Try a different search.' : 'Nothing found.'));
    const parts: HTMLElement[] = [h('div', { class: 'disc-grid' }, S.hits.map((hit) => hitCard(hit, kind, target)))];
    if (S.hits.length < S.total) {
      parts.push(h('div', { style: { textAlign: 'center', marginTop: '18px' } }, h('button', { class: 'btn', type: 'button', disabled: S.loadingMore, onClick: () => void search(true) }, S.loadingMore ? 'Loading…' : `Load more (${S.hits.length} of ${fmtNum(S.total)})`)));
    }
    results.replaceChildren(...parts);
  }

  page.appendChild(h('div', { class: 'page-head' },
    h('div', {}, h('h1', {}, 'Discover'), h('div', { class: 'sub' }, 'Mods, modpacks, resource packs and shaders from Modrinth')),
    h('div', { class: 'head-actions' }, segmented(KINDS, kind, (v) => { S.loaded = false; navigate({ page: 'discover', kind: v, instanceId: target?.id ?? instanceId }); }))));

  const bar = h('div', { class: 'toolbar' }, h('div', { class: 'search-box' }, icon('search', 16), input),
    select(SORTS, S.sort, (v) => { S.sort = v; void search(); }),
    h('button', { class: 'btn primary', type: 'button', onClick: () => { S.query = input.value.trim(); void search(); } }, 'Search'));
  if (kind !== 'modpacks') {
    if (state.instances.length === 0) {
      page.appendChild(emptyState('cube', 'Create an instance first', 'Content is installed into an instance.', h('button', { class: 'btn primary', type: 'button', onClick: () => navigate({ page: 'library' }) }, 'Go to library')));
      return page;
    }
    bar.insertBefore(select(state.instances.map((i) => ({ value: i.id, label: `Install to: ${i.name}` })), target!.id, (v) => { S.loaded = false; navigate({ page: 'discover', kind, instanceId: v }); }), bar.children[1]);
  }
  page.appendChild(bar);
  if (kind === 'mods' && target && target.loader === 'vanilla') {
    page.appendChild(h('div', { class: 'panel' }, `“${target.name}” is vanilla, so mods will not load there. Pick an instance that uses Fabric, Quilt, Forge or NeoForge.`));
  }
  page.appendChild(results);
  if (!S.loaded || S.key !== key) void search(); else paintResults();
  return page;
}

function hitCard(hit: SearchHit, kind: Kind, target: InstanceConfig | undefined): HTMLElement {
  return h('div', { class: 'disc-card' },
    hit.iconUrl ? h('img', { class: 'item-icon', src: hit.iconUrl, alt: '', loading: 'lazy' }) : h('div', { class: 'item-icon' }, icon('cube', 24)),
    h('div', { class: 'body' },
      h('div', {}, h('b', {}, hit.title), ' ', h('span', { class: 'sub' }, `by ${hit.author}`)),
      h('div', { class: 'desc' }, hit.description),
      h('div', { class: 'disc-foot' }, h('span', {}, icon('download', 12), ` ${fmtNum(hit.downloads)}`), h('span', {}, icon('heart', 12), ` ${fmtNum(hit.follows)}`),
        h('button', { class: 'btn small primary', type: 'button', onClick: () => void versionDialog(hit, kind, target) }, kind === 'modpacks' ? 'Install' : 'Add'))));
}

async function versionDialog(hit: SearchHit, kind: Kind, target: InstanceConfig | undefined): Promise<void> {
  let m: Modal | null = null;
  const list = h('div', { class: 'ver-list' }, spinner());
  const loader = kind === 'mods' && target && target.loader !== 'vanilla' ? target.loader : null;
  m = openModal(h('div', { class: 'dlg' }, dialogHead(hit.title, () => m!),
    h('p', { class: 'dlg-text' }, kind === 'modpacks' ? 'Choose a version. A new instance is created from it.' : `Choose a version for ${target?.name ?? 'your instance'} (Minecraft ${target?.mcVersion}). Required dependencies are installed too.`), list), true);
  const versions: ModVersion[] | undefined = await attempt(() => api.mrVersions(hit.projectId, kind, target?.mcVersion ?? '', loader));
  if (!versions) return m.close();
  if (versions.length === 0) {
    list.replaceChildren(h('div', { class: 'loading' }, kind === 'modpacks' ? 'No versions found.' : `No version of this ${kind === 'mods' ? 'mod' : 'pack'} supports Minecraft ${target?.mcVersion}${loader ? ` on ${loader}` : ''}.`));
    return;
  }
  list.replaceChildren(...versions.slice(0, 40).map((v) => h('div', { class: 'ver-row' },
    h('div', { style: { flex: '1', minWidth: '0' } }, h('b', {}, v.name || v.versionNumber),
      h('div', { class: 'sub' }, `${v.versionNumber} · ${v.versionType} · ${fmtDate(Date.parse(v.datePublished))} · ${v.gameVersions.slice(0, 3).join(', ')}${v.gameVersions.length > 3 ? '…' : ''}`)),
    h('button', { class: 'btn small primary', type: 'button', onClick: async () => {
      m!.close();
      if (kind === 'modpacks') {
        const made = await runTask(`Installing ${hit.title}`, (taskId) => api.mrInstallModpack(v.id, taskId));
        if (!made) return;
        await refreshInstances();
        toast(`Created “${made.name}”.`);
        navigate({ page: 'instance', id: made.id, tab: 'overview' });
      } else if (target) {
        const files = await runTask(`Installing ${hit.title}`, (taskId) => api.mrInstall(target.id, kind, v.id, hit.title, hit.iconUrl, taskId));
        if (files) toast(files.length > 1 ? `Installed ${hit.title} and ${files.length - 1} dependenc${files.length === 2 ? 'y' : 'ies'}.` : `Installed ${hit.title}.`);
      }
    } }, 'Install'))));
}
