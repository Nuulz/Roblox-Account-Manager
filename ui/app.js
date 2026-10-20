const { invoke } = window.__TAURI__.core;

// En Linux Roblox corre en Sober: se ocultan las opciones que solo existen en Windows.
invoke('app_platform').then((os) => {
  if (os === 'windows') return;
  document.body.classList.add('linux');
  document.querySelector('input[name="mode"][value="password"]').checked = true;
});
const { listen } = window.__TAURI__.event;
const appWin = window.__TAURI__.window.getCurrentWindow();
const $ = (id) => document.getElementById(id);
const dialog = window.__TAURI__.dialog;

function ask(title, text, { password = false, value = '' } = {}) {
  return new Promise((resolve) => {
    $('prompt-title').textContent = title;
    $('prompt-text').textContent = text;
    const inp = $('prompt-input');
    inp.type = password ? 'password' : 'text';
    inp.value = value;
    open('dlg-prompt');
    inp.focus();
    const done = (v) => { close('dlg-prompt'); $('prompt-ok').onclick = $('prompt-cancel').onclick = inp.onkeydown = null; resolve(v); };
    $('prompt-ok').onclick = () => done(inp.value);
    $('prompt-cancel').onclick = () => done(null);
    inp.onkeydown = (e) => { if (e.key === 'Enter') done(inp.value); if (e.key === 'Escape') done(null); };
  });
}

const confirmBox = (msg) => dialog.ask(msg, { title: 'Roblox Account Manager', kind: 'warning', okLabel: T('Sí'), cancelLabel: T('No') });

let accounts = [];
let settings = null;
const selected = new Set();
const collapsed = new Set();
let lastClicked = null;

function esc(s) {
  return String(s ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
}

function toast(kind, message) {
  const el = document.createElement('div');
  el.className = `toast ${kind}`;
  el.textContent = message;
  $('toasts').appendChild(el);
  setTimeout(() => el.remove(), 5000);
}

const open = (id) => $(id).classList.remove('hidden');
const close = (id) => $(id).classList.add('hidden');
// Ejecuta fn al pulsar Enter. Ojo: un "onkeydown = (e) => cond && fn()" devuelve false
// con el resto de teclas y eso cancela la escritura en el campo.
const onEnter = (el, fn) => el.addEventListener('keydown', (e) => { if (e.key === 'Enter') fn(); });
document.addEventListener('click', (e) => { const c = e.target.dataset?.close; if (c) close(c); });
document.addEventListener('keydown', (e) => {
  if (e.key === 'Escape') document.querySelectorAll('.overlay:not(#vault)').forEach((o) => o.classList.add('hidden'));
});

// ───────────────────────── tema

function applyTheme(t) {
  document.documentElement.dataset.theme = t;
  try { localStorage.setItem('theme', t); } catch {}
  appWin.setTheme(t).catch(() => {});
}
$('btn-theme').onclick = () => applyTheme(document.documentElement.dataset.theme === 'dark' ? 'light' : 'dark');
applyTheme(document.documentElement.dataset.theme || 'dark');

// ───────────────────────── bóveda

function showVault(info) {
  ['vault-locked', 'vault-legacy', 'vault-setup'].forEach((id) => $(id).classList.add('hidden'));
  $('vault-err').textContent = '';
  if (info.status === 'ready') {
    close('vault');
    loadMain();
    return;
  }
  open('vault');
  const panel = { locked: 'vault-locked', legacyLocked: 'vault-legacy', setup: 'vault-setup' }[info.status];
  $(panel).classList.remove('hidden');
  if (info.status === 'setup' && info.migrated > 0) {
    $('setup-msg').textContent = T('Se importaron {n} cuentas. Elige cómo protegerlas.', { n: info.migrated });
  }
  $(panel).querySelector('input')?.focus();
}

async function vaultCall(cmd, args) {
  try { showVault(await invoke(cmd, args)); }
  catch (e) { $('vault-err').textContent = e; }
}

$('vault-unlock').onclick = () => vaultCall('vault_unlock', { password: $('vault-pass').value });
$('legacy-unlock').onclick = () => vaultCall('vault_unlock_legacy', { password: $('legacy-pass').value });
$('setup-ok').onclick = () => {
  const mode = document.querySelector('input[name=mode]:checked').value;
  vaultCall('vault_configure', { mode, password: mode === 'password' ? $('setup-pass').value : null });
};
document.querySelectorAll('input[name=mode]').forEach((r) =>
  r.addEventListener('change', () => $('setup-pass').classList.toggle('hidden', r.value !== 'password' || !r.checked)));
onEnter($('vault-pass'), () => $('vault-unlock').click());
onEnter($('legacy-pass'), () => $('legacy-unlock').click());

// ───────────────────────── ajustes

async function saveSettings(mutate) {
  mutate(settings);
  try { await invoke('settings_set', { s: settings }); }
  catch (e) { toast('error', e); }
  showMulti();
}

// ───────────────────────── lista

function dotClass(a) {
  if (a.needsLogin) return 'login';
  return { 1: 'online', 2: 'game', 3: 'studio' }[a.presenceType] || '';
}

function dotTitle(a) {
  if (a.needsLogin) return T('Requiere iniciar sesión otra vez');
  return { 0: T('Desconectado'), 1: T('En línea'), 2: `${T('En juego')}${a.presenceLocation ? `: ${a.presenceLocation}` : ''}`, 3: T('En Studio') }[a.presenceType] || '';
}

function timer(a) {
  if (a.presenceType !== 2 || !a.sessionStart) return '';
  const m = Math.max(0, Math.floor((Date.now() - new Date(a.sessionStart)) / 60000));
  return `<span class="timer">${m >= 60 ? `${Math.floor(m / 60)}h ${m % 60}m` : `${m}m`}</span>`;
}

function visible() {
  const q = $('search').value.trim().toLowerCase();
  if (!q) return accounts;
  return accounts.filter((a) => [a.username, a.alias, a.description, a.group].some((s) => s.toLowerCase().includes(q)));
}

function groups(list) {
  const map = new Map();
  for (const a of list) {
    const g = a.group || 'Default';
    if (!map.has(g)) map.set(g, []);
    map.get(g).push(a);
  }
  return [...map.entries()].sort(([x], [y]) => (x === 'Default' ? -1 : y === 'Default' ? 1 : x.localeCompare(y)));
}

function orderedIds() {
  return groups(visible()).flatMap(([g, l]) => (collapsed.has(g) ? [] : l.map((a) => a.userId)));
}

function render() {
  const hide = settings?.general.hideUsernames;
  $('empty').classList.toggle('hidden', accounts.length > 0);
  $('rows').innerHTML = groups(visible()).map(([g, list]) => `
    <tr class="grp" data-group="${esc(g)}"><td colspan="3"><div class="gline">${esc(g)} <span class="n">(${list.length})</span><span class="caret">${collapsed.has(g) ? '▼' : '▲'}</span></div></td></tr>
    ${collapsed.has(g) ? '' : list.map((a) => `
    <tr class="acc ${selected.has(a.userId) ? 'sel' : ''}" data-id="${a.userId}">
      <td title="${esc(dotTitle(a))}"><span class="cell"><span class="dot ${dotClass(a)}"></span>${a.avatarUrl ? `<img class="av" src="${esc(a.avatarUrl)}" alt="">` : '<span class="av"></span>'}<span class="nm">${hide ? '••••••••' : esc(a.username)}</span>${timer(a)}${a.antiAfk ? '<span class="afk">AFK</span>' : ''}${a.autoRejoin ? '<span class="afk">RJ</span>' : ''}${noticeTag(a)}</span></td>
      <td>${esc(a.alias)}</td>
      <td title="${esc(a.description)}">${esc(a.description)}</td>
    </tr>`).join('')}`).join('');
}

function selectedAccounts() {
  return accounts.filter((a) => selected.has(a.userId));
}

function fillFields() {
  const sel = selectedAccounts();
  const a = sel.length === 1 ? sel[0] : null;
  $('alias').value = a?.alias ?? '';
  $('desc').value = a?.description ?? '';
  showAccount(a);
  if (a?.savedPlaceId) {
    $('place-id').value = a.savedPlaceId;
    $('job-id').value = a.savedJobId;
    lookupGame();
  }
}

$('rows').addEventListener('click', (e) => {
  const grp = e.target.closest('tr.grp');
  if (grp) {
    const g = grp.dataset.group;
    collapsed.has(g) ? collapsed.delete(g) : collapsed.add(g);
    render();
    return;
  }
  const tr = e.target.closest('tr.acc');
  if (!tr) return;
  const id = Number(tr.dataset.id);
  const ids = orderedIds();
  if (e.shiftKey && lastClicked != null) {
    const [i, j] = [ids.indexOf(lastClicked), ids.indexOf(id)].sort((x, y) => x - y);
    if (!e.ctrlKey) selected.clear();
    ids.slice(i, j + 1).forEach((x) => selected.add(x));
  } else if (e.ctrlKey) {
    selected.has(id) ? selected.delete(id) : selected.add(id);
    lastClicked = id;
  } else {
    selected.clear();
    selected.add(id);
    lastClicked = id;
  }
  render();
  fillFields();
});
$('rows').addEventListener('dblclick', (e) => { if (e.target.closest('tr.acc')) join(); });
document.addEventListener('keydown', (e) => {
  if (e.target.matches('input, textarea')) return;
  if (e.ctrlKey && e.key.toLowerCase() === 'a') { e.preventDefault(); orderedIds().forEach((id) => selected.add(id)); render(); }
  if (e.key === 'Delete') removeSelected();
});
$('search').oninput = render;
$('hide-names').onchange = (e) => saveSettings((s) => { s.general.hideUsernames = e.target.checked; }).then(render);
setInterval(render, 30000);

// ───────────────────────── menú contextual

$('rows').addEventListener('contextmenu', (e) => {
  const tr = e.target.closest('tr.acc');
  if (!tr) return;
  e.preventDefault();
  const id = Number(tr.dataset.id);
  if (!selected.has(id)) { selected.clear(); selected.add(id); render(); fillFields(); }
  const m = $('ctx');
  m.classList.remove('hidden');
  m.style.left = `${Math.min(e.clientX, innerWidth - 200)}px`;
  m.style.top = `${Math.min(e.clientY, innerHeight - 150)}px`;
});
document.addEventListener('click', () => close('ctx'));
$('ctx').addEventListener('click', (e) => {
  const act = e.target.dataset.act;
  if (act === 'join') join();
  else if (act === 'browser') {
    const sel = selectedAccounts();
    if (sel.length > 3) toast('info', T('Se abren solo las 3 primeras cuentas seleccionadas'));
    sel.slice(0, 3).forEach((a) => invoke('browser_open', { userId: a.userId }).catch((err) => toast('error', err)));
  }
  else if (act === 'relogin') relogin();
  else if (act === 'copy-user') copy(selectedAccounts().map((a) => a.username));
  else if (act?.startsWith('copy-')) copySecret(act.slice(5));
  else if (act === 'group') openGroup();
  else if (act === 'util') openUtil();
  else if (act === 'afk') toggleAfk();
  else if (act === 'rejoin') toggleRejoin();
  else if (act === 'remove') removeSelected();
});

async function copy(lines) {
  await navigator.clipboard.writeText(lines.join('\n'));
  toast('success', lines.length === 1 ? T('Copiado') : T('{n} líneas copiadas', { n: lines.length }));
}

async function copySecret(kind) {
  try {
    const out = [];
    for (const a of selectedAccounts()) out.push(await invoke('account_secret', { userId: a.userId, kind }));
    if (kind === 'password' && out.every((p) => !p)) return toast('error', T('No hay contraseña guardada'));
    copy(out);
  } catch (e) { toast('error', e); }
}

// ───────────────────────── acciones de cuenta

async function update(view) {
  try { await invoke('account_update', { view }); }
  catch (e) { toast('error', e); }
}

function needOne() {
  const sel = selectedAccounts();
  if (sel.length !== 1) { toast('error', T('Selecciona una cuenta')); return null; }
  return sel[0];
}

$('btn-alias').onclick = () => { const a = needOne(); if (a) update({ userId: a.userId, alias: $('alias').value.trim() }); };
onEnter($('alias'), () => $('btn-alias').click());
$('btn-desc').onclick = () => { const a = needOne(); if (a) update({ userId: a.userId, description: $('desc').value }).then(() => toast('success', T('Descripción guardada'))); };

async function toggleAfk() {
  const sel = selectedAccounts();
  if (!sel.length) return toast('error', T('Selecciona al menos una cuenta'));
  const on = !sel.every((a) => a.antiAfk);
  for (const a of sel) await update({ userId: a.userId, antiAfk: on });
  toast('success', T(on ? 'Anti-AFK activado en {n} cuenta(s)' : 'Anti-AFK desactivado en {n} cuenta(s)', { n: sel.length }));
}
$('btn-afk').onclick = toggleAfk;

async function toggleRejoin() {
  const sel = selectedAccounts();
  if (!sel.length) return toast('error', T('Selecciona al menos una cuenta'));
  const on = !sel.every((a) => a.autoRejoin);
  for (const a of sel) await update({ userId: a.userId, autoRejoin: on });
  toast('success', T(on ? 'Auto-rejoin activado en {n} cuenta(s)' : 'Auto-rejoin desactivado en {n} cuenta(s)', { n: sel.length }));
}
$('btn-rejoin').onclick = toggleRejoin;

function openGroup() {
  if (!selected.size) return toast('error', T('Selecciona al menos una cuenta'));
  $('group-list').innerHTML = [...new Set(accounts.map((a) => a.group))].map((g) => `<option value="${esc(g)}">`).join('');
  $('group-name').value = '';
  open('dlg-group');
  $('group-name').focus();
}

$('group-ok').onclick = async () => {
  const group = $('group-name').value.trim() || 'Default';
  for (const a of selectedAccounts()) await update({ userId: a.userId, group });
  close('dlg-group');
};
onEnter($('group-name'), () => $('group-ok').click());

async function removeSelected() {
  const sel = selectedAccounts();
  if (!sel.length) return;
  if (!await confirmBox(sel.length === 1 ? T('¿Eliminar {name}?', { name: sel[0].username }) : T('¿Eliminar {n} cuentas?', { n: sel.length }))) return;
  for (const a of sel) await invoke('account_remove', { userId: a.userId });
  selected.clear();
  fillFields();
}
$('btn-remove').onclick = removeSelected;

$('btn-save-game').onclick = async () => {
  const sel = selectedAccounts();
  if (!sel.length) return toast('error', T('Selecciona al menos una cuenta'));
  for (const a of sel) {
    await update({ userId: a.userId, fields: { ...a.fields, SavedPlaceId: $('place-id').value.trim(), SavedJobId: $('job-id').value.trim() } });
  }
  toast('success', T('Place/Job guardado en la cuenta'));
};

// ───────────────────────── agregar

function closeAdd() { close('dlg-add'); $('add-cookie').value = ''; $('add-pass').value = ''; $('add-err').textContent = ''; }
$('btn-add').onclick = (e) => {
  e.stopPropagation();
  const r = e.target.getBoundingClientRect();
  const m = $('add-menu');
  m.classList.remove('hidden');
  m.style.left = `${r.left}px`;
  m.style.top = `${r.top - m.offsetHeight - 2}px`;
};
document.addEventListener('click', () => close('add-menu'));
$('add-menu').addEventListener('click', (e) => {
  const what = e.target.dataset.add;
  if (what === 'login') invoke('browser_login', { userId: null }).then(() => toast('info', T('Inicia sesión en la ventana nueva; la cuenta se agregará sola.'))).catch((err) => toast('error', err));
  if (what === 'cookie') { open('dlg-add'); $('add-cookie').focus(); }
  if (what === 'import') importFile();
  if (what === 'export') exportFile();
});

async function importFile() {
  const path = await dialog.open({ title: T('Importar cuentas'), filters: [{ name: T('Cuentas'), extensions: ['json', 'dat'] }] });
  if (!path) return;
  let password = null;
  for (;;) {
    try {
      const n = await invoke('storage_import', { path, password });
      return toast('success', T('{n} cuentas importadas', { n }));
    } catch (e) {
      if (!String(e).toLowerCase().includes('contraseña')) return toast('error', e);
      password = await ask(T('Importar cuentas'), String(e), { password: true });
      if (password === null) return;
    }
  }
}

async function exportFile() {
  if (!await confirmBox(T('El archivo exportado NO va cifrado: incluye cookies y contraseñas en texto plano. ¿Continuar?'))) return;
  const path = await dialog.save({ title: T('Exportar cuentas'), defaultPath: T('cuentas.json'), filters: [{ name: 'JSON', extensions: ['json'] }] });
  if (!path) return;
  try { toast('success', T('{n} cuentas exportadas', { n: await invoke('storage_export', { path }) })); }
  catch (e) { toast('error', e); }
}
$('add-cancel').onclick = closeAdd;
$('add-ok').onclick = async () => {
  $('add-ok').disabled = true;
  try {
    const v = await invoke('account_add', { cookie: $('add-cookie').value, password: $('add-pass').value });
    toast('success', T('Cuenta {name} agregada', { name: v.username }));
    closeAdd();
  } catch (e) { $('add-err').textContent = e; }
  finally { $('add-ok').disabled = false; }
};

// ───────────────────────── unirse

function launch(ids, args) {
  if (ids.length === 1) invoke('join_server', { userId: ids[0], ...args }).catch((e) => toast('error', e));
  else invoke('launch_many', { ids, ...args });
}

function join() {
  const ids = orderedIds().filter((id) => selected.has(id));
  if (!ids.length) return toast('error', T('Selecciona al menos una cuenta'));
  const placeId = Number($('place-id').value.trim());
  if (!placeId) return toast('error', T('Escribe un Place ID'));
  launch(ids, { placeId, jobId: $('job-id').value.trim(), followUser: false, joinVip: false });
}
$('btn-join').onclick = join;

$('btn-follow').onclick = async () => {
  const ids = orderedIds().filter((id) => selected.has(id));
  if (!ids.length) return toast('error', T('Selecciona al menos una cuenta'));
  const name = $('follow-user').value.trim();
  if (!name) return toast('error', T('Escribe el usuario a seguir'));
  try {
    const userId = await invoke('user_id_by_name', { username: name });
    launch(ids, { placeId: userId, jobId: '', followUser: true, joinVip: false });
  } catch (e) { toast('error', T('No se encontró {name}: {err}', { name, err: e })); }
};
onEnter($('follow-user'), () => $('btn-follow').click());
$('btn-cancel').onclick = () => invoke('launch_cancel');

$('btn-shuffle').onclick = () => saveSettings((s) => { s.general.shuffleJobId = !s.general.shuffleJobId; }).then(syncToggles);

let gameTimer = null;
function lookupGame() {
  clearTimeout(gameTimer);
  const id = Number($('place-id').value.trim());
  if (!id) { $('game-name').innerHTML = '&nbsp;'; return; }
  gameTimer = setTimeout(async () => {
    const name = await invoke('place_name', { placeId: id }).catch(() => null);
    if (Number($('place-id').value.trim()) === id) {
      $('game-name').textContent = name || '';
      $('game-name').title = name || '';
    }
  }, 400);
}
$('place-id').oninput = lookupGame;

// Recuerda el último Place ID / Job ID / usuario a seguir entre sesiones (igual que la v3).
const launcherFields = { 'place-id': 'savedPlaceId', 'job-id': 'savedJobId', 'follow-user': 'savedFollowUser' };
function restoreLauncherFields() {
  for (const [id, key] of Object.entries(launcherFields)) if (!$(id).value) $(id).value = settings.general[key] || '';
  if ($('place-id').value) lookupGame();
}
let launcherSaveTimer;
for (const [id, key] of Object.entries(launcherFields)) {
  $(id).addEventListener('change', () => {
    clearTimeout(launcherSaveTimer);
    launcherSaveTimer = setTimeout(() => saveSettings((s) => { s.general[key] = $(id).value.trim(); }), 300);
  });
}

// ───────────────────────── multi roblox

async function showMulti() {
  const want = settings?.general.enableMultiRoblox;
  const active = await invoke('multi_roblox_status');
  $('multi-chk').checked = !!want;
  const el = $('multi');
  el.textContent = !want ? '' : active ? T('(activo)') : T('(bloqueado)');
  el.className = `multi-state ${!want ? '' : active ? 'on' : 'bad'}`;
  el.title = want && !active ? T('Roblox u otro programa ya lo tiene. Cierra Roblox y haz clic para reintentar.') : '';
}
$('multi-chk').onchange = (e) => saveSettings((s) => { s.general.enableMultiRoblox = e.target.checked; });
// Si Roblox ya está abierto, multi Roblox no se puede activar: ofrece cerrarlo y reintentar.
let fixingMulti = false;
async function fixMulti() {
  if (fixingMulti) return;
  fixingMulti = true;
  try { await fixMultiInner(); } finally { fixingMulti = false; }
}
async function fixMultiInner() {
  try { await invoke('multi_roblox_apply'); toast('success', T('Multi Roblox activado')); }
  catch {
    const n = (await invoke('processes_list')).length;
    if (n && await confirmBox(T('Hay {n} Roblox abierto(s) y no dejan activar multi Roblox. ¿Cerrarlos y activarlo?', { n }))) {
      try { await invoke('multi_roblox_force'); toast('success', T('Multi Roblox activado')); }
      catch (err) { toast('error', err); }
    } else if (!n) toast('error', T('Otro programa tiene el bloqueo de Roblox. Ciérralo y vuelve a intentarlo.'));
  }
  showMulti();
}
$('multi').onclick = (e) => {
  e.preventDefault();
  if ($('multi').classList.contains('bad')) fixMulti();
};

function syncToggles() {
  $('hide-names').checked = !!settings.general.hideUsernames;
  $('btn-shuffle').classList.toggle('on', !!settings.general.shuffleJobId);
  render();
}

// ───────────────────────── servidores

let srvCursor = null;
async function loadServers(more) {
  const placeId = Number($('srv-place').value.trim());
  if (!placeId) return;
  if (!more) { $('srv-rows').innerHTML = ''; srvCursor = null; }
  $('srv-msg').textContent = T('Cargando…');
  try {
    const page = await invoke('servers_list', { placeId, cursor: srvCursor ?? '' });
    srvCursor = page.nextCursor;
    $('srv-more').classList.toggle('hidden', !srvCursor);
    $('srv-rows').insertAdjacentHTML('beforeend', page.data.map((s) => {
      const mine = accounts.filter((a) => a.presenceType === 2 && a.gameId === s.id);
      const pct = s.maxPlayers ? Math.round((s.playing / s.maxPlayers) * 100) : 0;
      return `<tr class="${mine.length ? 'mine' : ''}" title="Job ID: ${esc(s.id)}">
        <td><div class="bar"><i style="width:${pct}%"></i><span>${s.playing}/${s.maxPlayers}</span></div></td>
        <td>${s.ping > 0 ? s.ping : '—'}</td><td>${Math.round(s.fps)}</td>
        <td class="region" data-region="${esc(s.id)}"><button class="lnk" data-reg="${esc(s.id)}">${T('ver')}</button></td>
        <td class="mine-cell">${mine.map((a) => `${a.avatarUrl ? `<img class="av" src="${esc(a.avatarUrl)}" alt="">` : ''}${esc(a.alias || a.username)}`).join(', ')}</td>
        <td><button data-job="${esc(s.id)}">${T('Usar')}</button></td></tr>`;
    }).join(''));
    $('srv-msg').textContent = T('{n} servidores', { n: $('srv-rows').children.length });
  } catch (e) { $('srv-msg').textContent = e; }
}
$('btn-servers').onclick = () => { open('dlg-servers'); $('srv-place').value ||= $('place-id').value; $('srv-place').focus(); };
$('srv-load').onclick = () => loadServers(false);
$('srv-more').onclick = () => loadServers(true);
onEnter($('srv-place'), () => loadServers(false));
async function loadRegion(jobId) {
  const cell = document.querySelector(`[data-region="${CSS.escape(jobId)}"]`);
  if (!cell || cell.dataset.done) return;
  cell.dataset.done = '1';
  cell.textContent = '…';
  try { cell.textContent = (await invoke('server_region', { placeId: Number($('srv-place').value.trim()), jobId })).label; }
  catch (e) { cell.textContent = '—'; cell.title = String(e); }
}
$('srv-regions').onclick = async () => {
  const cells = [...document.querySelectorAll('#srv-rows [data-region]')].filter((c) => !c.dataset.done).slice(0, 25);
  for (const c of cells) { await loadRegion(c.dataset.region); await new Promise((r) => setTimeout(r, 350)); }
};
$('srv-rows').addEventListener('click', (e) => {
  if (e.target.dataset.reg) return loadRegion(e.target.dataset.reg);
  const job = e.target.dataset.job;
  if (!job) return;
  $('place-id').value = $('srv-place').value.trim();
  $('job-id').value = job;
  lookupGame();
  close('dlg-servers');
});

// ───────────────────────── juegos

let games = { recent: [], favorites: [] };
let gamesTab = 'recent';
function renderGames() {
  const list = games[gamesTab];
  const favIds = new Set(games.favorites.map((g) => g.placeId));
  $('games-clear').classList.toggle('hidden', gamesTab !== 'recent' || !list.length);
  $('games-list').innerHTML = list.length ? list.map((g) => `
    <div class="game" data-place="${g.placeId}" data-ps="${esc(g.privateServer)}" title="${esc(g.name)}">
      <img src="${esc(g.icon)}" alt="">
      <button class="star" data-fav="${g.placeId}" title="${T('Favorito')}">${favIds.has(g.placeId) ? '★' : '☆'}</button>
      <div class="t">${esc(g.name)}</div>
    </div>`).join('') : `<p class="muted" style="padding:8px">${gamesTab === 'recent' ? T('Aún no hay juegos recientes.') : T('Marca juegos con ☆ en Recientes.')}</p>`;
}
$('btn-games').onclick = async () => { games = await invoke('games_list'); renderGames(); open('dlg-games'); };
document.querySelectorAll('.tab:not(.utab)').forEach((t) => t.onclick = () => {
  document.querySelectorAll('.tab:not(.utab)').forEach((x) => x.classList.toggle('on', x === t));
  gamesTab = t.dataset.tab;
  renderGames();
});
$('games-clear').onclick = () => invoke('game_clear_recent');
$('games-list').addEventListener('click', async (e) => {
  const fav = e.target.dataset.fav;
  if (fav) {
    const id = Number(fav);
    if (games.favorites.some((g) => g.placeId === id)) await invoke('game_remove_favorite', { placeId: id });
    else await invoke('game_add_favorite', { entry: games.recent.find((g) => g.placeId === id) });
    return;
  }
  const card = e.target.closest('.game');
  if (!card) return;
  $('place-id').value = card.dataset.place;
  $('job-id').value = card.dataset.ps;
  lookupGame();
  close('dlg-games');
});
listen('games', (e) => { games = e.payload; renderGames(); });

// ───────────────────────── utilidades de cuenta

let utilAcc = null;
let avatarFor = null;
const hm = (secs) => { const m = Math.round(secs / 60); return m >= 60 ? `${Math.floor(m / 60)}h ${m % 60}m` : `${m}m`; };

function showUtab(name) {
  document.querySelectorAll('.utab').forEach((t) => t.classList.toggle('on', t.dataset.utab === name));
  document.querySelectorAll('.upane').forEach((p) => p.classList.toggle('hidden', p.id !== `utab-${name}`));
  if (name === 'avatar') loadAvatar();
}
document.querySelectorAll('.utab').forEach((t) => t.onclick = () => showUtab(t.dataset.utab));

function openUtil() {
  const a = needOne();
  if (!a) return;
  utilAcc = a;
  avatarFor = null;
  window.avatar3d?.dispose();
  $('util-title').textContent = `${T('Estadísticas y avatar')}: ${a.alias || a.username}`;
  open('dlg-util');
  loadStats();
  showUtab('stats');
}
$('btn-util').onclick = openUtil;
new MutationObserver(() => { if ($('dlg-util').classList.contains('hidden')) { window.avatar3d?.dispose(); avatarFor = null; } })
  .observe($('dlg-util'), { attributes: true, attributeFilter: ['class'] });

async function loadStats() {
  const s = await invoke('stats_get', { userId: utilAcc.userId });
  $('st-total').textContent = hm(s.total);
  $('st-week').textContent = hm(s.week);
  $('st-sessions').textContent = s.sessions;
  const max = Math.max(60, ...s.days.map(([, v]) => v));
  $('st-chart').innerHTML = s.days.map(([d, v]) => `<i style="height:${Math.max(v ? 3 : 0, (v / max) * 100)}%" title="${d}: ${hm(v)}"></i>`).join('');
  $('st-games').innerHTML = s.games.length ? s.games.map((g) => `<tr><td>${esc(g.name || g.placeId)}</td><td>${hm(g.seconds)}</td><td class="muted">${g.lastPlayed ? new Date(g.lastPlayed).toLocaleDateString() : ''}</td></tr>`).join('')
    : `<tr><td colspan="3" class="muted">${T('Todavía no hay datos. Se registran mientras la app está abierta y la cuenta está en juego.')}</td></tr>`;
}

async function loadAvatar() {
  if (avatarFor === utilAcc.userId) return;
  avatarFor = utilAcc.userId;
  window.avatar3d?.dispose();
  $('avatar-msg').textContent = T('Generando modelo 3D…');
  try {
    const data = await invoke('account_util', { userId: utilAcc.userId, action: 'avatar3d', args: {} });
    if (avatarFor !== utilAcc.userId) return;
    $('avatar-msg').textContent = T('Arrastra para girar · rueda para acercar');
    window.avatar3d.render($('avatar-host'), data);
  } catch (e) { $('avatar-msg').textContent = String(e); avatarFor = null; }
}

async function util(action, args = {}, userId = accTarget?.userId) {
  if (!userId) throw T('Selecciona una cuenta');
  return invoke('account_util', { userId, action, args });
}
async function utilDo(btn, action, args, ok) {
  if (!accTarget) return toast('error', T('Selecciona una cuenta'));
  btn.disabled = true;
  try { const r = await util(action, args); toast('success', ok ?? (typeof r === 'string' ? T(r) : T('Hecho'))); return r; }
  catch (e) { toast('error', e); }
  finally { btn.disabled = false; }
}

$('pw-go').onclick = (e) => {
  if (!$('pw-cur').value || !$('pw-new').value) return toast('error', T('Escribe la contraseña actual y la nueva'));
  utilDo(e.target, 'password', { current: $('pw-cur').value, new: $('pw-new').value });
};
$('dn-go').onclick = (e) => utilDo(e.target, 'displayName', { name: $('dn-name').value.trim() });
$('pin-go').onclick = (e) => utilDo(e.target, 'unlockPin', { pin: $('pin-code').value.trim() }, T('PIN desbloqueado')).then(() => loadAccInfo(true));
$('ql-go').onclick = (e) => utilDo(e.target, 'quickLogin', { code: $('ql-code').value.trim() }, T('Inicio de sesión aprobado'));
$('so-go').onclick = async (e) => {
  if (await confirmBox(T('¿Cerrar la sesión de esta cuenta en todos los demás dispositivos?'))) utilDo(e.target, 'signoutOthers');
};
$('bl-load').onclick = async () => {
  $('bl-msg').textContent = T('Cargando…');
  try {
    const v = await util('blocked');
    const list = v.blockedUsers ?? (v.data?.blockedUserIds ?? []).map((id) => ({ userId: id }));
    $('bl-msg').textContent = T('{n} bloqueado(s)', { n: list.length });
    $('bl-list').innerHTML = list.map((u) => `<div class="r-row bl"><span class="grow">${esc(u.displayName || u.name || '')} <span class="muted">${esc(u.name ? '@' + u.name : u.userId)}</span></span><button data-unblock="${u.userId}">${T('Desbloquear')}</button></div>`).join('');
  } catch (e) { $('bl-msg').textContent = String(e); }
};
$('bl-list').addEventListener('click', async (e) => {
  const id = Number(e.target.dataset.unblock);
  if (id && await utilDo(e.target, 'unblock', { userId: id }, T('Desbloqueado')) !== undefined) e.target.closest('.bl').remove();
});

// ───────────────────────── procesos

let procsTimer = null;
function duration(secs) {
  const m = Math.floor(secs / 60);
  return m >= 60 ? `${Math.floor(m / 60)}h ${m % 60}m` : `${m}m`;
}
async function loadProcs() {
  if ($('dlg-procs').classList.contains('hidden')) { clearInterval(procsTimer); return; }
  const list = await invoke('processes_list');
  const now = Date.now() / 1000;
  const total = list.reduce((n, p) => n + p.memoryMb, 0);
  $('procs-sum').textContent = T('{n} abierto(s) · {mb} MB en total', { n: list.length, mb: total });
  $('procs-rows').innerHTML = list.length ? list.map((p) => {
    const a = accounts.find((x) => x.userId && p.trackerId && accountsTracker[x.userId] === p.trackerId);
    const name = a ? `${a.avatarUrl ? `<img class="av" src="${esc(a.avatarUrl)}" alt="">` : ''}${esc(a.alias || a.username)}` : `<span class="muted">${esc(p.title || T('Desconocida'))}</span>`;
    return `<tr><td><span class="cell">${name}</span></td><td>${p.pid}</td><td>${p.memoryMb} MB</td><td>${duration(now - p.startTime)}</td>
      <td>${p.hwnd ? `<button data-pmin="${p.hwnd}">${T('Minimizar')}</button> <button data-pclose="${p.hwnd}">${T('Cerrar')}</button> ` : ''}<button data-pkill="${p.pid}">${T('Matar')}</button></td></tr>`;
  }).join('') : `<tr><td colspan="5" class="muted">${T('No hay ningún Roblox abierto.')}</td></tr>`;
}
let accountsTracker = {};
$('btn-procs').onclick = async () => {
  accountsTracker = await invoke('accounts_trackers');
  open('dlg-procs');
  loadProcs();
  clearInterval(procsTimer);
  procsTimer = setInterval(loadProcs, 3000);
};
$('procs-rows').addEventListener('click', async (e) => {
  const d = e.target.dataset;
  if (d.pmin) await invoke('window_minimize', { hwnd: Number(d.pmin) });
  if (d.pclose) await invoke('window_close', { hwnd: Number(d.pclose) });
  if (d.pkill && await confirmBox(T('¿Forzar el cierre de este Roblox?'))) await invoke('process_kill', { pid: Number(d.pkill) });
  setTimeout(loadProcs, 600);
});
$('procs-min-all').onclick = async () => {
  for (const p of await invoke('processes_list')) if (p.hwnd) await invoke('window_minimize', { hwnd: p.hwnd });
};
$('procs-close-all').onclick = async () => {
  if (!await confirmBox(T('¿Cerrar todas las ventanas de Roblox?'))) return;
  for (const p of await invoke('processes_list')) if (p.hwnd) await invoke('window_close', { hwnd: p.hwnd });
  setTimeout(loadProcs, 1000);
};

// ───────────────────────── diálogo de ajustes

function settingSlot(el) {
  if (el.dataset.k) return [settings.general, el.dataset.k];
  if (el.dataset.w) return [settings.watcher, el.dataset.w];
  return [settings.webServer, el.dataset.ws];
}
$('btn-settings').onclick = async () => {
  settings = await invoke('settings_get');
  document.querySelectorAll('#dlg-settings [data-k], #dlg-settings [data-w], #dlg-settings [data-ws]').forEach((el) => {
    const [obj, key] = settingSlot(el);
    const v = obj[key];
    if (el.type === 'checkbox') el.checked = !!v; else el.value = v;
  });
  $('set-err').textContent = '';
  $('app-ver').textContent = `${T('Versión')} ${await invoke('app_version')}`;
  open('dlg-settings');
};
$('set-save').onclick = async () => {
  document.querySelectorAll('#dlg-settings [data-k], #dlg-settings [data-w], #dlg-settings [data-ws]').forEach((el) => {
    const [obj, key] = settingSlot(el);
    obj[key] = el.type === 'checkbox' ? el.checked : el.type === 'number' ? Number(el.value) : el.value;
  });
  const ws = settings.webServer;
  if (ws.enabled && ws.password.length < 6 && (ws.allowGetCookie || ws.allowGetAccounts || ws.allowLaunchAccount)) {
    toast('error', T('El servidor web necesita una contraseña de al menos 6 caracteres para esos permisos'));
  }
  try {
    await invoke('settings_set', { s: settings });
    close('dlg-settings');
  } catch (e) { $('set-err').textContent = e; }
  syncToggles();
  await showMulti();
  if ($('multi').classList.contains('bad')) fixMulti();
};

// ───────────────────────── actualizaciones

let updateUrl = '';
async function checkUpdate(manual) {
  try {
    const u = await invoke('update_check');
    if (u.available) {
      updateUrl = u.url;
      $('update-text').textContent = T('Hay una nueva versión: {latest} (tienes {current})', u);
      open('update-bar');
      close('dlg-settings');
    } else if (manual) toast('info', T('Ya tienes la última versión ({current})', u));
  } catch (e) { if (manual) toast('error', T('No se pudo buscar actualizaciones: {err}', { err: e })); }
}
$('btn-check-update').onclick = () => checkUpdate(true);
$('update-later').onclick = () => close('update-bar');
$('update-go').onclick = async () => {
  $('update-go').disabled = true;
  $('update-text').textContent = T('Descargando actualización…');
  try { await invoke('update_install', { url: updateUrl }); }
  catch (e) { toast('error', e); $('update-go').disabled = false; }
};

// ───────────────────────── eventos del backend

listen('accounts', (e) => { accounts = e.payload; render(); });
listen('toast', (e) => toast(e.payload.kind, T(e.payload.message)));
listen('launch-progress', (e) => {
  const p = e.payload;
  $('progress').classList.toggle('hidden', p.done);
  $('progress-text').textContent = T('Lanzando {current}/{total}: {username}', p);
});
listen('vip-privacy', (e) => toast('info', T('{username}: la privacidad de servidores privados no está en "Todos"', e.payload)));

async function loadMain() {
  for (const r of await invoke('health_last').catch(() => [])) health.set(r.userId, r);
  settings = await invoke('settings_get');
  accounts = await invoke('accounts_list');
  syncToggles();
  showMulti();
  restoreLauncherFields();
  invoke('accounts_refresh_avatars', { ids: accounts.map((a) => a.userId) });
  invoke('accounts_update_presence', { ids: accounts.map((a) => a.userId) });
  if (settings.general.checkForUpdates) checkUpdate(false);
}

invoke('vault_startup').then(showVault).catch((e) => toast('error', e));

// ───────────────────────── pestañas del panel derecho

document.querySelectorAll('[data-rtab]').forEach((b) => b.onclick = () => {
  document.querySelectorAll('[data-rtab]').forEach((x) => x.classList.toggle('on', x === b));
  document.querySelectorAll('.rpane').forEach((p) => p.classList.toggle('hidden', p.id !== `rpane-${b.dataset.rtab}`));
});
document.querySelectorAll('[data-proxy]').forEach((b) => b.onclick = (e) => {
  e.stopPropagation();
  const t = $(b.dataset.proxy);
  const r = b.getBoundingClientRect();
  t.dispatchEvent(new MouseEvent('click', { bubbles: true, clientX: r.left, clientY: r.bottom }));
});

// ───────────────────────── pestaña Cuenta

let accTarget = null;
const infoCache = new Map();
const accTabOpen = () => !$('rpane-account').classList.contains('hidden');

function showAccount(a) {
  const changed = accTarget?.userId !== a?.userId;
  accTarget = a;
  $('acc-none').classList.toggle('hidden', !!a);
  $('acc-body').classList.toggle('hidden', !a);
  if (!a) return;
  $('acc-av').src = a.avatarUrl || '';
  $('acc-name').textContent = a.alias || a.displayName || a.username;
  $('acc-user').textContent = `@${a.username} · ${a.userId}`;
  $('acc-age').textContent = a.ageDays > 0 ? `${a.ageDays} d` : T('hoy');
  $('fr-target').innerHTML = accounts.filter((x) => x.userId !== a.userId)
    .map((x) => `<option value="${x.userId}">${esc(x.alias ? `${x.alias} (${x.username})` : x.username)}</option>`).join('');
  if (!changed) return;
  if (document.activeElement?.id !== 'dn-name') $('dn-name').value = a.displayName || '';
  ['pw-cur', 'pw-new', 'pin-code', 'ql-code', 'em-new', 'em-pass'].forEach((id) => { $(id).value = ''; });
  $('bl-list').innerHTML = $('bl-msg').textContent = $('of-list').innerHTML = $('of-msg').textContent = '';
  if (accTabOpen()) loadAccInfo();
}

function paintInfo(i) {
  let email = T('sin correo');
  try {
    const e = typeof i.email === 'string' ? JSON.parse(i.email) : i.email;
    if (e?.emailAddress) email = T(e.verified ? 'verificado' : 'sin verificar');
  } catch { email = '?'; }
  $('acc-robux').textContent = i.robux ?? '?';
  $('acc-email').textContent = email;
  $('acc-pin').textContent = i.pinEnabled == null ? '?' : T(!i.pinEnabled ? 'no' : i.pinUnlockedUntil > 0 ? 'abierto' : 'activo');
}

async function loadAccInfo(force = false) {
  const a = accTarget;
  if (!a) return;
  const hit = infoCache.get(a.userId);
  if (hit && !force && Date.now() - hit.at < 120000) return paintInfo(hit.info);
  ['acc-robux', 'acc-email', 'acc-pin'].forEach((id) => { $(id).textContent = '…'; });
  try {
    const info = await util('info', {}, a.userId);
    infoCache.set(a.userId, { at: Date.now(), info });
    if (accTarget?.userId === a.userId) paintInfo(info);
  } catch (e) {
    if (accTarget?.userId === a.userId) ['acc-robux', 'acc-email', 'acc-pin'].forEach((id) => { $(id).textContent = '?'; });
    toast('error', e);
  }
}
$('acc-refresh').onclick = () => loadAccInfo(true);
document.querySelector('[data-rtab=account]').addEventListener('click', () => loadAccInfo());

$('of-load').onclick = async (e) => {
  const a = accTarget;
  if (!a) return;
  e.target.disabled = true;
  $('of-msg').textContent = T('Cargando…');
  try {
    const list = await util('outfits', {}, a.userId);
    $('of-msg').textContent = T(list.length ? 'Clic en un traje para ponérselo a las cuentas seleccionadas.' : 'Esta cuenta no tiene trajes guardados.');
    $('of-list').innerHTML = list.map((o) => `<button class="outfit" data-outfit="${o.id}" title="${esc(o.name)}">${o.image ? `<img src="${esc(o.image)}" alt="">` : '<i></i>'}<span>${esc(o.name)}</span></button>`).join('');
  } catch (err) { $('of-msg').textContent = String(err); }
  finally { e.target.disabled = false; }
};
$('of-list').addEventListener('click', async (e) => {
  const b = e.target.closest('[data-outfit]');
  if (!b) return;
  const targets = selectedAccounts();
  if (targets.length > 1 && !await confirmBox(T('¿Poner este traje a {n} cuentas?', { n: targets.length }))) return;
  b.disabled = true;
  let ok = 0;
  for (const t of targets) {
    try { const m = await util('applyOutfit', { outfitId: Number(b.dataset.outfit) }, t.userId); ok++; if (targets.length === 1) toast('success', T(m)); }
    catch (err) { toast('error', `${t.username}: ${err}`); }
  }
  if (targets.length > 1) toast('success', T('Traje aplicado en {ok} de {n} cuentas', { ok, n: targets.length }));
  b.disabled = false;
});

$('em-go').onclick = (e) => {
  const email = $('em-new').value.trim();
  if (!email.includes('@')) return toast('error', T('Escribe un correo válido'));
  utilDo(e.target, 'email', { email, password: $('em-pass').value }).then(() => loadAccInfo(true));
};
$('fp-go').onclick = (e) => utilDo(e.target, 'followPrivacy', { level: Number($('fp-level').value) });
$('fr-go').onclick = (e) => {
  const id = Number($('fr-target').value);
  if (!id) return toast('error', T('No hay otra cuenta a la que enviarla'));
  utilDo(e.target, 'friend', { targetId: id });
};
showAccount(null);

// ───────────────────────── volver a iniciar sesión con datos guardados

async function relogin(target) {
  const sel = target ? [target] : selectedAccounts();
  if (sel.length !== 1) return toast('error', T('Solo se puede rellenar una cuenta a la vez'));
  const a = sel[0];
  if (!a.hasPassword) return toast('error', T('Esta cuenta no tiene contraseña guardada'));
  const warn = [
    T('Se abrirá el login de Roblox con el usuario y la contraseña de {name} ya escritos.', { name: a.username }),
    '',
    T('• Tú tienes que pulsar "Iniciar sesión" y resolver el captcha o la verificación si aparece.'),
    T('• Hazlo solo con tus propias cuentas y no lo repitas muchas veces seguidas: Roblox puede bloquear temporalmente los inicios de sesión o pedir verificación.'),
    T('• Los datos solo se rellenan en www.roblox.com/login.'),
    '',
    T('¿Continuar?'),
  ].join('\n');
  if (!await confirmBox(warn)) return;
  invoke('browser_login', { userId: a.userId })
    .then(() => toast('info', T('Pulsa "Iniciar sesión" en la ventana nueva; la cuenta se actualizará sola.')))
    .catch((err) => toast('error', T(String(err))));
}

// ───────────────────────── perfiles de lanzamiento

let profiles = [];
let peId = 0;
let peOrder = [];

const accName = (a) => (a.alias ? `${a.alias} (${a.username})` : a.username);
const avImg = (a) => (a?.avatarUrl ? `<img class="av" src="${esc(a.avatarUrl)}" alt="">` : '<span class="av"></span>');

async function loadProfiles() {
  profiles = await invoke('profiles_list').catch(() => []);
  renderProfiles();
}

function renderProfiles() {
  const q = $('prof-quick');
  const keep = q.value;
  q.innerHTML = profiles.length
    ? profiles.map((p) => `<option value="${p.id}">${esc(p.name)} · ${p.userIds.length}</option>`).join('')
    : `<option value="">${esc(T('Sin perfiles: créalos en Herramientas'))}</option>`;
  if (profiles.some((p) => String(p.id) === keep)) q.value = keep;
  q.disabled = $('prof-quick-go').disabled = !profiles.length;

  $('prof-list').innerHTML = profiles.length ? profiles.map((p) => {
    const accs = p.userIds.map((id) => accounts.find((a) => a.userId === id));
    const found = accs.filter(Boolean);
    const missing = accs.length - found.length;
    const where = p.jobId ? (p.jobId.includes('privateServerLinkCode') ? T('servidor privado') : T('servidor fijo')) : T('servidor según ajustes');
    const delay = p.delaySecs == null ? T('retraso de Ajustes') : T('{n} s entre cuentas', { n: p.delaySecs });
    return `<div class="prof">
      <div class="avs">${found.slice(0, 5).map(avImg).join('')}${found.length > 5 ? `<span class="more">+${found.length - 5}</span>` : ''}</div>
      <div class="info"><b>${esc(p.name)}</b>
        <span>${esc(T('{n} cuenta(s)', { n: found.length }))}${missing ? ` · <span style="color:var(--bad);display:inline">${esc(T('{n} ya no existe(n)', { n: missing }))}</span>` : ''} · Place ${p.placeId} · ${esc(where)} · ${esc(delay)}</span></div>
      <button class="go" data-plaunch="${p.id}">${esc(T('Lanzar'))}</button><button data-pedit="${p.id}">${esc(T('Editar'))}</button><button data-pdel="${p.id}">✕</button>
    </div>`;
  }).join('') : `<p class="muted" style="padding:8px">${esc(T('Todavía no hay perfiles. Selecciona cuentas en la lista y pulsa "Nuevo perfil".'))}</p>`;
}

async function launchProfile(id) {
  const p = profiles.find((x) => x.id === id);
  if (!p) return;
  const n = p.userIds.filter((uid) => accounts.some((a) => a.userId === uid)).length;
  if (!n) return toast('error', T('Ninguna cuenta del perfil existe ya'));
  if (n > 1 && !await confirmBox(T('¿Lanzar "{name}"? Se abrirán {n} Roblox.', { name: p.name, n }))) return;
  try { await invoke('profile_launch', { id }); toast('info', T('Lanzando perfil {name}', { name: p.name })); }
  catch (e) { toast('error', T(String(e))); }
}

function openProfileEditor(p) {
  peId = p?.id ?? 0;
  $('pe-title').textContent = T(p ? 'Editar perfil' : 'Nuevo perfil');
  $('pe-name').value = p?.name ?? '';
  $('pe-place').value = p?.placeId ?? $('place-id').value.trim();
  $('pe-job').value = p?.jobId ?? $('job-id').value.trim();
  $('pe-delay').value = p?.delaySecs ?? '';
  $('pe-err').textContent = '';
  peOrder = p ? p.userIds.filter((id) => accounts.some((a) => a.userId === id)) : orderedIds().filter((id) => selected.has(id));
  renderPeAccs();
  peGame();
  open('dlg-profile-edit');
  $('pe-name').focus();
}

function renderPeAccs() {
  const rest = accounts.filter((a) => !peOrder.includes(a.userId));
  const row = (a, i) => `<div class="pe-acc ${i < 0 ? 'off' : ''}">
    <input type="checkbox" data-pe="${a.userId}" ${i >= 0 ? 'checked' : ''}>
    <span class="ord">${i >= 0 ? i + 1 : ''}</span>${avImg(a)}<span class="nm">${esc(accName(a))}</span>
    ${i >= 0 ? `<button data-up="${a.userId}" ${i === 0 ? 'disabled' : ''}>▲</button><button data-down="${a.userId}" ${i === peOrder.length - 1 ? 'disabled' : ''}>▼</button>` : ''}
  </div>`;
  $('pe-accs').innerHTML = peOrder.map((id, i) => row(accounts.find((a) => a.userId === id), i)).join('') + rest.map((a) => row(a, -1)).join('');
  $('pe-count').textContent = T('({n} elegidas)', { n: peOrder.length });
}

let peTimer = null;
function peGame() {
  clearTimeout(peTimer);
  const id = Number($('pe-place').value.trim());
  $('pe-game').textContent = '';
  if (!id) return;
  peTimer = setTimeout(async () => {
    const name = await invoke('place_name', { placeId: id }).catch(() => null);
    if (Number($('pe-place').value.trim()) === id) $('pe-game').textContent = name ? T('Juego: {name}', { name }) : T('No se encontró ese juego');
  }, 400);
}
$('pe-place').oninput = peGame;

$('pe-accs').addEventListener('click', (e) => {
  const d = e.target.dataset;
  if (d.pe) {
    const id = Number(d.pe);
    peOrder = e.target.checked ? [...peOrder, id] : peOrder.filter((x) => x !== id);
  } else if (d.up || d.down) {
    const id = Number(d.up || d.down);
    const i = peOrder.indexOf(id);
    const j = d.up ? i - 1 : i + 1;
    [peOrder[i], peOrder[j]] = [peOrder[j], peOrder[i]];
  } else return;
  renderPeAccs();
});
$('pe-all').onclick = () => { peOrder = [...peOrder, ...orderedIds().filter((id) => !peOrder.includes(id))]; renderPeAccs(); };
$('pe-none').onclick = () => { peOrder = []; renderPeAccs(); };

$('pe-save').onclick = async () => {
  const delay = $('pe-delay').value.trim();
  const profile = {
    id: peId,
    name: $('pe-name').value,
    userIds: peOrder,
    placeId: Number($('pe-place').value.trim()) || 0,
    jobId: $('pe-job').value,
    delaySecs: delay === '' ? null : Math.max(0, Math.round(Number(delay))),
  };
  try {
    await invoke('profile_save', { profile });
    close('dlg-profile-edit');
    toast('success', T('Perfil guardado'));
    loadProfiles();
  } catch (e) { $('pe-err').textContent = T(String(e)); }
};

$('btn-profiles').onclick = () => { renderProfiles(); open('dlg-profiles'); };
$('prof-new').onclick = () => openProfileEditor(null);
$('prof-list').addEventListener('click', async (e) => {
  const d = e.target.dataset;
  if (d.plaunch) { close('dlg-profiles'); launchProfile(Number(d.plaunch)); }
  if (d.pedit) openProfileEditor(profiles.find((p) => p.id === Number(d.pedit)));
  if (d.pdel) {
    const p = profiles.find((x) => x.id === Number(d.pdel));
    if (p && await confirmBox(T('¿Eliminar el perfil "{name}"? Las cuentas no se borran.', { name: p.name }))) {
      await invoke('profile_delete', { id: p.id }).catch((err) => toast('error', T(String(err))));
      loadProfiles();
    }
  }
});
$('prof-quick-go').onclick = () => launchProfile(Number($('prof-quick').value));
listen('accounts', () => { if (!$('dlg-profiles').classList.contains('hidden')) renderProfiles(); });
loadProfiles();

// ───────────────────────── salud de cuentas y Robux

const health = new Map();
let healthChecking = null;

async function openHealth() {
  for (const r of await invoke('health_last').catch(() => [])) health.set(r.userId, r);
  renderHealth();
  open('dlg-health');
}

const fmt = (n) => (n == null ? '—' : Number(n).toLocaleString(LANG === 'en' ? 'en-US' : 'es-ES'));
const ago = (iso) => {
  if (!iso) return '—';
  const m = Math.max(0, Math.round((Date.now() - new Date(iso)) / 60000));
  if (m < 1) return T('ahora');
  if (m < 60) return T('hace {n} min', { n: m });
  if (m < 1440) return T('hace {n} h', { n: Math.round(m / 60) });
  return T('hace {n} d', { n: Math.round(m / 1440) });
};

function noticeText(r) {
  if (!r || r.state !== 'ok') return '';
  const parts = [];
  if (r.friendRequests) parts.push(T('{n} solicitud(es)', { n: r.friendRequests }));
  if (r.unreadMessages) parts.push(T('{n} mensaje(s)', { n: r.unreadMessages }));
  if (parts.length) return `<span class="notice" title="${esc(parts.join(' · '))}">${esc(parts.join(' · '))}</span>`;
  return r.friendRequests == null && r.unreadMessages == null ? '' : `<span class="muted">${esc(T('nada'))}</span>`;
}

function noticeTag(a) {
  const r = health.get(a.userId);
  const n = (r?.state === 'ok' ? (r.friendRequests || 0) + (r.unreadMessages || 0) : 0);
  if (!n) return '';
  const title = [r.friendRequests ? T('{n} solicitud(es)', { n: r.friendRequests }) : '', r.unreadMessages ? T('{n} mensaje(s)', { n: r.unreadMessages }) : ''].filter(Boolean).join(' · ');
  return `<span class="afk ntag" title="${esc(title)}">${esc(T(n === 1 ? '{n} aviso' : '{n} avisos', { n }))}</span>`;
}

function healthState(r, a) {
  if (healthChecking?.has(a.userId) && !r?.fresh) return ['checking', T('Revisando…')];
  if (!r) return a.needsLogin ? ['expired', T('Requiere iniciar sesión')] : ['', T('Sin revisar')];
  return {
    ok: ['ok', T('Sesión válida')],
    expired: ['expired', T('Cookie vencida')],
    mismatch: ['mismatch', T('Cookie de otra cuenta')],
    unknown: ['', T('No se pudo comprobar')],
  }[r.state];
}

function renderHealth() {
  const rows = accounts.map((a) => ({ a, r: health.get(a.userId) }));
  const rank = (x) => (x.r?.state === 'expired' || x.r?.state === 'mismatch' || (!x.r && x.a.needsLogin) ? 0 : x.r?.state === 'ok' ? 2 : 1);
  rows.sort((x, y) => rank(x) - rank(y) || (y.r?.robux ?? -1) - (x.r?.robux ?? -1));
  const ok = rows.filter((x) => x.r?.state === 'ok');
  const bad = rows.filter((x) => rank(x) === 0);
  const total = ok.reduce((n, x) => n + (x.r.robux ?? 0), 0);
  const delta = ok.reduce((n, x) => n + (x.r.prevRobux != null && x.r.robux != null ? x.r.robux - x.r.prevRobux : 0), 0);
  $('hl-robux').textContent = ok.length ? fmt(total) : '—';
  $('hl-delta').innerHTML = delta ? `<span class="${delta > 0 ? 'up' : 'down'}">${delta > 0 ? '+' : ''}${fmt(delta)}</span> ${esc(T('desde la revisión anterior'))}` : '';
  $('hl-ok').textContent = ok.length;
  $('hl-bad').textContent = bad.length;
  $('hl-unk').textContent = rows.length - ok.length - bad.length;
  $('hl-rows').innerHTML = rows.map(({ a, r }) => {
    const [cls, label] = healthState(r, a);
    const d = r?.robux != null && r?.prevRobux != null ? r.robux - r.prevRobux : 0;
    const act = cls === 'expired' || cls === 'mismatch'
      ? `<button data-hlogin="${a.userId}">${esc(T('Iniciar sesión'))}</button>`
      : `<button data-hcheck="${a.userId}" ${healthChecking ? 'disabled' : ''}>${esc(T('Revisar'))}</button>`;
    return `<tr class="${cls === 'expired' || cls === 'mismatch' ? 'problem' : ''}">
      <td><span class="cell">${avImg(a)}<span class="nm">${esc(accName(a))}</span></span></td>
      <td title="${esc(r?.state === 'unknown' ? r.detail : r?.state === 'mismatch' ? T('La cookie guardada pertenece a {name}', { name: r.detail }) : '')}"><span class="st ${cls}"><i></i>${esc(label)}</span></td>
      <td class="num">${fmt(r?.robux)}</td>
      <td class="num">${d ? `<span class="${d > 0 ? 'up' : 'down'}">${d > 0 ? '+' : ''}${fmt(d)}</span>` : ''}</td>
      <td>${noticeText(r)}</td>
      <td>${a.ageDays > 0 ? `${a.ageDays} d` : T('hoy')}</td>
      <td class="muted">${esc(ago(r?.checkedAt))}</td>
      <td>${act}</td></tr>`;
  }).join('') || `<tr><td colspan="8" class="muted">${esc(T('No hay cuentas.'))}</td></tr>`;
}

async function runHealth(ids) {
  if (!ids.length) return toast('error', T('Selecciona al menos una cuenta'));
  healthChecking = new Set(ids);
  ids.forEach((id) => { const r = health.get(id); if (r) r.fresh = false; });
  $('hl-cancel').classList.remove('hidden');
  ['hl-all', 'hl-sel'].forEach((id) => { $(id).disabled = true; });
  $('hl-msg').textContent = T('Revisando 0 de {n}…', { n: ids.length });
  renderHealth();
  try { await invoke('health_check', { ids }); }
  catch (e) { toast('error', T(String(e))); endHealth(); }
}
function endHealth() {
  healthChecking = null;
  $('hl-cancel').classList.add('hidden');
  ['hl-all', 'hl-sel'].forEach((id) => { $(id).disabled = false; });
  renderHealth();
}
listen('health-progress', (e) => {
  const p = e.payload;
  if (p.row) { health.set(p.row.userId, { ...p.row, fresh: true }); render(); }
  if (p.done) {
    endHealth();
    const bad = [...health.values()].filter((r) => r.state === 'expired' || r.state === 'mismatch').length;
    $('hl-msg').textContent = bad ? T('Listo: {n} cuenta(s) necesitan iniciar sesión.', { n: bad }) : T('Listo: todas las sesiones revisadas están bien.');
  } else {
    $('hl-msg').textContent = T('Revisando {current} de {total}…', p);
    renderHealth();
  }
});

$('btn-health').onclick = openHealth;
$('hl-all').onclick = () => runHealth(accounts.map((a) => a.userId));
$('hl-sel').onclick = () => runHealth(orderedIds().filter((id) => selected.has(id)));
$('hl-cancel').onclick = () => { invoke('health_cancel'); $('hl-msg').textContent = T('Cancelando…'); };
$('hl-rows').addEventListener('click', (e) => {
  const d = e.target.dataset;
  if (d.hcheck) runHealth([Number(d.hcheck)]);
  if (d.hlogin) {
    const a = accounts.find((x) => x.userId === Number(d.hlogin));
    if (a?.hasPassword) relogin(a);
    else invoke('browser_login', { userId: null }).then(() => toast('info', T('Inicia sesión en la ventana nueva; la cuenta se agregará sola.'))).catch((err) => toast('error', T(String(err))));
  }
});
listen('accounts', () => { if (!$('dlg-health').classList.contains('hidden')) renderHealth(); });

// ───────────────────────── buscador de juegos

let findNext = null;
let findTerm = '';
let findSeq = 0;
const compact = (n) => new Intl.NumberFormat(LANG === 'en' ? 'en-US' : 'es-ES', { notation: 'compact', maximumFractionDigits: 1 }).format(n);

async function findGames(more) {
  const term = more ? findTerm : $('find-q').value.trim();
  if (!term) return $('find-q').focus();
  const seq = ++findSeq;
  if (!more) { findTerm = term; findNext = null; $('find-list').innerHTML = ''; }
  $('find-go').disabled = $('find-more').disabled = true;
  $('find-msg').textContent = T('Buscando…');
  try {
    const r = await invoke('games_search', { term, pageToken: more ? findNext : null });
    if (seq !== findSeq) return;
    findNext = r.next;
    const favIds = new Set((games.favorites || []).map((g) => g.placeId));
    const hideSpons = $('find-nospons').checked;
    $('find-list').insertAdjacentHTML('beforeend', r.games.filter((g) => !(hideSpons && g.sponsored)).map((g) => `
      <div class="fg" data-place="${g.placeId}" data-name="${esc(g.name)}" data-icon="${esc(g.icon)}">
        ${g.icon ? `<img src="${esc(g.icon)}" alt="" loading="lazy">` : '<div class="noimg"></div>'}
        <div class="body">
          <div class="nm" title="${esc(g.name)}">${esc(g.name)}</div>
          <div class="meta"><b>${compact(g.players)}</b> ${esc(T('jugando'))}${g.likes != null ? ` · ${esc(T('{n}% le gusta', { n: g.likes }))}` : ''}</div>
          ${g.creator ? `<div class="meta" title="${esc(g.creator)}">${esc(T('por {name}', { name: g.creator }))}${g.verified ? ` (${esc(T('verificado'))})` : ''}</div>` : ''}
          ${g.sponsored ? `<div class="tag">${esc(T('Patrocinado'))}</div>` : ''}
          <div class="btns">
            <button class="use" data-fuse="${g.placeId}">${esc(T('Usar'))}</button>
            <button data-fsrv="${g.placeId}">${esc(T('Servidores'))}</button>
            <button class="fav" data-ffav="${g.placeId}" title="${esc(T('Favorito'))}">${favIds.has(g.placeId) ? '★' : '☆'}</button>
          </div>
        </div>
      </div>`).join(''));
    const n = $('find-list').children.length;
    $('find-msg').textContent = n ? T('{n} juegos', { n }) : T('No se encontraron juegos.');
    $('find-more').classList.toggle('hidden', !findNext);
  } catch (e) {
    if (seq === findSeq) $('find-msg').textContent = T(String(e));
  } finally {
    if (seq === findSeq) $('find-go').disabled = $('find-more').disabled = false;
  }
}

$('btn-find').onclick = async () => {
  games = await invoke('games_list').catch(() => games);
  open('dlg-find');
  $('find-q').focus();
  $('find-q').select();
};
$('find-go').onclick = () => findGames(false);
$('find-more').onclick = () => findGames(true);
onEnter($('find-q'), () => findGames(false));
$('find-nospons').onchange = () => findTerm && findGames(false);
$('find-list').addEventListener('click', async (e) => {
  const d = e.target.dataset;
  const card = e.target.closest('.fg');
  if (!card) return;
  const placeId = Number(card.dataset.place);
  if (d.fuse) {
    $('place-id').value = placeId;
    $('job-id').value = '';
    lookupGame();
    close('dlg-find');
    document.querySelector('[data-rtab=launch]').click();
    toast('success', T('Juego listo en el Lanzador: {name}', { name: card.dataset.name }));
  } else if (d.fsrv) {
    close('dlg-find');
    $('place-id').value = placeId;
    lookupGame();
    $('btn-servers').click();
    $('srv-place').value = placeId;
    loadServers(false);
  } else if (d.ffav) {
    const isFav = (games.favorites || []).some((g) => g.placeId === placeId);
    try {
      if (isFav) await invoke('game_remove_favorite', { placeId });
      else await invoke('game_add_favorite', { entry: { placeId, name: card.dataset.name, icon: card.dataset.icon, privateServer: '' } });
      e.target.textContent = isFav ? '☆' : '★';
      toast('success', T(isFav ? 'Quitado de favoritos' : 'Agregado a favoritos'));
    } catch (err) { toast('error', T(String(err))); }
  }
});

// ───────────────────────── servidores privados

let vips = [];
let vipEditing = 0;

async function loadVips() {
  vips = await invoke('vips_list').catch(() => []);
  renderVips();
}

function vipSelText() {
  const n = selected.size;
  $('vip-sel').textContent = n ? T('Se unirán las {n} cuenta(s) seleccionadas en la lista.', { n }) : T('Selecciona cuentas en la lista para unirlas a un servidor.');
}

function renderVips() {
  vipSelText();
  $('vip-list').innerHTML = vips.length ? vips.map((v) => `
    <div class="vip ${v.id === vipEditing ? 'editing' : ''}">
      ${v.icon ? `<img src="${esc(v.icon)}" alt="">` : '<div class="noimg"></div>'}
      <div class="info"><b>${esc(v.name)}</b><span>${esc(v.gameName || T('Juego desconocido'))} · Place ${v.placeId}</span></div>
      <button class="go" data-vjoin="${v.id}">${esc(T('Unirse'))}</button>
      <button data-vcopy="${v.id}">${esc(T('Copiar enlace'))}</button>
      <button data-vedit="${v.id}">${esc(T('Editar'))}</button>
      <button data-vdel="${v.id}" title="${esc(T('Eliminar'))}">✕</button>
    </div>`).join('') : `<p class="muted" style="padding:8px">${esc(T('Todavía no hay servidores guardados. Pega un enlace arriba.'))}</p>`;
}

function vipForm(v) {
  vipEditing = v?.id ?? 0;
  $('vip-form-title').textContent = T(v ? 'Editar servidor' : 'Guardar un servidor');
  $('vip-link').value = v?.link ?? '';
  $('vip-name').value = v?.name ?? '';
  $('vip-err').textContent = '';
  $('vip-cancel-edit').classList.toggle('hidden', !v);
  renderVips();
}

$('btn-vips').onclick = () => { vipForm(null); loadVips(); open('dlg-vips'); $('vip-link').focus(); };
$('vip-cancel-edit').onclick = () => vipForm(null);
$('vip-save').onclick = async () => {
  const link = $('vip-link').value.trim();
  if (!link) return $('vip-link').focus();
  const sel = selectedAccounts();
  const server = { ...(vips.find((v) => v.id === vipEditing) || {}), id: vipEditing, name: $('vip-name').value, link };
  $('vip-save').disabled = true;
  $('vip-err').textContent = T('Comprobando enlace…');
  try {
    const v = await invoke('vip_save', { server, userId: sel.length === 1 ? sel[0].userId : null });
    toast('success', T('Servidor guardado: {name}', { name: v.name }));
    vipForm(null);
    loadVips();
  } catch (e) { $('vip-err').textContent = T(String(e)); }
  finally { $('vip-save').disabled = false; }
};
onEnter($('vip-link'), () => $('vip-save').click());
onEnter($('vip-name'), () => $('vip-save').click());

$('vip-list').addEventListener('click', async (e) => {
  const d = e.target.dataset;
  const v = vips.find((x) => x.id === Number(d.vjoin || d.vcopy || d.vedit || d.vdel));
  if (!v) return;
  if (d.vjoin) {
    const ids = orderedIds().filter((id) => selected.has(id));
    if (!ids.length) return toast('error', T('Selecciona al menos una cuenta'));
    if (ids.length > 1 && !await confirmBox(T('¿Unir {n} cuentas a "{name}"? Se abrirán {n} Roblox.', { n: ids.length, name: v.name }))) return;
    try { await invoke('vip_join', { id: v.id, ids }); toast('info', T('Uniéndose a {name}', { name: v.name })); close('dlg-vips'); }
    catch (err) { toast('error', T(String(err))); }
  }
  if (d.vcopy) { await navigator.clipboard.writeText(v.link); toast('success', T('Enlace copiado')); }
  if (d.vedit) { vipForm(v); $('vip-name').focus(); }
  if (d.vdel && await confirmBox(T('¿Eliminar "{name}" de tus servidores guardados?', { name: v.name }))) {
    await invoke('vip_delete', { id: v.id }).catch((err) => toast('error', T(String(err))));
    if (vipEditing === v.id) vipForm(null);
    loadVips();
  }
});
$('rows').addEventListener('click', () => { if (!$('dlg-vips').classList.contains('hidden')) vipSelText(); });

// ───────────────────────── grupos de Roblox

let grData = null;

async function loadGroups(ids) {
  if (!ids.length) return toast('error', T('Selecciona al menos una cuenta'));
  ['gr-all', 'gr-sel'].forEach((id) => { $(id).disabled = true; });
  $('gr-msg').textContent = T('Leyendo los grupos de {n} cuenta(s)…', { n: ids.length });
  try {
    grData = await invoke('groups_overview', { ids });
    renderGroups();
    if (grData.failed.length) toast('error', T('No se pudieron leer los grupos de {n} cuenta(s)', { n: grData.failed.length }));
  } catch (e) { $('gr-msg').textContent = T(String(e)); }
  finally { ['gr-all', 'gr-sel'].forEach((id) => { $(id).disabled = false; }); }
}

function renderGroups() {
  if (!grData) { $('gr-list').innerHTML = `<p class="muted" style="padding:8px">${esc(T('Pulsa "Cargar" para ver en qué grupos están tus cuentas.'))}</p>`; $('gr-msg').textContent = ''; return; }
  const q = $('gr-filter').value.trim().toLowerCase();
  const shared = $('gr-shared').checked;
  const list = grData.groups.filter((g) => (!q || g.name.toLowerCase().includes(q)) && (!shared || g.members.length > 1));
  const owned = grData.groups.filter((g) => g.members.some((m) => m.owner)).length;
  $('gr-msg').textContent = T('{n} grupo(s) · {s} en común · {o} tuyos', { n: grData.groups.length, s: grData.groups.filter((g) => g.members.length > 1).length, o: owned })
    + (list.length !== grData.groups.length ? ` · ${T('mostrando {n}', { n: list.length })}` : '');
  $('gr-list').innerHTML = list.map((g) => `
    <div class="grp-card">
      ${g.icon ? `<img src="${esc(g.icon)}" alt="" loading="lazy">` : '<div class="noimg"></div>'}
      <div class="info">
        <b title="${esc(g.name)}">${esc(g.name)}${g.verified ? ` <span class="sub">(${esc(T('verificado'))})</span>` : ''}</b>
        <div class="sub">${esc(T('{n} miembros', { n: fmt(g.memberCount) }))} · ${esc(T(g.publicEntry ? 'Entrada libre' : 'Con solicitud'))} · ID ${g.id}</div>
        <div class="mems">${g.members.map((m) => {
          const a = accounts.find((x) => x.userId === m.userId);
          return `<span class="mem ${m.owner ? 'owner' : ''}" title="${esc(T('Rango {r}', { r: m.rank }))}">${avImg(a)}${esc(a ? (a.alias || a.username) : m.userId)} <span class="rk">${esc(m.owner ? T('Dueño') : m.role)}</span></span>`;
        }).join('')}</div>
      </div>
      <div class="acts">
        <button data-gopen="${g.id}" data-guser="${g.members[0].userId}">${esc(T('Abrir'))}</button>
        <button data-gcopy="${g.id}">${esc(T('Copiar ID'))}</button>
      </div>
    </div>`).join('') || `<p class="muted" style="padding:8px">${esc(T('Ningún grupo coincide.'))}</p>`;
}

$('btn-groups').onclick = () => { renderGroups(); open('dlg-groups'); };
$('gr-all').onclick = () => loadGroups(accounts.map((a) => a.userId));
$('gr-sel').onclick = () => loadGroups(orderedIds().filter((id) => selected.has(id)));
$('gr-filter').oninput = $('gr-shared').onchange = renderGroups;
$('gr-list').addEventListener('click', async (e) => {
  const d = e.target.dataset;
  if (d.gopen) invoke('browser_open', { userId: Number(d.guser), path: `/communities/${d.gopen}` }).catch((err) => toast('error', T(String(err))));
  if (d.gcopy) { await navigator.clipboard.writeText(d.gcopy); toast('success', T('Copiado')); }
});

// ───────────────────────── inventario y limiteds

let invData = null;

async function loadInventory(ids) {
  if (!ids.length) return toast('error', T('Selecciona al menos una cuenta'));
  ['inv-all', 'inv-sel'].forEach((id) => { $(id).disabled = true; });
  $('inv-msg').textContent = T('Leyendo el inventario de {n} cuenta(s)… puede tardar un poco.', { n: ids.length });
  try { invData = await invoke('inventory_overview', { ids }); renderInventory(); }
  catch (e) { $('inv-msg').textContent = T(String(e)); }
  finally { ['inv-all', 'inv-sel'].forEach((id) => { $(id).disabled = false; }); }
}

function renderInventory() {
  const accSel = $('inv-acc');
  if (!invData) {
    ['inv-rap', 'inv-count', 'inv-accs', 'inv-priv'].forEach((id) => { $(id).textContent = '—'; });
    accSel.innerHTML = `<option value="">${esc(T('Todas las cuentas'))}</option>`;
    $('inv-rows').innerHTML = `<tr><td colspan="6" class="muted">${esc(T('Pulsa "Cargar" para ver los limiteds de tus cuentas.'))}</td></tr>`;
    $('inv-msg').textContent = '';
    return;
  }
  const keep = accSel.value;
  const accs = invData.accounts;
  accSel.innerHTML = `<option value="">${esc(T('Todas las cuentas'))}</option>` + accs.filter((x) => x.items.length)
    .map((x) => { const a = accounts.find((y) => y.userId === x.userId); return `<option value="${x.userId}">${esc(a ? accName(a) : x.userId)} · ${fmt(x.rap)}</option>`; }).join('');
  if ([...accSel.options].some((o) => o.value === keep)) accSel.value = keep;
  $('inv-rap').textContent = fmt(accs.reduce((n, x) => n + x.rap, 0));
  $('inv-count').textContent = fmt(accs.reduce((n, x) => n + x.items.length, 0));
  $('inv-accs').textContent = accs.filter((x) => x.items.length).length;
  $('inv-priv').textContent = accs.filter((x) => x.private).length;
  const q = $('inv-filter').value.trim().toLowerCase();
  const only = Number(accSel.value) || null;
  const rows = accs.flatMap((x) => x.items.map((it) => ({ it, uid: x.userId })))
    .filter(({ it, uid }) => (!only || uid === only) && (!q || it.name.toLowerCase().includes(q)))
    .sort((p, r) => r.it.rap - p.it.rap);
  const problems = accs.filter((x) => x.error || x.truncated);
  $('inv-msg').textContent = T('{n} objeto(s) mostrados', { n: rows.length })
    + (problems.length ? ` · ${T('{n} cuenta(s) con datos incompletos', { n: problems.length })}` : '');
  $('inv-rows').innerHTML = rows.slice(0, 500).map(({ it, uid }) => {
    const a = accounts.find((y) => y.userId === uid);
    const icon = invData.icons[it.assetId];
    return `<tr>
      <td>${icon ? `<img src="${esc(icon)}" alt="" loading="lazy">` : '<div class="noimg"></div>'}</td>
      <td title="${esc(it.name)}">${esc(it.name)}${it.onHold ? ` <span class="hold">${esc(T('en espera'))}</span>` : ''}</td>
      <td><span class="cell">${avImg(a)}<span class="nm">${esc(a ? (a.alias || a.username) : uid)}</span></span></td>
      <td class="num">${fmt(it.rap)}</td>
      <td class="muted">${it.serial != null ? `#${it.serial}${it.stock ? ` / ${fmt(it.stock)}` : ''}` : ''}</td>
      <td><button data-iopen="${it.assetId}" data-iuser="${uid}">${esc(T('Ver'))}</button></td></tr>`;
  }).join('') || `<tr><td colspan="6" class="muted">${esc(T('Ningún limited coincide.'))}</td></tr>`;
}

$('btn-inv').onclick = () => { renderInventory(); open('dlg-inv'); };
$('inv-all').onclick = () => loadInventory(accounts.map((a) => a.userId));
$('inv-sel').onclick = () => loadInventory(orderedIds().filter((id) => selected.has(id)));
$('inv-filter').oninput = $('inv-acc').onchange = renderInventory;
$('inv-rows').addEventListener('click', (e) => {
  const d = e.target.dataset;
  if (d.iopen) invoke('browser_open', { userId: Number(d.iuser), path: `/catalog/${d.iopen}` }).catch((err) => toast('error', T(String(err))));
});

// ───────────────────────── copiar avatar

let avSource = null;

function avSelText() {
  const n = orderedIds().filter((id) => selected.has(id) && id !== avSource).length;
  $('av-sel').textContent = n ? T('Se aplicará a {n} cuenta(s) seleccionadas.', { n }) : T('Selecciona en la lista las cuentas que recibirán el avatar.');
  $('av-apply').disabled = !avSource || !n;
}

async function loadAvatarSource() {
  const name = $('av-src-user').value.trim();
  let id = Number($('av-src-acc').value) || null;
  $('av-load').disabled = true;
  $('av-items').innerHTML = `<div class="muted">${esc(T('Cargando…'))}</div>`;
  $('av-res').innerHTML = '';
  try {
    if (name) id = /^\d+$/.test(name) ? Number(name) : await invoke('user_id_by_name', { username: name });
    if (!id) throw T('Elige una cuenta o escribe un usuario');
    const p = await invoke('avatar_preview', { userId: id });
    avSource = id;
    $('av-img').src = p.image || '';
    $('av-type').textContent = p.avatarType ? T('Tipo de cuerpo: {t}', { t: p.avatarType }) : '';
    $('av-items').innerHTML = p.items.length
      ? p.items.map(([, n, t]) => `<div><span title="${esc(n)}">${esc(n)}</span><span>${esc(t)}</span></div>`).join('')
      : `<div class="muted">${esc(T('Este avatar no lleva objetos.'))}</div>`;
  } catch (e) {
    avSource = null;
    $('av-img').src = '';
    $('av-type').textContent = '';
    $('av-items').innerHTML = `<div class="err">${esc(T(String(e)))}</div>`;
  } finally { $('av-load').disabled = false; avSelText(); }
}

$('btn-avcopy').onclick = () => {
  $('av-src-acc').innerHTML = `<option value="">${esc(T('Una de tus cuentas…'))}</option>` + accounts.map((a) => `<option value="${a.userId}">${esc(accName(a))}</option>`).join('');
  avSelText();
  open('dlg-avcopy');
};
$('av-src-acc').onchange = () => { if ($('av-src-acc').value) { $('av-src-user').value = ''; loadAvatarSource(); } };
$('av-src-user').oninput = () => { if ($('av-src-user').value) $('av-src-acc').value = ''; };
onEnter($('av-src-user'), () => loadAvatarSource());
$('av-load').onclick = loadAvatarSource;
$('av-apply').onclick = async () => {
  const targets = orderedIds().filter((id) => selected.has(id) && id !== avSource);
  if (!avSource || !targets.length) return;
  if (!await confirmBox(T('¿Cambiar el avatar de {n} cuenta(s)? Su avatar actual se reemplaza; si quieres conservarlo, guárdalo antes como traje en Roblox.', { n: targets.length }))) return;
  $('av-apply').disabled = true;
  $('av-res').innerHTML = `<span class="muted">${esc(T('Aplicando, una cuenta por segundo…'))}</span>`;
  try {
    const res = await invoke('avatar_copy', { source: avSource, targets });
    $('av-res').innerHTML = res.map((r) => {
      const a = accounts.find((x) => x.userId === r.userId);
      const who = esc(a ? (a.alias || a.username) : r.userId);
      if (!r.ok) return `<span class="bad">${who}: ${esc(T(r.error))}</span>`;
      return `<span class="ok">${who}: ${esc(r.missing ? T('aplicado; {n} objeto(s) no se pusieron porque la cuenta no los tiene', { n: r.missing }) : T('aplicado'))}</span>`;
    }).join('');
  } catch (e) { $('av-res').innerHTML = `<span class="bad">${esc(T(String(e)))}</span>`; }
  finally { avSelText(); }
};
$('rows').addEventListener('click', () => { if (!$('dlg-avcopy').classList.contains('hidden')) avSelText(); });

// ───────────────────────── amigos conectados

let frData = null;

async function loadFriends(ids) {
  if (!ids.length) return toast('error', T('Selecciona al menos una cuenta'));
  ['fr-all', 'fr-sel'].forEach((id) => { $(id).disabled = true; });
  $('fr-msg').textContent = T('Leyendo los amigos de {n} cuenta(s)…', { n: ids.length });
  try {
    frData = await invoke('friends_overview', { ids });
    renderFriends();
    if (frData.failed.length) toast('error', T('No se pudieron leer los amigos de {n} cuenta(s)', { n: frData.failed.length }));
  } catch (e) { $('fr-msg').textContent = T(String(e)); }
  finally { ['fr-all', 'fr-sel'].forEach((id) => { $(id).disabled = false; }); }
}

function renderFriends() {
  if (!frData) { $('fr-list').innerHTML = `<p class="muted" style="padding:8px">${esc(T('Pulsa "Cargar" para ver los amigos de tus cuentas.'))}</p>`; $('fr-msg').textContent = ''; return; }
  const q = $('fr-filter').value.trim().toLowerCase();
  const onlyOn = $('fr-online').checked;
  const all = frData.friends;
  const list = all.filter((f) => (!onlyOn || f.presence > 0) && (!q || [f.name, f.displayName, f.location].some((x) => (x || '').toLowerCase().includes(q))));
  $('fr-msg').textContent = T('{n} amigo(s) · {g} en juego · {o} en línea', { n: all.length, g: all.filter((f) => f.presence === 2).length, o: all.filter((f) => f.presence > 0).length })
    + (list.length !== all.length ? ` · ${T('mostrando {n}', { n: list.length })}` : '');
  $('fr-list').innerHTML = list.map((f) => `
    <div class="us">
      ${f.avatar ? `<img src="${esc(f.avatar)}" alt="" loading="lazy">` : '<div class="noimg"></div>'}
      <div class="info">
        <b>${esc(f.displayName || f.name || f.id)}</b>
        <div class="sub">@${esc(f.name)} · ID ${f.id}</div>
        <div class="sub"><span class="pres p${f.presence}"><i></i>${esc(presLabel(f))}</span></div>
        <div class="via">${f.via.map((id) => { const a = accounts.find((x) => x.userId === id); return `<span class="mem">${avImg(a)}${esc(a ? (a.alias || a.username) : id)}</span>`; }).join('')}</div>
      </div>
      <div class="acts">
        <button class="go" data-fjoin="${f.id}" ${f.presence === 2 ? '' : 'disabled'}>${esc(T('Unirse ({n})', { n: f.via.length }))}</button>
        <button data-fjoinsel="${f.id}" ${f.presence === 2 ? '' : 'disabled'}>${esc(T('Con selección'))}</button>
        <button data-fprof="${f.id}">${esc(T('Abrir en Roblox'))}</button>
      </div>
    </div>`).join('') || `<p class="muted" style="padding:8px">${esc(T(onlyOn ? 'Ningún amigo conectado ahora mismo.' : 'Ningún amigo coincide.'))}</p>`;
}

async function followWith(f, ids) {
  ids = ids.filter((id) => id !== f.id);
  if (!ids.length) return toast('error', T('Selecciona al menos una cuenta'));
  if (ids.length > 1 && !await confirmBox(T('¿Seguir a {name} con {n} cuentas? Se abrirán {n} Roblox.', { name: f.name, n: ids.length }))) return;
  launch(ids, { placeId: f.id, jobId: '', followUser: true, joinVip: false });
  toast('info', T('Siguiendo a {name}…', { name: f.name }));
  close('dlg-friends');
}

$('btn-friends').onclick = () => { renderFriends(); open('dlg-friends'); };
$('fr-all').onclick = () => loadFriends(accounts.map((a) => a.userId));
$('fr-sel').onclick = () => loadFriends(orderedIds().filter((id) => selected.has(id)));
$('fr-filter').oninput = $('fr-online').onchange = renderFriends;
$('fr-list').addEventListener('click', (e) => {
  const d = e.target.dataset;
  const f = frData?.friends.find((x) => x.id === Number(d.fjoin || d.fjoinsel || d.fprof));
  if (!f) return;
  if (d.fjoin) followWith(f, orderedIds().filter((id) => f.via.includes(id)));
  if (d.fjoinsel) followWith(f, orderedIds().filter((id) => selected.has(id)));
  if (d.fprof) invoke('browser_open', { userId: f.via[0], path: `/users/${f.id}/profile` }).catch((err) => toast('error', T(String(err))));
});

// ───────────────────────── buscar usuarios

function renderProfile(p) {
  const sec = (title, body) => `<div class="psec"><div class="pt">${esc(title)}</div>${body}</div>`;
  const none = `<span class="muted">${esc(T('Ninguno'))}</span>`;
  const date = (iso) => new Date(iso).toLocaleDateString(LANG === 'en' ? 'en-US' : 'es-ES');
  const games = (list) => (list.length ? `<div class="pgames">${list.map((g) => `
    <button class="pgame" data-pgame="${g.placeId ?? ''}" data-pname="${esc(g.name)}" title="${esc(T('Usar en el Lanzador'))}">
      ${g.icon ? `<img src="${esc(g.icon)}" alt="" loading="lazy">` : '<i></i>'}<span>${esc(g.name)}</span><small>${esc(T('{n} visitas', { n: compact(g.visits) }))}</small>
    </button>`).join('')}</div>` : none);
  const lim = p.limiteds;
  const out = [
    `<div class="pline">${esc(T('Amigos: {f} · Seguidores: {s} · Siguiendo: {g}', { f: p.friends ?? '?', s: p.followers ?? '?', g: p.following ?? '?' }))}`
      + (p.created ? ` · ${esc(T('Cuenta creada: {d}', { d: date(p.created) }))}` : '')
      + (p.banned ? ` · <span class="err">${esc(T('Cuenta suspendida'))}</span>` : '') + '</div>',
    sec(T('Nombres anteriores'), p.previousNames.length ? p.previousNames.map((n) => `<span class="mem">${esc(n)}</span>`).join(' ') : none),
    sec(T('Insignias de Roblox'), p.badges.length ? `<div class="pbadges">${p.badges.map((b) => `<span class="mem" title="${esc(b.description || '')}">${b.image ? `<img src="${esc(b.image)}" alt="">` : ''}${esc(b.name)}</span>`).join('')}</div>` : none),
    sec(T('Grupos ({n})', { n: p.groups.length }), p.groups.length ? `<div class="via">${p.groups.map((g) => `<span class="mem ${g.owner ? 'owner' : ''}" title="${esc(T('{n} miembros', { n: fmt(g.members) }))}">${esc(g.name)} <span class="rk">${esc(g.owner ? T('Dueño') : g.role)}${g.primary ? ` · ${esc(T('principal'))}` : ''}</span></span>`).join('')}</div>` : none),
    sec(T('Limiteds'), !lim ? `<span class="muted">${esc(T('No se pudo leer'))}</span>` : lim.private ? `<span class="muted">${esc(T('Inventario privado'))}</span>`
      : esc(T(lim.more ? '{n}+ limiteds · RAP de los primeros 100: {rap}' : '{n} limiteds · RAP {rap}', { n: lim.count, rap: fmt(lim.rap) }))),
    sec(T('Juegos favoritos'), games(p.favorites)),
    sec(T('Juegos creados'), games(p.createdGames)),
  ];
  if (p.description) out.push(sec(T('Descripción'), `<div class="pdesc">${esc(p.description)}</div>`));
  return out.join('');
}

let usResults = [];
const presLabel = (u) => {
  if (u.presence === 2) return T(u.location ? 'En juego: {game}' : 'En juego', { game: u.location });
  return { 0: T('Desconectado'), 1: T('En línea'), 3: T('En Studio') }[u.presence] ?? T('Estado desconocido');
};

function usSelText() {
  const n = selected.size;
  $('us-sel').textContent = n ? T('"Seguir" usará las {n} cuenta(s) seleccionadas en la lista.', { n }) : T('Selecciona cuentas en la lista para seguir a alguien con ellas.');
}

async function searchUsers() {
  const q = $('us-q').value.trim();
  if (!q) return $('us-q').focus();
  $('us-go').disabled = true;
  $('us-list').innerHTML = `<p class="muted" style="padding:8px">${esc(T('Buscando…'))}</p>`;
  try {
    usResults = await invoke('users_search', { keyword: q });
    renderUsers();
  } catch (e) { $('us-list').innerHTML = `<p class="err" style="padding:8px">${esc(T(String(e)))}</p>`; }
  finally { $('us-go').disabled = false; }
}

function renderUsers() {
  usSelText();
  $('us-list').innerHTML = usResults.map((u) => `
    <div class="us" data-uid="${u.id}">
      ${u.avatar ? `<img src="${esc(u.avatar)}" alt="">` : '<div class="noimg"></div>'}
      <div class="info">
        <b>${esc(u.displayName)}${u.mine ? `<span class="mine">${esc(T('tu cuenta'))}</span>` : ''}</b>
        <div class="sub">@${esc(u.name)}${u.verified ? ` (${esc(T('verificado'))})` : ''} · ID ${u.id}</div>
        <div class="sub"><span class="pres p${u.presence}"><i></i>${esc(presLabel(u))}</span></div>
        <div class="uprof hidden" id="us-prof-${u.id}"></div>
      </div>
      <div class="acts">
        <button class="go" data-ufollow="${u.id}">${esc(T('Seguir a su juego'))}</button>
        <button data-uprof="${u.id}">${esc(T('Ver perfil'))}</button>
        <button data-uopen="${u.id}">${esc(T('Abrir en Roblox'))}</button>
      </div>
    </div>`).join('') || `<p class="muted" style="padding:8px">${esc(T('No se encontraron usuarios.'))}</p>`;
}

$('btn-users').onclick = () => { usSelText(); open('dlg-users'); $('us-q').focus(); };
$('us-go').onclick = searchUsers;
onEnter($('us-q'), () => searchUsers());
$('us-list').addEventListener('click', async (e) => {
  const d = e.target.closest('[data-pgame]')?.dataset ?? e.target.dataset;
  if (d.pgame !== undefined && !d.pgame) return;
  const u = d.pgame ? {} : usResults.find((x) => x.id === Number(d.ufollow || d.uprof || d.uopen));
  if (!u) return;
  if (d.ufollow) {
    const ids = orderedIds().filter((id) => selected.has(id) && id !== u.id);
    if (!ids.length) return toast('error', T('Selecciona al menos una cuenta'));
    if (u.presence === 0 && !await confirmBox(T('{name} aparece desconectado. ¿Intentar seguirlo igualmente?', { name: u.name }))) return;
    if (ids.length > 1 && !await confirmBox(T('¿Seguir a {name} con {n} cuentas? Se abrirán {n} Roblox.', { name: u.name, n: ids.length }))) return;
    launch(ids, { placeId: u.id, jobId: '', followUser: true, joinVip: false });
    toast('info', T('Siguiendo a {name}…', { name: u.name }));
    close('dlg-users');
  }
  if (d.uprof) {
    const box = $(`us-prof-${u.id}`);
    if (!box.classList.contains('hidden')) { box.classList.add('hidden'); return; }
    box.textContent = T('Cargando…');
    box.classList.remove('hidden');
    try {
      box.innerHTML = renderProfile(await invoke('user_profile', { userId: u.id }));
    } catch (err) { box.textContent = T(String(err)); }
  }
  if (d.pgame) {
    $('place-id').value = d.pgame;
    $('job-id').value = '';
    lookupGame();
    close('dlg-users');
    document.querySelector('[data-rtab=launch]').click();
    toast('success', T('Juego listo en el Lanzador: {name}', { name: d.pname }));
    return;
  }
  if (d.uopen) {
    const via = orderedIds().find((id) => selected.has(id)) ?? accounts[0]?.userId;
    if (!via) return toast('error', T('Agrega al menos una cuenta'));
    invoke('browser_open', { userId: via, path: `/users/${u.id}/profile` }).catch((err) => toast('error', T(String(err))));
  }
});
$('rows').addEventListener('click', () => { if (!$('dlg-users').classList.contains('hidden')) usSelText(); });

// ───────────────────────── ayuda (icono ?)

const helpTip = document.createElement('div');
helpTip.className = 'help-tip hidden';
helpTip.setAttribute('role', 'tooltip');
document.body.appendChild(helpTip);
let helpFor = null;

function showHelp(el) {
  helpFor = el;
  helpTip.textContent = el.dataset.help;
  helpTip.classList.remove('hidden');
  const r = el.getBoundingClientRect();
  const w = helpTip.offsetWidth, h = helpTip.offsetHeight;
  const left = Math.max(8, Math.min(r.left + r.width / 2 - w / 2, innerWidth - w - 8));
  const below = r.bottom + 6 + h <= innerHeight - 8;
  helpTip.style.left = `${left}px`;
  helpTip.style.top = `${below ? r.bottom + 6 : Math.max(8, r.top - h - 6)}px`;
}
function hideHelp() { helpFor = null; helpTip.classList.add('hidden'); }

document.addEventListener('mouseover', (e) => { const h = e.target.closest?.('.help'); if (h && h !== helpFor) showHelp(h); });
document.addEventListener('mouseout', (e) => { if (e.target.closest?.('.help') && !e.relatedTarget?.closest?.('.help')) hideHelp(); });
document.addEventListener('focusin', (e) => { if (e.target.classList?.contains('help')) showHelp(e.target); });
document.addEventListener('focusout', (e) => { if (e.target.classList?.contains('help')) hideHelp(); });
// Dentro de un label o un botón, el "?" no debe marcar la casilla ni pulsar el botón.
document.addEventListener('click', (e) => {
  const h = e.target.closest?.('.help');
  if (!h) { if (helpFor) hideHelp(); return; }
  e.preventDefault();
  e.stopPropagation();
  showHelp(h);
}, true);
document.addEventListener('keydown', (e) => { if (e.key === 'Escape') hideHelp(); });
document.addEventListener('scroll', hideHelp, true);

// ───────────────────────── idioma

$('lang-sel').value = LANG;
$('lang-sel').onchange = (e) => {
  setLang(e.target.value);
  render();
  showMulti();
  renderProfiles();
  const a = accTarget;
  accTarget = null;
  showAccount(a);
};
