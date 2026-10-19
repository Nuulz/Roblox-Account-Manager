//! Amigos conectados de todas las cuentas: quién está en línea, en qué juego, y con qué cuentas eres su amigo.

use std::collections::BTreeMap;
use std::time::Duration;

use serde::Serialize;

use crate::ops::CmdResult;
use crate::roblox::api;
use crate::state::st;

/// Pausa entre cuentas para no disparar el límite de peticiones de Roblox.
const PACE: Duration = Duration::from_millis(500);

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Friend {
    pub id: i64,
    pub name: String,
    pub display_name: String,
    pub avatar: String,
    /// 0 desconectado, 1 en línea, 2 en juego, 3 en Studio.
    pub presence: i32,
    pub location: String,
    pub place_id: Option<i64>,
    pub game_id: Option<String>,
    /// Tus cuentas que son amigas de esta persona.
    pub via: Vec<i64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    pub friends: Vec<Friend>,
    pub failed: Vec<(i64, String)>,
}

/// Lo más informativo gana: en juego con lugar > en juego > Studio > en línea > desconectado.
fn score(p: i32, has_place: bool) -> i32 {
    match p {
        2 if has_place => 5,
        2 => 4,
        3 => 3,
        1 => 2,
        _ => 0,
    }
}

pub async fn overview(ids: Vec<i64>) -> CmdResult<Overview> {
    let mine: Vec<(i64, String)> = st()
        .accounts
        .read()
        .iter()
        .filter(|a| ids.contains(&a.user_id))
        .map(|a| (a.user_id, a.security_token.clone()))
        .collect();
    if mine.is_empty() {
        return Err("Selecciona al menos una cuenta".into());
    }
    let own: Vec<i64> = st().accounts.read().iter().map(|a| a.user_id).collect();
    let mut map: BTreeMap<i64, Friend> = BTreeMap::new();
    let mut failed = Vec::new();
    for (i, (uid, cookie)) in mine.iter().enumerate() {
        if i > 0 {
            tokio::time::sleep(PACE).await;
        }
        let list = match api().friend_ids(*uid).await {
            Ok(l) => l,
            Err(e) => {
                failed.push((*uid, e.to_string()));
                continue;
            }
        };
        // Con la cookie de la propia cuenta Roblox enseña el juego de sus amigos si su privacidad lo permite.
        let presence = api().presence(&list, Some(cookie)).await.unwrap_or_default();
        for fid in list {
            if own.contains(&fid) {
                continue;
            }
            let f = map.entry(fid).or_insert_with(|| Friend {
                id: fid,
                name: String::new(),
                display_name: String::new(),
                avatar: String::new(),
                presence: 0,
                location: String::new(),
                place_id: None,
                game_id: None,
                via: Vec::new(),
            });
            f.via.push(*uid);
            if let Some(p) = presence.iter().find(|p| p.user_id == fid) {
                let place = p.root_place_id.or(p.place_id);
                if score(p.user_presence_type, place.is_some()) > score(f.presence, f.place_id.is_some()) {
                    f.presence = p.user_presence_type;
                    f.location = p.last_location.clone().unwrap_or_default();
                    f.place_id = place;
                    f.game_id = p.game_id.clone();
                }
            }
        }
    }
    let fids: Vec<i64> = map.keys().copied().collect();
    let (names, avatars) = tokio::join!(api().user_names(&fids), api().headshots(&fids));
    let mut friends: Vec<Friend> = map
        .into_values()
        .map(|mut f| {
            if let Some((n, d)) = names.get(&f.id) {
                f.name = n.clone();
                f.display_name = d.clone();
            }
            f.avatar = avatars.get(&f.id).cloned().unwrap_or_default();
            f
        })
        .collect();
    friends.sort_by(|a, b| {
        score(b.presence, b.place_id.is_some())
            .cmp(&score(a.presence, a.place_id.is_some()))
            .then(b.via.len().cmp(&a.via.len()))
            .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(Overview { friends, failed })
}
