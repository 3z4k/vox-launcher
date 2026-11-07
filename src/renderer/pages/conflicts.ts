// Shown before launching when the mods in an instance look like they will cause trouble.
import { h, icon, openModal } from '../dom';
import type { Conflict } from '../types';

/** Resolves true when the player chooses to launch anyway. */
export function conflictDialog(name: string, problems: Conflict[]): Promise<boolean> {
  return new Promise((resolve) => {
    let go = false;
    const errors = problems.filter((p) => p.severity === 'error').length;
    const m = openModal(h('div', { class: 'dlg' },
      h('h2', {}, errors ? 'This instance probably won’t start' : 'Possible mod problems'),
      h('p', { class: 'dlg-text' }, `${name} has ${problems.length} thing${problems.length === 1 ? '' : 's'} worth a look before you play.`),
      h('div', { class: 'conflict-list' }, problems.map((p) => h('div', { class: `conflict ${p.severity}` },
        h('span', { class: 'conflict-ic' }, icon(p.severity === 'error' ? 'info' : 'sparkle', 16)),
        h('div', {}, h('b', {}, p.title), h('div', { class: 'sub' }, p.detail),
          p.files.length ? h('div', { class: 'conflict-files mono' }, p.files.join('  ·  ')) : null)))),
      h('div', { class: 'dlg-actions' },
        h('button', { class: 'btn', type: 'button', onClick: () => m.close() }, 'Cancel'),
        h('button', { class: `btn ${errors ? '' : 'primary'}`, type: 'button', onClick: () => { go = true; m.close(); } }, 'Launch anyway')),
    ), true);
    m.el.addEventListener('close', () => resolve(go));
  });
}
