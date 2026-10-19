//! Perfiles de lanzamiento: un grupo de cuentas, un juego y un retraso que se lanzan con un clic.

use once_cell::sync::Lazy;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use crate::ops::{self, CmdResult, JoinRequest};
use crate::util::{data_path, write_atomic};

const FILE: &str = "profiles.json";

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Profile {
    pub id: i64,
    pub name: String,
    /// En el orden en que se lanzan.
    pub user_ids: Vec<i64>,
    pub place_id: i64,
    /// Job ID, enlace de servidor privado o vacío (servidor según los ajustes).
    pub job_id: String,
    /// Segundos entre cuentas; `None` usa el ajuste general.
    pub delay_secs: Option<u64>,
}

static PROFILES: Lazy<RwLock<Vec<Profile>>> = Lazy::new(|| {
    let list = std::fs::read_to_string(data_path(FILE))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    RwLock::new(list)
});

fn save(list: &[Profile]) -> CmdResult<()> {
    let json = serde_json::to_vec_pretty(list).map_err(ops::err)?;
    write_atomic(&data_path(FILE), &json).map_err(ops::err)
}

pub fn list() -> Vec<Profile> {
    PROFILES.read().clone()
}

/// Crea el perfil si `id` es 0; si no, lo reemplaza.
pub fn upsert(mut p: Profile) -> CmdResult<Profile> {
    p.name = p.name.trim().to_string();
    p.job_id = p.job_id.trim().to_string();
    if p.name.is_empty() {
        return Err("El perfil necesita un nombre".into());
    }
    if p.place_id <= 0 {
        return Err("El perfil necesita un Place ID".into());
    }
    if p.user_ids.is_empty() {
        return Err("El perfil necesita al menos una cuenta".into());
    }
    let mut list = PROFILES.write();
    if p.id == 0 {
        p.id = chrono::Local::now().timestamp_millis();
        list.push(p.clone());
    } else {
        let slot = list.iter_mut().find(|x| x.id == p.id).ok_or("Perfil no encontrado")?;
        *slot = p.clone();
    }
    save(&list)?;
    Ok(p)
}

pub fn delete(id: i64) -> CmdResult<()> {
    let mut list = PROFILES.write();
    list.retain(|p| p.id != id);
    save(&list)
}

pub async fn launch(id: i64) -> CmdResult<()> {
    let p = PROFILES.read().iter().find(|p| p.id == id).cloned().ok_or("Perfil no encontrado")?;
    let req = JoinRequest { place_id: p.place_id, job_id: p.job_id, follow_user: false, join_vip: false };
    tauri::async_runtime::spawn(ops::launch_many_with(p.user_ids, req, p.delay_secs, false));
    Ok(())
}
