use std::collections::BTreeMap;

use chrono::{DateTime, Local, Utc};
use serde::{Deserialize, Serialize};

use crate::roblox::Presence;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Account {
    pub user_id: i64,
    pub username: String,
    pub display_name: String,
    pub alias: String,
    pub description: String,
    pub password: String,
    pub group: String,
    pub security_token: String,
    pub fields: BTreeMap<String, String>,
    pub last_use: DateTime<Local>,
    pub last_attempted_refresh: DateTime<Local>,
    pub added: DateTime<Local>,
    pub browser_tracker_id: String,
    pub anti_afk: bool,
    pub valid: bool,

    #[serde(skip)]
    pub rt: Runtime,
}

/// Estado que solo vive mientras la app está abierta.
#[derive(Clone, Debug, Default)]
pub struct Runtime {
    pub needs_login: bool,
    pub pin_unlocked_until: Option<DateTime<Local>>,
    pub presence: Option<Presence>,
    pub game_session_start: Option<DateTime<Local>>,
    pub anti_afk_misses: u8,
    pub avatar_url: Option<String>,
}

impl Default for Account {
    fn default() -> Self {
        let epoch = DateTime::<Utc>::UNIX_EPOCH.with_timezone(&Local);
        Self {
            user_id: 0,
            username: String::new(),
            display_name: String::new(),
            alias: String::new(),
            description: String::new(),
            password: String::new(),
            group: "Default".into(),
            security_token: String::new(),
            fields: BTreeMap::new(),
            last_use: Local::now(),
            last_attempted_refresh: epoch,
            added: Local::now(),
            browser_tracker_id: String::new(),
            anti_afk: false,
            valid: true,
            rt: Runtime::default(),
        }
    }
}

impl Account {
    pub fn field(&self, name: &str) -> String {
        self.fields.get(name).cloned().unwrap_or_default()
    }

    pub fn ensure_tracker_id(&mut self) -> String {
        if self.browser_tracker_id.is_empty() {
            use rand::Rng;
            let mut r = rand::thread_rng();
            self.browser_tracker_id = format!("{}{}", r.gen_range(100000..175000), r.gen_range(100000..900000));
        }
        self.browser_tracker_id.clone()
    }

    pub fn to_view(&self) -> AccountView {
        let p = self.rt.presence.as_ref();
        AccountView {
            user_id: self.user_id,
            username: self.username.clone(),
            display_name: self.display_name.clone(),
            alias: self.alias.clone(),
            description: self.description.clone(),
            group: self.group.clone(),
            has_password: !self.password.is_empty(),
            last_use: self.last_use.to_rfc3339(),
            added: self.added.to_rfc3339(),
            anti_afk: self.anti_afk,
            needs_login: self.rt.needs_login,
            valid: self.valid,
            presence_type: p.map(|p| p.user_presence_type).unwrap_or(-1),
            presence_location: p.and_then(|p| p.last_location.clone()).unwrap_or_default(),
            place_id: p.and_then(|p| p.place_id),
            game_id: p.and_then(|p| p.game_id.clone()),
            session_start: self.rt.game_session_start.map(|d| d.to_rfc3339()),
            avatar_url: self.rt.avatar_url.clone(),
            saved_place_id: self.field("SavedPlaceId"),
            saved_job_id: self.field("SavedJobId"),
            fields: self.fields.clone(),
            age_days: (Local::now() - self.added).num_days(),
        }
    }
}

/// Lo que ve la interfaz. Nunca incluye la cookie ni la contraseña.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountView {
    pub user_id: i64,
    pub username: String,
    pub display_name: String,
    pub alias: String,
    pub description: String,
    pub group: String,
    pub has_password: bool,
    pub last_use: String,
    pub added: String,
    pub anti_afk: bool,
    pub needs_login: bool,
    pub valid: bool,
    pub presence_type: i32,
    pub presence_location: String,
    pub place_id: Option<i64>,
    pub game_id: Option<String>,
    pub session_start: Option<String>,
    pub avatar_url: Option<String>,
    pub saved_place_id: String,
    pub saved_job_id: String,
    pub fields: BTreeMap<String, String>,
    pub age_days: i64,
}

/// Campos editables que el frontend puede enviar para actualizar una cuenta.
#[derive(Debug, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AccountPatch {
    pub user_id: i64,
    pub alias: Option<String>,
    pub description: Option<String>,
    pub group: Option<String>,
    pub password: Option<String>,
    pub anti_afk: Option<bool>,
    pub fields: Option<BTreeMap<String, String>>,
}

impl Default for AccountPatch {
    fn default() -> Self {
        Self { user_id: 0, alias: None, description: None, group: None, password: None, anti_afk: None, fields: None }
    }
}

impl AccountPatch {
    pub fn apply(self, a: &mut Account) {
        if let Some(v) = self.alias { a.alias = v; }
        if let Some(v) = self.description { a.description = v; }
        if let Some(v) = self.group { a.group = v; }
        if let Some(v) = self.password { a.password = v; }
        if let Some(v) = self.anti_afk { a.anti_afk = v; }
        if let Some(v) = self.fields { a.fields = v; }
    }
}

/// Formato de las versiones anteriores (AccountData.json), solo para migrar.
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct LegacyAccount {
    #[serde(rename = "Valid")]
    pub valid: bool,
    #[serde(rename = "SecurityToken")]
    pub security_token: Option<String>,
    #[serde(rename = "Username")]
    pub username: Option<String>,
    #[serde(rename = "LastUse")]
    pub last_use: Option<String>,
    #[serde(rename = "Alias")]
    pub alias: Option<String>,
    #[serde(rename = "Description")]
    pub description: Option<String>,
    #[serde(rename = "Password")]
    pub password: Option<String>,
    #[serde(rename = "Group")]
    pub group: Option<String>,
    #[serde(rename = "UserID")]
    pub user_id: i64,
    #[serde(rename = "Fields")]
    pub fields: Option<BTreeMap<String, String>>,
    #[serde(rename = "BrowserTrackerID")]
    pub browser_tracker_id: Option<String>,
    #[serde(rename = "AntiAfkEnabled")]
    pub anti_afk: bool,
}

impl Default for LegacyAccount {
    fn default() -> Self {
        Self {
            valid: true,
            security_token: None,
            username: None,
            last_use: None,
            alias: None,
            description: None,
            password: None,
            group: None,
            user_id: 0,
            fields: None,
            browser_tracker_id: None,
            anti_afk: false,
        }
    }
}

impl From<LegacyAccount> for Account {
    fn from(l: LegacyAccount) -> Self {
        let last_use = l
            .last_use
            .as_deref()
            .and_then(|s| {
                DateTime::parse_from_rfc3339(s)
                    .map(|d| d.with_timezone(&Local))
                    .ok()
                    .or_else(|| {
                        chrono::NaiveDateTime::parse_from_str(s.split('.').next().unwrap_or(s), "%Y-%m-%dT%H:%M:%S")
                            .ok()
                            .and_then(|n| n.and_local_timezone(Local).single())
                    })
            })
            .unwrap_or_else(Local::now);
        Account {
            user_id: l.user_id,
            username: l.username.unwrap_or_default(),
            alias: l.alias.unwrap_or_default(),
            description: l.description.unwrap_or_default(),
            password: l.password.unwrap_or_default(),
            group: l.group.filter(|g| !g.is_empty()).unwrap_or_else(|| "Default".into()),
            security_token: l.security_token.unwrap_or_default(),
            fields: l.fields.unwrap_or_default(),
            last_use,
            browser_tracker_id: l.browser_tracker_id.unwrap_or_default(),
            anti_afk: l.anti_afk,
            valid: l.valid,
            ..Default::default()
        }
    }
}

/// Extrae cookies .ROBLOSECURITY de un texto arbitrario (pegado, arrastrado, archivo...).
pub fn extract_cookies(text: &str) -> Vec<String> {
    use once_cell::sync::Lazy;
    static RE: Lazy<regex::Regex> = Lazy::new(|| {
        regex::Regex::new(r"_\|WARNING:-DO-NOT-SHARE-THIS\.--Sharing-this-will-allow-someone-to-log-in-as-you-and-to-steal-your-ROBUX-and-items\.\|_?[A-Za-z0-9]+").unwrap()
    });
    let mut out: Vec<String> = RE.find_iter(text).map(|m| m.as_str().to_string()).collect();
    if out.is_empty() {
        // Cookies sin la advertencia (solo el valor hexadecimal largo)
        for line in text.lines().map(str::trim) {
            if line.len() > 100 && line.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '|' || c == '-' || c == '.') {
                out.push(line.to_string());
            }
        }
    }
    out.dedup();
    out
}

/// Ordena como antes: grupos con número al inicio ("1 Farm") se ordenan por ese número.
pub fn group_sort_key(group: &str) -> (u64, String) {
    let digits: String = group.chars().take_while(|c| c.is_ascii_digit()).collect();
    let n = digits.parse::<u64>().unwrap_or(u64::MAX);
    (n, group.to_lowercase())
}
