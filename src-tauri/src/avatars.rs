//! Copiar el avatar de una cuenta (tuya o de cualquier usuario) a tus cuentas.
//! Solo se ponen los objetos que cada cuenta ya tiene; no se compra nada.

use std::time::Duration;

use serde::Serialize;
use serde_json::Value;

use crate::ops::{self, CmdResult};
use crate::roblox::api;
use crate::state::st;

/// Pausa entre cuentas para no disparar el límite de peticiones de Roblox.
const PACE: Duration = Duration::from_millis(1000);

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub user_id: i64,
    pub image: String,
    pub avatar_type: String,
    pub items: Vec<(i64, String, String)>,
    /// El JSON tal como lo devuelve Roblox; se reenvía sin cambios al aplicar.
    pub avatar: Value,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Applied {
    pub user_id: i64,
    pub ok: bool,
    /// Objetos que la cuenta no tiene y por eso no se pusieron.
    pub missing: usize,
    pub error: String,
}

pub async fn preview(user_id: i64) -> CmdResult<Preview> {
    let url = format!("https://avatar.roblox.com/v1/users/{user_id}/avatar");
    let (avatar, image) = tokio::join!(api().public_json(&url), api().avatar_image(user_id));
    let avatar = avatar.map_err(ops::err)?;
    let items = avatar["assets"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|a| {
            Some((
                a["id"].as_i64()?,
                a["name"].as_str().unwrap_or_default().to_string(),
                a["assetType"]["name"].as_str().unwrap_or_default().to_string(),
            ))
        })
        .collect();
    Ok(Preview {
        user_id,
        image: image.unwrap_or_default(),
        avatar_type: avatar["playerAvatarType"].as_str().unwrap_or_default().to_string(),
        items,
        avatar,
    })
}

pub async fn apply(source: i64, targets: Vec<i64>) -> CmdResult<Vec<Applied>> {
    let p = preview(source).await?;
    let list: Vec<(i64, String)> = st()
        .accounts
        .read()
        .iter()
        .filter(|a| targets.contains(&a.user_id) && a.user_id != source)
        .map(|a| (a.user_id, a.security_token.clone()))
        .collect();
    if list.is_empty() {
        return Err("Selecciona al menos una cuenta distinta de la de origen".into());
    }
    let mut out = Vec::new();
    for (i, (id, cookie)) in list.into_iter().enumerate() {
        if i > 0 {
            tokio::time::sleep(PACE).await;
        }
        out.push(match api().set_avatar(&cookie, &p.avatar).await {
            Ok(missing) => Applied { user_id: id, ok: true, missing: missing.len(), error: String::new() },
            Err(e) => Applied { user_id: id, ok: false, missing: 0, error: e.to_string() },
        });
    }
    let ids: Vec<i64> = out.iter().filter(|r| r.ok).map(|r| r.user_id).collect();
    tauri::async_runtime::spawn(ops::refresh_avatars(ids));
    Ok(out)
}
