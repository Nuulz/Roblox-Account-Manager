use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

use once_cell::sync::{Lazy, OnceCell};
use parking_lot::{Mutex, RwLock};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::account::{Account, AccountView};
use crate::crypto::{MasterKey, VaultMode};
use crate::games::GamesData;
use crate::settings::Settings;

#[derive(Default)]
pub struct Vault {
    pub mode: Option<VaultMode>,
    pub key: Option<MasterKey>,
}

pub struct AppState {
    pub app: OnceCell<AppHandle>,
    pub accounts: RwLock<Vec<Account>>,
    pub settings: RwLock<Settings>,
    pub vault: Mutex<Vault>,
    pub unlocked: AtomicBool,
    pub games: RwLock<GamesData>,
    /// Cada lanzamiento múltiple guarda la generación con la que empezó; cancelar = incrementar.
    pub launch_gen: AtomicU64,
    pub launch_next: tokio::sync::Notify,
    pub launching: AtomicBool,
    pub save_lock: Mutex<()>,
    pub started: Instant,
    pub roblox_version: RwLock<Option<String>>,
}

pub static STATE: Lazy<AppState> = Lazy::new(|| AppState {
    app: OnceCell::new(),
    accounts: RwLock::new(Vec::new()),
    settings: RwLock::new(Settings::default()),
    vault: Mutex::new(Vault::default()),
    unlocked: AtomicBool::new(false),
    games: RwLock::new(GamesData::default()),
    launch_gen: AtomicU64::new(0),
    launch_next: tokio::sync::Notify::new(),
    launching: AtomicBool::new(false),
    save_lock: Mutex::new(()),
    started: Instant::now(),
    roblox_version: RwLock::new(None),
});

pub fn st() -> &'static AppState {
    &STATE
}

#[derive(Clone, Serialize)]
pub struct Toast {
    pub kind: &'static str,
    pub message: String,
}

impl AppState {
    pub fn emit<S: Serialize + Clone>(&self, event: &str, payload: S) {
        if let Some(app) = self.app.get() {
            let _ = app.emit(event, payload);
        }
    }

    pub fn toast(&self, kind: &'static str, message: impl Into<String>) {
        let message = message.into();
        log::info!("[{kind}] {message}");
        self.emit("toast", Toast { kind, message });
    }

    pub fn account_views(&self) -> Vec<AccountView> {
        self.accounts.read().iter().map(Account::to_view).collect()
    }

    pub fn emit_accounts(&self) {
        self.emit("accounts", self.account_views());
    }

    pub fn is_unlocked(&self) -> bool {
        self.unlocked.load(Ordering::SeqCst)
    }

    pub fn cookie_of(&self, user_id: i64) -> Option<String> {
        self.accounts
            .read()
            .iter()
            .find(|a| a.user_id == user_id)
            .map(|a| a.security_token.clone())
    }

    /// Cookie de la cuenta usada más recientemente (para peticiones que requieren estar logueado).
    pub fn any_cookie(&self) -> Option<String> {
        self.accounts
            .read()
            .iter()
            .filter(|a| a.valid && !a.rt.needs_login)
            .max_by_key(|a| a.last_use)
            .map(|a| a.security_token.clone())
    }

    pub fn with_account<R>(&self, user_id: i64, f: impl FnOnce(&mut Account) -> R) -> Option<R> {
        let mut list = self.accounts.write();
        list.iter_mut().find(|a| a.user_id == user_id).map(f)
    }

    pub fn find_by_name_or_id(&self, key: &str) -> Option<Account> {
        self.accounts
            .read()
            .iter()
            .find(|a| a.username.eq_ignore_ascii_case(key) || a.user_id.to_string() == key)
            .cloned()
    }

    pub fn cancel_launching(&self) {
        self.launch_gen.fetch_add(1, Ordering::SeqCst);
        self.launch_next.notify_waiters();
    }
}
