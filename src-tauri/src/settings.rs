use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

use crate::util::{data_path, write_atomic};

pub const SETTINGS_FILE: &str = "settings.json";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct GeneralSettings {
    pub check_for_updates: bool,
    pub account_join_delay: u64,
    pub async_join: bool,
    pub save_passwords: bool,
    pub server_region_format: String,
    pub ip_api_link: String,
    pub max_recent_games: usize,
    pub shuffle_job_id: bool,
    pub shuffle_lowest_server: bool,
    pub shuffle_page_count: u32,
    pub auto_cookie_refresh: bool,
    pub auto_close_last_process: bool,
    pub show_presence: bool,
    pub presence_update_minutes: u64,
    pub session_check_minutes: u64,
    pub unlock_fps: bool,
    pub max_fps: u32,
    pub custom_client_settings: String,
    pub enable_multi_roblox: bool,
    pub lock_roblox_cookies: bool,
    pub hide_usernames: bool,
    pub disable_images: bool,
    pub use_proxies: bool,
    pub is_teleport: bool,
    pub anti_afk_min_minutes: u64,
    pub anti_afk_max_minutes: u64,
    pub auto_rejoin_max_per_hour: u32,
    pub saved_place_id: String,
    pub saved_job_id: String,
    pub saved_follow_user: String,
    pub dev_mode: bool,
    pub show_groups: bool,
}

impl Default for GeneralSettings {
    fn default() -> Self {
        Self {
            check_for_updates: true,
            account_join_delay: 8,
            async_join: false,
            save_passwords: true,
            server_region_format: "<city>, <countryCode>".into(),
            ip_api_link: "http://ip-api.com/json/<ip>".into(),
            max_recent_games: 8,
            shuffle_job_id: false,
            shuffle_lowest_server: false,
            shuffle_page_count: 5,
            auto_cookie_refresh: true,
            auto_close_last_process: true,
            show_presence: true,
            presence_update_minutes: 2,
            session_check_minutes: 10,
            unlock_fps: false,
            max_fps: 120,
            custom_client_settings: String::new(),
            enable_multi_roblox: false,
            lock_roblox_cookies: true,
            hide_usernames: false,
            disable_images: false,
            use_proxies: false,
            is_teleport: false,
            anti_afk_min_minutes: 10,
            anti_afk_max_minutes: 16,
            auto_rejoin_max_per_hour: 5,
            saved_place_id: String::new(),
            saved_job_id: String::new(),
            saved_follow_user: String::new(),
            dev_mode: false,
            show_groups: true,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct WebServerSettings {
    pub enabled: bool,
    pub port: u16,
    pub password: String,
    pub every_request_requires_password: bool,
    pub allow_external_connections: bool,
    pub allow_get_cookie: bool,
    pub allow_get_accounts: bool,
    pub allow_launch_account: bool,
    pub allow_account_editing: bool,
}

impl Default for WebServerSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            port: 7963,
            password: String::new(),
            every_request_requires_password: false,
            allow_external_connections: false,
            allow_get_cookie: false,
            allow_get_accounts: false,
            allow_launch_account: false,
            allow_account_editing: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct NexusSettings {
    pub start_on_launch: bool,
    pub port: u16,
    pub allow_external_connections: bool,
    pub relaunch_delay: f64,
    pub launcher_delay: u64,
    pub save_output: bool,
    pub auto_minimize_enabled: bool,
    pub auto_minimize_interval: u64,
    pub auto_close_enabled: bool,
    /// minutos
    pub auto_close_interval: u64,
    /// 0 = por instancia, 1 = global
    pub auto_close_type: u8,
    pub max_instances: u32,
    pub internet_check: bool,
    pub use_presence: bool,
}

impl Default for NexusSettings {
    fn default() -> Self {
        Self {
            start_on_launch: false,
            port: 5242,
            allow_external_connections: false,
            relaunch_delay: 60.0,
            launcher_delay: 9,
            save_output: false,
            auto_minimize_enabled: false,
            auto_minimize_interval: 15,
            auto_close_enabled: false,
            auto_close_interval: 60,
            auto_close_type: 0,
            max_instances: 99,
            internet_check: true,
            use_presence: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct WatcherSettings {
    pub enabled: bool,
    pub exit_on_beta: bool,
    pub exit_if_no_connection: bool,
    pub no_connection_timeout: u64,
    pub save_window_positions: bool,
    pub verify_data_model: bool,
    pub ignore_existing_processes: bool,
    pub close_if_memory_low: bool,
    pub memory_low_value: u64,
    pub close_if_window_title: bool,
    pub expected_window_title: String,
    pub scan_interval: u64,
    pub read_interval: u64,
}

impl Default for WatcherSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            exit_on_beta: false,
            exit_if_no_connection: false,
            no_connection_timeout: 60,
            save_window_positions: false,
            verify_data_model: true,
            ignore_existing_processes: true,
            close_if_memory_low: false,
            memory_low_value: 200,
            close_if_window_title: false,
            expected_window_title: "Roblox".into(),
            scan_interval: 6,
            read_interval: 250,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ThemeSettings {
    pub accent: String,
    pub background: String,
    pub surface: String,
    pub surface_alt: String,
    pub border: String,
    pub text: String,
    pub muted: String,
    pub radius: u32,
    pub compact: bool,
}

impl Default for ThemeSettings {
    fn default() -> Self {
        Self {
            accent: "#3b82f6".into(),
            background: "#0f1014".into(),
            surface: "#16171d".into(),
            surface_alt: "#1d1f27".into(),
            border: "#272a35".into(),
            text: "#e7e9ee".into(),
            muted: "#8b90a0".into(),
            radius: 10,
            compact: false,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub general: GeneralSettings,
    pub web_server: WebServerSettings,
    pub nexus: NexusSettings,
    pub watcher: WatcherSettings,
    pub theme: ThemeSettings,
    /// Respuestas recordadas de diálogos "no volver a preguntar".
    pub prompts: BTreeMap<String, bool>,
}

impl Settings {
    pub fn load() -> Self {
        let path = data_path(SETTINGS_FILE);
        if let Ok(text) = std::fs::read_to_string(&path) {
            match serde_json::from_str(text.trim_start_matches('\u{feff}')) {
                Ok(s) => return s,
                Err(e) => log::error!("settings.json inválido, usando valores por defecto: {e}"),
            }
        }
        let mut s = Settings::default();
        let legacy = data_path("RAMSettings.ini");
        if legacy.exists() {
            if let Ok(text) = std::fs::read_to_string(&legacy) {
                s.import_legacy_ini(&text);
                log::info!("Ajustes importados desde RAMSettings.ini");
            }
        }
        s.save();
        s
    }

    pub fn save(&self) {
        if let Ok(json) = serde_json::to_vec_pretty(self) {
            if let Err(e) = write_atomic(&data_path(SETTINGS_FILE), &json) {
                log::error!("No se pudo guardar settings.json: {e}");
            }
        }
    }

    /// Importa los valores conocidos del RAMSettings.ini de versiones anteriores.
    pub fn import_legacy_ini(&mut self, text: &str) {
        let ini = parse_ini(text);
        let get = |sec: &str, key: &str| ini.get(sec).and_then(|s| s.get(key)).cloned();
        let b = |sec: &str, key: &str, dst: &mut bool| {
            if let Some(v) = get(sec, key) {
                *dst = v.eq_ignore_ascii_case("true");
            }
        };
        macro_rules! num {
            ($sec:expr, $key:expr, $dst:expr) => {
                if let Some(v) = get($sec, $key).and_then(|v| v.parse().ok()) {
                    $dst = v;
                }
            };
        }
        macro_rules! text {
            ($sec:expr, $key:expr, $dst:expr) => {
                if let Some(v) = get($sec, $key) {
                    $dst = v;
                }
            };
        }

        let g = &mut self.general;
        b("General", "CheckForUpdates", &mut g.check_for_updates);
        num!("General", "AccountJoinDelay", g.account_join_delay);
        b("General", "AsyncJoin", &mut g.async_join);
        b("General", "SavePasswords", &mut g.save_passwords);
        text!("General", "ServerRegionFormat", g.server_region_format);
        text!("General", "IPApiLink", g.ip_api_link);
        num!("General", "MaxRecentGames", g.max_recent_games);
        b("General", "ShuffleJobId", &mut g.shuffle_job_id);
        b("General", "ShuffleChoosesLowestServer", &mut g.shuffle_lowest_server);
        num!("General", "ShufflePageCount", g.shuffle_page_count);
        b("General", "AutoCookieRefresh", &mut g.auto_cookie_refresh);
        b("General", "AutoCloseLastProcess", &mut g.auto_close_last_process);
        b("General", "ShowPresence", &mut g.show_presence);
        b("General", "UnlockFPS", &mut g.unlock_fps);
        num!("General", "MaxFPSValue", g.max_fps);
        text!("General", "CustomClientSettings", g.custom_client_settings);
        b("General", "EnableMultiRbx", &mut g.enable_multi_roblox);
        b("General", "HideUsernames", &mut g.hide_usernames);
        b("General", "DisableImages", &mut g.disable_images);
        b("General", "UseProxies", &mut g.use_proxies);
        text!("General", "SavedPlaceId", g.saved_place_id);
        text!("General", "SavedFollowUser", g.saved_follow_user);
        b("Developer", "DevMode", &mut g.dev_mode);

        let w = &mut self.web_server;
        b("Developer", "EnableWebServer", &mut w.enabled);
        num!("WebServer", "WebServerPort", w.port);
        text!("WebServer", "Password", w.password);
        b("WebServer", "EveryRequestRequiresPassword", &mut w.every_request_requires_password);
        b("WebServer", "AllowExternalConnections", &mut w.allow_external_connections);
        b("WebServer", "AllowGetCookie", &mut w.allow_get_cookie);
        b("WebServer", "AllowGetAccounts", &mut w.allow_get_accounts);
        b("WebServer", "AllowLaunchAccount", &mut w.allow_launch_account);
        b("WebServer", "AllowAccountEditing", &mut w.allow_account_editing);

        let n = &mut self.nexus;
        b("AccountControl", "StartOnLaunch", &mut n.start_on_launch);
        num!("AccountControl", "NexusPort", n.port);
        b("AccountControl", "AllowExternalConnections", &mut n.allow_external_connections);
        num!("AccountControl", "RelaunchDelay", n.relaunch_delay);
        num!("AccountControl", "LauncherDelayNumber", n.launcher_delay);
        b("AccountControl", "SaveOutput", &mut n.save_output);
        b("AccountControl", "AutoMinimizeEnabled", &mut n.auto_minimize_enabled);
        num!("AccountControl", "AutoMinimizeInterval", n.auto_minimize_interval);
        b("AccountControl", "AutoCloseEnabled", &mut n.auto_close_enabled);
        num!("AccountControl", "AutoCloseInterval", n.auto_close_interval);
        num!("AccountControl", "AutoCloseType", n.auto_close_type);
        num!("AccountControl", "MaxInstances", n.max_instances);
        b("AccountControl", "InternetCheck", &mut n.internet_check);
        b("AccountControl", "UsePresence", &mut n.use_presence);

        let wt = &mut self.watcher;
        b("Watcher", "Enabled", &mut wt.enabled);
        b("Watcher", "ExitOnBeta", &mut wt.exit_on_beta);
        b("Watcher", "ExitIfNoConnection", &mut wt.exit_if_no_connection);
        num!("Watcher", "NoConnectionTimeout", wt.no_connection_timeout);
        b("Watcher", "SaveWindowPositions", &mut wt.save_window_positions);
        b("Watcher", "VerifyDataModel", &mut wt.verify_data_model);
        b("Watcher", "IgnoreExistingProcesses", &mut wt.ignore_existing_processes);
        b("Watcher", "CloseRbxMemory", &mut wt.close_if_memory_low);
        num!("Watcher", "MemoryLowValue", wt.memory_low_value);
        b("Watcher", "CloseRbxWindowTitle", &mut wt.close_if_window_title);
        text!("Watcher", "ExpectedWindowTitle", wt.expected_window_title);
        num!("Watcher", "ScanInterval", wt.scan_interval);
        num!("Watcher", "ReadInterval", wt.read_interval);
    }
}

fn parse_ini(text: &str) -> HashMap<String, HashMap<String, String>> {
    let mut out: HashMap<String, HashMap<String, String>> = HashMap::new();
    let mut section = String::new();
    for line in text.lines() {
        let line = line.trim().trim_start_matches('\u{feff}');
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = line[1..line.len() - 1].trim().to_string();
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            out.entry(section.clone())
                .or_default()
                .insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    out
}
