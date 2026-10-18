//! Actualizaciones desde los releases de GitHub (Nuulz/Roblox-Account-Manager).
//! El release debe tener como primer archivo un .zip con "Roblox Account Manager.exe",
//! igual que en la 3.x, para que las versiones antiguas también puedan actualizarse.

use std::io::Read;

use serde::Serialize;
use serde_json::Value;

pub const REPO: &str = "Nuulz/Roblox-Account-Manager";

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub current: String,
    pub latest: String,
    pub available: bool,
    pub notes: String,
    pub url: String,
}

fn current_version() -> semver::Version {
    semver::Version::parse(env!("CARGO_PKG_VERSION")).expect("versión del paquete")
}

fn parse_tag(tag: &str) -> Option<semver::Version> {
    let t = tag.trim().trim_start_matches(['v', 'V']);
    semver::Version::parse(t).ok().or_else(|| {
        // Etiquetas tipo "3.7" o "4"
        let mut parts: Vec<&str> = t.split('.').collect();
        while parts.len() < 3 {
            parts.push("0");
        }
        semver::Version::parse(&parts[..3].join(".")).ok()
    })
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(concat!("RobloxAccountManager/", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("cliente http")
}

pub async fn check() -> Result<UpdateInfo, String> {
    let v: Value = client()
        .get(format!("https://api.github.com/repos/{REPO}/releases/latest"))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    let tag = v["tag_name"].as_str().ok_or("Release sin tag_name")?;
    let latest = parse_tag(tag).ok_or_else(|| format!("Versión no válida: {tag}"))?;
    let url = v["assets"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|a| a["browser_download_url"].as_str())
        .find(|u| u.to_ascii_lowercase().ends_with(".zip"))
        .unwrap_or_default()
        .to_string();
    let cur = current_version();
    Ok(UpdateInfo {
        current: cur.to_string(),
        latest: latest.to_string(),
        available: latest > cur && !url.is_empty(),
        notes: v["body"].as_str().unwrap_or_default().to_string(),
        url,
    })
}

/// Descarga, reemplaza el ejecutable y reinicia la app.
pub async fn install(url: &str) -> Result<(), String> {
    let bytes = client().get(url).send().await.map_err(|e| e.to_string())?.bytes().await.map_err(|e| e.to_string())?;
    let new_exe = tokio::task::spawn_blocking(move || extract_exe(&bytes)).await.map_err(|e| e.to_string())??;

    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let old = exe.with_extension("exe.old");
    let _ = std::fs::remove_file(&old);
    // En Windows un .exe en ejecución no se puede sobrescribir, pero sí renombrar.
    std::fs::rename(&exe, &old).map_err(|e| format!("No se pudo reemplazar el ejecutable: {e}"))?;
    if let Err(e) = std::fs::write(&exe, &new_exe) {
        let _ = std::fs::rename(&old, &exe);
        return Err(format!("No se pudo escribir la nueva versión: {e}"));
    }
    crate::storage::save_now().ok();
    std::process::Command::new(&exe).spawn().map_err(|e| e.to_string())?;
    std::process::exit(0);
}

fn extract_exe(zip_bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(zip_bytes)).map_err(|e| e.to_string())?;
    let mut fallback = None;
    for i in 0..zip.len() {
        let name = zip.by_index(i).map_err(|e| e.to_string())?.name().to_string();
        let file = name.rsplit(['/', '\\']).next().unwrap_or(&name).to_string();
        if file.eq_ignore_ascii_case("Roblox Account Manager.exe") {
            return read_entry(&mut zip, i);
        }
        if fallback.is_none() && file.to_ascii_lowercase().ends_with(".exe") {
            fallback = Some(i);
        }
    }
    match fallback {
        Some(i) => read_entry(&mut zip, i),
        None => Err("El zip de la actualización no contiene un .exe".into()),
    }
}

fn read_entry<R: std::io::Read + std::io::Seek>(zip: &mut zip::ZipArchive<R>, i: usize) -> Result<Vec<u8>, String> {
    let mut f = zip.by_index(i).map_err(|e| e.to_string())?;
    let mut buf = Vec::with_capacity(f.size() as usize);
    f.read_to_end(&mut buf).map_err(|e| e.to_string())?;
    Ok(buf)
}

/// Borra el ejecutable anterior que deja una actualización.
pub fn cleanup() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::fs::remove_file(exe.with_extension("exe.old"));
    }
}
