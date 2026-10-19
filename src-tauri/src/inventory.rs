//! Inventario de limiteds de las cuentas, con el valor de reventa (RAP) total.

use std::collections::HashMap;
use std::time::Duration;

use serde::Serialize;

use crate::ops::CmdResult;
use crate::roblox::api;
use crate::state::st;

/// Pausa entre páginas y entre cuentas para no disparar el límite de peticiones de Roblox.
const PACE: Duration = Duration::from_millis(500);
/// 10 páginas de 100: suficiente para casi cualquier cuenta sin tardar demasiado.
const MAX_PAGES: usize = 10;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub asset_id: i64,
    pub user_asset_id: i64,
    pub name: String,
    pub rap: i64,
    pub original_price: Option<i64>,
    pub serial: Option<i64>,
    pub stock: Option<i64>,
    pub on_hold: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountInventory {
    pub user_id: i64,
    pub private: bool,
    pub items: Vec<Item>,
    pub rap: i64,
    /// Había más de MAX_PAGES páginas: la lista y el total están incompletos.
    pub truncated: bool,
    pub error: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    pub accounts: Vec<AccountInventory>,
    pub icons: HashMap<i64, String>,
}

async fn one(user_id: i64) -> AccountInventory {
    let mut inv = AccountInventory { user_id, private: false, items: Vec::new(), rap: 0, truncated: false, error: String::new() };
    match api().can_view_inventory(user_id).await {
        Ok(false) => {
            inv.private = true;
            return inv;
        }
        Ok(true) => {}
        Err(e) => {
            inv.error = e.to_string();
            return inv;
        }
    }
    let mut cursor: Option<String> = None;
    for page in 0..MAX_PAGES {
        if page > 0 {
            tokio::time::sleep(PACE).await;
        }
        let (data, next) = match api().collectibles_page(user_id, cursor.as_deref()).await {
            Ok(r) => r,
            Err(e) => {
                inv.error = e.to_string();
                break;
            }
        };
        for d in data {
            let Some(asset_id) = d["assetId"].as_i64() else { continue };
            inv.items.push(Item {
                asset_id,
                user_asset_id: d["userAssetId"].as_i64().unwrap_or(0),
                name: d["name"].as_str().unwrap_or_default().to_string(),
                rap: d["recentAveragePrice"].as_i64().unwrap_or(0),
                original_price: d["originalPrice"].as_i64(),
                serial: d["serialNumber"].as_i64(),
                stock: d["assetStock"].as_i64(),
                on_hold: d["isOnHold"].as_bool().unwrap_or(false),
            });
        }
        cursor = next;
        if cursor.is_none() {
            break;
        }
        if page + 1 == MAX_PAGES {
            inv.truncated = true;
        }
    }
    inv.items.sort_by(|a, b| b.rap.cmp(&a.rap));
    inv.rap = inv.items.iter().map(|i| i.rap).sum();
    inv
}

pub async fn overview(ids: Vec<i64>) -> CmdResult<Overview> {
    let known: Vec<i64> = st().accounts.read().iter().map(|a| a.user_id).filter(|id| ids.contains(id)).collect();
    if known.is_empty() {
        return Err("Selecciona al menos una cuenta".into());
    }
    let mut accounts = Vec::new();
    for (i, id) in known.into_iter().enumerate() {
        if i > 0 {
            tokio::time::sleep(PACE).await;
        }
        accounts.push(one(id).await);
    }
    let mut assets: Vec<i64> = accounts.iter().flat_map(|a| a.items.iter().map(|i| i.asset_id)).collect();
    assets.sort_unstable();
    assets.dedup();
    let icons = api().asset_icons(&assets).await;
    Ok(Overview { accounts, icons })
}
