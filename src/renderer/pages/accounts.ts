// Accounts (Microsoft + offline) and the skin / cape manager.
import { api } from '../bridge';
import { attempt, card, colorFor, confirmDialog, emptyState, formDialog, h, icon, iconButton, segmented, spinner, toast } from '../dom';
import { refreshAccounts, scheduleRender, state } from '../state';
import type { AccountInfo, LibSkin, SkinInfo } from '../types';

let skin: SkinInfo | null = null;
let skinFor = '';
let skinLoading = false;
let skinError = '';

async function loadSkin(accountId: string): Promise<void> {
  skinLoading = true;
  skinError = '';
  scheduleRender();
  try {
    skin = await api.getSkinInfo();
    skinFor = accountId;
  } catch (e) {
    skin = null;
    skinError = e instanceof Error ? e.message : String(e);
  }
  skinLoading = false;
  scheduleRender();
}

async function signIn(): Promise<void> {
  state.loggingIn = true;
  state.loginCode = null;
  scheduleRender();
  try {
    await api.login();
    toast('Signed in.');
    skinFor = '';
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);
    if (!/cancel/i.test(msg)) toast(msg.replace(/^Error: /, ''), true);
  } finally {
    state.loggingIn = false;
    state.loginCode = null;
    await refreshAccounts();
    scheduleRender();
  }
}

function accountRow(a: AccountInfo): HTMLElement {
  return h('div', { class: 'item' },
    h('div', { class: 'avatar', style: { background: colorFor(a.id), width: '40px', height: '40px', borderRadius: '10px' } }, a.name.charAt(0).toUpperCase()),
    h('div', { class: 'grow' }, h('div', { class: 'title' }, a.name), h('div', { class: 'badge-row' }, h('span', { class: 'chip' }, a.offline ? 'Offline' : 'Microsoft'), a.active ? h('span', { class: 'chip accent' }, 'Active') : null)),
    h('div', { class: 'item-actions' },
      a.active ? null : h('button', { class: 'btn small', type: 'button', onClick: async () => { await attempt(() => api.setActiveAccount(a.id)); await refreshAccounts(); skinFor = ''; scheduleRender(); } }, 'Use'),
      iconButton('trash', `Remove ${a.name}`, async () => {
        if (!(await confirmDialog(`Remove ${a.name} from this launcher? You can sign in again later.`, 'Remove'))) return;
        await attempt(() => api.removeAccount(a.id));
        await refreshAccounts();
        skinFor = '';
        scheduleRender();
      }, true)));
}

/** Draws a front or back view of a skin texture onto a 16x32 canvas. */
export function drawSkin(canvas: HTMLCanvasElement, src: string, slim: boolean, back = false): void {
  const img = new Image();
  img.onload = () => {
    const ctx = canvas.getContext('2d');
    if (!ctx) return;
    ctx.imageSmoothingEnabled = false;
    ctx.clearRect(0, 0, 16, 32);
    const modern = img.height >= 64;
    const arm = slim ? 3 : 4;
    const part = (sx: number, sy: number, w: number, hh: number, dx: number, dy: number, flip = false) => {
      if (flip) {
        ctx.save();
        ctx.translate(dx + w, dy);
        ctx.scale(-1, 1);
        ctx.drawImage(img, sx, sy, w, hh, 0, 0, w, hh);
        ctx.restore();
      } else ctx.drawImage(img, sx, sy, w, hh, dx, dy, w, hh);
    };
    // dx for arms: the arm on the viewer's left sits at 4 - arm, the one on the right at 12.
    const rightArmX = back ? 12 : 4 - arm;
    const leftArmX = back ? 4 - arm : 12;
    const rightLegX = back ? 8 : 4;
    const leftLegX = back ? 4 : 8;
    // [front, back] source rectangles
    const head = back ? [24, 8] : [8, 8];
    const body = back ? [32, 20] : [20, 20];
    const rArm = back ? [52, 20] : [44, 20];
    const lArm = modern ? (back ? [44, 52] : [36, 52]) : rArm;
    const rLeg = back ? [12, 20] : [4, 20];
    const lLeg = modern ? (back ? [28, 52] : [20, 52]) : rLeg;
    part(head[0], head[1], 8, 8, 4, 0);
    part(body[0], body[1], 8, 12, 4, 8);
    part(rArm[0], rArm[1], arm, 12, rightArmX, 8);
    part(lArm[0], lArm[1], arm, 12, leftArmX, 8, !modern);
    part(rLeg[0], rLeg[1], 4, 12, rightLegX, 20);
    part(lLeg[0], lLeg[1], 4, 12, leftLegX, 20, !modern);
    if (modern) {
      const hat = back ? [56, 8] : [40, 8];
      const jacket = back ? [32, 36] : [20, 36];
      const rSleeve = back ? [52, 36] : [44, 36];
      const lSleeve = back ? [60, 52] : [52, 52];
      const rPant = back ? [12, 36] : [4, 36];
      const lPant = back ? [12, 52] : [4, 52];
      part(hat[0], hat[1], 8, 8, 4, 0);
      part(jacket[0], jacket[1], 8, 12, 4, 8);
      part(rSleeve[0], rSleeve[1], arm, 12, rightArmX, 8);
      part(lSleeve[0], lSleeve[1], arm, 12, leftArmX, 8);
      part(rPant[0], rPant[1], 4, 12, rightLegX, 20);
      part(lPant[0], lPant[1], 4, 12, leftLegX, 20);
    }
  };
  img.src = src;
}

let library: LibSkin[] | null = null;
let backView = false;
let pendingVariant: 'classic' | 'slim' = 'classic';

async function loadLibrary(): Promise<void> {
  library = (await attempt(() => api.skinLibrary())) ?? [];
  scheduleRender();
}

function miniSkin(src: string, slim: boolean): HTMLCanvasElement {
  const c = h('canvas', { width: 16, height: 32, class: 'mini-skin', 'aria-hidden': 'true' });
  drawSkin(c, src, slim);
  return c;
}

function librarySection(active: AccountInfo): HTMLElement {
  if (library === null) void loadLibrary();
  const sec = card('Skin library', 'Skins you save here can be worn with one click.');
  const online = !active.offline;
  const addFile = async () => {
    const p = await attempt(() => api.pickFile('skin'));
    if (!p) return;
    const r = await formDialog('Add skin to library', [
      { key: 'name', label: 'Name', placeholder: 'My skin', value: (p.split(/[\\/]/).pop() ?? '').replace(/\.png$/i, '') },
      { key: 'variant', label: 'Arm style', type: 'select', value: pendingVariant, options: [{ value: 'classic', label: 'Classic arms (Steve)' }, { value: 'slim', label: 'Slim arms (Alex)' }] },
    ], 'Add');
    if (!r) return;
    pendingVariant = r.variant as 'classic' | 'slim';
    if (await attempt(() => api.skinLibraryAdd(p, String(r.name), pendingVariant))) void loadLibrary();
  };
  const saveCurrent = async () => {
    const r = await formDialog('Save current skin', [{ key: 'name', label: 'Name', placeholder: 'My current skin' }], 'Save');
    if (!r) return;
    if (await attempt(() => api.skinLibraryAdd(null, String(r.name), 'classic'))) { toast('Saved to your library.'); void loadLibrary(); }
  };
  sec.appendChild(h('div', { class: 'toolbar' },
    h('button', { class: 'btn primary', type: 'button', onClick: () => void addFile() }, icon('plus', 16), 'Add skin file…'),
    online ? h('button', { class: 'btn', type: 'button', onClick: () => void saveCurrent() }, icon('save', 16), 'Save current skin') : null));
  if (library === null) { sec.appendChild(spinner()); return sec; }
  if (library.length === 0) {
    sec.appendChild(emptyState('user', 'No saved skins', 'Add a 64×64 PNG, or save the skin you are wearing, to build your collection.'));
    return sec;
  }
  sec.appendChild(h('div', { class: 'skin-grid' }, library.map((sk) => h('div', { class: 'skin-card' },
    h('div', { class: 'skin-thumb' }, miniSkin(sk.data, sk.variant === 'slim')),
    h('div', { class: 'title' }, sk.name),
    h('span', { class: 'chip' }, sk.variant === 'slim' ? 'Slim' : 'Classic'),
    h('div', { class: 'skin-actions' },
      h('button', { class: 'btn small primary', type: 'button', disabled: !online, title: online ? '' : 'Sign in with Microsoft to wear skins', onClick: async () => {
        if (await attempt(() => api.skinLibraryApply(sk.id, null).then(() => true))) { toast(`Now wearing “${sk.name}”.`); skinFor = ''; scheduleRender(); }
      } }, 'Wear'),
      iconButton('edit', `Edit ${sk.name}`, async () => {
        const r = await formDialog('Edit skin', [
          { key: 'name', label: 'Name', value: sk.name },
          { key: 'variant', label: 'Arm style', type: 'select', value: sk.variant, options: [{ value: 'classic', label: 'Classic arms (Steve)' }, { value: 'slim', label: 'Slim arms (Alex)' }] },
        ], 'Save');
        if (!r) return;
        if (await attempt(() => api.skinLibraryUpdate(sk.id, String(r.name), r.variant as 'classic' | 'slim').then(() => true))) void loadLibrary();
      }),
      iconButton('trash', `Delete ${sk.name}`, async () => {
        if (!(await confirmDialog(`Remove “${sk.name}” from your library? Your Minecraft account is not changed.`, 'Remove'))) return;
        await attempt(() => api.skinLibraryRemove(sk.id));
        void loadLibrary();
      }, true))))));
  return sec;
}

function skinPanel(active: AccountInfo): HTMLElement {
  const panel = card('Current skin', active.name);
  if (active.offline) {
    panel.appendChild(h('p', { class: 'sub' }, 'Offline accounts have no skin to manage. Sign in with Microsoft to change your skin. You can still build a library below.'));
    return panel;
  }
  if (skinFor !== active.id && !skinLoading && !skinError) void loadSkin(active.id);
  if (skinLoading) { panel.appendChild(spinner('Loading your profile…')); return panel; }
  if (skinError || !skin) {
    panel.appendChild(h('p', { class: 'sub' }, skinError || 'Could not load your profile.'));
    panel.appendChild(h('button', { class: 'btn', type: 'button', onClick: () => { skinError = ''; skinFor = ''; scheduleRender(); } }, icon('refresh', 16), 'Try again'));
    return panel;
  }
  const info = skin;
  const canvas = h('canvas', { width: 16, height: 32, 'aria-label': 'Skin preview' });
  if (info.skin) drawSkin(canvas, info.skin, info.variant === 'slim', backView);
  panel.appendChild(h('div', { class: 'skin-stage' }, canvas,
    segmented<'front' | 'back'>([{ value: 'front', label: 'Front' }, { value: 'back', label: 'Back' }], backView ? 'back' : 'front', (v) => { backView = v === 'back'; scheduleRender(); })));
  let variant = info.variant;
  const variantSel = h('select', { 'aria-label': 'Arm style', onChange: () => { variant = variantSel.value as 'classic' | 'slim'; } },
    h('option', { value: 'classic' }, 'Classic arms (Steve)'), h('option', { value: 'slim' }, 'Slim arms (Alex)'));
  variantSel.value = variant;
  panel.appendChild(h('div', { class: 'toolbar' }, variantSel,
    h('button', { class: 'btn primary', type: 'button', onClick: async () => {
      const p = await attempt(() => api.pickFile('skin'));
      if (!p) return;
      if (await attempt(() => api.uploadSkin(p, variant).then(() => true))) { toast('Skin updated.'); skinFor = ''; scheduleRender(); }
    } }, icon('upload', 16), 'Upload skin…'),
    h('button', { class: 'btn', type: 'button', onClick: async () => {
      if (!(await confirmDialog('Reset to the default skin?', 'Reset', false))) return;
      if (await attempt(() => api.resetSkin().then(() => true))) { toast('Skin reset.'); skinFor = ''; scheduleRender(); }
    } }, 'Reset')));
  if (info.capes.length) {
    panel.appendChild(h('div', { class: 'group-title' }, 'Capes'));
    panel.appendChild(h('div', { class: 'cape-row' },
      h('button', { class: `cape${info.capes.some((c) => c.active) ? '' : ' active'}`, type: 'button', title: 'No cape', style: { height: '90px' }, onClick: async () => { await attempt(() => api.setCape(null)); skinFor = ''; scheduleRender(); } }, h('span', { class: 'sub' }, 'None')),
      ...info.capes.map((c) => {
        const cv = h('canvas', { width: 10, height: 16 });
        if (c.data) {
          const im = new Image();
          im.onload = () => { const x = cv.getContext('2d'); if (x) { x.imageSmoothingEnabled = false; x.drawImage(im, 1, 1, 10, 16, 0, 0, 10, 16); } };
          im.src = c.data;
        }
        return h('button', { class: `cape${c.active ? ' active' : ''}`, type: 'button', title: c.alias, onClick: async () => { await attempt(() => api.setCape(c.id)); skinFor = ''; scheduleRender(); } }, cv);
      })));
  }
  return panel;
}

export function renderAccounts(): HTMLElement {
  const page = h('div', { class: 'page' });
  page.appendChild(h('div', { class: 'page-head' }, h('div', {}, h('h1', {}, 'Accounts'), h('div', { class: 'sub' }, 'Sign in to play online, or add an offline profile for singleplayer')),
    h('div', { class: 'head-actions' },
      h('button', { class: 'btn', type: 'button', onClick: async () => {
        const r = await formDialog('Offline account', [{ key: 'name', label: 'Username', placeholder: 'Steve', hint: '3–16 letters, numbers or underscores.' }], 'Add', 'Offline accounts cannot join online-mode servers or use a skin.');
        if (!r) return;
        if (await attempt(() => api.addOfflineAccount(String(r.name).trim()).then(() => true))) { await refreshAccounts(); scheduleRender(); }
      } }, icon('user', 16), 'Add offline'),
      h('button', { class: 'btn primary', type: 'button', disabled: state.loggingIn, onClick: () => void signIn() }, icon('plus', 16), 'Sign in with Microsoft'))));

  if (state.loggingIn) {
    const code = state.loginCode;
    page.appendChild(card('Sign in', null,
      code
        ? h('div', {}, h('p', {}, 'Open the page below, then enter this code:'), h('div', { class: 'code-box' }, code.userCode),
          h('div', { class: 'head-actions' },
            h('button', { class: 'btn primary', type: 'button', onClick: () => void attempt(() => api.openExternal(code.verificationUri)) }, icon('external', 16), 'Open sign-in page'),
            h('button', { class: 'btn', type: 'button', onClick: async () => { await navigator.clipboard.writeText(code.userCode).catch(() => undefined); toast('Code copied.'); } }, icon('copy', 16), 'Copy code'),
            h('button', { class: 'btn ghost', type: 'button', onClick: () => void attempt(() => api.cancelLogin()) }, 'Cancel')),
          h('p', { class: 'hint', style: { marginTop: '10px' } }, 'Waiting for you to finish in the browser…'))
        : spinner('Requesting a sign-in code…')));
  }

  if (state.accounts.length === 0) {
    page.appendChild(emptyState('user', 'No accounts yet', 'Sign in with Microsoft to play online. An offline profile is enough for singleplayer.'));
    return page;
  }
  const active = state.accounts.find((a) => a.active) ?? state.accounts[0];
  page.appendChild(h('div', { class: 'acct-grid' },
    h('div', {}, h('div', { class: 'list' }, state.accounts.map(accountRow))),
    skinPanel(active)));
  page.appendChild(librarySection(active));
  return page;
}
