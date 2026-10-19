//! Grupos de Roblox de todas las cuentas: en qué grupos está cada una y con qué rango.

use std::collections::BTreeMap;
use std::time::Duration;

use serde::Serialize;

use crate::ops::CmdResult;
use crate::roblox::api;
use crate::state::st;

/// Pausa entre cuentas para no disparar el límite de peticiones de Roblox.
const PACE: Duration = Duration::from_millis(400);

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Member {
    pub user_id: i64,
    pub role: String,
    pub rank: i64,
    pub owner: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub id: i64,
    pub name: String,
    pub member_count: i64,
    pub verified: bool,
    pub public_entry: bool,
    pub icon: String,
    pub members: Vec<Member>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    pub groups: Vec<Group>,
    /// Cuentas cuya lista no se pudo leer, con el motivo.
    pub failed: Vec<(i64, String)>,
}

pub async fn overview(ids: Vec<i64>) -> CmdResult<Overview> {
    let known: Vec<i64> = st().accounts.read().iter().map(|a| a.user_id).filter(|id| ids.contains(id)).collect();
    if known.is_empty() {
        return Err("Selecciona al menos una cuenta".into());
    }
    let mut groups: BTreeMap<i64, Group> = BTreeMap::new();
    let mut failed = Vec::new();
    for (i, id) in known.iter().enumerate() {
        if i > 0 {
            tokio::time::sleep(PACE).await;
        }
        let list = match api().user_groups(*id).await {
            Ok(l) => l,
            Err(e) => {
                failed.push((*id, e.to_string()));
                continue;
            }
        };
        for item in list {
            let g = &item["group"];
            let Some(gid) = g["id"].as_i64() else { continue };
            let entry = groups.entry(gid).or_insert_with(|| Group {
                id: gid,
                name: g["name"].as_str().unwrap_or_default().to_string(),
                member_count: g["memberCount"].as_i64().unwrap_or(0),
                verified: g["hasVerifiedBadge"].as_bool().unwrap_or(false),
                public_entry: g["publicEntryAllowed"].as_bool().unwrap_or(false),
                icon: String::new(),
                members: Vec::new(),
            });
            entry.members.push(Member {
                user_id: *id,
                role: item["role"]["name"].as_str().unwrap_or_default().to_string(),
                rank: item["role"]["rank"].as_i64().unwrap_or(0),
                owner: g["owner"]["userId"].as_i64() == Some(*id),
            });
        }
    }
    let gids: Vec<i64> = groups.keys().copied().collect();
    let icons = api().group_icons(&gids).await;
    let mut out: Vec<Group> = groups
        .into_values()
        .map(|mut g| {
            g.icon = icons.get(&g.id).cloned().unwrap_or_default();
            g.members.sort_by(|a, b| b.rank.cmp(&a.rank));
            g
        })
        .collect();
    out.sort_by(|a, b| b.members.len().cmp(&a.members.len()).then(b.member_count.cmp(&a.member_count)));
    Ok(Overview { groups: out, failed })
}
