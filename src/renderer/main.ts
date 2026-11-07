// Entry point: loads state, wires events, draws the sidebar and routes between pages.
import { api } from './bridge';
import { emptyState, faceAvatar, h, icon, logo, type IconName } from './dom';
import {
  applyAppearance, navigate, startFromRequest, refreshAccounts, refreshInstances, scheduleRender, setHooks, state, wireEvents, type Route,
} from './state';
import { renderDownloads, taskControls, taskLine } from './pages/downloads';
import { renderAccounts } from './pages/accounts';
import { mountConsole, syncConsole } from './pages/console';
import { showCrashHelper } from './pages/crash';
import { checkForUpdate, mountUpdateBanner } from './pages/update';
import { renderDiscover } from './pages/discover';
import { renderInstance } from './pages/instance';
import { renderLibrary } from './pages/library';
import { renderSettings } from './pages/settings';

const sidebar = document.getElementById('sidebar')!;
const main = document.getElementById('main')!;
const tray = document.getElementById('tray')!;

const NAV: { page: Route['page']; label: string; icon: IconName; go: () => Route }[] = [
  { page: 'library', label: 'Library', icon: 'grid', go: () => ({ page: 'library' }) },
  { page: 'discover', label: 'Discover', icon: 'compass', go: () => ({ page: 'discover', kind: 'mods', instanceId: null }) },
  { page: 'downloads', label: 'Downloads', icon: 'download', go: () => ({ page: 'downloads' }) },
  { page: 'accounts', label: 'Accounts', icon: 'user', go: () => ({ page: 'accounts' }) },
  { page: 'settings', label: 'Settings', icon: 'gear', go: () => ({ page: 'settings', section: 'general' }) },
];

function drawSidebar(): void {
  const active = state.accounts.find((a) => a.active);
  sidebar.replaceChildren(
    h('div', { class: 'brand' }, logo(34), h('div', {}, 'Vox', h('small', {}, 'Launcher'))),
    ...NAV.map((n) => h('button', { class: `nav-item${state.route.page === n.page ? ' active' : ''}`, type: 'button', 'aria-current': state.route.page === n.page ? 'page' : null, title: n.label, onClick: () => navigate(n.go()) },
      icon(n.icon, 19), h('span', { class: 'label' }, n.label))),
    h('div', { class: 'nav-spacer' }),
    h('button', { class: 'side-account', type: 'button', onClick: () => navigate({ page: 'accounts' }), 'aria-label': 'Accounts' },
      active ? faceAvatar(active, 32, 8) : h('div', { class: 'avatar', style: { background: 'var(--hover)', width: '32px', height: '32px', borderRadius: '8px' } }, icon('user', 16)),
      h('div', {}, h('div', { class: 'name' }, active ? active.name : 'No account'), h('div', { class: 'sub' }, active ? (active.offline ? 'Offline' : 'Microsoft') : 'Sign in to play online'))));
}

let lastKey = '';
function routeKey(r: Route): string {
  return r.page === 'instance' ? `instance:${r.id}:${r.tab}` : r.page === 'discover' ? `discover:${r.kind}` : r.page === 'settings' ? `settings:${r.section}` : r.page;
}

function render(): void {
  // A dialog or the lightbox is open: redrawing the page behind it is harmless but keeps scroll where it was.
  const key = routeKey(state.route);
  const scroll = key === lastKey ? main.scrollTop : 0;
  lastKey = key;
  drawSidebar();
  const r = state.route;
  let page: HTMLElement;
  try {
    page = r.page === 'library' ? renderLibrary()
      : r.page === 'instance' ? renderInstance(r.id, r.tab)
      : r.page === 'discover' ? renderDiscover(r.kind, r.instanceId)
      : r.page === 'accounts' ? renderAccounts()
      : r.page === 'downloads' ? renderDownloads()
      : renderSettings(r.section);
  } catch (e) {
    page = h('div', { class: 'page' }, emptyState('info', 'Something went wrong drawing this page', e instanceof Error ? e.message : String(e)));
  }
  main.replaceChildren(page);
  main.scrollTop = scroll;
  syncConsole();
}

function drawTray(): void {
  if (state.route.page === 'downloads') { tray.replaceChildren(); scheduleRender(); return; }
  tray.replaceChildren(...[...state.tasks.values()].map((t) => {
    const value = t.total ? Math.round((t.current / t.total) * 100) : 0;
    return h('div', { class: `task${t.done ? ' done' : ''}` }, h('b', {}, t.title),
      h('div', { class: `pbar${t.total && !t.done ? '' : t.done ? '' : ' indeterminate'}` }, h('div', { style: { width: `${t.done ? 100 : value}%` } })),
      h('div', { class: 'sub' }, taskLine(t)), taskControls(t));
  }));
}

async function boot(): Promise<void> {
  setHooks(render, drawTray);
  wireEvents();
  document.addEventListener('vox-rerender', render);
  mountConsole();
  mountUpdateBanner();
  document.addEventListener('vox-crash', (e) => {
    const d = (e as CustomEvent<{ id: string; code: number }>).detail;
    setTimeout(() => void showCrashHelper(d.id, d.code), 700);
  });
  try {
    state.settings = await api.getSettings();
  } catch { /* defaults apply */ }
  applyAppearance(state.settings);
  const [, , running, vanilla] = await Promise.all([
    refreshInstances(), refreshAccounts(), api.running().catch(() => [] as string[]), api.hasVanillaDir().catch(() => false),
  ]);
  state.running = new Set(running);
  state.hasVanillaDir = vanilla;
  render();
  scheduleRender();
  const requested = await api.takeLaunchRequest().catch(() => null);
  if (requested) startFromRequest(requested);
  if (state.settings.checkLauncherUpdates) setTimeout(() => void checkForUpdate(false), 2500);
}

void boot();
