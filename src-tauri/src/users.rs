//! Buscar cualquier usuario de Roblox y ver su perfil, para seguirlo con tus cuentas.

use serde::Serialize;
use serde_json::Value;

use crate::ops::{self, CmdResult};
use crate::roblox::api;
use crate::state::st;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Found {
    pub id: i64,
    pub name: String,
    pub display_name: String,
    pub verified: bool,
    pub avatar: String,
    /// 0 desconectado, 1 en línea, 2 en juego, 3 en Studio; -1 si no se pudo leer.
    pub presence: i32,
    pub location: String,
    pub place_id: Option<i64>,
    /// La cuenta es una de las tuyas.
    pub mine: bool,
}

/// Coincidencia exacta por nombre primero y luego la búsqueda general de Roblox.
pub async fn search(keyword: &str) -> CmdResult<Vec<Found>> {
    let keyword = keyword.trim().trim_start_matches('@');
    if keyword.len() < 3 {
        return Err("Escribe al menos 3 caracteres".into());
    }
    let mut list: Vec<(i64, String, String, bool)> = Vec::new();
    if let Ok(id) = keyword.parse::<i64>() {
        if let Ok(u) = api().user_info(id).await {
            if let Some(name) = u["name"].as_str() {
                list.push((id, name.to_string(), u["displayName"].as_str().unwrap_or(name).to_string(), u["hasVerifiedBadge"].as_bool().unwrap_or(false)));
            }
        }
    }
    let cookie = st().any_cookie();
    if let Ok(id) = api().user_id(keyword, cookie.as_deref()).await {
        if !list.iter().any(|x| x.0 == id) {
            if let Ok(u) = api().user_info(id).await {
                let name = u["name"].as_str().unwrap_or(keyword).to_string();
                list.push((id, name.clone(), u["displayName"].as_str().unwrap_or(&name).to_string(), u["hasVerifiedBadge"].as_bool().unwrap_or(false)));
            }
        }
    }
    match api().search_users(keyword).await {
        Ok(found) => {
            for u in found {
                let Some(id) = u["id"].as_i64() else { continue };
                if list.iter().any(|x| x.0 == id) {
                    continue;
                }
                let name = u["name"].as_str().unwrap_or_default().to_string();
                list.push((id, name.clone(), u["displayName"].as_str().unwrap_or(&name).to_string(), u["hasVerifiedBadge"].as_bool().unwrap_or(false)));
            }
        }
        Err(e) if list.is_empty() => return Err(ops::err(e)),
        Err(_) => {}
    }
    let ids: Vec<i64> = list.iter().map(|x| x.0).collect();
    let (avatars, presence) = tokio::join!(api().headshots_big(&ids), api().presence(&ids, cookie.as_deref()));
    let presence = presence.unwrap_or_default();
    let mine: Vec<i64> = st().accounts.read().iter().map(|a| a.user_id).collect();
    Ok(list
        .into_iter()
        .map(|(id, name, display_name, verified)| {
            let p = presence.iter().find(|p| p.user_id == id);
            Found {
                id,
                name,
                display_name,
                verified,
                avatar: avatars.get(&id).cloned().unwrap_or_default(),
                presence: p.map(|p| p.user_presence_type).unwrap_or(-1),
                location: p.and_then(|p| p.last_location.clone()).unwrap_or_default(),
                place_id: p.and_then(|p| p.root_place_id.or(p.place_id)),
                mine: mine.contains(&id),
            }
        })
        .collect())
}

/// Juegos de una lista v2 de games.roblox.com (favoritos o creados), con icono.
async fn game_list(url: &str) -> Vec<Value> {
    let Ok(v) = api().public_json(url).await else { return Vec::new() };
    let raw = v["data"].as_array().cloned().unwrap_or_default();
    let ids: Vec<i64> = raw.iter().filter_map(|g| g["id"].as_i64()).collect();
    let icons = api().universe_icons(&ids).await;
    raw.iter()
        .filter_map(|g| {
            let universe = g["id"].as_i64()?;
            Some(serde_json::json!({
                "universeId": universe,
                "placeId": g["rootPlace"]["id"].as_i64(),
                "name": g["name"].as_str().unwrap_or_default(),
                "visits": g["placeVisits"].as_i64().unwrap_or(0),
                "icon": icons.get(&universe).cloned().unwrap_or_default(),
            }))
        })
        .collect()
}

/// Limiteds de la primera página (hasta 100): suficiente para un resumen del perfil.
async fn limiteds_summary(id: i64) -> Value {
    match api().can_view_inventory(id).await {
        Ok(false) => return serde_json::json!({ "private": true }),
        Err(_) => return Value::Null,
        Ok(true) => {}
    }
    match api().collectibles_page(id, None).await {
        Ok((items, next)) => serde_json::json!({
            "private": false,
            "count": items.len(),
            "more": next.is_some(),
            "rap": items.iter().filter_map(|i| i["recentAveragePrice"].as_i64()).sum::<i64>(),
        }),
        Err(_) => Value::Null,
    }
}

/// Perfil completo con datos públicos: si una parte falla, el resto se muestra igual.
pub async fn profile(id: i64) -> CmdResult<Value> {
    let base = format!("https://friends.roblox.com/v1/users/{id}");
    let (fr, fo, fg) = (format!("{base}/friends/count"), format!("{base}/followers/count"), format!("{base}/followings/count"));
    let history_url = format!("https://users.roblox.com/v1/users/{id}/username-history?limit=50&sortOrder=Desc");
    let badges_url = format!("https://accountinformation.roblox.com/v1/users/{id}/roblox-badges");
    let primary_url = format!("https://groups.roblox.com/v1/users/{id}/groups/primary/role");
    let fav_url = format!("https://games.roblox.com/v2/users/{id}/favorite/games?limit=10");
    let made_url = format!("https://games.roblox.com/v2/users/{id}/games?limit=10&sortOrder=Desc");
    let (info, friends, followers, following, history, badges, primary, groups, favorites, created, limiteds) = tokio::join!(
        api().user_info(id),
        api().count(&fr),
        api().count(&fo),
        api().count(&fg),
        api().public_json(&history_url),
        api().public_json(&badges_url),
        api().public_json(&primary_url),
        api().user_groups(id),
        game_list(&fav_url),
        game_list(&made_url),
        limiteds_summary(id),
    );
    let info = info.map_err(ops::err)?;
    let previous: Vec<String> = history
        .ok()
        .and_then(|v| v["data"].as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|n| n["name"].as_str().map(str::to_string))
        .collect();
    let badges: Vec<Value> = badges
        .ok()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .map(|b| serde_json::json!({ "name": b["name"], "image": b["imageUrl"], "description": b["description"] }))
        .collect();
    let primary_id = primary.ok().and_then(|v| v["group"]["id"].as_i64());
    let groups: Vec<Value> = groups
        .unwrap_or_default()
        .iter()
        .map(|g| {
            serde_json::json!({
                "id": g["group"]["id"],
                "name": g["group"]["name"],
                "members": g["group"]["memberCount"],
                "role": g["role"]["name"],
                "primary": g["group"]["id"].as_i64().is_some() && g["group"]["id"].as_i64() == primary_id,
                "owner": g["group"]["owner"]["userId"].as_i64() == Some(id),
            })
        })
        .collect();
    Ok(serde_json::json!({
        "id": id,
        "description": info["description"].as_str().unwrap_or_default(),
        "created": info["created"].as_str().unwrap_or_default(),
        "banned": info["isBanned"].as_bool().unwrap_or(false),
        "friends": friends,
        "followers": followers,
        "following": following,
        "previousNames": previous,
        "badges": badges,
        "groups": groups,
        "favorites": favorites,
        "createdGames": created,
        "limiteds": limiteds,
    }))
}
