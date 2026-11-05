// The game console: a pop-up panel (not a page). It opens when a game launches (if enabled in Settings) and can be
// toggled from the corner button or an instance's header. Output is read from the live log buffer in state.ts.
import { h, icon, iconButton, select, toast } from '../dom';
import { nameOf, state, stopGame } from '../state';

export function colorLine(line: string): HTMLElement {
  const cls = /\b(ERROR|FATAL|Exception|Caused by)\b/.test(line) ? 'l-err' : /\bWARN\b/.test(line) ? 'l-warn' : /^\[Vox\]|^Vox:/.test(line) ? 'l-dim' : '';
  return h('div', { class: cls }, line || ' ');
}

const root = h('div', { id: 'console-popup' });
let open = false;
let big = false;
let current: string | null = null;
let filter = '';
let box: HTMLElement | null = null;
let signature = '';
let stick = true;

const ids = (): string[] => [...new Set([...state.logs.keys(), ...state.running])];

function paintLines(): void {
  if (!box || !current) return;
  const q = filter.toLowerCase();
  box.replaceChildren(...(state.logs.get(current) ?? []).filter((l) => !q || l.toLowerCase().includes(q)).map(colorLine));
  if (stick) box.scrollTop = box.scrollHeight;
}

function draw(): void {
  const all = ids();
  if (all.length === 0) {
    root.replaceChildren();
    box = null;
    return;
  }
  if (!current || !all.includes(current)) current = all[all.length - 1];
  const id = current;
  const running = state.running.has(id);
  if (!open) {
    box = null;
    root.className = 'closed';
    root.replaceChildren(h('button', { class: 'console-fab', type: 'button', 'aria-label': 'Show console', onClick: () => setConsole({ open: true }) },
      running ? h('span', { class: 'run-dot' }) : null, icon('terminal', 17), 'Console'));
    return;
  }
  root.className = big ? 'big' : '';
  box = h('div', { class: 'console', tabIndex: 0, 'aria-label': 'Game output' });
  box.addEventListener('scroll', () => { stick = !!box && box.scrollTop + box.clientHeight >= box.scrollHeight - 24; });
  const search = h('input', { type: 'search', placeholder: 'Filter', 'aria-label': 'Filter output', value: filter, onInput: () => { filter = search.value; paintLines(); } });
  root.replaceChildren(h('div', { class: 'console-panel', role: 'dialog', 'aria-label': 'Console' },
    h('div', { class: 'console-head' },
      h('div', { class: 'console-title' }, running ? h('span', { class: 'run-dot' }) : null, all.length > 1
        ? select(all.map((i) => ({ value: i, label: nameOf(i) })), id, (v) => { current = v; signature = ''; draw(); })
        : h('b', {}, nameOf(id)), h('span', { class: 'sub' }, running ? 'Running' : 'Stopped')),
      h('div', { class: 'console-tools' }, search,
        iconButton('copy', 'Copy output', async () => { await navigator.clipboard.writeText((state.logs.get(id) ?? []).join('\n')).catch(() => undefined); toast('Copied the output.'); }),
        running ? iconButton('stop', 'Stop the game', () => stopGame(id), true) : null,
        iconButton('layers', big ? 'Smaller' : 'Larger', () => { big = !big; signature = ''; draw(); }),
        iconButton('close', 'Hide console', () => setConsole({ open: false })))),
    box));
  stick = true;
  paintLines();
}

export function setConsole(opts: { open?: boolean; id?: string }): void {
  if (opts.id) current = opts.id;
  if (opts.open !== undefined) open = opts.open;
  signature = '';
  draw();
}

/** Called after every render: redraws only when something that changes the panel's shape did. */
export function syncConsole(): void {
  const sig = `${open}|${current}|${[...state.running].join()}|${ids().join()}`;
  if (sig === signature) return;
  signature = sig;
  draw();
}

export function mountConsole(): void {
  document.body.appendChild(root);
  document.addEventListener('vox-console', (e) => {
    const d = (e as CustomEvent<{ id?: string; open?: boolean; toggle?: boolean }>).detail;
    setConsole({ id: d.id, open: d.toggle ? !open : d.open });
  });
  document.addEventListener('vox-log', (e) => {
    const d = (e as CustomEvent<{ id: string; line: string }>).detail;
    if (!open || !box || d.id !== current) return;
    if (filter && !d.line.toLowerCase().includes(filter.toLowerCase())) return;
    box.appendChild(colorLine(d.line));
    while (box.childElementCount > state.settings.consoleMaxLines) box.firstElementChild?.remove();
    if (stick) box.scrollTop = box.scrollHeight;
  });
}

