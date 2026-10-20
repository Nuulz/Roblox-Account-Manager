//! Parches de ClientAppSettings.json (desbloqueo de FPS o archivo personalizado).

use std::path::PathBuf;

use crate::state::st;

/// Carpeta de la versión instalada de Roblox (la que tenga RobloxPlayerBeta.exe).
#[cfg(windows)]
pub fn roblox_version_folder() -> Option<PathBuf> {
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_CLASSES_ROOT, RRF_RT_REG_SZ};
    // 1) La que registra el protocolo roblox-player
    unsafe {
        let mut buf = [0u16; 1024];
        let mut size = (buf.len() * 2) as u32;
        let ok = RegGetValueW(
            HKEY_CLASSES_ROOT,
            &windows::core::HSTRING::from(r"roblox-player\DefaultIcon"),
            None,
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr() as *mut _),
            Some(&mut size),
        );
        if ok.is_ok() {
            let len = buf.iter().position(|&c| c == 0).unwrap_or(0);
            let raw = String::from_utf16_lossy(&buf[..len]);
            let exe = raw.trim_matches('"').split(',').next().unwrap_or_default().trim_matches('"').to_string();
            if let Some(dir) = PathBuf::from(exe).parent() {
                if dir.join("RobloxPlayerBeta.exe").exists() {
                    return Some(dir.to_path_buf());
                }
            }
        }
    }
    // 2) La más reciente en %LOCALAPPDATA%\Roblox\Versions
    let versions = crate::util::local_app_data().join("Roblox").join("Versions");
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
    for entry in std::fs::read_dir(versions).ok()?.flatten() {
        let p = entry.path();
        if p.join("RobloxPlayerBeta.exe").exists() {
            let t = entry.metadata().and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH);
            if best.as_ref().map(|(bt, _)| t > *bt).unwrap_or(true) {
                best = Some((t, p));
            }
        }
    }
    best.map(|(_, p)| p)
}

/// En Linux Sober guarda las versiones por su cuenta; ver `sober::launch`.
#[cfg(not(windows))]
pub fn roblox_version_folder() -> Option<PathBuf> {
    None
}

pub fn patch() {
    let (custom, unlock, fps) = {
        let s = st().settings.read();
        (s.general.custom_client_settings.clone(), s.general.unlock_fps, s.general.max_fps)
    };
    if custom.is_empty() && !unlock {
        return;
    }
    let Some(folder) = roblox_version_folder() else {
        log::warn!("No se encontró la carpeta de Roblox para aplicar ClientAppSettings");
        return;
    };
    let dir = folder.join("ClientSettings");
    let _ = std::fs::create_dir_all(&dir);
    let file = dir.join("ClientAppSettings.json");
    if !custom.is_empty() && std::path::Path::new(&custom).exists() {
        if let Err(e) = std::fs::copy(&custom, &file) {
            log::error!("No se pudo copiar ClientAppSettings personalizado: {e}");
        }
        return;
    }
    let mut json: serde_json::Value = std::fs::read_to_string(&file)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .filter(|v: &serde_json::Value| v.is_object())
        .unwrap_or_else(|| serde_json::json!({}));
    json["DFIntTaskSchedulerTargetFps"] = serde_json::json!(fps);
    let _ = std::fs::write(&file, json.to_string());
}
