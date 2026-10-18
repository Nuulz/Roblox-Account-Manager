//! Juegos recientes y favoritos.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::state::st;
use crate::util::{data_path, write_atomic};

const GAMES_FILE: &str = "games.json";

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct GameEntry {
    pub place_id: i64,
    pub name: String,
    pub icon: String,
    /// Solo favoritos: enlace o accessCode de servidor privado.
    pub private_server: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct GamesData {
    pub recent: Vec<GameEntry>,
    pub favorites: Vec<GameEntry>,
}

impl GamesData {
    pub fn load() -> Self {
        if let Ok(text) = std::fs::read_to_string(data_path(GAMES_FILE)) {
            if let Ok(g) = serde_json::from_str(&text) {
                return g;
            }
        }
        let mut g = GamesData::default();
        g.recent = legacy_list("RecentGames.json");
        g.favorites = legacy_list("FavoriteGames.json");
        if !g.recent.is_empty() || !g.favorites.is_empty() {
            g.save();
        }
        g
    }

    pub fn save(&self) {
        if let Ok(json) = serde_json::to_vec_pretty(self) {
            let _ = write_atomic(&data_path(GAMES_FILE), &json);
        }
    }
}

fn legacy_list(file: &str) -> Vec<GameEntry> {
    let Ok(text) = std::fs::read_to_string(data_path(file)) else { return Vec::new() };
    let Ok(Value::Array(items)) = serde_json::from_str::<Value>(text.trim_start_matches('\u{feff}')) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|g| {
            let place_id = g["PlaceID"].as_i64().or_else(|| g["Details"]["placeId"].as_i64())?;
            let name = g["Name"]
                .as_str()
                .or_else(|| g["Details"]["name"].as_str())
                .unwrap_or("???")
                .to_string();
            Some(GameEntry {
                place_id,
                name,
                icon: g["ImageUrl"].as_str().unwrap_or_default().to_string(),
                private_server: g["PrivateServer"].as_str().unwrap_or_default().to_string(),
            })
        })
        .collect()
}

pub async fn add_recent(place_id: i64) {
    if place_id <= 0 {
        return;
    }
    let api = crate::roblox::api();
    let name = api.place_name(place_id).await.unwrap_or_else(|| place_id.to_string());
    let icon = api.place_icons(&[place_id]).await.remove(&place_id).unwrap_or_default();
    let max = st().settings.read().general.max_recent_games.max(1);
    {
        let mut games = st().games.write();
        games.recent.retain(|g| g.place_id != place_id);
        games.recent.insert(0, GameEntry { place_id, name, icon, private_server: String::new() });
        games.recent.truncate(max);
        games.save();
    }
    st().emit("games", st().games.read().clone());
}
