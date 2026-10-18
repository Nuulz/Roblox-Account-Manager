//! Región de cada servidor: IP del servidor (vía join-game-instance) → ubicación (ip-api), con caché.

use std::collections::HashMap;

use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::roblox::api;
use crate::state::st;
use crate::util::{data_path, write_atomic};

const FILE: &str = "regions.json";

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Region {
    pub label: String,
    pub city: String,
    pub country: String,
    pub country_code: String,
    pub lat: f64,
    pub lon: f64,
}

/// IP del servidor → ubicación (persistente: los centros de datos casi no cambian).
static BY_IP: Lazy<Mutex<HashMap<String, Region>>> = Lazy::new(|| {
    Mutex::new(std::fs::read_to_string(data_path(FILE)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default())
});
/// JobId → IP (solo mientras la app está abierta).
static JOB_IP: Lazy<Mutex<HashMap<String, String>>> = Lazy::new(Default::default);

fn save() {
    if let Ok(json) = serde_json::to_vec(&*BY_IP.lock()) {
        let _ = write_atomic(&data_path(FILE), &json);
    }
}

/// Solo usa la caché (no hace peticiones).
pub fn cached(job_id: &str) -> Option<Region> {
    let ip = JOB_IP.lock().get(job_id).cloned()?;
    BY_IP.lock().get(&ip).cloned()
}

pub async fn region(place_id: i64, job_id: &str) -> Result<Region, String> {
    if let Some(r) = cached(job_id) {
        return Ok(r);
    }
    let known = JOB_IP.lock().get(job_id).cloned();
    let ip = match known {
        Some(ip) => ip,
        None => {
            let cookie = st().any_cookie().ok_or("Hace falta al menos una cuenta válida")?;
            let v = api().join_game_instance(&cookie, place_id, job_id, false).await.map_err(|e| e.to_string())?;
            let js = &v["joinScript"];
            let ip = js["UdmuxEndpoints"][0]["Address"]
                .as_str()
                .or_else(|| js["MachineAddress"].as_str())
                .ok_or_else(|| v["message"].as_str().unwrap_or("Roblox no devolvió la IP del servidor").to_string())?
                .to_string();
            JOB_IP.lock().insert(job_id.to_string(), ip.clone());
            ip
        }
    };
    let hit = BY_IP.lock().get(&ip).cloned();
    if let Some(r) = hit {
        return Ok(r);
    }
    let (fmt, link) = {
        let g = &st().settings.read().general;
        (g.server_region_format.clone(), g.ip_api_link.clone())
    };
    let v = api().ip_info(&link, &ip).await.map_err(|e| format!("No se pudo ubicar {ip}: {e}"))?;
    let s = |k: &str| v[k].as_str().unwrap_or_default().to_string();
    let label = fmt
        .replace("<city>", &s("city"))
        .replace("<countryCode>", &s("countryCode"))
        .replace("<country>", &s("country"))
        .replace("<regionName>", &s("regionName"))
        .replace("<region>", &s("region"))
        .trim_matches([',', ' '])
        .to_string();
    let r = Region {
        label: if label.is_empty() { ip.clone() } else { label },
        city: s("city"),
        country: s("country"),
        country_code: s("countryCode"),
        lat: v["lat"].as_f64().unwrap_or(0.0),
        lon: v["lon"].as_f64().unwrap_or(0.0),
    };
    BY_IP.lock().insert(ip, r.clone());
    save();
    Ok(r)
}
