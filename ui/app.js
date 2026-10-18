const { invoke } = window.__TAURI__.core;
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

const confirmBox = (msg) => dialog.ask(msg, { title: 'Roblox Account Manager', kind: 'warning', okLabel: 'Sí', cancelLabel: 'No' });

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
    $('setup-msg').textContent = `Se importaron ${info.migrated} cuentas. Elige cómo protegerlas.`;
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
$('vault-pass').onkeydown = (e) => e.key === 'Enter' && $('vault-unlock').click();
$('legacy-pass').onkeydown = (e) => e.key === 'Enter' && $('legacy-unlock').click();

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
  if (a.needsLogin) return 'Requiere iniciar sesión otra vez';
  return { 0: 'Desconectado', 1: 'En línea', 2: `En juego${a.presenceLocation ? `: ${a.presenceLocation}` : ''}`, 3: 'En Studio' }[a.presenceType] || '';
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
      <td title="${esc(dotTitle(a))}"><span class="cell"><span class="dot ${dotClass(a)}"></span>${a.avatarUrl ? `<img class="av" src="${esc(a.avatarUrl)}" alt="">` : '<span class="av"></span>'}<span class="nm">${hide ? '••••••••' : esc(a.username)}</span>${timer(a)}${a.antiAfk ? '<span class="afk">AFK</span>' : ''}</span></td>
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
  else if (act === 'copy-user') copy(selectedAccounts().map((a) => a.username));
  else if (act?.startsWith('copy-')) copySecret(act.slice(5));
  else if (act === 'group') openGroup();
  else if (act === 'util') openUtil();
  else if (act === 'afk') toggleAfk();
  else if (act === 'remove') removeSelected();
});

async function copy(lines) {
  await navigator.clipboard.writeText(lines.join('\n'));
  toast('success', lines.length === 1 ? 'Copiado' : `${lines.length} líneas copiadas`);
}

async function copySecret(kind) {
  try {
    const out = [];
    for (const a of selectedAccounts()) out.push(await invoke('account_secret', { userId: a.userId, kind }));
    if (kind === 'password' && out.every((p) => !p)) return toast('error', 'No hay contraseña guardada');
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
  if (sel.length !== 1) { toast('error', 'Selecciona una cuenta'); return null; }
  return sel[0];
}

$('btn-alias').onclick = () => { const a = needOne(); if (a) update({ userId: a.userId, alias: $('alias').value.trim() }); };
$('alias').onkeydown = (e) => e.key === 'Enter' && $('btn-alias').click();
$('btn-desc').onclick = () => { const a = needOne(); if (a) update({ userId: a.userId, description: $('desc').value }).then(() => toast('success', 'Descripción guardada')); };

async function toggleAfk() {
  const sel = selectedAccounts();
  if (!sel.length) return toast('error', 'Selecciona al menos una cuenta');
  const on = !sel.every((a) => a.antiAfk);
  for (const a of sel) await update({ userId: a.userId, antiAfk: on });
  toast('success', `Anti-AFK ${on ? 'activado' : 'desactivado'} en ${sel.length} cuenta(s)`);
}
$('btn-afk').onclick = toggleAfk;

function openGroup() {
  if (!selected.size) return toast('error', 'Selecciona al menos una cuenta');
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
$('group-name').onkeydown = (e) => e.key === 'Enter' && $('group-ok').click();

async function removeSelected() {
  const sel = selectedAccounts();
  if (!sel.length) return;
  if (!await confirmBox(sel.length === 1 ? `¿Eliminar ${sel[0].username}?` : `¿Eliminar ${sel.length} cuentas?`)) return;
  for (const a of sel) await invoke('account_remove', { userId: a.userId });
  selected.clear();
  fillFields();
}
$('btn-remove').onclick = removeSelected;

$('btn-save-game').onclick = async () => {
  const sel = selectedAccounts();
  if (!sel.length) return toast('error', 'Selecciona al menos una cuenta');
  for (const a of sel) {
    await update({ userId: a.userId, fields: { ...a.fields, SavedPlaceId: $('place-id').value.trim(), SavedJobId: $('job-id').value.trim() } });
  }
  toast('success', 'Place/Job guardado en la cuenta');
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
  if (what === 'cookie') { open('dlg-add'); $('add-cookie').focus(); }
  if (what === 'import') importFile();
  if (what === 'export') exportFile();
});

async function importFile() {
  const path = await dialog.open({ title: 'Importar cuentas', filters: [{ name: 'Cuentas', extensions: ['json', 'dat'] }] });
  if (!path) return;
  let password = null;
  for (;;) {
    try {
      const n = await invoke('storage_import', { path, password });
      return toast('success', `${n} cuentas importadas`);
    } catch (e) {
      if (!String(e).toLowerCase().includes('contraseña')) return toast('error', e);
      password = await ask('Importar cuentas', String(e), { password: true });
      if (password === null) return;
    }
  }
}

async function exportFile() {
  if (!await confirmBox('El archivo exportado NO va cifrado: incluye cookies y contraseñas en texto plano. ¿Continuar?')) return;
  const path = await dialog.save({ title: 'Exportar cuentas', defaultPath: 'cuentas.json', filters: [{ name: 'JSON', extensions: ['json'] }] });
  if (!path) return;
  try { toast('success', `${await invoke('storage_export', { path })} cuentas exportadas`); }
  catch (e) { toast('error', e); }
}
$('add-cancel').onclick = closeAdd;
$('add-ok').onclick = async () => {
  $('add-ok').disabled = true;
  try {
    const v = await invoke('account_add', { cookie: $('add-cookie').value, password: $('add-pass').value });
    toast('success', `Cuenta ${v.username} agregada`);
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
  if (!ids.length) return toast('error', 'Selecciona al menos una cuenta');
  const placeId = Number($('place-id').value.trim());
  if (!placeId) return toast('error', 'Escribe un Place ID');
  launch(ids, { placeId, jobId: $('job-id').value.trim(), followUser: false, joinVip: false });
}
$('btn-join').onclick = join;

$('btn-follow').onclick = async () => {
  const ids = orderedIds().filter((id) => selected.has(id));
  if (!ids.length) return toast('error', 'Selecciona al menos una cuenta');
  const name = $('follow-user').value.trim();
  if (!name) return toast('error', 'Escribe el usuario a seguir');
  try {
    const userId = await invoke('user_id_by_name', { username: name });
    launch(ids, { placeId: userId, jobId: '', followUser: true, joinVip: false });
  } catch (e) { toast('error', `No se encontró ${name}: ${e}`); }
};
$('follow-user').onkeydown = (e) => e.key === 'Enter' && $('btn-follow').click();
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

// ───────────────────────── multi roblox

async function showMulti() {
  const want = settings?.general.enableMultiRoblox;
  const active = await invoke('multi_roblox_status');
  $('multi-chk').checked = !!want;
  const el = $('multi');
  el.textContent = !want ? '' : active ? '(activo)' : '(bloqueado)';
  el.className = `multi-state ${!want ? '' : active ? 'on' : 'bad'}`;
  el.title = want && !active ? 'Roblox u otro programa ya lo tiene. Cierra Roblox y haz clic para reintentar.' : '';
}
$('multi-chk').onchange = (e) => saveSettings((s) => { s.general.enableMultiRoblox = e.target.checked; });
$('multi').onclick = async (e) => {
  e.preventDefault();
  if (!$('multi').classList.contains('bad')) return;
  try { await invoke('multi_roblox_apply'); toast('success', 'Multi Roblox activado'); }
  catch (err) { toast('error', err); }
  showMulti();
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
  $('srv-msg').textContent = 'Cargando…';
  try {
    const page = await invoke('servers_list', { placeId, cursor: srvCursor ?? '' });
    srvCursor = page.nextCursor;
    $('srv-more').classList.toggle('hidden', !srvCursor);
    $('srv-rows').insertAdjacentHTML('beforeend', page.data.map((s) => {
      const mine = accounts.filter((a) => a.presenceType === 2 && a.gameId === s.id);
      const pct = s.maxPlayers ? Math.round((s.playing / s.maxPlayers) * 100) : 0;
      return `<tr class="${mine.length ? 'mine' : ''}" title="Job ID: ${esc(s.id)}">
        <td><div class="bar"><i style="width:${pct}%"></i><span>${s.playing}/${s.maxPlayers}</span></div></td>
        <td>${s.ping}</td><td>${Math.round(s.fps)}</td>
        <td class="region" data-region="${esc(s.id)}"><button class="lnk" data-reg="${esc(s.id)}">ver</button></td>
        <td class="mine-cell">${mine.map((a) => `${a.avatarUrl ? `<img class="av" src="${esc(a.avatarUrl)}" alt="">` : ''}${esc(a.alias || a.username)}`).join(', ')}</td>
        <td><button data-job="${esc(s.id)}">Usar</button></td></tr>`;
    }).join(''));
    $('srv-msg').textContent = `${$('srv-rows').children.length} servidores`;
  } catch (e) { $('srv-msg').textContent = e; }
}
$('btn-servers').onclick = () => { open('dlg-servers'); $('srv-place').value ||= $('place-id').value; $('srv-place').focus(); };
$('srv-load').onclick = () => loadServers(false);
$('srv-more').onclick = () => loadServers(true);
$('srv-place').onkeydown = (e) => e.key === 'Enter' && loadServers(false);
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
      <button class="star" data-fav="${g.placeId}" title="Favorito">${favIds.has(g.placeId) ? '★' : '☆'}</button>
      <div class="t">${esc(g.name)}</div>
    </div>`).join('') : `<p class="muted" style="padding:8px">${gamesTab === 'recent' ? 'Aún no hay juegos recientes.' : 'Marca juegos con ☆ en Recientes.'}</p>`;
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
  if (name === 'account') loadAccInfo();
}
document.querySelectorAll('.utab').forEach((t) => t.onclick = () => showUtab(t.dataset.utab));

function openUtil() {
  const a = needOne();
  if (!a) return;
  utilAcc = a;
  avatarFor = null;
  window.avatar3d?.dispose();
  $('util-title').textContent = `Utilidades de cuenta: ${a.alias || a.username}`;
  $('dn-name').value = a.displayName || '';
  ['pw-cur', 'pw-new', 'pin-code', 'ql-code'].forEach((id) => { $(id).value = ''; });
  $('bl-list').innerHTML = '';
  $('bl-msg').textContent = '';
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
    : '<tr><td colspan="3" class="muted">Todavía no hay datos. Se registran mientras la app está abierta y la cuenta está en juego.</td></tr>';
}

async function loadAvatar() {
  if (avatarFor === utilAcc.userId) return;
  avatarFor = utilAcc.userId;
  window.avatar3d?.dispose();
  $('avatar-msg').textContent = 'Generando modelo 3D…';
  try {
    const data = await invoke('account_util', { userId: utilAcc.userId, action: 'avatar3d', args: {} });
    if (avatarFor !== utilAcc.userId) return;
    $('avatar-msg').textContent = 'Arrastra para girar · rueda para acercar';
    window.avatar3d.render($('avatar-host'), data);
  } catch (e) { $('avatar-msg').textContent = String(e); avatarFor = null; }
}

async function util(action, args = {}) {
  return invoke('account_util', { userId: utilAcc.userId, action, args });
}
async function utilDo(btn, action, args, ok) {
  btn.disabled = true;
  try { const r = await util(action, args); toast('success', ok ?? (typeof r === 'string' ? r : 'Hecho')); return r; }
  catch (e) { toast('error', e); }
  finally { btn.disabled = false; }
}

async function loadAccInfo() {
  $('acc-info').textContent = 'Cargando…';
  try {
    const i = await util('info');
    const pin = !i.pinEnabled ? 'sin PIN' : i.pinUnlockedUntil > 0 ? 'PIN desbloqueado' : 'PIN activo';
    let email = i.email || '?';
    try {
      const e = JSON.parse(i.email);
      email = e.emailAddress ? `${e.emailAddress}${e.verified ? ' (verificado)' : ' (sin verificar)'}` : 'sin correo';
    } catch {}
    $('acc-info').textContent = `Robux: ${i.robux ?? '?'} · Correo: ${email} · ${pin}`;
  } catch (e) { $('acc-info').textContent = String(e); }
}

$('pw-go').onclick = (e) => {
  if (!$('pw-cur').value || !$('pw-new').value) return toast('error', 'Escribe la contraseña actual y la nueva');
  utilDo(e.target, 'password', { current: $('pw-cur').value, new: $('pw-new').value });
};
$('dn-go').onclick = (e) => utilDo(e.target, 'displayName', { name: $('dn-name').value.trim() });
$('pin-go').onclick = (e) => utilDo(e.target, 'unlockPin', { pin: $('pin-code').value.trim() }, 'PIN desbloqueado').then(loadAccInfo);
$('ql-go').onclick = (e) => utilDo(e.target, 'quickLogin', { code: $('ql-code').value.trim() }, 'Inicio de sesión aprobado');
$('so-go').onclick = async (e) => {
  if (await confirmBox('¿Cerrar la sesión de esta cuenta en todos los demás dispositivos?')) utilDo(e.target, 'signoutOthers');
};
$('bl-load').onclick = async () => {
  $('bl-msg').textContent = 'Cargando…';
  try {
    const v = await util('blocked');
    const list = v.blockedUsers ?? (v.data?.blockedUserIds ?? []).map((id) => ({ userId: id }));
    $('bl-msg').textContent = `${list.length} bloqueado(s)`;
    $('bl-list').innerHTML = list.map((u) => `<div class="r-row bl"><span class="grow">${esc(u.displayName || u.name || '')} <span class="muted">${esc(u.name ? '@' + u.name : u.userId)}</span></span><button data-unblock="${u.userId}">Desbloquear</button></div>`).join('');
  } catch (e) { $('bl-msg').textContent = String(e); }
};
$('bl-list').addEventListener('click', async (e) => {
  const id = Number(e.target.dataset.unblock);
  if (id && await utilDo(e.target, 'unblock', { userId: id }, 'Desbloqueado') !== undefined) e.target.closest('.bl').remove();
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
  $('procs-sum').textContent = `${list.length} abierto(s) · ${total} MB en total`;
  $('procs-rows').innerHTML = list.length ? list.map((p) => {
    const a = accounts.find((x) => x.userId && p.trackerId && accountsTracker[x.userId] === p.trackerId);
    const name = a ? `${a.avatarUrl ? `<img class="av" src="${esc(a.avatarUrl)}" alt="">` : ''}${esc(a.alias || a.username)}` : `<span class="muted">${esc(p.title || 'Desconocida')}</span>`;
    return `<tr><td><span class="cell">${name}</span></td><td>${p.pid}</td><td>${p.memoryMb} MB</td><td>${duration(now - p.startTime)}</td>
      <td>${p.hwnd ? `<button data-pmin="${p.hwnd}">Minimizar</button> <button data-pclose="${p.hwnd}">Cerrar</button> ` : ''}<button data-pkill="${p.pid}">Matar</button></td></tr>`;
  }).join('') : '<tr><td colspan="5" class="muted">No hay ningún Roblox abierto.</td></tr>';
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
  if (d.pkill && await confirmBox('¿Forzar el cierre de este Roblox?')) await invoke('process_kill', { pid: Number(d.pkill) });
  setTimeout(loadProcs, 600);
});
$('procs-min-all').onclick = async () => {
  for (const p of await invoke('processes_list')) if (p.hwnd) await invoke('window_minimize', { hwnd: p.hwnd });
};
$('procs-close-all').onclick = async () => {
  if (!await confirmBox('¿Cerrar todas las ventanas de Roblox?')) return;
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
  $('app-ver').textContent = `Versión ${await invoke('app_version')}`;
  open('dlg-settings');
};
$('set-save').onclick = async () => {
  document.querySelectorAll('#dlg-settings [data-k], #dlg-settings [data-w], #dlg-settings [data-ws]').forEach((el) => {
    const [obj, key] = settingSlot(el);
    obj[key] = el.type === 'checkbox' ? el.checked : el.type === 'number' ? Number(el.value) : el.value;
  });
  const ws = settings.webServer;
  if (ws.enabled && ws.password.length < 6 && (ws.allowGetCookie || ws.allowGetAccounts || ws.allowLaunchAccount)) {
    toast('error', 'El servidor web necesita una contraseña de al menos 6 caracteres para esos permisos');
  }
  try {
    await invoke('settings_set', { s: settings });
    close('dlg-settings');
  } catch (e) { $('set-err').textContent = e; }
  syncToggles();
  showMulti();
};

// ───────────────────────── actualizaciones

let updateUrl = '';
async function checkUpdate(manual) {
  try {
    const u = await invoke('update_check');
    if (u.available) {
      updateUrl = u.url;
      $('update-text').textContent = `Hay una nueva versión: ${u.latest} (tienes ${u.current})`;
      open('update-bar');
      close('dlg-settings');
    } else if (manual) toast('info', `Ya tienes la última versión (${u.current})`);
  } catch (e) { if (manual) toast('error', `No se pudo buscar actualizaciones: ${e}`); }
}
$('btn-check-update').onclick = () => checkUpdate(true);
$('update-later').onclick = () => close('update-bar');
$('update-go').onclick = async () => {
  $('update-go').disabled = true;
  $('update-text').textContent = 'Descargando actualización…';
  try { await invoke('update_install', { url: updateUrl }); }
  catch (e) { toast('error', e); $('update-go').disabled = false; }
};

// ───────────────────────── eventos del backend

listen('accounts', (e) => { accounts = e.payload; render(); });
listen('toast', (e) => toast(e.payload.kind, e.payload.message));
listen('launch-progress', (e) => {
  const p = e.payload;
  $('progress').classList.toggle('hidden', p.done);
  $('progress-text').textContent = `Lanzando ${p.current}/${p.total}: ${p.username}`;
});
listen('vip-privacy', (e) => toast('info', `${e.payload.username}: la privacidad de servidores privados no está en "Todos"`));

async function loadMain() {
  settings = await invoke('settings_get');
  accounts = await invoke('accounts_list');
  syncToggles();
  showMulti();
  invoke('accounts_refresh_avatars', { ids: accounts.map((a) => a.userId) });
  invoke('accounts_update_presence', { ids: accounts.map((a) => a.userId) });
  if (settings.general.checkForUpdates) checkUpdate(false);
}

invoke('vault_startup').then(showVault).catch((e) => toast('error', e));
