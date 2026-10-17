const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const appWin = window.__TAURI__.window.getCurrentWindow();
const $ = (id) => document.getElementById(id);

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
  else if (act === 'copy-user') navigator.clipboard.writeText(selectedAccounts().map((a) => a.username).join('\n'));
  else if (act === 'group') openGroup();
  else if (act === 'afk') toggleAfk();
  else if (act === 'remove') removeSelected();
});

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
$('btn-group').onclick = openGroup;
$('group-ok').onclick = async () => {
  const group = $('group-name').value.trim() || 'Default';
  for (const a of selectedAccounts()) await update({ userId: a.userId, group });
  close('dlg-group');
};
$('group-name').onkeydown = (e) => e.key === 'Enter' && $('group-ok').click();

async function removeSelected() {
  const sel = selectedAccounts();
  if (!sel.length) return;
  if (!confirm(sel.length === 1 ? `¿Eliminar ${sel[0].username}?` : `¿Eliminar ${sel.length} cuentas?`)) return;
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
$('btn-add').onclick = () => { open('dlg-add'); $('add-cookie').focus(); };
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
    $('srv-rows').insertAdjacentHTML('beforeend', page.data.map((s) => `
      <tr><td>${s.playing}/${s.maxPlayers}</td><td>${s.ping}</td><td>${Math.round(s.fps)}</td>
      <td class="mono">${esc(s.id)}</td><td><button data-job="${esc(s.id)}">Usar</button></td></tr>`).join(''));
    $('srv-msg').textContent = `${$('srv-rows').children.length} servidores`;
  } catch (e) { $('srv-msg').textContent = e; }
}
$('btn-servers').onclick = () => { open('dlg-servers'); $('srv-place').value ||= $('place-id').value; $('srv-place').focus(); };
$('srv-load').onclick = () => loadServers(false);
$('srv-more').onclick = () => loadServers(true);
$('srv-place').onkeydown = (e) => e.key === 'Enter' && loadServers(false);
$('srv-rows').addEventListener('click', (e) => {
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
document.querySelectorAll('.tab').forEach((t) => t.onclick = () => {
  document.querySelectorAll('.tab').forEach((x) => x.classList.toggle('on', x === t));
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

// ───────────────────────── diálogo de ajustes

$('btn-settings').onclick = async () => {
  settings = await invoke('settings_get');
  document.querySelectorAll('#dlg-settings [data-k], #dlg-settings [data-w]').forEach((el) => {
    const v = el.dataset.k ? settings.general[el.dataset.k] : settings.watcher[el.dataset.w];
    if (el.type === 'checkbox') el.checked = !!v; else el.value = v;
  });
  $('set-err').textContent = '';
  $('app-ver').textContent = `Versión ${await invoke('app_version')}`;
  open('dlg-settings');
};
$('set-save').onclick = async () => {
  document.querySelectorAll('#dlg-settings [data-k], #dlg-settings [data-w]').forEach((el) => {
    const v = el.type === 'checkbox' ? el.checked : Number(el.value);
    if (el.dataset.k) settings.general[el.dataset.k] = v; else settings.watcher[el.dataset.w] = v;
  });
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
