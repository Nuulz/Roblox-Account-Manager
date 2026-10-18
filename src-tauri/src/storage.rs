//! Carga/guardado del archivo de cuentas y migración desde versiones anteriores.

use std::sync::atomic::Ordering;
use std::time::{Duration, SystemTime};

use serde::Serialize;

use crate::account::{Account, LegacyAccount};
use crate::crypto::{self, MasterKey, VaultMode};
use crate::state::st;
use crate::util::{data_path, write_atomic};

pub const ACCOUNTS_FILE: &str = "accounts.dat";
const LEGACY_FILE: &str = "AccountData.json";

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum VaultStatus {
    /// Cuentas cargadas y listas.
    Ready,
    /// Hay que escribir la contraseña maestra.
    Locked,
    /// Primera vez (o migración): elegir cómo cifrar.
    Setup,
    /// Archivo antiguo protegido con contraseña: escribirla para migrar.
    LegacyLocked,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultInfo {
    pub status: VaultStatus,
    pub mode: Option<VaultMode>,
    pub migrated: usize,
}

pub fn vault_info(status: VaultStatus, migrated: usize) -> VaultInfo {
    VaultInfo { status, mode: st().vault.lock().mode, migrated }
}

fn set_accounts(list: Vec<Account>) {
    let mut list = list;
    list.retain(|a| a.user_id > 0 && !a.security_token.is_empty());
    let mut seen = std::collections::HashSet::new();
    list.retain(|a| seen.insert(a.user_id));
    *st().accounts.write() = list;
}

/// Decide qué mostrar al arrancar.
pub fn startup() -> VaultInfo {
    let path = data_path(ACCOUNTS_FILE);
    if let Ok(data) = std::fs::read(&path) {
        match crypto::header_mode(&data) {
            Some(VaultMode::Password) => {
                st().vault.lock().mode = Some(VaultMode::Password);
                return vault_info(VaultStatus::Locked, 0);
            }
            Some(mode) => match crypto::open(&data, None) {
                Ok(plain) => {
                    load_json(&plain);
                    st().vault.lock().mode = Some(mode);
                    st().unlocked.store(true, Ordering::SeqCst);
                    return vault_info(VaultStatus::Ready, 0);
                }
                Err(e) => {
                    log::error!("No se pudo abrir {ACCOUNTS_FILE}: {e}");
                    let _ = std::fs::copy(&path, path.with_extension("dat.unreadable"));
                }
            },
            None => log::error!("{ACCOUNTS_FILE} tiene un formato desconocido"),
        }
    }

    // Migración desde versiones anteriores
    let legacy = data_path(LEGACY_FILE);
    if let Ok(data) = std::fs::read(&legacy) {
        if !data.is_empty() {
            if crypto::legacy_is_password_locked(&data) {
                return vault_info(VaultStatus::LegacyLocked, 0);
            }
            match crypto::legacy_open_unlocked(&data).and_then(|p| import_legacy_json(&p)) {
                Ok(n) => return vault_info(VaultStatus::Setup, n),
                Err(e) => log::error!("No se pudo migrar {LEGACY_FILE}: {e}"),
            }
        }
    }
    vault_info(VaultStatus::Setup, 0)
}

fn load_json(plain: &[u8]) {
    match serde_json::from_slice::<Vec<Account>>(plain) {
        Ok(list) => set_accounts(list),
        Err(e) => log::error!("JSON de cuentas inválido: {e}"),
    }
}

pub fn import_legacy_json(plain: &[u8]) -> Result<usize, String> {
    let text = String::from_utf8_lossy(plain);
    let list: Vec<LegacyAccount> =
        serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|e| format!("JSON antiguo inválido: {e}"))?;
    let mut accounts: Vec<Account> = list.into_iter().map(Account::from).collect();
    accounts.retain(|a| a.user_id > 0);
    let n = accounts.len();
    let mut current = st().accounts.read().clone();
    for a in accounts {
        if let Some(existing) = current.iter_mut().find(|x| x.user_id == a.user_id) {
            *existing = a;
        } else {
            current.push(a);
        }
    }
    set_accounts(current);
    Ok(n)
}

pub fn unlock(password: &str) -> Result<VaultInfo, String> {
    let data = std::fs::read(data_path(ACCOUNTS_FILE)).map_err(|e| e.to_string())?;
    let salt = crypto::password_salt(&data).ok_or("El archivo no está protegido con contraseña")?;
    let key = MasterKey::derive(password, Some(salt))?;
    let plain = crypto::open(&data, Some(&key))?;
    load_json(&plain);
    {
        let mut v = st().vault.lock();
        v.mode = Some(VaultMode::Password);
        v.key = Some(key);
    }
    st().unlocked.store(true, Ordering::SeqCst);
    Ok(vault_info(VaultStatus::Ready, 0))
}

pub fn unlock_legacy(password: &str) -> Result<VaultInfo, String> {
    let data = std::fs::read(data_path(LEGACY_FILE)).map_err(|e| e.to_string())?;
    let plain = crypto::legacy_open_password(&data, password)?;
    let n = import_legacy_json(&plain)?;
    Ok(vault_info(VaultStatus::Setup, n))
}

/// Elige (o cambia) el modo de cifrado y guarda inmediatamente.
pub fn configure(mode: VaultMode, password: Option<&str>) -> Result<VaultInfo, String> {
    let key = match mode {
        VaultMode::Password => {
            let p = password.unwrap_or_default();
            if p.chars().count() < 4 {
                return Err("La contraseña debe tener al menos 4 caracteres".into());
            }
            Some(MasterKey::derive(p, None)?)
        }
        _ => None,
    };
    {
        let mut v = st().vault.lock();
        v.mode = Some(mode);
        v.key = key;
    }
    st().unlocked.store(true, Ordering::SeqCst);
    save_now()?;
    // Tras migrar, el archivo antiguo se renombra para no volver a importarlo.
    let legacy = data_path(LEGACY_FILE);
    if legacy.exists() {
        let _ = std::fs::rename(&legacy, data_path("AccountData.json.migrated"));
    }
    Ok(vault_info(VaultStatus::Ready, 0))
}

pub fn verify_password(password: &str) -> bool {
    let Ok(data) = std::fs::read(data_path(ACCOUNTS_FILE)) else { return false };
    let Some(salt) = crypto::password_salt(&data) else { return false };
    MasterKey::derive(password, Some(salt)).map(|k| crypto::open(&data, Some(&k)).is_ok()).unwrap_or(false)
}

pub fn save_now() -> Result<(), String> {
    let s = st();
    if !s.is_unlocked() {
        return Ok(());
    }
    let _guard = s.save_lock.lock();
    let (mode, key) = {
        let v = s.vault.lock();
        match v.mode {
            Some(m) => (m, v.key.clone()),
            None => return Ok(()),
        }
    };
    let mut list = s.accounts.read().clone();
    if !s.settings.read().general.save_passwords {
        for a in &mut list {
            a.password.clear();
        }
    }
    let json = serde_json::to_vec(&list).map_err(|e| e.to_string())?;
    let sealed = crypto::seal(&json, mode, key.as_ref())?;
    let path = data_path(ACCOUNTS_FILE);

    // Copia de seguridad cada 8 horas
    let backup = data_path("accounts.dat.bak");
    let stale = std::fs::metadata(&backup)
        .and_then(|m| m.modified())
        .map(|t| SystemTime::now().duration_since(t).unwrap_or_default() > Duration::from_secs(8 * 3600))
        .unwrap_or(true);
    if stale && path.exists() {
        let _ = std::fs::copy(&path, &backup);
    }
    write_atomic(&path, &sealed).map_err(|e| format!("No se pudo guardar las cuentas: {e}"))
}

/// Guarda en segundo plano (no bloquea la interfaz).
pub fn save() {
    std::thread::spawn(|| {
        if let Err(e) = save_now() {
            st().toast("error", e);
        }
    });
}

/// Exporta las cuentas sin cifrar (el usuario lo pide explícitamente).
pub fn export_plain(path: &std::path::Path) -> Result<usize, String> {
    let list = st().accounts.read().clone();
    let json = serde_json::to_vec_pretty(&list).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())?;
    Ok(list.len())
}

/// Importa un archivo exportado o un AccountData.json antiguo.
pub fn import_file(path: &std::path::Path, password: Option<&str>) -> Result<usize, String> {
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    let plain = if crypto::header_mode(&data).is_some() {
        let key = match (crypto::password_salt(&data), password) {
            (Some(salt), Some(p)) => Some(MasterKey::derive(p, Some(salt))?),
            (Some(_), None) => return Err("Ese archivo necesita su contraseña".into()),
            _ => None,
        };
        crypto::open(&data, key.as_ref())?
    } else if crypto::legacy_is_password_locked(&data) {
        crypto::legacy_open_password(&data, password.ok_or("Ese archivo necesita su contraseña")?)?
    } else if let Ok(p) = crypto::legacy_open_unlocked(&data) {
        p
    } else {
        data
    };
    let n = if let Ok(list) = serde_json::from_slice::<Vec<Account>>(&plain) {
        if list.iter().all(|a| a.user_id > 0) && !list.is_empty() {
            let n = list.len();
            let mut current = st().accounts.read().clone();
            for a in list {
                match current.iter_mut().find(|x| x.user_id == a.user_id) {
                    Some(e) => *e = a,
                    None => current.push(a),
                }
            }
            set_accounts(current);
            n
        } else {
            import_legacy_json(&plain)?
        }
    } else {
        import_legacy_json(&plain)?
    };
    save();
    st().emit_accounts();
    Ok(n)
}
