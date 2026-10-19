//! Ventanas de navegador: abrir roblox.com con la sesión de una cuenta o agregar una cuenta iniciando sesión.
//! Cada cuenta usa su propia carpeta de perfil para que sus cookies y caché no se mezclen.
//! WebView2 se bloquea si se leen cookies desde el hilo principal, por eso todo corre en tareas async.

use std::path::PathBuf;
use std::time::Duration;

use tauri::webview::Cookie;
use tauri::{Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use crate::ops::{self, CmdResult};
use crate::state::st;
use crate::util::data_path;

const LOGIN_LABEL: &str = "browser-login";
const COOKIE_NAME: &str = ".ROBLOSECURITY";

fn profile_dir(key: &str) -> PathBuf {
    data_path("WebView2Data").join(key)
}

fn roblox_url(path: &str) -> tauri::Url {
    format!("https://www.roblox.com{path}").parse().expect("URL fija válida")
}

fn build(label: &str, title: String, profile: &str) -> CmdResult<WebviewWindow> {
    let app = st().app.get().ok_or("La aplicación no está lista")?;
    WebviewWindowBuilder::new(app, label, WebviewUrl::External("about:blank".parse().map_err(ops::err)?))
        .title(title)
        .inner_size(1000.0, 760.0)
        .data_directory(profile_dir(profile))
        .build()
        .map_err(ops::err)
}

/// Abre roblox.com con la sesión de la cuenta ya iniciada.
pub async fn open_account(user_id: i64, path: Option<String>) -> CmdResult<()> {
    let app = st().app.get().ok_or("La aplicación no está lista")?;
    let label = format!("browser-{user_id}");
    if let Some(w) = app.get_webview_window(&label) {
        let _ = w.unminimize();
        return w.set_focus().map_err(ops::err);
    }
    let (cookie, name) = st()
        .accounts
        .read()
        .iter()
        .find(|a| a.user_id == user_id)
        .map(|a| (a.security_token.clone(), if a.alias.is_empty() { a.username.clone() } else { a.alias.clone() }))
        .ok_or("Cuenta no encontrada")?;
    let w = build(&label, format!("Roblox · {name}"), &user_id.to_string())?;
    w.set_cookie(
        Cookie::build((COOKIE_NAME, cookie))
            .domain(".roblox.com")
            .path("/")
            .secure(true)
            .http_only(true)
            .build(),
    )
    .map_err(ops::err)?;
    let path = path.filter(|p| p.starts_with('/')).unwrap_or_else(|| "/home".into());
    w.navigate(roblox_url(&path)).map_err(ops::err)
}

/// Abre la página de inicio de sesión en un perfil limpio y agrega la cuenta en cuanto aparece su cookie.
pub async fn login() -> CmdResult<()> {
    let app = st().app.get().ok_or("La aplicación no está lista")?;
    if let Some(w) = app.get_webview_window(LOGIN_LABEL) {
        return w.set_focus().map_err(ops::err);
    }
    cleanup_login_profiles();
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    let w = build(LOGIN_LABEL, "Iniciar sesión en Roblox".into(), &format!("_login_{stamp}"))?;
    w.navigate(roblox_url("/login")).map_err(ops::err)?;
    tauri::async_runtime::spawn(watch_login(w));
    Ok(())
}

async fn watch_login(w: WebviewWindow) {
    let mut tried = String::new();
    loop {
        tokio::time::sleep(Duration::from_millis(1000)).await;
        let Some(app) = st().app.get() else { return };
        if app.get_webview_window(LOGIN_LABEL).is_none() {
            return;
        }
        let Ok(cookies) = w.cookies_for_url(roblox_url("/")) else { continue };
        let Some(token) = cookies.iter().find(|c| c.name() == COOKIE_NAME).map(|c| c.value().to_string()) else { continue };
        if token == tried {
            continue;
        }
        tried = token.clone();
        match ops::add_account(&token, "").await {
            Ok(acc) => {
                st().toast("success", format!("Cuenta agregada: {}", acc.username));
                let _ = w.close();
                return;
            }
            Err(e) => log::warn!("Cookie capturada pero no válida todavía: {e}"),
        }
    }
}

/// Borra perfiles de inicio de sesión anteriores. Los que siguen abiertos están bloqueados y se saltan.
fn cleanup_login_profiles() {
    let Ok(dir) = std::fs::read_dir(data_path("WebView2Data")) else { return };
    for e in dir.flatten() {
        if e.file_name().to_string_lossy().starts_with("_login_") {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
}
