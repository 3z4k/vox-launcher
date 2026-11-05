// "A new version is available" banner and dialog, plus the manual check used by Settings.
import { api } from '../bridge';
import { dialogHead, fmtBytes, h, icon, openModal, toast, type Modal } from '../dom';
import { runTask, state } from '../state';
import type { LauncherUpdate } from '../types';

const KEY = 'vox.dismissedUpdate';
const dismissed = (): string => { try { return localStorage.getItem(KEY) ?? ''; } catch { return ''; } };
const dismiss = (v: string): void => { try { localStorage.setItem(KEY, v); } catch { /* private mode */ } };

const host = h('div', { id: 'update-banner' });
export function mountUpdateBanner(): void {
  document.body.appendChild(host);
}

async function installNow(u: LauncherUpdate): Promise<void> {
  if (!u.assetUrl || !u.assetName) return void api.openExternal(u.pageUrl).catch(() => undefined);
  const { assetUrl, assetName, assetSize } = u;
  await runTask(`Downloading Vox Launcher ${u.version}`, (taskId) => api.installLauncherUpdate(assetUrl, assetName, assetSize, taskId));
}

function notesDialog(u: LauncherUpdate): void {
  let m: Modal | null = null;
  m = openModal(h('div', { class: 'dlg' }, dialogHead(`Vox Launcher ${u.version}`, () => m!),
    h('p', { class: 'sub' }, `You have ${u.current}.${u.assetSize ? ` The installer is ${fmtBytes(u.assetSize)}.` : ''}`),
    h('pre', { class: 'evidence', style: { whiteSpace: 'pre-wrap', maxHeight: '260px', overflow: 'auto' } }, u.notes.trim() || 'No release notes.'),
    h('div', { class: 'dlg-actions' },
      h('button', { class: 'btn', type: 'button', onClick: () => void api.openExternal(u.pageUrl).catch(() => undefined) }, icon('external', 16), 'Release page'),
      h('button', { class: 'btn primary', type: 'button', onClick: () => { m!.close(); void installNow(u); } }, icon('download', 16), 'Update now'))));
}

export function showUpdate(u: LauncherUpdate | null): void {
  state.update = u;
  if (!u || dismissed() === u.version) return host.replaceChildren();
  host.replaceChildren(h('div', { class: 'update-card', role: 'status' },
    h('div', { class: 'update-text' }, h('b', {}, `Vox Launcher ${u.version} is available`), h('span', { class: 'sub' }, `You are on ${u.current}.`)),
    h('div', { class: 'head-actions' },
      h('button', { class: 'btn small', type: 'button', onClick: () => notesDialog(u) }, "What's new"),
      h('button', { class: 'btn small primary', type: 'button', onClick: () => void installNow(u) }, icon('download', 15), 'Update'),
      h('button', { class: 'btn small ghost', type: 'button', onClick: () => { dismiss(u.version); host.replaceChildren(); } }, 'Later'))));
}

/** Checks GitHub; `manual` also reports "up to date" and errors. */
export async function checkForUpdate(manual: boolean): Promise<void> {
  try {
    const u = await api.checkLauncherUpdate();
    if (manual && u) { try { localStorage.removeItem(KEY); } catch { /* ignore */ } }
    showUpdate(u);
    if (u && !manual) void api.notify('Vox Launcher update', `Version ${u.version} is available.`).catch(() => undefined);
    if (manual && !u) toast('You are on the latest version.');
  } catch (e) {
    if (manual) toast(e instanceof Error ? e.message : String(e), true);
  }
}

