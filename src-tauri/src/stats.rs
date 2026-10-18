//! Estadísticas de juego por cuenta (tiempo por día y por juego), guardadas en stats.json.

use std::collections::BTreeMap;
use std::time::Duration;

use chrono::Local;
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::state::st;
use crate::util::{data_path, write_atomic};

const FILE: &str = "stats.json";
const TICK_SECS: u64 = 60;
const KEEP_DAYS: i64 = 120;

#[derive(Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct GameTime {
    name: String,
    seconds: u64,
    last_played: String,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct AccountStats {
    /// "AAAA-MM-DD" -> segundos
    days: BTreeMap<String, u64>,
    games: BTreeMap<i64, GameTime>,
    sessions: u64,
    last_session: String,
}

#[derive(Default, Serialize, Deserialize)]
struct Stats(BTreeMap<i64, AccountStats>);

static STATS: Lazy<Mutex<Stats>> = Lazy::new(|| {
    let s = std::fs::read_to_string(data_path(FILE)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    Mutex::new(s)
});

fn save(s: &Stats) {
    if let Ok(json) = serde_json::to_vec(s) {
        let _ = write_atomic(&data_path(FILE), &json);
    }
}

pub async fn stats_loop() {
    loop {
        tokio::time::sleep(Duration::from_secs(TICK_SECS)).await;
        if !st().is_unlocked() {
            continue;
        }
        let playing: Vec<(i64, i64, String)> = st()
            .accounts
            .read()
            .iter()
            .filter_map(|a| {
                let p = a.rt.presence.as_ref()?;
                let start = a.rt.game_session_start.map(|d| d.to_rfc3339()).unwrap_or_default();
                p.in_game().then(|| (a.user_id, p.root_place_id.or(p.place_id).unwrap_or(0), start))
            })
            .collect();

        let mut unknown = Vec::new();
        {
            let mut s = STATS.lock();
            let today = Local::now().format("%Y-%m-%d").to_string();
            let now = Local::now().to_rfc3339();
            for (id, place, start) in &playing {
                let acc = s.0.entry(*id).or_default();
                *acc.days.entry(today.clone()).or_default() += TICK_SECS;
                if acc.last_session != *start {
                    acc.sessions += 1;
                    acc.last_session = start.clone();
                }
                if *place > 0 {
                    let g = acc.games.entry(*place).or_default();
                    g.seconds += TICK_SECS;
                    g.last_played = now.clone();
                    if g.name.is_empty() {
                        unknown.push(*place);
                    }
                }
                let cutoff = (Local::now() - chrono::Duration::days(KEEP_DAYS)).format("%Y-%m-%d").to_string();
                acc.days.retain(|d, _| *d >= cutoff);
            }
            if !playing.is_empty() {
                save(&s);
            }
        }

        unknown.sort_unstable();
        unknown.dedup();
        for place in unknown {
            if let Some(name) = crate::roblox::api().place_name(place).await {
                let mut s = STATS.lock();
                for acc in s.0.values_mut() {
                    if let Some(g) = acc.games.get_mut(&place) {
                        g.name = name.clone();
                    }
                }
                save(&s);
            }
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameStat {
    place_id: i64,
    name: String,
    seconds: u64,
    last_played: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatsView {
    /// Últimos 30 días, del más antiguo al de hoy.
    days: Vec<(String, u64)>,
    games: Vec<GameStat>,
    total: u64,
    week: u64,
    sessions: u64,
}

pub fn view(user_id: i64) -> StatsView {
    let s = STATS.lock();
    let empty = AccountStats::default();
    let acc = s.0.get(&user_id).unwrap_or(&empty);
    let today = Local::now().date_naive();
    let days: Vec<(String, u64)> = (0..30)
        .rev()
        .map(|i| {
            let d = (today - chrono::Duration::days(i)).format("%Y-%m-%d").to_string();
            let v = acc.days.get(&d).copied().unwrap_or(0);
            (d, v)
        })
        .collect();
    let week = days.iter().rev().take(7).map(|(_, v)| v).sum();
    let mut games: Vec<GameStat> = acc
        .games
        .iter()
        .map(|(id, g)| GameStat { place_id: *id, name: g.name.clone(), seconds: g.seconds, last_played: g.last_played.clone() })
        .collect();
    games.sort_by(|a, b| b.seconds.cmp(&a.seconds));
    games.truncate(10);
    let total = acc.games.values().map(|g| g.seconds).sum();
    StatsView { days, games, total, week, sessions: acc.sessions }
}
