// Download queue: everything running in the background, with speed, pause and cancel.
import { api } from '../bridge';
import { attempt, emptyState, fmtBytes, h, icon } from '../dom';
import { state, taskLog, tasksChanged } from '../state';
import type { Task } from '../state';

export function taskControls(t: Task): HTMLElement | null {
  if (t.done) return null;
  return h('div', { class: 'task-actions' },
    h('button', { class: 'btn small', type: 'button', onClick: () => { t.paused = !t.paused; tasksChanged(); void attempt(() => api.taskPause(t.id, t.paused)); } }, t.paused ? 'Resume' : 'Pause'),
    h('button', { class: 'btn small danger', type: 'button', onClick: () => void attempt(() => api.taskCancel(t.id)) }, 'Cancel'));
}

export function taskLine(t: Task): string {
  if (t.done) return 'Done';
  if (t.paused) return 'Paused';
  const speed = t.bps > 0 ? ` · ${fmtBytes(t.bps)}/s` : '';
  return `${t.stage}${t.total > 1 ? ` (${t.current}/${t.total})` : ''}${speed}`;
}

export function renderDownloads(): HTMLElement {
  const active = [...state.tasks.values()];
  const rows = active.map((t) => {
    const value = t.total ? Math.round((t.current / t.total) * 100) : 0;
    return h('div', { class: 'dl-row' },
      h('div', { class: 'dl-main' },
        h('b', {}, t.title),
        h('div', { class: `pbar${t.total || t.done ? '' : ' indeterminate'}` }, h('div', { style: { width: `${t.done ? 100 : value}%` } })),
        h('div', { class: 'sub' }, `${taskLine(t)}${t.bytes ? ` · ${fmtBytes(t.bytes)} downloaded` : ''}`)),
      taskControls(t));
  });
  const past = taskLog.map((l) => h('div', { class: 'dl-row past' },
    h('div', { class: 'dl-main' }, h('b', {}, l.title),
      h('div', { class: 'sub' }, `${l.status === 'done' ? 'Finished' : l.status === 'cancelled' ? 'Cancelled' : `Failed${l.note ? `: ${l.note}` : ''}`}${l.bytes ? ` · ${fmtBytes(l.bytes)}` : ''}`)),
    h('span', { class: `chip ${l.status}` }, l.status)));
  return h('div', { class: 'page' },
    h('div', { class: 'page-head' }, h('div', {}, h('h1', {}, 'Downloads'), h('p', { class: 'sub' }, 'Installs, imports and updates running in the background'))),
    active.length ? h('div', { class: 'panel dl-list' }, ...rows) : emptyState('download', 'Nothing downloading', 'Installs and updates show up here while they run, with their speed. You can pause or cancel them.'),
    past.length ? h('h3', { class: 'dl-title' }, 'Recent') : null,
    past.length ? h('div', { class: 'panel dl-list' }, ...past) : null,
    h('span', { hidden: true }, icon('download', 1)));
}
