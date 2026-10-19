//! Servidores privados guardados: un enlace con nombre que se abre con cualquier cuenta.
//! Acepta el enlace clásico (`/games/{placeId}?privateServerLinkCode=...`) y el compartido
//! (`/share?code=...&type=Server`); el compartido se resuelve una vez al guardarlo.

use once_cell::sync::Lazy;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use crate::ops::{self, CmdResult, JoinRequest};
use crate::roblox::api;
use crate::state::st;
use crate::util::{data_path, write_atomic};

const FILE: &str = "private_servers.json";

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PrivateServer {
    pub id: i64,
    pub name: String,
    pub place_id: i64,
    /// Siempre en formato clásico, que es el que entiende `join_server`.
    pub link: String,
    pub game_name: String,
    pub icon: String,
}

static LIST: Lazy<RwLock<Vec<PrivateServer>>> = Lazy::new(|| {
    let list = std::fs::read_to_string(data_path(FILE))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    RwLock::new(list)
});

fn save(list: &[PrivateServer]) -> CmdResult<()> {
    let json = serde_json::to_vec_pretty(list).map_err(ops::err)?;
    write_atomic(&data_path(FILE), &json).map_err(ops::err)
}

pub fn list() -> Vec<PrivateServer> {
    LIST.read().clone()
}

enum Parsed {
    Classic { place_id: i64, code: String },
    Share { code: String },
}

fn parse(link: &str) -> Option<Parsed> {
    static CLASSIC: Lazy<regex::Regex> =
        Lazy::new(|| regex::Regex::new(r"roblox\.com/games/(\d+)[^?#\s]*\?[^#\s]*privateServerLinkCode=([\w-]+)").unwrap());
    static SHARE: Lazy<regex::Regex> =
        Lazy::new(|| regex::Regex::new(r"roblox\.com/share\?[^#\s]*\bcode=([\w-]+)").unwrap());
    if let Some(c) = CLASSIC.captures(link) {
        return Some(Parsed::Classic { place_id: c[1].parse().ok()?, code: c[2].to_string() });
    }
    let c = SHARE.captures(link)?;
    if link.contains("type=") && !link.contains("type=Server") {
        return None;
    }
    Some(Parsed::Share { code: c[1].to_string() })
}

/// Convierte cualquier enlace aceptado al formato clásico y completa el nombre y el icono del juego.
/// Un enlace compartido se resuelve con la cookie de `user_id` (o la primera cuenta con sesión).
pub async fn resolve(link: &str, user_id: Option<i64>) -> CmdResult<PrivateServer> {
    let link = link.trim();
    let (place_id, code) = match parse(link).ok_or("Ese enlace no es de un servidor privado de Roblox")? {
        Parsed::Classic { place_id, code } => (place_id, code),
        Parsed::Share { code } => {
            let cookie = {
                let accs = st().accounts.read();
                user_id
                    .and_then(|id| accs.iter().find(|a| a.user_id == id))
                    .or_else(|| accs.iter().find(|a| !a.rt.needs_login))
                    .map(|a| a.security_token.clone())
                    .ok_or("Se necesita al menos una cuenta con sesión válida para leer enlaces compartidos")?
            };
            api().resolve_server_share_link(&cookie, &code).await.map_err(ops::err)?
        }
    };
    let link = format!("https://www.roblox.com/games/{place_id}?privateServerLinkCode={code}");
    let (game_name, icon) = match api().place_info(place_id).await {
        Ok(i) => (i.name, i.icon),
        Err(_) => (String::new(), String::new()),
    };
    Ok(PrivateServer { id: 0, name: String::new(), place_id, link, game_name, icon })
}

/// Crea (id 0) o reemplaza un servidor guardado. El enlace se vuelve a resolver si cambió.
pub async fn upsert(mut v: PrivateServer, user_id: Option<i64>) -> CmdResult<PrivateServer> {
    let old_link = LIST.read().iter().find(|x| x.id == v.id && v.id != 0).map(|x| x.link.clone());
    if old_link.as_deref() != Some(v.link.trim()) {
        let r = resolve(&v.link, user_id).await?;
        v.place_id = r.place_id;
        v.link = r.link;
        v.game_name = r.game_name;
        v.icon = r.icon;
    }
    v.name = v.name.trim().to_string();
    if v.name.is_empty() {
        v.name = if v.game_name.is_empty() { format!("Servidor {}", v.place_id) } else { v.game_name.clone() };
    }
    let mut list = LIST.write();
    if v.id == 0 {
        v.id = chrono::Local::now().timestamp_millis();
        list.push(v.clone());
    } else {
        let slot = list.iter_mut().find(|x| x.id == v.id).ok_or("Servidor no encontrado")?;
        *slot = v.clone();
    }
    save(&list)?;
    Ok(v)
}

pub fn delete(id: i64) -> CmdResult<()> {
    let mut list = LIST.write();
    list.retain(|x| x.id != id);
    save(&list)
}

/// Une las cuentas indicadas al servidor privado, en orden y con el retraso de los ajustes.
pub async fn join(id: i64, ids: Vec<i64>) -> CmdResult<()> {
    if ids.is_empty() {
        return Err("Selecciona al menos una cuenta".into());
    }
    let v = LIST.read().iter().find(|x| x.id == id).cloned().ok_or("Servidor no encontrado")?;
    let req = JoinRequest { place_id: v.place_id, job_id: v.link, follow_user: false, join_vip: false };
    if ids.len() == 1 {
        return ops::join_server(ids[0], req).await;
    }
    tauri::async_runtime::spawn(ops::launch_many_with(ids, req, None, false));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_links() {
        assert!(matches!(
            parse("https://www.roblox.com/games/920587237/Adopt-Me?privateServerLinkCode=1234567890abc"),
            Some(Parsed::Classic { place_id: 920587237, ref code }) if code == "1234567890abc"
        ));
        assert!(matches!(
            parse("https://www.roblox.com/share?code=7f3e2a1b9c&type=Server"),
            Some(Parsed::Share { ref code }) if code == "7f3e2a1b9c"
        ));
        assert!(parse("https://www.roblox.com/share?code=abc&type=ExperienceDetails").is_none());
        assert!(parse("https://www.roblox.com/games/920587237/Adopt-Me").is_none());
        assert!(parse("https://evil.example/?privateServerLinkCode=1").is_none());
    }
}
