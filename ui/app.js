const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const $ = (id) => document.getElementById(id);

let accounts = [];
const selected = new Set();
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

// ───────────────────────── bóveda

function showVault(info) {
  ['vault-locked', 'vault-legacy', 'vault-setup'].forEach((id) => $(id).classList.add('hidden'));
  $('vault-err').textContent = '';
  if (info.status === 'ready') {
    $('vault').classList.add('hidden');
    loadMain();
    return;
  }
  $('vault').classList.remove('hidden');
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

// ───────────────────────── lista

function presence(a) {
  if (a.needsLogin) return '<span class="pill login">Requiere login</span>';
  switch (a.presenceType) {
    case 1: return '<span class="pill online">En línea</span>';
    case 2: return `<span class="pill game" title="${esc(a.presenceLocation)}">En juego${sessionTime(a)}</span>`;
    case 3: return '<span class="pill studio">Studio</span>';
    case 0: return '<span class="pill">Desconectado</span>';
    default: return '<span class="pill">—</span>';
  }
}

function sessionTime(a) {
  if (!a.sessionStart) return '';
  const m = Math.floor((Date.now() - new Date(a.sessionStart)) / 60000);
  return ` · ${m >= 60 ? `${Math.floor(m / 60)}h ${m % 60}m` : `${m}m`}`;
}

function ago(iso) {
  const d = (Date.now() - new Date(iso)) / 1000;
  if (d < 60) return 'ahora';
  if (d < 3600) return `hace ${Math.floor(d / 60)} min`;
  if (d < 86400) return `hace ${Math.floor(d / 3600)} h`;
  return `hace ${Math.floor(d / 86400)} d`;
}

function visible() {
  const q = $('search').value.trim().toLowerCase();
  if (!q) return accounts;
  return accounts.filter((a) => [a.username, a.displayName, a.alias, a.group, String(a.userId)].some((s) => s.toLowerCase().includes(q)));
}

function render() {
  const list = visible();
  $('empty').classList.toggle('hidden', accounts.length > 0);
  $('rows').innerHTML = list.map((a) => `
    <tr data-id="${a.userId}" class="${selected.has(a.userId) ? 'sel' : ''}">
      <td class="chk"><input type="checkbox" ${selected.has(a.userId) ? 'checked' : ''}></td>
      <td><div class="acc">
        ${a.avatarUrl ? `<img src="${esc(a.avatarUrl)}" alt="">` : '<div class="ph"></div>'}
        <div><div class="name">${esc(a.alias || a.displayName || a.username)}${a.antiAfk ? ' <span class="pill">AFK</span>' : ''}</div>
        <div class="sub">@${esc(a.username)}</div></div>
      </div></td>
      <td>${esc(a.group)}</td>
      <td>${presence(a)}</td>
      <td class="muted">${ago(a.lastUse)}</td>
    </tr>`).join('');
  renderDetails();
}

function renderDetails() {
  const ids = [...selected];
  if (ids.length !== 1) {
    $('details').innerHTML = ids.length ? `${ids.length} cuentas seleccionadas.` : 'Selecciona una cuenta.';
    return;
  }
  const a = accounts.find((x) => x.userId === ids[0]);
  if (!a) return;
  $('details').innerHTML = `<dl>
    <dt>Usuario</dt><dd>${esc(a.username)}</dd>
    <dt>ID</dt><dd>${a.userId}</dd>
    <dt>Alias</dt><dd>${esc(a.alias) || '—'}</dd>
    <dt>Descripción</dt><dd>${esc(a.description) || '—'}</dd>
    <dt>Contraseña</dt><dd>${a.hasPassword ? 'guardada' : '—'}</dd>
    <dt>Antigüedad</dt><dd>${a.ageDays} días en el gestor</dd>
  </dl>`;
}

$('rows').addEventListener('click', (e) => {
  const tr = e.target.closest('tr');
  if (!tr) return;
  const id = Number(tr.dataset.id);
  const ids = visible().map((a) => a.userId);
  if (e.shiftKey && lastClicked != null) {
    const [i, j] = [ids.indexOf(lastClicked), ids.indexOf(id)].sort((x, y) => x - y);
    ids.slice(i, j + 1).forEach((x) => selected.add(x));
  } else if (e.ctrlKey || e.target.type === 'checkbox') {
    selected.has(id) ? selected.delete(id) : selected.add(id);
  } else {
    selected.clear();
    selected.add(id);
  }
  lastClicked = id;
  render();
});

$('sel-all').onchange = (e) => {
  selected.clear();
  if (e.target.checked) visible().forEach((a) => selected.add(a.userId));
  render();
};
$('search').oninput = render;

// ───────────────────────── menú contextual

$('rows').addEventListener('contextmenu', (e) => {
  const tr = e.target.closest('tr');
  if (!tr) return;
  e.preventDefault();
  const id = Number(tr.dataset.id);
  if (!selected.has(id)) { selected.clear(); selected.add(id); render(); }
  const m = $('ctx');
  m.classList.remove('hidden');
  m.style.left = `${Math.min(e.clientX, innerWidth - 220)}px`;
  m.style.top = `${Math.min(e.clientY, innerHeight - 120)}px`;
});
document.addEventListener('click', () => $('ctx').classList.add('hidden'));

$('ctx').addEventListener('click', async (e) => {
  const act = e.target.dataset.act;
  const ids = [...selected];
  try {
    if (act === 'remove') {
      for (const id of ids) await invoke('account_remove', { userId: id });
      selected.clear();
    } else if (act === 'afk') {
      for (const id of ids) {
        const a = accounts.find((x) => x.userId === id);
        await invoke('account_update', { view: { userId: id, antiAfk: !a.antiAfk } });
      }
    } else if (act === 'edit' && ids.length === 1) {
      const a = accounts.find((x) => x.userId === ids[0]);
      const alias = prompt('Alias', a.alias);
      if (alias === null) return;
      const group = prompt('Grupo', a.group);
      if (group === null) return;
      await invoke('account_update', { view: { userId: a.userId, alias, group: group || 'Default' } });
    }
  } catch (err) { toast('error', err); }
});

// ───────────────────────── agregar

function closeAdd() { $('dlg-add').classList.add('hidden'); $('add-cookie').value = ''; $('add-pass').value = ''; $('add-err').textContent = ''; }
$('btn-add').onclick = () => { $('dlg-add').classList.remove('hidden'); $('add-cookie').focus(); };
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

$('btn-join').onclick = () => {
  const ids = visible().map((a) => a.userId).filter((id) => selected.has(id));
  if (!ids.length) return toast('error', 'Selecciona al menos una cuenta');
  const placeId = Number($('place-id').value.trim());
  if (!placeId) return toast('error', 'Escribe un Place ID');
  const args = { placeId, jobId: $('job-id').value.trim(), followUser: $('follow').checked, joinVip: false };
  if (ids.length === 1) {
    invoke('join_server', { userId: ids[0], ...args }).catch((e) => toast('error', e));
  } else {
    invoke('launch_many', { ids, ...args });
  }
};
$('btn-cancel').onclick = () => invoke('launch_cancel');

// ───────────────────────── eventos del backend

listen('accounts', (e) => { accounts = e.payload; render(); });
listen('toast', (e) => toast(e.payload.kind, e.payload.message));
listen('launch-progress', (e) => {
  const p = e.payload;
  $('btn-cancel').classList.toggle('hidden', p.done);
  $('progress').textContent = p.done ? '' : `Lanzando ${p.current}/${p.total}: ${p.username}`;
});
listen('vip-privacy', (e) => toast('info', `${e.payload.username}: la privacidad de servidores privados no está en "Todos"`));

async function loadMain() {
  accounts = await invoke('accounts_list');
  render();
  const ver = await invoke('roblox_version');
  $('rbx-ver').textContent = ver ? `Roblox ${ver}` : 'Roblox no encontrado';
  invoke('accounts_refresh_avatars', { ids: accounts.map((a) => a.userId) });
  invoke('accounts_update_presence', { ids: accounts.map((a) => a.userId) });
}

invoke('vault_startup').then(showVault).catch((e) => toast('error', e));
