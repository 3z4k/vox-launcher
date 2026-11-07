// Import instances from Prism / MultiMC, the official launcher and the CurseForge app.
import { api } from '../bridge';
import { attempt, dialogHead, h, openModal, spinner, toast, type Modal } from '../dom';
import { navigate, refreshInstances, runTask } from '../state';
import type { ImportCandidate } from '../types';

const LOADER_NAMES: Record<string, string> = { vanilla: 'Vanilla', fabric: 'Fabric', quilt: 'Quilt', forge: 'Forge', neoforge: 'NeoForge' };

function describe(c: ImportCandidate): string {
  const loader = c.loader === 'vanilla' ? 'Vanilla' : `${LOADER_NAMES[c.loader]}${c.loaderVersion ? ` ${c.loaderVersion}` : ''}`;
  return [`Minecraft ${c.mcVersion || '?'}`, loader, `${c.mods} mod${c.mods === 1 ? '' : 's'}`, `${c.worlds} world${c.worlds === 1 ? '' : 's'}`].join(' · ');
}

export async function importLauncherDialog(): Promise<void> {
  let m: Modal | null = null;
  const found = new Map<string, ImportCandidate>();
  const picked = new Set<string>();
  let includeWorlds = false;
  const list = h('div', { class: 'import-list' }, spinner('Looking for other launchers…'));
  const importBtn = h('button', { class: 'btn primary', type: 'button', disabled: true }, 'Import');

  const refreshButton = () => {
    importBtn.disabled = picked.size === 0;
    importBtn.textContent = picked.size ? `Import ${picked.size} instance${picked.size === 1 ? '' : 's'}` : 'Import';
  };

  const paint = () => {
    const items = [...found.values()];
    if (items.length === 0) {
      list.replaceChildren(h('div', { class: 'loading' }, 'Nothing found in the usual places. Use “Choose a folder…” to point at a Prism, MultiMC or CurseForge instance (or its instances folder), or at a .minecraft folder.'));
      return refreshButton();
    }
    const groups = new Map<string, ImportCandidate[]>();
    for (const c of items) groups.set(c.sourceLabel, [...(groups.get(c.sourceLabel) ?? []), c]);
    list.replaceChildren(...[...groups].flatMap(([label, rows]) => [
      h('div', { class: 'import-group' }, label),
      ...rows.map((c) => {
        const box = h('input', { type: 'checkbox', checked: picked.has(c.key), disabled: Boolean(c.problem) });
        box.addEventListener('change', () => { if (box.checked) picked.add(c.key); else picked.delete(c.key); refreshButton(); });
        return h('label', { class: `ver-row import-row${c.problem ? ' disabled' : ''}` }, box,
          h('div', { style: { flex: '1', minWidth: '0' } }, h('b', {}, c.name), h('div', { class: 'sub' }, describe(c)), c.problem ? h('div', { class: 'sub warn' }, `⚠ ${c.problem}`) : null));
      }),
    ]));
    refreshButton();
  };

  const add = (rows: ImportCandidate[], select = false) => {
    for (const c of rows) {
      found.set(c.key, c);
      if (select && !c.problem) picked.add(c.key);
    }
    paint();
  };

  const worldsBox = h('input', { type: 'checkbox' });
  worldsBox.addEventListener('change', () => { includeWorlds = worldsBox.checked; });

  m = openModal(h('div', { class: 'dlg' }, dialogHead('Import from another launcher', () => m!),
    h('p', { class: 'dlg-text' }, 'Pick the instances to copy into Vox. The originals are not changed, and sign-in data is never copied. Mod loaders are downloaded the first time you play.'),
    list,
    h('label', { class: 'check', style: { marginTop: '10px' } }, worldsBox, 'Also copy worlds (can be large)'),
    h('div', { class: 'dlg-actions' },
      h('button', { class: 'btn', type: 'button', onClick: async () => {
        const dir = await attempt(() => api.pickFolder());
        if (!dir) return;
        const rows = await attempt(() => api.scanImportFolder(dir));
        if (!rows) return;
        if (rows.length === 0) return toast('No instances were found in that folder.', true);
        add(rows, true);
      } }, 'Choose a folder…'),
      h('button', { class: 'btn', type: 'button', onClick: () => { for (const c of found.values()) if (!c.problem) picked.add(c.key); paint(); } }, 'Select all'),
      importBtn)), true);

  importBtn.addEventListener('click', async () => {
    const keys = [...picked];
    m!.close();
    const made = await runTask(`Importing ${keys.length} instance${keys.length === 1 ? '' : 's'}`, (taskId) => api.importInstances(keys, includeWorlds, taskId));
    if (!made || made.length === 0) return;
    await refreshInstances();
    toast(made.length === 1 ? `Imported “${made[0].name}”.` : `Imported ${made.length} instances.`);
    navigate(made.length === 1 ? { page: 'instance', id: made[0].id, tab: 'overview' } : { page: 'library' });
  });

  const rows = await attempt(() => api.detectImports());
  found.clear();
  add(rows ?? []);
}
