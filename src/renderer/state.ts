// Global UI state, navigation, background tasks and game events.
import { CUSTOM_VAR_NAMES, customVars, isLight, luminance } from './theme';
import { api } from './bridge';
import { conflictDialog } from './pages/conflicts';
import { attempt, errMsg, heads, toast } from './dom';
import type { AccountInfo, ContentKind, DeviceCode, InstanceConfig, LauncherUpdate, McVersion, Settings, TaskEvent } from './types';

export type Route =
  | { page: 'library' }
  | { page: 'instance'; id: string; tab: string }
  | { page: 'discover'; kind: ContentKind | 'modpacks'; instanceId: string | null }
  | { page: 'accounts' }
  | { page: 'downloads' }
  | { page: 'settings'; section: string };

export interface Prog {
  stage: string;
  current: number;
  total: number;
}

export interface Task {
  id: string;
  title: string;
  stage: string;
  current: number;
  total: number;
  done: boolean;
  bytes: number;
  bps: number;
  paused: boolean;
  startedAt: number;
}

export interface TaskLog {
  title: string;
  status: 'done' | 'failed' | 'cancelled';
  bytes: number;
  at: number;
  note: string;
}
export const taskLog: TaskLog[] = [];

export const DEFAULT_SETTINGS: Settings = {
  defaultMemoryMb: 4096, defaultMinMemoryMb: 512, javaPath: '', autoDownloadJava: true, jvmArgs: '', defaultWidth: 0, defaultHeight: 0,
  defaultFullscreen: false, wrapper: '', preLaunch: '', postExit: '', envVars: '', launcherAction: 'keep', showConsoleOnLaunch: true, checkLauncherUpdates: true, updateRepo: '', notifications: true,
  consoleMaxLines: 2000, showSnapshots: false, checkModUpdates: true, downloadThreads: 16, proxyUrl: '', discordEnabled: true,
  discordShowInstance: true, discordShowTime: true, discordShowServer: false, theme: 'voxel', layout: 'cover', accent: '', uiScale: 100, compact: false, reduceMotion: false, warnModConflicts: true, customTheme: {},
};

export const state = {
  route: { page: 'library' } as Route,
  instances: [] as InstanceConfig[],
  accounts: [] as AccountInfo[],
  settings: { ...DEFAULT_SETTINGS } as Settings,
  running: new Set<string>(),
  progress: new Map<string, Prog>(),
  logs: new Map<string, string[]>(),
  versions: null as McVersion[] | null,
  tasks: new Map<string, Task>(),
  loginCode: null as DeviceCode | null,
  loggingIn: false,
  hasVanillaDir: false,
  update: null as LauncherUpdate | null,
  /** Instances the user stopped on purpose, so the crash helper stays quiet. */
  stopped: new Set<string>(),
};

let renderHook: () => void = () => undefined;
let tasksHook: () => void = () => undefined;
export function tasksChanged(): void {
  tasksHook();
}
export function setHooks(render: () => void, tasks: () => void): void {
  renderHook = render;
  tasksHook = tasks;
}

let queued = false;
/** Re-renders the current page on the next animation frame (many calls collapse into one). */
export function scheduleRender(): void {
  if (queued) return;
  queued = true;
  requestAnimationFrame(() => {
    queued = false;
    renderHook();
  });
}

export function navigate(route: Route): void {
  state.route = route;
  renderHook();
}

export const instanceById = (id: string): InstanceConfig | undefined => state.instances.find((i) => i.id === id);
export const nameOf = (id: string): string => instanceById(id)?.name ?? id;

export async function refreshInstances(): Promise<void> {
  state.instances = (await attempt(() => api.listInstances())) ?? state.instances;
}
export async function refreshAccounts(): Promise<void> {
  state.accounts = (await attempt(() => api.listAccounts())) ?? state.accounts;
  // Faces load in the background so the account list never waits on the network.
  void api.accountSkins().then((m) => {
    const before = heads.size;
    heads.clear();
    for (const [id, url] of Object.entries(m)) heads.set(id, url);
    if (heads.size !== before || before > 0) scheduleRender();
  }).catch(() => undefined);
}

/** Saves a partial change to an instance and keeps the local copy in sync. */
export async function patchInstance(id: string, patch: Partial<InstanceConfig>): Promise<InstanceConfig | undefined> {
  const next = await attempt(() => api.updateInstance(id, patch));
  if (next) state.instances = state.instances.map((i) => (i.id === id ? next : i));
  return next;
}

export async function loadVersions(): Promise<McVersion[] | undefined> {
  if (!state.versions) state.versions = (await attempt(() => api.listVersions())) ?? null;
  return state.versions ?? undefined;
}

// ---- appearance ---------------------------------------------------------------------------------

export function applyAppearance(s: Settings): void {
  const root = document.documentElement;
  root.dataset.theme = s.theme;
  root.dataset.layout = s.layout;
  root.dataset.compact = String(s.compact);
  root.dataset.motion = s.reduceMotion ? 'reduced' : 'full';
  const custom = s.theme === 'custom';
  for (const v of CUSTOM_VAR_NAMES) root.style.removeProperty(v);
  if (custom) {
    for (const [k, v] of Object.entries(customVars(s.customTheme))) root.style.setProperty(k, v);
    root.style.colorScheme = isLight(s.customTheme) ? 'light' : 'dark';
  } else {
    root.style.removeProperty('color-scheme');
  }
  if (!custom && /^#[0-9a-f]{6}$/i.test(s.accent)) {
    root.style.setProperty('--accent', s.accent);
    root.style.setProperty('--accent-text', s.accent);
    root.style.setProperty('--on-accent', luminance(s.accent) > 0.4 ? '#0e1116' : '#ffffff');
  } else if (!custom) {
    for (const v of ['--accent', '--accent-text', '--on-accent']) root.style.removeProperty(v);
  }
  const app = document.getElementById('app');
  if (app) {
    const z = s.uiScale / 100;
    (app.style as CSSStyleDeclaration & { zoom: string }).zoom = String(z);
    app.style.height = `${100 / z}vh`;
  }
}

// ---- background tasks (installs, imports, exports) ---------------------------------------------

export async function runTask<T>(title: string, fn: (taskId: string) => Promise<T>): Promise<T | undefined> {
  const id = `t-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 6)}`;
  state.tasks.set(id, { id, title, stage: 'Starting…', current: 0, total: 0, done: false, bytes: 0, bps: 0, paused: false, startedAt: Date.now() });
  tasksHook();
  const note = (status: TaskLog['status'], msg: string): void => {
    taskLog.unshift({ title, status, bytes: state.tasks.get(id)?.bytes ?? 0, at: Date.now(), note: msg });
    taskLog.length = Math.min(taskLog.length, 30);
  };
  try {
    const out = await fn(id);
    note('done', '');
    void api.notify('Vox Launcher', `${title} finished.`).catch(() => undefined);
    return out;
  } catch (e) {
    const cancelled = /cancelled/i.test(errMsg(e));
    note(cancelled ? 'cancelled' : 'failed', cancelled ? '' : errMsg(e));
    if (cancelled) toast(`${title} cancelled.`);
    else {
      toast(errMsg(e), true);
      void api.notify('Vox Launcher', `${title} failed: ${errMsg(e)}`).catch(() => undefined);
    }
    return undefined;
  } finally {
    const t = state.tasks.get(id);
    if (t) t.done = true;
    tasksHook();
    setTimeout(() => {
      state.tasks.delete(id);
      tasksHook();
    }, 1800);
  }
}

// ---- launching -----------------------------------------------------------------------------------

export function stopGame(id: string): void {
  state.stopped.add(id);
  void attempt(() => api.kill(id));
}

export async function launch(id: string, join?: { server?: string; world?: string }): Promise<void> {
  if (state.settings.warnModConflicts) {
    const problems = await api.checkConflicts(id).catch(() => []);
    if (problems.length > 0) {
      const name = state.instances.find((i) => i.id === id)?.name ?? 'This instance';
      if (!(await conflictDialog(name, problems))) return;
    }
  }
  state.progress.set(id, { stage: 'Starting…', current: 0, total: 0 });
  state.logs.set(id, []);
  scheduleRender();
  if (state.settings.showConsoleOnLaunch) document.dispatchEvent(new CustomEvent('vox-console', { detail: { id, open: true } }));
  try {
    await api.launch(id, join);
  } catch (e) {
    toast(errMsg(e), true);
  } finally {
    state.progress.delete(id);
    scheduleRender();
  }
}

function pct(p: Prog): number {
  return p.total ? Math.round((p.current / p.total) * 100) : 0;
}

export function progressText(p: Prog): string {
  return `${p.stage}${p.total > 1 ? ` (${p.current}/${p.total})` : ''}`;
}

/** A shortcut (or a second start of the launcher) asked for this instance to be started. */
export function startFromRequest(id: string): void {
  const inst = state.instances.find((i) => i.id === id);
  if (!inst) return toast('That shortcut points to an instance that no longer exists.', true);
  if (state.running.has(id) || state.progress.has(id)) return;
  void launch(id);
}

export function wireEvents(): void {
  api.on('app:launch', (id) => startFromRequest(id));
  api.on('game:progress', (p) => {
    const first = !state.progress.has(p.id);
    state.progress.set(p.id, { stage: p.stage, current: p.current, total: p.total });
    // Update the bars in place so a download does not rebuild the page ten times a second.
    const fills = document.querySelectorAll<HTMLElement>(`[data-pfill="${p.id}"]`);
    if (first || fills.length === 0) return scheduleRender();
    const value = pct(p);
    fills.forEach((f) => (f.style.width = `${value}%`));
    document.querySelectorAll<HTMLElement>(`[data-pbar="${p.id}"]`).forEach((b) => b.setAttribute('aria-valuenow', String(value)));
    document.querySelectorAll<HTMLElement>(`[data-plabel="${p.id}"]`).forEach((l) => (l.textContent = progressText(state.progress.get(p.id)!)));
  });
  api.on('game:log', (e) => {
    const lines = state.logs.get(e.id) ?? [];
    lines.push(e.line);
    const max = state.settings.consoleMaxLines;
    if (lines.length > max) lines.splice(0, lines.length - max);
    state.logs.set(e.id, lines);
    document.dispatchEvent(new CustomEvent('vox-log', { detail: e }));
  });
  api.on('game:state', (e) => {
    if (e.running) state.running.add(e.id);
    else {
      state.running.delete(e.id);
      void refreshInstances().then(scheduleRender);
      const stoppedByUser = state.stopped.delete(e.id);
      if (e.code && e.code !== 0 && !stoppedByUser) {
        void api.notify('Minecraft stopped unexpectedly', `${nameOf(e.id)} exited with code ${e.code}. Open Vox to see what went wrong.`).catch(() => undefined);
        document.dispatchEvent(new CustomEvent('vox-crash', { detail: { id: e.id, code: e.code } }));
      }
    }
    state.progress.delete(e.id);
    scheduleRender();
  });
  api.on('auth:code', (c) => {
    state.loginCode = c;
    scheduleRender();
  });
  api.on('task:speed', (e) => {
    const task = state.tasks.get(e.taskId);
    if (!task) return;
    task.bytes = e.bytes;
    task.bps = e.bps;
    task.paused = e.paused;
    tasksHook();
  });
  api.on('task:progress', (t: TaskEvent) => {
    const task = state.tasks.get(t.taskId);
    if (!task) return;
    task.stage = t.stage;
    task.current = t.current;
    task.total = t.total;
    tasksHook();
  });
}
