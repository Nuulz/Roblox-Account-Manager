mod account;
mod avatars;
mod browser;
mod clientsettings;
mod crypto;
mod friends;
mod games;
mod groups;
mod inventory;
mod health;
mod ops;
mod process;
mod profiles;
mod regions;
mod roblox;
mod settings;
mod stats;
mod state;
mod storage;
mod updater;
mod users;
mod util;
mod vips;
mod webserver;

use ops::{CmdResult, JoinRequest};
use state::st;

// ─────────────────────────────────── vault

#[tauri::command]
fn vault_startup() -> storage::VaultInfo {
    storage::startup()
}

#[tauri::command]
fn vault_unlock(password: String) -> CmdResult<storage::VaultInfo> {
    storage::unlock(&password)
}

#[tauri::command]
fn vault_unlock_legacy(password: String) -> CmdResult<storage::VaultInfo> {
    storage::unlock_legacy(&password)
}

#[tauri::command]
fn vault_configure(mode: crypto::VaultMode, password: Option<String>) -> CmdResult<storage::VaultInfo> {
    storage::configure(mode, password.as_deref())
}

#[tauri::command]
fn vault_verify_password(password: String) -> bool {
    storage::verify_password(&password)
}

// ─────────────────────────────────── cuentas

#[tauri::command]
fn accounts_list() -> Vec<account::AccountView> {
    st().account_views()
}

#[tauri::command]
async fn account_add(cookie: String, password: String) -> CmdResult<account::AccountView> {
    ops::add_account(&cookie, &password).await
}

#[tauri::command]
fn account_remove(user_id: i64) {
    st().accounts.write().retain(|a| a.user_id != user_id);
    storage::save();
    st().emit_accounts();
}

#[tauri::command]
fn account_update(view: account::AccountPatch) -> CmdResult<()> {
    st().with_account(view.user_id, |a| view.apply(a)).ok_or("Cuenta no encontrada")?;
    storage::save();
    st().emit_accounts();
    Ok(())
}

/// Datos sensibles para copiar al portapapeles. Nunca se envían en la lista normal.
#[tauri::command]
fn account_secret(user_id: i64, kind: String) -> CmdResult<String> {
    let a = st().accounts.read().iter().find(|a| a.user_id == user_id).cloned().ok_or("Cuenta no encontrada")?;
    Ok(match kind.as_str() {
        "password" => a.password,
        "cookie" => a.security_token,
        "combo" => format!("{}:{}", a.username, a.password),
        "combo-cookie" => format!("{}:{}:{}", a.username, a.password, a.security_token),
        _ => return Err("Tipo desconocido".into()),
    })
}

/// Utilidades de cuenta. `action` elige la operación y `args` lleva sus datos.
#[tauri::command]
async fn account_util(user_id: i64, action: String, args: serde_json::Value) -> CmdResult<serde_json::Value> {
    use serde_json::json;
    let api = roblox::api();
    let cookie = st().cookie_of(user_id).ok_or("Cuenta no encontrada")?;
    let arg = |k: &str| args[k].as_str().unwrap_or_default().to_string();
    let set_cookie = |c: String| {
        st().with_account(user_id, |a| a.security_token = c);
        storage::save();
    };
    match action.as_str() {
        "info" => {
            let (robux, email, pin) =
                tokio::join!(api.robux(&cookie, user_id), api.email_info(&cookie), api.pin_status(&cookie));
            // Si no se pudo leer el PIN (cookie vencida, límite…) se envía null para no mostrar "no".
            let (pin_on, pin_until) = match pin {
                Ok((on, until)) => (json!(on), json!(until)),
                Err(_) => (serde_json::Value::Null, serde_json::Value::Null),
            };
            Ok(json!({ "robux": robux, "email": email, "pinEnabled": pin_on, "pinUnlockedUntil": pin_until }))
        }
        "avatar3d" => api.avatar_3d(&cookie, user_id).await.map_err(ops::err),
        "password" => {
            let new = arg("new");
            let c = api.change_password(&cookie, &arg("current"), &new).await.map_err(ops::err)?;
            if let Some(c) = c {
                set_cookie(c);
            }
            st().with_account(user_id, |a| a.password = new);
            storage::save();
            Ok(json!("Contraseña cambiada"))
        }
        "displayName" => {
            api.set_display_name(&cookie, user_id, &arg("name")).await.map_err(ops::err)?;
            st().with_account(user_id, |a| a.display_name = arg("name"));
            st().emit_accounts();
            Ok(json!("Nombre visible cambiado"))
        }
        "unlockPin" => api.unlock_pin(&cookie, &arg("pin")).await.map(|t| json!(t)).map_err(ops::err),
        "quickLogin" => {
            let code = arg("code");
            let info = api.quick_login_enter(&cookie, &code).await.map_err(ops::err)?;
            api.quick_login_validate(&cookie, &code).await.map_err(ops::err)?;
            Ok(info)
        }
        "outfits" => {
            let list = api.outfits(user_id).await.map_err(ops::err)?;
            let ids: Vec<i64> = list.iter().map(|(id, _)| *id).collect();
            let thumbs = api.outfit_thumbnails(&ids).await;
            Ok(json!(list
                .into_iter()
                .map(|(id, name)| json!({ "id": id, "name": name, "image": thumbs.get(&id) }))
                .collect::<Vec<_>>()))
        }
        "applyOutfit" => {
            let id = args["outfitId"].as_i64().ok_or("outfitId inválido")?;
            let details = api.outfit_details(id).await.map_err(ops::err)?;
            let invalid = api.set_avatar(&cookie, &details).await.map_err(ops::err)?;
            Ok(json!(if invalid.is_empty() {
                "Traje aplicado".to_string()
            } else {
                format!("Traje aplicado; {} accesorio(s) no se pudieron poner porque la cuenta no los tiene", invalid.len())
            }))
        }
        "email" => {
            api.change_email(&cookie, &arg("password"), &arg("email")).await.map_err(ops::err)?;
            Ok(json!("Correo cambiado; Roblox enviará un mensaje para verificarlo"))
        }
        "followPrivacy" => {
            let level = args["level"].as_u64().ok_or("Nivel inválido")? as u8;
            api.set_follow_privacy(&cookie, level).await.map_err(ops::err)?;
            Ok(json!("Privacidad actualizada"))
        }
        "friend" => {
            let id = args["targetId"].as_i64().ok_or("Cuenta destino inválida")?;
            api.send_friend_request(&cookie, id).await.map_err(ops::err)?;
            Ok(json!("Solicitud de amistad enviada"))
        }
        "blocked" => api.blocked_list(&cookie).await.map_err(ops::err),
        "unblock" => {
            let id = args["userId"].as_i64().ok_or("userId inválido")?;
            api.set_blocked(&cookie, id, false).await.map(|(_, m)| json!(m)).map_err(ops::err)
        }
        "signoutOthers" => {
            if let Some(c) = api.signout_other_sessions(&cookie).await.map_err(ops::err)? {
                set_cookie(c);
            }
            Ok(json!("Se cerraron las demás sesiones"))
        }
        _ => Err("Acción desconocida".into()),
    }
}

#[tauri::command]
fn accounts_trackers() -> std::collections::HashMap<i64, String> {
    st().accounts.read().iter().map(|a| (a.user_id, a.browser_tracker_id.clone())).collect()
}

#[tauri::command]
fn stats_get(user_id: i64) -> stats::StatsView {
    stats::view(user_id)
}

#[tauri::command]
async fn accounts_refresh_avatars(ids: Vec<i64>) {
    ops::refresh_avatars(ids).await;
}

#[tauri::command]
async fn accounts_update_presence(ids: Vec<i64>) {
    ops::update_presence(ids).await;
}

// ─────────────────────────────────── lanzar

#[tauri::command]
async fn join_server(
    user_id: i64,
    place_id: i64,
    job_id: String,
    follow_user: bool,
    join_vip: bool,
) -> CmdResult<()> {
    ops::join_server(user_id, JoinRequest { place_id, job_id, follow_user, join_vip }).await
}

#[tauri::command]
async fn launch_many(
    ids: Vec<i64>,
    place_id: i64,
    job_id: String,
    follow_user: bool,
    join_vip: bool,
) {
    ops::launch_many(ids, JoinRequest { place_id, job_id, follow_user, join_vip }).await;
}

#[tauri::command]
fn launch_cancel() {
    st().cancel_launching();
}

#[tauri::command]
fn launch_next() {
    ops::next_account();
}

// ─────────────────────────────────── servidores

#[tauri::command]
async fn servers_list(place_id: i64, cursor: String) -> CmdResult<roblox::ServersPage> {
    roblox::api()
        .servers(place_id, if cursor.is_empty() { None } else { Some(cursor.as_str()) })
        .await
        .map_err(ops::err)
}

#[tauri::command]
async fn server_region(place_id: i64, job_id: String) -> CmdResult<regions::Region> {
    regions::region(place_id, &job_id).await
}

// ─────────────────────────────────── juegos

#[tauri::command]
async fn user_id_by_name(username: String) -> CmdResult<i64> {
    let cookie = st().any_cookie();
    roblox::api().user_id(username.trim(), cookie.as_deref()).await.map_err(ops::err)
}

#[tauri::command]
async fn place_name(place_id: i64) -> Option<String> {
    roblox::api().place_name(place_id).await
}

#[tauri::command]
fn games_list() -> games::GamesData {
    st().games.read().clone()
}

#[tauri::command]
fn game_add_favorite(entry: games::GameEntry) {
    let mut g = st().games.write();
    if !g.favorites.iter().any(|e| e.place_id == entry.place_id) {
        g.favorites.push(entry);
        g.save();
    }
    st().emit("games", g.clone());
}

#[tauri::command]
fn game_remove_favorite(place_id: i64) {
    let mut g = st().games.write();
    g.favorites.retain(|e| e.place_id != place_id);
    g.save();
    st().emit("games", g.clone());
}

#[tauri::command]
fn game_clear_recent() {
    let mut g = st().games.write();
    g.recent.clear();
    g.save();
    st().emit("games", g.clone());
}

/// Busca juegos por nombre. Devuelve tarjetas listas para la UI y el token de la página siguiente.
#[tauri::command]
async fn games_search(term: String, page_token: Option<String>) -> CmdResult<serde_json::Value> {
    use serde_json::{json, Value};
    let term = term.trim();
    if term.is_empty() {
        return Err("Escribe qué juego buscar".into());
    }
    let api = roblox::api();
    let (raw, next) = api.search_games(term, page_token.as_deref()).await.map_err(ops::err)?;
    let ids: Vec<i64> = raw.iter().filter_map(|g| g["universeId"].as_i64()).collect();
    let icons = api.universe_icons(&ids).await;
    let games: Vec<Value> = raw
        .iter()
        .filter_map(|g| {
            let universe = g["universeId"].as_i64()?;
            let place = g["rootPlaceId"].as_i64()?;
            let up = g["totalUpVotes"].as_i64().unwrap_or(0);
            let down = g["totalDownVotes"].as_i64().unwrap_or(0);
            Some(json!({
                "placeId": place,
                "universeId": universe,
                "name": g["name"].as_str().unwrap_or_default(),
                "players": g["playerCount"].as_i64().unwrap_or(0),
                "likes": if up + down > 0 { Some(up * 100 / (up + down)) } else { None },
                "creator": g["creatorName"].as_str().unwrap_or_default(),
                "verified": g["creatorHasVerifiedBadge"].as_bool().unwrap_or(false),
                "sponsored": g["isSponsored"].as_bool().unwrap_or(false),
                "icon": icons.get(&universe).cloned().unwrap_or_default(),
            }))
        })
        .collect();
    Ok(json!({ "games": games, "next": next }))
}

#[tauri::command]
async fn game_info(place_id: i64) -> CmdResult<roblox::PlaceInfo> {
    roblox::api().place_info(place_id).await.map_err(ops::err)
}

// ─────────────────────────────────── configuración

#[tauri::command]
fn settings_get() -> settings::Settings {
    st().settings.read().clone()
}

#[tauri::command]
fn settings_set(s: settings::Settings) -> CmdResult<()> {
    s.save();
    *st().settings.write() = s.clone();
    st().emit("settings", s);
    webserver::apply();
    // Los ajustes ya quedaron guardados; si multi Roblox no se puede activar, la UI lo
    // muestra como "(bloqueado)" y ofrece cerrar Roblox, sin impedir el guardado.
    if let Err(e) = apply_process_settings() {
        log::warn!("{e}");
    }
    Ok(())
}

/// Aplica multi Roblox y el bloqueo de cookies según la configuración actual.
fn apply_process_settings() -> CmdResult<()> {
    let (multi, lock) = {
        let g = &st().settings.read().general;
        (g.enable_multi_roblox, g.lock_roblox_cookies)
    };
    process::set_cookie_lock(multi && lock);
    if !process::set_multi_roblox(multi) {
        return Err("No se pudo activar multi Roblox: cierra todas las ventanas de Roblox y vuelve a intentarlo".into());
    }
    Ok(())
}

#[tauri::command]
fn multi_roblox_status() -> bool {
    process::multi_active()
}

#[tauri::command]
fn multi_roblox_apply() -> CmdResult<()> {
    apply_process_settings()
}

/// Cierra todos los Roblox abiertos y vuelve a intentar activar multi Roblox.
#[tauri::command]
async fn multi_roblox_force() -> CmdResult<()> {
    for p in process::roblox_processes() {
        process::kill(p.pid);
    }
    for _ in 0..20 {
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        if process::roblox_processes().is_empty() && apply_process_settings().is_ok() {
            return Ok(());
        }
    }
    apply_process_settings()
}

// ─────────────────────────────────── actualizaciones

#[tauri::command]
async fn update_check() -> CmdResult<updater::UpdateInfo> {
    updater::check().await
}

#[tauri::command]
async fn update_install(url: String) -> CmdResult<()> {
    updater::install(&url).await
}

#[tauri::command]
fn app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

// ─────────────────────────────────── almacenamiento

#[tauri::command]
fn storage_export(path: String) -> CmdResult<usize> {
    storage::export_plain(std::path::Path::new(&path))
}

#[tauri::command]
fn storage_import(path: String, password: Option<String>) -> CmdResult<usize> {
    storage::import_file(std::path::Path::new(&path), password.as_deref())
}

// ─────────────────────────────────── procesos / ventanas

#[tauri::command]
fn processes_list() -> Vec<process::RbxProcess> {
    process::roblox_processes()
}

#[tauri::command]
fn process_kill(pid: u32) {
    process::kill(pid);
}

#[tauri::command]
fn window_minimize(hwnd: isize) {
    process::minimize(hwnd);
}

#[tauri::command]
fn window_close(hwnd: isize) {
    process::close_window(hwnd);
}

#[tauri::command]
fn window_move(hwnd: isize, x: i32, y: i32, w: i32, h: i32) {
    process::move_window(hwnd, x, y, w, h);
}

// ─────────────────────────────────── revisión de cuentas

#[tauri::command]
async fn health_check(ids: Vec<i64>) -> CmdResult<()> {
    health::check(ids).await
}

#[tauri::command]
fn health_last() -> Vec<health::HealthRow> {
    health::last()
}

#[tauri::command]
fn health_cancel() {
    health::cancel();
}

// ─────────────────────────────────── copiar avatar

#[tauri::command]
async fn avatar_preview(user_id: i64) -> CmdResult<avatars::Preview> {
    avatars::preview(user_id).await
}

#[tauri::command]
async fn avatar_copy(source: i64, targets: Vec<i64>) -> CmdResult<Vec<avatars::Applied>> {
    avatars::apply(source, targets).await
}

// ─────────────────────────────────── amigos

#[tauri::command]
async fn friends_overview(ids: Vec<i64>) -> CmdResult<friends::Overview> {
    friends::overview(ids).await
}

// ─────────────────────────────────── inventario y usuarios

#[tauri::command]
async fn inventory_overview(ids: Vec<i64>) -> CmdResult<inventory::Overview> {
    inventory::overview(ids).await
}

#[tauri::command]
async fn users_search(keyword: String) -> CmdResult<Vec<users::Found>> {
    users::search(&keyword).await
}

#[tauri::command]
async fn user_profile(user_id: i64) -> CmdResult<serde_json::Value> {
    users::profile(user_id).await
}

// ─────────────────────────────────── grupos de Roblox

#[tauri::command]
async fn groups_overview(ids: Vec<i64>) -> CmdResult<groups::Overview> {
    groups::overview(ids).await
}

// ─────────────────────────────────── servidores privados

#[tauri::command]
fn vips_list() -> Vec<vips::PrivateServer> {
    vips::list()
}

#[tauri::command]
async fn vip_resolve(link: String, user_id: Option<i64>) -> CmdResult<vips::PrivateServer> {
    vips::resolve(&link, user_id).await
}

#[tauri::command]
async fn vip_save(server: vips::PrivateServer, user_id: Option<i64>) -> CmdResult<vips::PrivateServer> {
    vips::upsert(server, user_id).await
}

#[tauri::command]
fn vip_delete(id: i64) -> CmdResult<()> {
    vips::delete(id)
}

#[tauri::command]
async fn vip_join(id: i64, ids: Vec<i64>) -> CmdResult<()> {
    vips::join(id, ids).await
}

// ─────────────────────────────────── perfiles de lanzamiento

#[tauri::command]
fn profiles_list() -> Vec<profiles::Profile> {
    profiles::list()
}

#[tauri::command]
fn profile_save(profile: profiles::Profile) -> CmdResult<profiles::Profile> {
    profiles::upsert(profile)
}

#[tauri::command]
fn profile_delete(id: i64) -> CmdResult<()> {
    profiles::delete(id)
}

#[tauri::command]
async fn profile_launch(id: i64) -> CmdResult<()> {
    profiles::launch(id).await
}

// ─────────────────────────────────── navegador

#[tauri::command]
async fn browser_open(user_id: i64, path: Option<String>) -> CmdResult<()> {
    browser::open_account(user_id, path).await
}

#[tauri::command]
async fn browser_login(user_id: Option<i64>) -> CmdResult<()> {
    browser::login(user_id).await
}

// ─────────────────────────────────── utilidades

#[tauri::command]
fn roblox_version() -> Option<String> {
    st().roblox_version.read().clone()
}

#[tauri::command]
fn app_uptime_secs() -> u64 {
    st().started.elapsed().as_secs()
}

// ─────────────────────────────────── setup de la app

fn init_log() {
    use simplelog::{ConfigBuilder, LevelFilter, WriteLogger};
    let path = util::data_path("ram.log");
    // Evita que el log crezca sin límite
    if std::fs::metadata(&path).map(|m| m.len() > 2_000_000).unwrap_or(false) {
        let _ = std::fs::remove_file(&path);
    }
    if let Ok(f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        // Por defecto simplelog escribe la hora en UTC; se usa la hora local del equipo
        let mut cfg = ConfigBuilder::new();
        let _ = cfg.set_time_offset_to_local();
        // Fecha, hora y desfase respecto a UTC, p. ej. "2026-10-19 19:44:52 UTC-03:00"
        cfg.set_time_format_custom(simplelog::format_description!(
            "[year]-[month]-[day] [hour]:[minute]:[second] UTC[offset_hour sign:mandatory]:[offset_minute]"
        ));
        let _ = WriteLogger::init(LevelFilter::Info, cfg.build(), f);
    }
    log::info!("Roblox Account Manager {} iniciado", env!("CARGO_PKG_VERSION"));
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            st().app.set(app.handle().clone()).ok();
            init_log();
            updater::cleanup();

            // Cargar configuración
            *st().settings.write() = settings::Settings::load();
            // Cargar juegos
            *st().games.write() = games::GamesData::load();
            // Detectar versión de Roblox
            if let Some(ver) = clientsettings::roblox_version_folder()
                .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            {
                *st().roblox_version.write() = Some(ver);
            }

            if let Err(e) = apply_process_settings() {
                log::warn!("{e}");
            }

            webserver::apply();

            // Tareas de fondo
            tauri::async_runtime::spawn(ops::presence_loop());
            tauri::async_runtime::spawn(ops::session_loop());
            tauri::async_runtime::spawn(ops::cookie_refresh_loop());
            tauri::async_runtime::spawn(ops::anti_afk_loop());
            tauri::async_runtime::spawn(ops::auto_rejoin_loop());
            tauri::async_runtime::spawn(stats::stats_loop());

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // vault
            vault_startup,
            vault_unlock,
            vault_unlock_legacy,
            vault_configure,
            vault_verify_password,
            // cuentas
            accounts_list,
            account_add,
            account_remove,
            account_update,
            account_secret,
            stats_get,
            accounts_trackers,
            account_util,
            accounts_refresh_avatars,
            accounts_update_presence,
            // lanzar
            join_server,
            launch_many,
            launch_cancel,
            launch_next,
            // servidores
            servers_list,
            server_region,
            // juegos
            games_list,
            user_id_by_name,
            place_name,
            game_add_favorite,
            game_remove_favorite,
            game_clear_recent,
            game_info,
            games_search,
            // configuración
            settings_get,
            settings_set,
            multi_roblox_apply,
            multi_roblox_force,
            multi_roblox_status,
            // actualizaciones
            update_check,
            update_install,
            app_version,
            // almacenamiento
            storage_export,
            storage_import,
            // procesos
            processes_list,
            process_kill,
            window_close,
            window_minimize,
            window_move,
            // revisión
            health_check,
            health_last,
            health_cancel,
            // copiar avatar
            avatar_preview,
            avatar_copy,
            // amigos
            friends_overview,
            // inventario y usuarios
            inventory_overview,
            users_search,
            user_profile,
            // grupos
            groups_overview,
            // servidores privados
            vips_list,
            vip_resolve,
            vip_save,
            vip_delete,
            vip_join,
            // perfiles
            profiles_list,
            profile_save,
            profile_delete,
            profile_launch,
            // navegador
            browser_open,
            browser_login,
            // utilidades
            roblox_version,
            app_uptime_secs,
        ])
        .run(tauri::generate_context!())
        .expect("No se pudo iniciar la aplicación");
}
