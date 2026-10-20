//! Operaciones de cuentas: agregar, lanzar, presencia, sesiones.

use std::sync::atomic::Ordering;
use std::time::Duration;

use chrono::Local;
use once_cell::sync::Lazy;
use serde::Serialize;

use crate::account::{Account, AccountView};
use crate::roblox::api;
use crate::state::st;
use crate::{clientsettings, games, process, storage};

pub type CmdResult<T> = Result<T, String>;

pub fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

/// Valida la cookie y agrega la cuenta (o actualiza la existente).
pub async fn add_account(cookie: &str, password: &str) -> CmdResult<AccountView> {
    let cookie = cookie.trim();
    if cookie.is_empty() {
        return Err("Cookie vacía".into());
    }
    let user = api().validate_cookie(cookie).await.map_err(err)?;
    let view = {
        let mut list = st().accounts.write();
        let acc = match list.iter_mut().find(|a| a.user_id == user.id) {
            Some(existing) => {
                existing.security_token = cookie.to_string();
                if !password.is_empty() {
                    existing.password = password.to_string();
                }
                existing.username = user.name.clone();
                existing.display_name = user.display_name.clone();
                existing.last_use = Local::now();
                existing.valid = true;
                existing.rt.needs_login = false;
                existing.clone()
            }
            None => {
                let acc = Account {
                    user_id: user.id,
                    username: user.name.clone(),
                    display_name: user.display_name.clone(),
                    password: password.to_string(),
                    security_token: cookie.to_string(),
                    ..Default::default()
                };
                list.push(acc.clone());
                acc
            }
        };
        acc.to_view()
    };
    storage::save();
    st().emit_accounts();
    tokio::spawn(refresh_avatars(vec![user.id]));
    Ok(view)
}

pub async fn refresh_avatars(ids: Vec<i64>) {
    if st().settings.read().general.disable_images || ids.is_empty() {
        return;
    }
    let shots = api().headshots(&ids).await;
    if shots.is_empty() {
        return;
    }
    {
        let mut list = st().accounts.write();
        for a in list.iter_mut() {
            if let Some(u) = shots.get(&a.user_id) {
                a.rt.avatar_url = Some(u.clone());
            }
        }
    }
    st().emit_accounts();
}

// ------------------------------------------------------------------ lanzar

#[derive(Clone, Debug, Default)]
pub struct JoinRequest {
    pub place_id: i64,
    pub job_id: String,
    pub follow_user: bool,
    pub join_vip: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LaunchProgress {
    current: usize,
    total: usize,
    username: String,
    done: bool,
}

pub fn next_account() {
    st().launch_next.notify_one();
}

pub fn build_launch_url(ticket: &str, tracker: &str, req: &JoinRequest, access_code: &str, link_code: &str, teleport: bool) -> String {
    let launch_time = crate::util::unix_millis();
    let place_launcher = if req.join_vip {
        format!(
            "https://assetgame.roblox.com/game/PlaceLauncher.ashx?request=RequestPrivateGame&placeId={}&accessCode={}&linkCode={}",
            req.place_id, access_code, link_code
        )
    } else if req.follow_user {
        format!("https://assetgame.roblox.com/game/PlaceLauncher.ashx?request=RequestFollowUser&userId={}", req.place_id)
    } else {
        let has_job = !req.job_id.is_empty();
        format!(
            "https://assetgame.roblox.com/game/PlaceLauncher.ashx?request=RequestGame{}&browserTrackerId={}&placeId={}{}&isPlayTogetherGame=false{}",
            if has_job { "Job" } else { "" },
            tracker,
            req.place_id,
            if has_job { format!("&gameId={}", req.job_id) } else { String::new() },
            if teleport { "&isTeleport=true" } else { "" }
        )
    };
    format!(
        "roblox-player:1+launchmode:play+gameinfo:{ticket}+launchtime:{launch_time}+placelauncherurl:{}+browsertrackerid:{tracker}+robloxLocale:en_us+gameLocale:en_us+channel:+LaunchExp:InApp",
        urlencoding::encode(&place_launcher)
    )
}

/// Une una cuenta a un juego / servidor / usuario.
pub async fn join_server(user_id: i64, mut req: JoinRequest) -> CmdResult<()> {
    let original = req.clone();
    let (cookie, tracker, username) = st()
        .with_account(user_id, |a| (a.security_token.clone(), a.ensure_tracker_id(), a.username.clone()))
        .ok_or("Cuenta no encontrada")?;

    tokio::task::spawn_blocking(clientsettings::patch).await.ok();

    let (shuffle, lowest, pages, auto_close, teleport) = {
        let s = st().settings.read();
        (
            s.general.shuffle_job_id,
            s.general.shuffle_lowest_server,
            s.general.shuffle_page_count,
            s.general.auto_close_last_process,
            s.general.is_teleport,
        )
    };

    if shuffle && req.job_id.is_empty() && !req.follow_user && !req.join_vip {
        req.job_id = api().random_job_id(req.place_id, lowest, pages).await;
    }

    let ticket = api()
        .auth_ticket(&cookie)
        .await
        .map_err(|e| format!("{username}: ticket inválido, Roblox probablemente cerró la sesión. {e}"))?;

    if auto_close {
        for p in process::processes_for_tracker(&tracker) {
            if let Some(h) = p.hwnd {
                process::close_window(h);
                tokio::time::sleep(Duration::from_millis(250)).await;
                process::close_window(h);
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
            process::kill(p.pid);
        }
    }

    static LINK: Lazy<regex::Regex> = Lazy::new(|| regex::Regex::new(r"privateServerLinkCode=([^&\s]+)").unwrap());
    let link_code = LINK.captures(&req.job_id).map(|c| c[1].to_string()).unwrap_or_default();
    let mut access_code = req.job_id.clone();
    if !link_code.is_empty() {
        if let Some(code) = api().private_server_access_code(&cookie, req.place_id, &link_code).await {
            req.join_vip = true;
            access_code = code;
        } else {
            return Err("No se pudo resolver el enlace del servidor privado".into());
        }
    }

    if req.join_vip {
        if api().private_server_privacy_is_everyone(&cookie).await == Some(false) {
            st().emit("vip-privacy", serde_json::json!({ "userId": user_id, "username": username }));
        }
    }

    // En Linux Sober entra con un enlace roblox://; para seguir a alguien se busca dónde está.
    #[cfg(target_os = "linux")]
    if req.follow_user {
        let p = api()
            .presence(&[req.place_id], Some(&cookie))
            .await
            .ok()
            .and_then(|l| l.into_iter().next())
            .filter(|p| p.in_game() && p.place_id.is_some())
            .ok_or("Ese usuario no está en un juego, o su privacidad no deja seguirlo")?;
        req = JoinRequest { place_id: p.place_id.unwrap_or_default(), job_id: p.game_id.unwrap_or_default(), ..Default::default() };
    }

    st().with_account(user_id, |a| a.last_use = Local::now());
    storage::save();

    if !req.follow_user && req.place_id > 0 {
        tokio::spawn(games::add_recent(req.place_id));
    }

    #[cfg(windows)]
    let launched = {
        let url = build_launch_url(&ticket, &tracker, &req, &access_code, &link_code, teleport);
        tokio::task::spawn_blocking(move || process::shell_launch_and_wait(&url)).await.map_err(err)?
    };
    #[cfg(target_os = "linux")]
    let launched = {
        let _ = (&ticket, teleport);
        let uri = crate::sober::join_uri(req.place_id, if req.join_vip { "" } else { &req.job_id }, if req.join_vip { &access_code } else { "" }, &link_code);
        let tracker = tracker.clone();
        tokio::task::spawn_blocking(move || crate::sober::launch(&tracker, &cookie, &uri)).await.map_err(err)?
    };
    next_account();
    launched?;
    st().with_account(user_id, |a| {
        a.rt.last_join = Some(original);
        a.rt.seen_in_game = false;
        a.rt.rejoin_misses = 0;
        a.rt.off_game_since = None;
    });
    tokio::spawn(adjust_window_position(user_id));
    Ok(())
}

/// Lanza varias cuentas en cola, respetando el retraso o esperando al lanzador.
pub async fn launch_many(ids: Vec<i64>, req: JoinRequest) {
    launch_many_with(ids, req, None, true).await
}

/// `delay` reemplaza el retraso de los ajustes; con `use_saved`, cada cuenta usa su Place/Job guardado.
pub async fn launch_many_with(ids: Vec<i64>, req: JoinRequest, delay: Option<u64>, use_saved: bool) {
    st().cancel_launching();
    let generation = st().launch_gen.load(Ordering::SeqCst);
    st().launching.store(true, Ordering::SeqCst);
    let total = ids.len();
    for (i, id) in ids.into_iter().enumerate() {
        if st().launch_gen.load(Ordering::SeqCst) != generation {
            break;
        }
        let Some(acc) = st().accounts.read().iter().find(|a| a.user_id == id).cloned() else { continue };
        st().emit(
            "launch-progress",
            LaunchProgress { current: i + 1, total, username: acc.username.clone(), done: false },
        );
        let mut r = req.clone();
        if use_saved && !r.follow_user {
            if let Ok(pid) = acc.field("SavedPlaceId").parse::<i64>() {
                r.place_id = pid;
            }
            let job = acc.field("SavedJobId");
            if !job.is_empty() {
                if let Some(code) = job.strip_prefix("VIP:") {
                    r.job_id = code.to_string();
                    r.join_vip = true;
                } else {
                    r.job_id = job;
                }
            }
        }
        if let Err(e) = join_server(id, r).await {
            st().toast("error", e);
        }
        if i + 1 == total {
            break;
        }
        let (async_join, delay) = {
            let s = st().settings.read();
            (s.general.async_join && delay.is_none(), delay.unwrap_or(s.general.account_join_delay))
        };
        if async_join {
            st().launch_next.notified().await;
        } else {
            tokio::time::sleep(Duration::from_secs(delay)).await;
        }
    }
    st().launching.store(false, Ordering::SeqCst);
    st().emit("launch-progress", LaunchProgress { current: total, total, username: String::new(), done: true });
}

/// Restaura la posición/tamaño guardados de la ventana de Roblox de esta cuenta.
pub async fn adjust_window_position(user_id: i64) {
    if !st().settings.read().watcher.save_window_positions {
        return;
    }
    let Some((tracker, x, y, w, h)) = st().with_account(user_id, |a| {
        let p = |k: &str| a.field(k).parse::<i32>().ok();
        Some((a.browser_tracker_id.clone(), p("Window_Position_X")?, p("Window_Position_Y")?, p("Window_Width")?, p("Window_Height")?))
    }).flatten() else {
        return;
    };
    let deadline = std::time::Instant::now() + Duration::from_secs(45);
    while std::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(350)).await;
        if let Some(hwnd) = process::processes_for_tracker(&tracker).into_iter().rev().find_map(|p| p.hwnd) {
            process::move_window(hwnd, x, y, w, h);
            return;
        }
    }
}

// ---------------------------------------------------------------- presencia

pub async fn update_presence(ids: Vec<i64>) {
    if ids.is_empty() || !crate::util::is_connected_to_internet() {
        return;
    }
    let cookie = st().any_cookie();
    let Ok(list) = api().presence(&ids, cookie.as_deref()).await else { return };
    let procs = tokio::task::spawn_blocking(process::roblox_processes).await.unwrap_or_default();
    {
        let mut accounts = st().accounts.write();
        for p in list {
            if let Some(a) = accounts.iter_mut().find(|a| a.user_id == p.user_id) {
                if p.in_game() {
                    a.rt.seen_in_game = true;
                    if a.rt.game_session_start.is_none() {
                        a.rt.game_session_start = Some(session_start_for(a, &procs));
                    }
                } else {
                    a.rt.game_session_start = None;
                }
                a.rt.presence = Some(p);
            }
        }
    }
    st().emit_accounts();
}

/// Desde cuándo está en juego: hora de inicio de su Roblox (por BrowserTrackerId),
/// o el último lanzamiento desde la app si no se encuentra el proceso.
fn session_start_for(a: &Account, procs: &[process::RbxProcess]) -> chrono::DateTime<Local> {
    use chrono::TimeZone;
    let from_proc = procs
        .iter()
        .filter(|p| !a.browser_tracker_id.is_empty() && p.tracker_id == a.browser_tracker_id)
        .map(|p| p.start_time)
        .min()
        .and_then(|t| Local.timestamp_opt(t as i64, 0).single());
    if let Some(t) = from_proc {
        return t;
    }
    let now = Local::now();
    if (now - a.last_use).num_hours() < 12 { a.last_use } else { now }
}

pub async fn presence_loop() {
    loop {
        let (enabled, minutes) = {
            let s = st().settings.read();
            (s.general.show_presence, s.general.presence_update_minutes.max(1))
        };
        if enabled && st().is_unlocked() {
            let ids: Vec<i64> = st().accounts.read().iter().map(|a| a.user_id).collect();
            update_presence(ids).await;
        }
        tokio::time::sleep(Duration::from_secs(minutes * 60)).await;
    }
}

/// Revisa periódicamente si cada cookie sigue válida y marca "Requiere login".
pub async fn session_loop() {
    tokio::time::sleep(Duration::from_secs(5)).await;
    loop {
        if st().is_unlocked() {
            let list: Vec<(i64, String)> =
                st().accounts.read().iter().map(|a| (a.user_id, a.security_token.clone())).collect();
            for (id, cookie) in list {
                if let Ok(state) = api().authenticated_user(&cookie).await {
                    let changed = st()
                        .with_account(id, |a| {
                            let needs = state.is_none();
                            let changed = a.rt.needs_login != needs;
                            a.rt.needs_login = needs;
                            if let Some(u) = &state {
                                a.valid = true;
                                if !u.display_name.is_empty() {
                                    a.display_name = u.display_name.clone();
                                }
                            }
                            changed
                        })
                        .unwrap_or(false);
                    if changed {
                        st().emit_accounts();
                    }
                }
                tokio::time::sleep(Duration::from_millis(1500)).await;
            }
        }
        let minutes = st().settings.read().general.session_check_minutes.max(2);
        tokio::time::sleep(Duration::from_secs(minutes * 60)).await;
    }
}

/// Refresca cookies de cuentas sin usar hace tiempo para que no caduquen.
pub async fn cookie_refresh_loop() {
    loop {
        tokio::time::sleep(Duration::from_secs(5 * 60)).await;
        if !st().settings.read().general.auto_cookie_refresh || !st().is_unlocked() {
            continue;
        }
        let now = Local::now();
        let due: Vec<(i64, String)> = st()
            .accounts
            .read()
            .iter()
            .filter(|a| {
                a.field("NoCookieRefresh") != "true"
                    && (now - a.last_use).num_days() > 20
                    && (now - a.last_attempted_refresh).num_days() >= 7
            })
            .map(|a| (a.user_id, a.security_token.clone()))
            .collect();
        for (id, cookie) in due {
            st().with_account(id, |a| a.last_attempted_refresh = Local::now());
            if let Ok(Some(new_cookie)) = api().signout_other_sessions(&cookie).await {
                st().with_account(id, |a| {
                    a.security_token = new_cookie;
                    a.last_use = Local::now();
                });
                log::info!("Cookie refrescada para {id}");
            }
            storage::save();
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    }
}

// ---------------------------------------------------------------- anti-AFK

const VK_SPACE: u32 = 0x20;
const VK_W: u32 = 0x57;
const VK_A: u32 = 0x41;
const VK_D: u32 = 0x44;

pub async fn anti_afk_loop() {
    use rand::Rng;
    loop {
        let (min, max) = {
            let s = st().settings.read();
            (s.general.anti_afk_min_minutes.max(1), s.general.anti_afk_max_minutes.max(1))
        };
        let (lo, hi) = (min.min(max), min.max(max));
        let secs = rand::thread_rng().gen_range(lo * 60..=hi * 60);
        tokio::time::sleep(Duration::from_secs(secs)).await;
        tick_anti_afk();
    }
}

fn tick_anti_afk() {
    if cfg!(not(windows)) {
        return;
    }
    use rand::seq::SliceRandom;
    let sequences: [&[u32]; 5] = [&[VK_SPACE], &[VK_W, VK_SPACE], &[VK_A, VK_SPACE], &[VK_D, VK_SPACE], &[VK_W]];
    let targets: Vec<(i64, String)> = st()
        .accounts
        .read()
        .iter()
        .filter(|a| a.anti_afk)
        .map(|a| (a.user_id, a.browser_tracker_id.clone()))
        .collect();
    if targets.is_empty() {
        return;
    }
    let procs = process::roblox_processes();
    let mut changed = false;
    for (id, tracker) in targets {
        let hwnd = procs.iter().find(|p| !tracker.is_empty() && p.tracker_id == tracker).and_then(|p| p.hwnd);
        st().with_account(id, |a| match hwnd {
            Some(h) => {
                let seq = sequences.choose(&mut rand::thread_rng()).copied().unwrap_or(&[VK_SPACE]);
                for vk in seq {
                    process::post_key(h, *vk);
                }
                a.rt.anti_afk_misses = 0;
            }
            None => {
                a.rt.anti_afk_misses += 1;
                if a.rt.anti_afk_misses >= 2 {
                    a.anti_afk = false;
                    a.rt.anti_afk_misses = 0;
                    changed = true;
                }
            }
        });
    }
    if changed {
        storage::save();
        st().emit_accounts();
    }
}

// ---------------------------------------------------------------- auto-rejoin

/// Si el Roblox de una cuenta marcada se cierra o queda fuera del juego, vuelve a entrar
/// al mismo juego/servidor del último lanzamiento.
pub async fn auto_rejoin_loop() {
    loop {
        tokio::time::sleep(Duration::from_secs(20)).await;
        if !st().is_unlocked() || st().launching.load(Ordering::SeqCst) {
            continue;
        }
        for (id, req) in rejoin_due().await {
            let name = st().with_account(id, |a| a.username.clone()).unwrap_or_default();
            st().toast("info", format!("{name}: Roblox se cerró, volviendo a entrar"));
            if let Err(e) = join_server(id, req).await {
                st().toast("error", e);
            }
            let delay = st().settings.read().general.account_join_delay.max(3);
            tokio::time::sleep(Duration::from_secs(delay)).await;
        }
    }
}

async fn rejoin_due() -> Vec<(i64, JoinRequest)> {
    let procs = tokio::task::spawn_blocking(process::roblox_processes).await.unwrap_or_default();
    let max_per_hour = st().settings.read().general.auto_rejoin_max_per_hour.max(1) as usize;
    let now = Local::now();
    let mut due = Vec::new();
    let mut changed = false;
    for a in st().accounts.write().iter_mut().filter(|a| a.auto_rejoin) {
        let Some(req) = a.rt.last_join.clone() else { continue };
        let running: Vec<_> = procs
            .iter()
            .filter(|p| !a.browser_tracker_id.is_empty() && p.tracker_id == a.browser_tracker_id)
            .collect();

        // Proceso abierto pero la presencia dice que no está en un juego (desconectado / expulsado).
        let stuck = if running.is_empty() {
            false
        } else {
            // Solo si ya lo vimos en juego: así una presencia oculta no cierra un Roblox que funciona.
            let off = a.rt.seen_in_game
                && a.rt.presence.as_ref().is_some_and(|p| !p.in_game())
                && a.rt.game_session_start.is_none()
                && (now - a.last_use).num_minutes() >= 3;
            if off {
                let since = *a.rt.off_game_since.get_or_insert(now);
                (now - since).num_minutes() >= 3
            } else {
                a.rt.off_game_since = None;
                false
            }
        };

        if !running.is_empty() && !stuck {
            a.rt.rejoin_misses = 0;
            continue;
        }
        // Dos revisiones seguidas sin proceso, para no confundir un cierre con el arranque.
        a.rt.rejoin_misses += 1;
        if running.is_empty() && a.rt.rejoin_misses < 2 {
            continue;
        }
        a.rt.rejoin_misses = 0;
        a.rt.off_game_since = None;
        a.rt.rejoins.retain(|t| (now - *t).num_minutes() < 60);
        if a.rt.rejoins.len() >= max_per_hour {
            a.auto_rejoin = false;
            a.rt.rejoins.clear();
            changed = true;
            st().toast("error", format!("{}: auto-rejoin desactivado, se cerró {max_per_hour} veces en una hora", a.username));
            continue;
        }
        a.rt.rejoins.push(now);
        if stuck {
            for p in &running {
                process::kill(p.pid);
            }
        }
        due.push((a.user_id, req));
    }
    if changed {
        storage::save();
        st().emit_accounts();
    }
    due
}
