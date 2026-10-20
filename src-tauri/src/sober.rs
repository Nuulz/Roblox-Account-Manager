//! Roblox en Linux a través de Sober (Flatpak org.vinegarhq.Sober).
//!
//! Cada cuenta tiene su propia "casa": Sober se abre con HOME apuntando a
//! `sober/<tracker>` dentro de la carpeta de datos, así cada una guarda su propia sesión.
//! Sober no se deja abrir dos veces en la misma red, así que con varias cuentas cada
//! instancia corre dentro de `pasta`, que le da una red propia con salida a internet.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

use once_cell::sync::Lazy;

pub const APP_ID: &str = "org.vinegarhq.Sober";
const SHARED: [&str; 4] = ["packages", "assets", "exe", "asset_overlay"];

/// Instancias lanzadas desde la app, por tracker.
static RUNNING: Lazy<Mutex<HashMap<String, Child>>> = Lazy::new(|| Mutex::new(HashMap::new()));

pub fn real_home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/tmp"))
}

pub fn profiles_dir() -> PathBuf {
    crate::util::data_path("sober")
}

fn sober_data(home: &Path) -> PathBuf {
    home.join(".var/app").join(APP_ID).join("data/sober")
}

fn sober_config(home: &Path) -> PathBuf {
    home.join(".var/app").join(APP_ID).join("config/sober/config.json")
}

pub fn installed() -> bool {
    Command::new("flatpak")
        .args(["info", APP_ID])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

pub fn pasta() -> Option<PathBuf> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path)
        .chain([real_home().join(".local/bin")])
        .map(|d| d.join("pasta"))
        .find(|p| p.is_file())
}

/// Prepara la casa de la cuenta. Los archivos del juego se copian desde el Sober normal
/// (no enlaces: Sober los bloquea y dos instancias no pueden compartirlos) y se vuelven
/// a copiar cuando el Sober normal se actualiza.
fn prepare_profile(tracker: &str, cookie: &str) -> Result<PathBuf, String> {
    let home = profiles_dir().join(tracker);
    let (src, dst) = (sober_data(&real_home()), sober_data(&home));
    if !src.join("state").exists() {
        return Err("Abre Sober una vez para que descargue Roblox antes de usarlo desde aquí.".into());
    }
    std::fs::create_dir_all(&dst).map_err(|e| e.to_string())?;

    let outdated = std::fs::read(src.join("state")).ok() != std::fs::read(dst.join("state")).ok();
    if outdated {
        for part in SHARED {
            let (s, d) = (src.join(part), dst.join(part));
            if !s.exists() {
                continue;
            }
            let _ = std::fs::remove_dir_all(&d);
            let ok = Command::new("cp").arg("-a").arg(&s).arg(&d).status().is_ok_and(|s| s.success());
            if !ok {
                return Err(format!("No se pudo copiar {part} de Sober"));
            }
        }
        std::fs::copy(src.join("state"), dst.join("state")).map_err(|e| e.to_string())?;
    }

    write_config(&home)?;

    let cookies = dst.join("cookies");
    std::fs::write(&cookies, format!(".ROBLOSECURITY={cookie}")).map_err(|e| e.to_string())?;
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(&cookies, std::fs::Permissions::from_mode(0o600));
    Ok(home)
}

/// Copia la configuración del Sober normal y le aplica el límite de FPS de los ajustes.
fn write_config(home: &Path) -> Result<(), String> {
    let (unlock, fps) = {
        let s = crate::state::st().settings.read();
        (s.general.unlock_fps, s.general.max_fps)
    };
    // El archivo trae comentarios // al inicio, que no son JSON.
    let strip = |t: String| t.lines().filter(|l| !l.trim_start().starts_with("//")).collect::<Vec<_>>().join("\n");
    let mut json: serde_json::Value = std::fs::read_to_string(sober_config(&real_home()))
        .ok()
        .map(strip)
        .and_then(|t| serde_json::from_str(&t).ok())
        .filter(|v: &serde_json::Value| v.is_object())
        .unwrap_or_else(|| serde_json::json!({}));
    if unlock {
        if !json["fflags"].is_object() {
            json["fflags"] = serde_json::json!({});
        }
        json["fflags"]["DFIntTaskSchedulerTargetFps"] = serde_json::json!(fps);
    }
    let path = sober_config(home);
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    std::fs::write(path, serde_json::to_string_pretty(&json).unwrap_or_default()).map_err(|e| e.to_string())
}

/// Enlace que Sober entiende para entrar a un juego.
pub fn join_uri(place_id: i64, job_id: &str, access_code: &str, link_code: &str) -> String {
    let mut uri = format!("roblox://experiences/start?placeId={place_id}");
    if !link_code.is_empty() {
        uri += &format!("&linkCode={}", urlencoding::encode(link_code));
    } else if !access_code.is_empty() {
        uri += &format!("&accessCode={}", urlencoding::encode(access_code));
    } else if !job_id.is_empty() {
        uri += &format!("&gameInstanceId={}", urlencoding::encode(job_id));
    }
    uri
}

/// Instancias vivas lanzadas desde la app: (tracker, pid).
pub fn running() -> Vec<(String, u32)> {
    let mut map = RUNNING.lock().unwrap();
    map.retain(|_, c| matches!(c.try_wait(), Ok(None)));
    map.iter().map(|(t, c)| (t.clone(), c.id())).collect()
}

pub fn stop(tracker: &str) {
    if let Some(mut c) = RUNNING.lock().unwrap().remove(tracker) {
        let _ = c.kill();
        let _ = c.wait();
    }
}

pub fn launch(tracker: &str, cookie: &str, uri: &str) -> Result<(), String> {
    if !installed() {
        return Err("Sober no está instalado (flatpak install flathub org.vinegarhq.Sober)".into());
    }
    let home = prepare_profile(tracker, cookie)?;
    stop(tracker);

    let others = running().len();
    let mut cmd = match pasta() {
        Some(p) => {
            let mut c = Command::new(p);
            c.args(["--config-net", "--quiet", "--"]);
            c.arg("flatpak");
            c
        }
        None if others > 0 => {
            return Err("Para abrir varias cuentas a la vez en Linux hace falta pasta (paquete passt).".into());
        }
        None => Command::new("flatpak"),
    };
    cmd.args(["run", APP_ID, uri])
        .env("HOME", &home)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let child = cmd.spawn().map_err(|e| format!("No se pudo abrir Sober: {e}"))?;
    log::info!("Sober lanzado para {tracker} (pid {})", child.id());
    RUNNING.lock().unwrap().insert(tracker.to_string(), child);
    Ok(())
}
