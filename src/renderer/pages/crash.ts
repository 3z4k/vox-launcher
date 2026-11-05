// Crash helper: explains in plain English why the game stopped and offers one-click fixes.
import { api } from '../bridge';
import { attempt, dialogHead, h, icon, openModal, spinner, toast, type Modal } from '../dom';
import { instanceById, nameOf, navigate, patchInstance, state } from '../state';
import type { CrashReport, Finding, FixAction } from '../types';

const DRIVER_HELP = 'Open the NVIDIA app or GeForce Experience, AMD Software: Adrenalin Edition, or the Intel Driver & Support Assistant and install the newest graphics driver. Windows Settings → Windows Update → Advanced options → Optional updates can also list driver updates. Restart the PC afterwards.';

async function applyFix(id: string, a: FixAction, m: Modal, note: HTMLElement): Promise<void> {
  switch (a.kind) {
    case 'memory': {
      if (a.value && await patchInstance(id, { memoryMb: a.value })) toast(`${nameOf(id)} now uses up to ${a.value} MB.`);
      return;
    }
    case 'verify':
      if (await patchInstance(id, { verifyNext: true })) toast('Game files will be checked on the next launch.');
      return;
    case 'mods':
      m.close();
      navigate({ page: 'instance', id, tab: 'mods' });
      return;
    case 'java':
      m.close();
      navigate({ page: 'settings', section: 'java' });
      return;
    case 'account':
      m.close();
      navigate({ page: 'accounts' });
      return;
    case 'drivers':
      note.textContent = DRIVER_HELP;
      note.style.display = '';
  }
}

function findingCard(id: string, f: Finding, m: Modal): HTMLElement {
  const note = h('p', { class: 'hint', style: { display: 'none', marginTop: '8px' } });
  return h('div', { class: 'finding' },
    h('h3', {}, f.title),
    h('p', {}, f.explanation),
    f.evidence.length ? h('pre', { class: 'evidence' }, f.evidence.join('\n')) : null,
    h('p', { class: 'fix' }, h('b', {}, 'What to do: '), f.fix),
    f.action ? h('div', { class: 'head-actions' }, h('button', { class: 'btn primary small', type: 'button', onClick: () => void applyFix(id, f.action!, m, note) }, f.action.label)) : null,
    note);
}

/** Shows the helper for an instance. `code` is the game's exit code when it just stopped. */
export async function showCrashHelper(id: string, code: number | null): Promise<void> {
  if (!instanceById(id)) return;
  const body = h('div', {}, spinner('Reading the crash report…'));
  let report: CrashReport | undefined;
  const m: Modal = openModal(h('div', { class: 'dlg' }, dialogHead(code === null ? 'Crash helper' : `${nameOf(id)} stopped unexpectedly`, () => m), body), true);
  const tail = (state.logs.get(id) ?? []).slice(-300).join('\n');
  report = await attempt(() => api.analyzeCrash(id, tail, code));
  if (!report) return m.close();
  const r = report;
  body.replaceChildren(
    h('p', { class: 'sub' }, `Looked at ${r.source}${code !== null ? ` · exit code ${code}` : ''}. These are the most likely causes, best match first.`),
    ...r.findings.map((f) => findingCard(id, f, m)),
    h('div', { class: 'dlg-actions' },
      h('button', { class: 'btn', type: 'button', onClick: async () => { await navigator.clipboard.writeText(r.text).catch(() => undefined); toast('Copied the report. Paste it where you are asking for help.'); } }, icon('copy', 16), 'Copy report'),
      h('button', { class: 'btn', type: 'button', onClick: () => { m.close(); navigate({ page: 'instance', id, tab: 'logs' }); } }, icon('file', 16), 'Open logs'),
      h('button', { class: 'btn primary', type: 'button', onClick: () => m.close() }, 'Close')));
}
