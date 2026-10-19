//! Revisión de cuentas: estado de la cookie y Robux de todas las cuentas, con el cambio desde la revisión anterior.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use once_cell::sync::Lazy;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use crate::ops::CmdResult;
use crate::roblox::api;
use crate::state::st;
use crate::util::{data_path, write_atomic};

const FILE: &str = "health.json";
/// Pausa entre cuentas para no disparar el límite de peticiones de Roblox.
const PACE: Duration = Duration::from_millis(700);

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum CookieState {
    Ok,
    /// Roblox rechaza la cookie: hay que volver a iniciar sesión.
    Expired,
    /// La cookie es válida pero pertenece a otra cuenta.
    Mismatch,
    /// No se pudo comprobar (red, límite de peticiones…).
    Unknown,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthRow {
    pub user_id: i64,
    pub state: CookieState,
    pub robux: Option<i64>,
    /// Robux de la revisión anterior, para mostrar el cambio.
    pub prev_robux: Option<i64>,
    pub detail: String,
    pub checked_at: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    current: usize,
    total: usize,
    row: Option<HealthRow>,
    done: bool,
}

static LAST: Lazy<RwLock<HashMap<i64, HealthRow>>> = Lazy::new(|| {
    let map = std::fs::read_to_string(data_path(FILE))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    RwLock::new(map)
});
static RUNNING: AtomicBool = AtomicBool::new(false);
static CANCEL: AtomicBool = AtomicBool::new(false);

pub fn last() -> Vec<HealthRow> {
    LAST.read().values().cloned().collect()
}

pub fn cancel() {
    CANCEL.store(true, Ordering::SeqCst);
}

/// Revisa las cuentas indicadas una por una. El avance llega por el evento `health-progress`.
pub async fn check(ids: Vec<i64>) -> CmdResult<()> {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return Err("Ya hay una revisión en curso".into());
    }
    CANCEL.store(false, Ordering::SeqCst);
    let list: Vec<(i64, String)> = st()
        .accounts
        .read()
        .iter()
        .filter(|a| ids.contains(&a.user_id))
        .map(|a| (a.user_id, a.security_token.clone()))
        .collect();
    let total = list.len();
    tauri::async_runtime::spawn(async move {
        for (i, (id, cookie)) in list.into_iter().enumerate() {
            if CANCEL.load(Ordering::SeqCst) {
                break;
            }
            let row = check_one(id, &cookie).await;
            LAST.write().insert(id, row.clone());
            st().emit("health-progress", Progress { current: i + 1, total, row: Some(row), done: false });
            tokio::time::sleep(PACE).await;
        }
        if let Ok(json) = serde_json::to_vec_pretty(&*LAST.read()) {
            let _ = write_atomic(&data_path(FILE), &json);
        }
        RUNNING.store(false, Ordering::SeqCst);
        st().emit("health-progress", Progress { current: total, total, row: None, done: true });
    });
    Ok(())
}

async fn check_one(id: i64, cookie: &str) -> HealthRow {
    let prev_robux = LAST.read().get(&id).and_then(|r| r.robux.or(r.prev_robux));
    let (state, detail) = match api().authenticated_user(cookie).await {
        Ok(Some(u)) if u.id == id => (CookieState::Ok, String::new()),
        Ok(Some(u)) => (CookieState::Mismatch, u.name),
        Ok(None) => (CookieState::Expired, String::new()),
        Err(e) => (CookieState::Unknown, e.to_string()),
    };
    if state != CookieState::Unknown {
        let needs = state != CookieState::Ok;
        let changed = st()
            .with_account(id, |a| std::mem::replace(&mut a.rt.needs_login, needs) != needs)
            .unwrap_or(false);
        if changed {
            st().emit_accounts();
        }
    }
    let robux = if state == CookieState::Ok { api().robux(cookie, id).await } else { None };
    HealthRow { user_id: id, state, robux, prev_robux, detail, checked_at: chrono::Local::now().to_rfc3339() }
}
