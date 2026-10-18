//! Cliente HTTP para las APIs web de Roblox.
//! Todas las funciones reciben la cookie como texto para no mantener bloqueos durante los `await`.

use std::collections::HashMap;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use once_cell::sync::Lazy;
use reqwest::header::{HeaderMap, HeaderValue, COOKIE, REFERER, SET_COOKIE, USER_AGENT};
use reqwest::{Client, Method, RequestBuilder, Response, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const BROWSER_UA: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";
const GAME_REFERER: &str = "https://www.roblox.com/games/4924922222/Brookhaven-RP";

pub struct Api {
    http: Client,
    join: Client,
}

pub static API: Lazy<Api> = Lazy::new(|| {
    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::from_static(BROWSER_UA));
    let http = Client::builder()
        .default_headers(headers)
        .timeout(Duration::from_secs(25))
        .redirect(reqwest::redirect::Policy::none())
        .gzip(true)
        .build()
        .expect("http client");
    let join = Client::builder()
        .user_agent("Roblox/WinInet")
        .timeout(Duration::from_secs(25))
        .build()
        .expect("join client");
    Api { http, join }
});

pub fn api() -> &'static Api {
    &API
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Presence {
    pub user_presence_type: i32,
    pub last_location: Option<String>,
    pub place_id: Option<i64>,
    pub root_place_id: Option<i64>,
    pub game_id: Option<String>,
    pub universe_id: Option<i64>,
    pub user_id: i64,
}

impl Presence {
    pub fn in_game(&self) -> bool {
        self.user_presence_type == 2
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerInfo {
    pub id: String,
    pub max_players: i64,
    pub playing: i64,
    pub fps: f64,
    pub ping: i64,
    pub kind: String,
    pub name: String,
    pub access_code: String,
    pub vip_server_id: i64,
    #[serde(skip_serializing)]
    pub player_tokens: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServersPage {
    pub data: Vec<ServerInfo>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaceInfo {
    pub place_id: i64,
    pub name: String,
    pub description: String,
    pub icon: String,
    pub universe_id: i64,
    pub max_players: i64,
    pub visits: i64,
    pub created: String,
    pub updated: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthUser {
    pub id: i64,
    pub name: String,
    pub display_name: String,
}

fn cookie_header(cookie: &str) -> String {
    format!(".ROBLOSECURITY={cookie}")
}

pub fn new_cookie_from(resp: &Response) -> Option<String> {
    resp.headers().get_all(SET_COOKIE).iter().find_map(|v| {
        let s = v.to_str().ok()?;
        let rest = s.strip_prefix(".ROBLOSECURITY=")?;
        let val = rest.split(';').next()?.to_string();
        (!val.is_empty()).then_some(val)
    })
}

fn error_message(body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v["errors"][0]["message"].as_str().map(str::to_string))
        .unwrap_or_else(|| body.chars().take(300).collect())
}

impl Api {
    pub fn req(&self, method: Method, url: &str, cookie: Option<&str>) -> RequestBuilder {
        let mut r = self.http.request(method, url);
        if let Some(c) = cookie.filter(|c| !c.is_empty()) {
            r = r.header(COOKIE, cookie_header(c));
        }
        r
    }

    /// Obtiene un X-CSRF-TOKEN válido para la cookie.
    pub async fn csrf(&self, cookie: &str) -> Result<String> {
        let resp = self
            .req(Method::POST, "https://auth.roblox.com/v1/authentication-ticket/", Some(cookie))
            .header(REFERER, GAME_REFERER)
            .header("Content-Length", "0")
            .send()
            .await?;
        if let Some(t) = resp.headers().get("x-csrf-token").and_then(|v| v.to_str().ok()) {
            if !t.is_empty() {
                return Ok(t.to_string());
            }
        }
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        bail!("[{}] La sesión de la cuenta expiró o es inválida. {}", status.as_u16(), error_message(&body))
    }

    /// Petición autenticada con cookie + CSRF.
    pub async fn authed(&self, method: Method, url: &str, cookie: &str) -> Result<RequestBuilder> {
        let token = self.csrf(cookie).await?;
        Ok(self.req(method, url, Some(cookie)).header("X-CSRF-TOKEN", token))
    }

    /// Ok(Some) = sesión válida, Ok(None) = cerrada (401), Err = no se pudo saber.
    pub async fn authenticated_user(&self, cookie: &str) -> Result<Option<AuthUser>> {
        let resp = self
            .req(Method::GET, "https://users.roblox.com/v1/users/authenticated", Some(cookie))
            .send()
            .await?;
        match resp.status() {
            StatusCode::OK => {
                let v: Value = resp.json().await?;
                let id = v["id"].as_i64().unwrap_or(0);
                if id <= 0 {
                    bail!("Respuesta inesperada");
                }
                Ok(Some(AuthUser {
                    id,
                    name: v["name"].as_str().unwrap_or_default().to_string(),
                    display_name: v["displayName"].as_str().unwrap_or_default().to_string(),
                }))
            }
            StatusCode::UNAUTHORIZED => Ok(None),
            s => bail!("Estado {s}"),
        }
    }

    /// Valida una cookie con reintentos (la API limita si se agregan muchas seguidas).
    pub async fn validate_cookie(&self, cookie: &str) -> Result<AuthUser> {
        let mut last_err = anyhow!("Cookie inválida");
        for attempt in 0..4u64 {
            if attempt > 0 {
                tokio::time::sleep(Duration::from_millis(1500 * attempt)).await;
            }
            match self.authenticated_user(cookie).await {
                Ok(Some(u)) => return Ok(u),
                Ok(None) => bail!("La cookie no es válida o la sesión fue cerrada"),
                Err(e) => last_err = e,
            }
        }
        Err(last_err)
    }

    pub async fn auth_ticket(&self, cookie: &str) -> Result<String> {
        let resp = self
            .authed(Method::POST, "https://auth.roblox.com/v1/authentication-ticket/", cookie)
            .await?
            .header(REFERER, GAME_REFERER)
            .header("RBXAuthenticationNegotiation", "1")
            .header("Content-Type", "application/json")
            .body("{}")
            .send()
            .await?;
        if let Some(t) = resp.headers().get("rbx-authentication-ticket").and_then(|v| v.to_str().ok()) {
            return Ok(t.to_string());
        }
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        bail!("No se pudo obtener el ticket de autenticación [{}] {}", status.as_u16(), error_message(&body))
    }

    pub async fn user_id(&self, username: &str, cookie: Option<&str>) -> Result<i64> {
        let resp = self
            .req(Method::POST, "https://users.roblox.com/v1/usernames/users", cookie)
            .json(&json!({ "usernames": [username], "excludeBannedUsers": false }))
            .send()
            .await?;
        let status = resp.status();
        let v: Value = resp.json().await.unwrap_or(Value::Null);
        v["data"][0]["id"]
            .as_i64()
            .ok_or_else(|| anyhow!("No se encontró el usuario \"{username}\" [{}]", status.as_u16()))
    }

    pub async fn user_info(&self, user_id: i64) -> Result<Value> {
        Ok(self
            .req(Method::GET, &format!("https://users.roblox.com/v1/users/{user_id}"), None)
            .send()
            .await?
            .json()
            .await?)
    }

    pub async fn presence(&self, ids: &[i64], cookie: Option<&str>) -> Result<Vec<Presence>> {
        let mut out = Vec::new();
        for chunk in ids.chunks(50) {
            let resp = self
                .req(Method::POST, "https://presence.roblox.com/v1/presence/users", cookie)
                .json(&json!({ "userIds": chunk }))
                .send()
                .await?;
            if !resp.status().is_success() {
                bail!("Presencia: estado {}", resp.status());
            }
            let v: Value = resp.json().await?;
            if let Some(arr) = v["userPresences"].as_array() {
                for p in arr {
                    if let Ok(p) = serde_json::from_value::<Presence>(p.clone()) {
                        out.push(p);
                    }
                }
            }
        }
        Ok(out)
    }

    pub async fn headshots(&self, ids: &[i64]) -> HashMap<i64, String> {
        let mut out = HashMap::new();
        for chunk in ids.chunks(100) {
            let list = chunk.iter().map(i64::to_string).collect::<Vec<_>>().join(",");
            let url = format!(
                "https://thumbnails.roblox.com/v1/users/avatar-headshot?userIds={list}&size=48x48&format=Png&isCircular=false"
            );
            if let Ok(resp) = self.req(Method::GET, &url, None).send().await {
                if let Ok(v) = resp.json::<Value>().await {
                    for d in v["data"].as_array().into_iter().flatten() {
                        if let (Some(id), Some(img)) = (d["targetId"].as_i64(), d["imageUrl"].as_str()) {
                            out.insert(id, img.to_string());
                        }
                    }
                }
            }
        }
        out
    }

    pub async fn place_icons(&self, place_ids: &[i64]) -> HashMap<i64, String> {
        let mut out = HashMap::new();
        for chunk in place_ids.chunks(50) {
            let list = chunk.iter().map(i64::to_string).collect::<Vec<_>>().join(",");
            let url = format!(
                "https://thumbnails.roblox.com/v1/places/gameicons?placeIds={list}&returnPolicy=PlaceHolder&size=150x150&format=Png&isCircular=false"
            );
            if let Ok(resp) = self.req(Method::GET, &url, None).send().await {
                if let Ok(v) = resp.json::<Value>().await {
                    for d in v["data"].as_array().into_iter().flatten() {
                        if let (Some(id), Some(img)) = (d["targetId"].as_i64(), d["imageUrl"].as_str()) {
                            out.insert(id, img.to_string());
                        }
                    }
                }
            }
        }
        out
    }

    pub async fn universe_icons(&self, universe_ids: &[i64]) -> HashMap<i64, String> {
        let mut out = HashMap::new();
        for chunk in universe_ids.chunks(50) {
            let list = chunk.iter().map(i64::to_string).collect::<Vec<_>>().join(",");
            let url = format!(
                "https://thumbnails.roblox.com/v1/games/icons?universeIds={list}&returnPolicy=PlaceHolder&size=150x150&format=Png&isCircular=false"
            );
            if let Ok(resp) = self.req(Method::GET, &url, None).send().await {
                if let Ok(v) = resp.json::<Value>().await {
                    for d in v["data"].as_array().into_iter().flatten() {
                        if let (Some(id), Some(img)) = (d["targetId"].as_i64(), d["imageUrl"].as_str()) {
                            out.insert(id, img.to_string());
                        }
                    }
                }
            }
        }
        out
    }

    pub async fn place_name(&self, place_id: i64) -> Option<String> {
        let v: Value = self
            .req(Method::GET, &format!("https://economy.roblox.com/v2/assets/{place_id}/details"), None)
            .header("Accept", "application/json")
            .send()
            .await
            .ok()?
            .json()
            .await
            .ok()?;
        v["Name"].as_str().map(str::to_string)
    }

    pub async fn universe_of_place(&self, place_id: i64) -> Result<i64> {
        let v: Value = self
            .req(Method::GET, &format!("https://apis.roblox.com/universes/v1/places/{place_id}/universe"), None)
            .send()
            .await?
            .json()
            .await?;
        v["universeId"].as_i64().ok_or_else(|| anyhow!("No se encontró el universo del lugar {place_id}"))
    }

    pub async fn universe_places(&self, universe_id: i64) -> Result<Vec<(i64, String)>> {
        let mut out = Vec::new();
        let mut cursor = String::new();
        loop {
            let url = format!(
                "https://develop.roblox.com/v1/universes/{universe_id}/places?sortOrder=Asc&limit=100&cursor={cursor}"
            );
            let v: Value = self.req(Method::GET, &url, None).send().await?.json().await?;
            for p in v["data"].as_array().into_iter().flatten() {
                out.push((p["id"].as_i64().unwrap_or(0), p["name"].as_str().unwrap_or_default().to_string()));
            }
            match v["nextPageCursor"].as_str() {
                Some(c) if !c.is_empty() => cursor = c.to_string(),
                _ => break,
            }
        }
        Ok(out)
    }

    pub async fn servers_page(&self, place_id: i64, cursor: Option<&str>, vip: bool, cookie: Option<&str>) -> Result<ServersPage> {
        let cursor_q = cursor.filter(|c| !c.is_empty()).map(|c| format!("&cursor={c}")).unwrap_or_default();
        let url = if vip {
            format!("https://games.roblox.com/v1/games/{place_id}/private-servers?limit=25{cursor_q}")
        } else {
            format!("https://games.roblox.com/v1/games/{place_id}/servers/Public?sortOrder=Asc&excludeFullGames=false&limit=100{cursor_q}")
        };
        let resp = self.req(Method::GET, &url, cookie).header("Accept", "application/json").send().await?;
        let status = resp.status();
        if status == StatusCode::TOO_MANY_REQUESTS {
            bail!("Roblox está limitando las peticiones, espera unos segundos");
        }
        let v: Value = resp.json().await.context("Respuesta de servidores inválida")?;
        if !status.is_success() {
            bail!("[{}] {}", status.as_u16(), error_message(&v.to_string()));
        }
        let data = v["data"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|s| ServerInfo {
                id: s["id"].as_str().unwrap_or_default().to_string(),
                max_players: s["maxPlayers"].as_i64().unwrap_or(0),
                playing: s["playing"].as_i64().unwrap_or(0),
                fps: s["fps"].as_f64().unwrap_or(0.0),
                ping: s["ping"].as_i64().unwrap_or(0),
                kind: if vip { "VIP".into() } else { "Public".into() },
                name: s["name"].as_str().unwrap_or_default().to_string(),
                access_code: s["accessCode"].as_str().unwrap_or_default().to_string(),
                vip_server_id: s["vipServerId"].as_i64().unwrap_or(0),
                player_tokens: s["playerTokens"]
                    .as_array()
                    .map(|a| a.iter().filter_map(|t| t.as_str().map(str::to_string)).collect())
                    .unwrap_or_default(),
            })
            .collect();
        Ok(ServersPage {
            data,
            next_cursor: v["nextPageCursor"].as_str().filter(|c| !c.is_empty()).map(str::to_string),
        })
    }

    /// Elige un JobId aleatorio (o el más vacío) entre los servidores públicos con hueco.
    pub async fn random_job_id(&self, place_id: i64, lowest: bool, max_pages: u32) -> String {
        use rand::seq::SliceRandom;
        let mut valid: Vec<String> = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..max_pages.max(1) {
            let Ok(page) = self.servers_page(place_id, cursor.as_deref(), false, None).await else { break };
            for s in &page.data {
                if s.playing != s.max_players && s.playing > 0 && s.max_players > 1 {
                    valid.push(s.id.clone());
                }
            }
            if lowest {
                break;
            }
            match page.next_cursor {
                Some(c) => cursor = Some(c),
                None => break,
            }
        }
        if lowest {
            return valid.first().cloned().unwrap_or_default();
        }
        valid.choose(&mut rand::thread_rng()).cloned().unwrap_or_default()
    }

    pub async fn join_game_instance(&self, cookie: &str, place_id: i64, job_id: &str, teleport: bool) -> Result<Value> {
        let mut body = json!({ "placeId": place_id, "gameId": job_id, "gameJoinAttemptId": uuid::Uuid::new_v4().to_string() });
        if teleport {
            body["isTeleport"] = json!(true);
        }
        let resp = self
            .join
            .post("https://gamejoin.roblox.com/v1/join-game-instance")
            .header(COOKIE, cookie_header(cookie))
            .json(&body)
            .send()
            .await?;
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            bail!("[{}] {}", status.as_u16(), error_message(&text));
        }
        Ok(serde_json::from_str(&text).unwrap_or(Value::Null))
    }

    pub async fn join_game(&self, cookie: &str, place_id: i64) -> Result<()> {
        self.join
            .post("https://gamejoin.roblox.com/v1/join-game")
            .header(COOKIE, cookie_header(cookie))
            .json(&json!({ "placeId": place_id }))
            .send()
            .await?;
        Ok(())
    }

    /// Convierte un privateServerLinkCode en el accessCode del servidor privado.
    pub async fn private_server_access_code(&self, cookie: &str, place_id: i64, link_code: &str) -> Option<String> {
        static RE: Lazy<regex::Regex> = Lazy::new(|| {
            regex::Regex::new(r"Roblox\.GameLauncher\.joinPrivateGame\(\d+,\s*'(\w+-\w+-\w+-\w+-\w+)'").unwrap()
        });
        let url = format!("https://www.roblox.com/games/{place_id}?privateServerLinkCode={link_code}");
        for host in [url.clone(), url.replace("www.roblox.com", "web.roblox.com")] {
            let Ok(req) = self.authed(Method::GET, &host, cookie).await else { continue };
            if let Ok(resp) = req.header(REFERER, GAME_REFERER).send().await {
                if resp.status() == StatusCode::OK {
                    let body = resp.text().await.unwrap_or_default();
                    if let Some(c) = RE.captures(&body) {
                        return Some(c[1].to_string());
                    }
                }
            }
        }
        // API moderna: resolver el enlace compartido
        let req = self
            .authed(Method::POST, "https://apis.roblox.com/sharelinks/v1/resolve-link", cookie)
            .await
            .ok()?;
        let v: Value = req
            .json(&json!({ "linkId": link_code, "linkType": "Server" }))
            .send()
            .await
            .ok()?
            .json()
            .await
            .ok()?;
        v["privateServerInviteData"]["linkCode"].as_str().map(str::to_string)
    }

    pub async fn private_server_privacy_is_everyone(&self, cookie: &str) -> Option<bool> {
        let resp = self
            .req(Method::GET, "https://www.roblox.com/account/settings/private-server-invite-privacy", Some(cookie))
            .header(REFERER, "https://www.roblox.com/my/account")
            .send()
            .await
            .ok()?;
        if !resp.status().is_success() {
            return None;
        }
        Some(resp.text().await.ok()?.contains("\"AllUsers\""))
    }

    pub async fn set_private_server_privacy_everyone(&self, cookie: &str) -> Result<()> {
        self.authed(Method::POST, "https://www.roblox.com/account/settings/private-server-invite-privacy", cookie)
            .await?
            .header(REFERER, "https://www.roblox.com/my/account")
            .form(&[("PrivateServerInvitePrivacy", "AllUsers")])
            .send()
            .await?;
        Ok(())
    }

    // ------------------------------------------------------------------ cuenta

    /// (pin activado, desbloqueado hasta en segundos)
    pub async fn pin_status(&self, cookie: &str) -> Result<(bool, i64)> {
        let v: Value = self
            .req(Method::GET, "https://auth.roblox.com/v1/account/pin/", Some(cookie))
            .header(REFERER, "https://www.roblox.com/")
            .send()
            .await?
            .json()
            .await?;
        Ok((v["isEnabled"].as_bool().unwrap_or(false), v["unlockedUntil"].as_i64().unwrap_or(0)))
    }

    pub async fn unlock_pin(&self, cookie: &str, pin: &str) -> Result<i64> {
        let resp = self
            .authed(Method::POST, "https://auth.roblox.com/v1/account/pin/unlock", cookie)
            .await?
            .header(REFERER, "https://www.roblox.com/")
            .form(&[("pin", pin)])
            .send()
            .await?;
        let status = resp.status();
        let v: Value = resp.json().await.unwrap_or(Value::Null);
        if !status.is_success() {
            bail!("PIN incorrecto o bloqueado: {}", error_message(&v.to_string()));
        }
        Ok(v["unlockedUntil"].as_i64().unwrap_or(0))
    }

    pub async fn set_follow_privacy(&self, cookie: &str, level: u8) -> Result<()> {
        let value = ["All", "Followers", "Following", "Friends", "NoOne"].get(level as usize).copied().unwrap_or("All");
        let resp = self
            .authed(Method::POST, "https://www.roblox.com/account/settings/follow-me-privacy", cookie)
            .await?
            .header(REFERER, "https://www.roblox.com/my/account")
            .form(&[("FollowMePrivacy", value)])
            .send()
            .await?;
        if !resp.status().is_success() {
            bail!("Roblox rechazó el cambio [{}]", resp.status().as_u16());
        }
        Ok(())
    }

    /// Devuelve la cookie nueva si Roblox la rotó.
    pub async fn change_password(&self, cookie: &str, current: &str, new: &str) -> Result<Option<String>> {
        let resp = self
            .authed(Method::POST, "https://auth.roblox.com/v2/user/passwords/change", cookie)
            .await?
            .header(REFERER, "https://www.roblox.com/")
            .json(&json!({ "currentPassword": current, "newPassword": new }))
            .send()
            .await?;
        let new_cookie = new_cookie_from(&resp);
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            bail!("No se pudo cambiar la contraseña: {}", error_message(&body));
        }
        Ok(new_cookie)
    }

    pub async fn change_email(&self, cookie: &str, password: &str, email: &str) -> Result<()> {
        let resp = self
            .authed(Method::POST, "https://accountsettings.roblox.com/v1/email", cookie)
            .await?
            .header(REFERER, "https://www.roblox.com/")
            .json(&json!({ "password": password, "emailAddress": email }))
            .send()
            .await?;
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            bail!("No se pudo cambiar el correo (¿contraseña incorrecta?): {}", error_message(&body));
        }
        Ok(())
    }

    /// Cierra las demás sesiones y devuelve la cookie nueva.
    pub async fn signout_other_sessions(&self, cookie: &str) -> Result<Option<String>> {
        let resp = self
            .authed(Method::POST, "https://www.roblox.com/authentication/signoutfromallsessionsandreauthenticate", cookie)
            .await?
            .header(REFERER, "https://www.roblox.com/")
            .header("Content-Length", "0")
            .send()
            .await?;
        if !resp.status().is_success() {
            bail!("No se pudo cerrar las otras sesiones [{}]", resp.status().as_u16());
        }
        Ok(new_cookie_from(&resp))
    }

    pub async fn blocked_list(&self, cookie: &str) -> Result<Value> {
        let resp = self
            .req(Method::GET, "https://accountsettings.roblox.com/v1/users/get-detailed-blocked-users", Some(cookie))
            .send()
            .await?;
        if resp.status().is_success() {
            return Ok(resp.json().await?);
        }
        let v: Value = self
            .req(Method::GET, "https://apis.roblox.com/user-blocking-api/v1/users/get-blocked-users?cursor=&count=100", Some(cookie))
            .send()
            .await?
            .json()
            .await?;
        Ok(v)
    }

    pub async fn blocked_ids(&self, cookie: &str) -> Result<Vec<i64>> {
        let v = self.blocked_list(cookie).await?;
        let mut ids = Vec::new();
        for item in v["blockedUsers"].as_array().into_iter().flatten() {
            if let Some(id) = item["userId"].as_i64().or_else(|| item.as_i64()) {
                ids.push(id);
            }
        }
        for item in v["data"]["blockedUserIds"].as_array().into_iter().flatten() {
            if let Some(id) = item.as_i64() {
                ids.push(id);
            }
        }
        Ok(ids)
    }

    pub async fn set_blocked(&self, cookie: &str, user_id: i64, block: bool) -> Result<(u16, String)> {
        let action = if block { "block" } else { "unblock" };
        let resp = self
            .authed(Method::POST, &format!("https://accountsettings.roblox.com/v1/users/{user_id}/{action}"), cookie)
            .await?
            .header("Content-Length", "0")
            .send()
            .await?;
        if resp.status().is_success() {
            let s = resp.status().as_u16();
            return Ok((s, resp.text().await.unwrap_or_default()));
        }
        let action = if block { "block-user" } else { "unblock-user" };
        let resp = self
            .authed(Method::POST, &format!("https://apis.roblox.com/user-blocking-api/v1/users/{user_id}/{action}"), cookie)
            .await?
            .header("Content-Length", "0")
            .send()
            .await?;
        let s = resp.status().as_u16();
        Ok((s, resp.text().await.unwrap_or_default()))
    }

    pub async fn send_friend_request(&self, cookie: &str, user_id: i64) -> Result<()> {
        let resp = self
            .authed(Method::POST, &format!("https://friends.roblox.com/v1/users/{user_id}/request-friendship"), cookie)
            .await?
            .json(&json!({ "friendshipOriginSourceType": 0 }))
            .send()
            .await?;
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            bail!("No se pudo enviar la solicitud: {}", error_message(&body));
        }
        Ok(())
    }

    pub async fn set_display_name(&self, cookie: &str, user_id: i64, name: &str) -> Result<()> {
        let resp = self
            .authed(Method::PATCH, &format!("https://users.roblox.com/v1/users/{user_id}/display-names"), cookie)
            .await?
            .json(&json!({ "newDisplayName": name }))
            .send()
            .await?;
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            bail!("{}", error_message(&body));
        }
        Ok(())
    }

    /// Aplica un avatar en formato JSON (el de outfits/details o el de la API de avatar).
    /// Devuelve los IDs de assets que la cuenta no posee.
    pub async fn set_avatar(&self, cookie: &str, avatar: &Value) -> Result<Vec<i64>> {
        let token = self.csrf(cookie).await?;
        let post = |url: &str, body: Value| {
            self.req(Method::POST, url, Some(cookie)).header("X-CSRF-TOKEN", token.clone()).json(&body).send()
        };
        if let Some(t) = avatar.get("playerAvatarType").and_then(|v| v.as_str().map(str::to_string).or_else(|| v.as_i64().map(|n| if n == 3 { "R15".into() } else { "R6".into() }))) {
            let _ = post("https://avatar.roblox.com/v1/avatar/set-player-avatar-type", json!({ "playerAvatarType": t })).await;
        }
        if let Some(scales) = avatar.get("scales").or_else(|| avatar.get("scale")) {
            let _ = post("https://avatar.roblox.com/v1/avatar/set-scales", scales.clone()).await;
        }
        if let Some(colors) = avatar.get("bodyColors") {
            let _ = post("https://avatar.roblox.com/v1/avatar/set-body-colors", colors.clone()).await;
        }
        let mut invalid = Vec::new();
        if let Some(assets) = avatar.get("assets") {
            let resp = post("https://avatar.roblox.com/v2/avatar/set-wearing-assets", json!({ "assets": assets })).await?;
            if resp.status().is_success() {
                let v: Value = resp.json().await.unwrap_or(Value::Null);
                invalid = v["invalidAssetIds"]
                    .as_array()
                    .map(|a| a.iter().filter_map(Value::as_i64).collect())
                    .unwrap_or_default();
            } else {
                let body = resp.text().await.unwrap_or_default();
                bail!("No se pudo aplicar el avatar: {}", error_message(&body));
            }
        }
        Ok(invalid)
    }

    pub async fn outfits(&self, user_id: i64) -> Result<Vec<(i64, String)>> {
        let v: Value = self
            .req(Method::GET, &format!("https://avatar.roblox.com/v1/users/{user_id}/outfits?page=1&itemsPerPage=50&isEditable=true"), None)
            .send()
            .await?
            .json()
            .await?;
        Ok(v["data"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|o| (o["id"].as_i64().unwrap_or(0), o["name"].as_str().unwrap_or_default().to_string()))
            .collect())
    }

    pub async fn outfit_thumbnails(&self, ids: &[i64]) -> HashMap<i64, String> {
        let mut out = HashMap::new();
        for chunk in ids.chunks(50) {
            let list = chunk.iter().map(i64::to_string).collect::<Vec<_>>().join(",");
            let url = format!("https://thumbnails.roblox.com/v1/users/outfits?userOutfitIds={list}&size=150x150&format=Png&isCircular=false");
            if let Ok(resp) = self.req(Method::GET, &url, None).send().await {
                if let Ok(v) = resp.json::<Value>().await {
                    for d in v["data"].as_array().into_iter().flatten() {
                        if let (Some(id), Some(img)) = (d["targetId"].as_i64(), d["imageUrl"].as_str()) {
                            out.insert(id, img.to_string());
                        }
                    }
                }
            }
        }
        out
    }

    pub async fn outfit_details(&self, outfit_id: i64) -> Result<Value> {
        Ok(self
            .req(Method::GET, &format!("https://avatar.roblox.com/v1/outfits/{outfit_id}/details"), None)
            .send()
            .await?
            .json()
            .await?)
    }

    pub async fn asset_names(&self, ids: &[i64]) -> HashMap<i64, String> {
        let mut out = HashMap::new();
        for id in ids {
            if let Some(name) = self.place_name(*id).await {
                out.insert(*id, name);
            }
        }
        out
    }

    pub async fn quick_login_enter(&self, cookie: &str, code: &str) -> Result<Value> {
        let resp = self
            .authed(Method::POST, "https://apis.roblox.com/auth-token-service/v1/login/enterCode", cookie)
            .await?
            .json(&json!({ "code": code }))
            .send()
            .await?;
        let status = resp.status();
        let v: Value = resp.json().await.unwrap_or(Value::Null);
        if !status.is_success() {
            bail!("Código inválido: {}", error_message(&v.to_string()));
        }
        Ok(v)
    }

    pub async fn quick_login_validate(&self, cookie: &str, code: &str) -> Result<()> {
        let resp = self
            .authed(Method::POST, "https://apis.roblox.com/auth-token-service/v1/login/validateCode", cookie)
            .await?
            .json(&json!({ "code": code }))
            .send()
            .await?;
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            bail!("No se pudo validar el código: {}", error_message(&body));
        }
        Ok(())
    }

    pub async fn email_info(&self, cookie: &str) -> String {
        match self.req(Method::GET, "https://accountsettings.roblox.com/v1/email", Some(cookie)).send().await {
            Ok(r) => r.text().await.unwrap_or_default(),
            Err(e) => e.to_string(),
        }
    }

    pub async fn robux(&self, cookie: &str, user_id: i64) -> Option<i64> {
        let v: Value = self
            .req(Method::GET, &format!("https://economy.roblox.com/v1/users/{user_id}/currency"), Some(cookie))
            .send()
            .await
            .ok()?
            .json()
            .await
            .ok()?;
        v["robux"].as_i64()
    }

    pub async fn search_games(&self, term: &str, page_token: Option<&str>) -> Result<(Vec<Value>, Option<String>)> {
        let session = uuid::Uuid::new_v4().to_string();
        let mut url = format!(
            "https://apis.roblox.com/search-api/omni-search?searchQuery={}&sessionId={session}&pageType=all",
            urlencoding::encode(term)
        );
        if let Some(t) = page_token.filter(|t| !t.is_empty()) {
            url.push_str(&format!("&pageToken={}", urlencoding::encode(t)));
        }
        let v: Value = self.req(Method::GET, &url, None).send().await?.json().await?;
        let mut games = Vec::new();
        for group in v["searchResults"].as_array().into_iter().flatten() {
            if group["contentGroupType"].as_str() != Some("Game") {
                continue;
            }
            for g in group["contents"].as_array().into_iter().flatten() {
                games.push(g.clone());
            }
        }
        Ok((games, v["nextPageToken"].as_str().filter(|s| !s.is_empty()).map(str::to_string)))
    }

    pub async fn client_version(&self) -> Option<String> {
        let v: Value = self
            .req(Method::GET, "https://clientsettings.roblox.com/v2/client-version/WindowsPlayer", None)
            .send()
            .await
            .ok()?
            .json()
            .await
            .ok()?;
        v["clientVersionUpload"].as_str().map(str::to_string)
    }

    pub async fn ip_info(&self, link: &str, ip: &str) -> Result<Value> {
        let url = link.replace("<ip>", ip);
        Ok(self.http.get(url).send().await?.json().await?)
    }

    pub async fn get_json(&self, url: &str) -> Result<Value> {
        Ok(self.http.get(url).header("Accept", "application/vnd.github+json").send().await?.json().await?)
    }

    pub fn raw(&self) -> &Client {
        &self.http
    }

    /// Miniaturas de jugadores a partir de los "playerTokens" de un servidor.
    pub async fn token_headshots(&self, tokens: &[String]) -> Vec<String> {
        let mut out = Vec::new();
        for chunk in tokens.chunks(100) {
            let body: Vec<Value> = chunk
                .iter()
                .map(|t| {
                    json!({
                        "requestId": format!("0:{t}:AvatarHeadshot:48x48:png:regular"),
                        "type": "AvatarHeadShot",
                        "targetId": 0,
                        "token": t,
                        "format": "png",
                        "size": "48x48"
                    })
                })
                .collect();
            if let Ok(resp) = self.req(Method::POST, "https://thumbnails.roblox.com/v1/batch", None).json(&body).send().await {
                if let Ok(v) = resp.json::<Value>().await {
                    for d in v["data"].as_array().into_iter().flatten() {
                        if let Some(u) = d["imageUrl"].as_str() {
                            out.push(u.to_string());
                        }
                    }
                }
            }
        }
        out
    }

    /// Wrapper público con firma simple para los comandos Tauri.
    pub async fn servers(&self, place_id: i64, cursor: Option<&str>) -> Result<ServersPage> {
        let cookie = crate::state::st().any_cookie();
        self.servers_page(place_id, cursor, false, cookie.as_deref()).await
    }

    /// Información pública de un lugar (nombre, descripción, icono, etc.).
    pub async fn place_info(&self, place_id: i64) -> Result<PlaceInfo> {
        let universe_id = self.universe_of_place(place_id).await?;
        let url = format!("https://games.roblox.com/v1/games?universeIds={universe_id}");
        let v: Value = self.req(Method::GET, &url, None).send().await?.json().await?;
        let g = v["data"].as_array().and_then(|a| a.first()).cloned().unwrap_or(Value::Null);
        let icon = self.place_icons(&[place_id]).await;
        Ok(PlaceInfo {
            place_id,
            name: g["name"].as_str().unwrap_or_default().to_string(),
            description: g["description"].as_str().unwrap_or_default().to_string(),
            icon: icon.get(&place_id).cloned().unwrap_or_default(),
            universe_id,
            max_players: g["maxPlayers"].as_i64().unwrap_or(0),
            visits: g["visits"].as_i64().unwrap_or(0),
            created: g["created"].as_str().unwrap_or_default().to_string(),
            updated: g["updated"].as_str().unwrap_or_default().to_string(),
        })
    }

    /// Consulta ip-api.com (u otro endpoint configurado) para obtener la región de un IP.
    pub async fn region_for_ip(&self, ip: &str, api_url: &str, fmt: &str) -> Option<String> {
        let url = api_url.replace("<ip>", ip);
        let v: Value = self.req(Method::GET, &url, None).send().await.ok()?.json().await.ok()?;
        Some(
            fmt.replace("<city>", v["city"].as_str().unwrap_or(""))
                .replace("<country>", v["country"].as_str().unwrap_or(""))
                .replace("<countryCode>", v["countryCode"].as_str().unwrap_or(""))
                .replace("<regionName>", v["regionName"].as_str().unwrap_or(""))
                .replace("<region>", v["region"].as_str().unwrap_or(""))
                .trim_matches([',', ' '].as_ref())
                .to_string(),
        )
    }

    /// Descarga un archivo del CDN de Roblox por su hash, probando los servidores t0..t7.
    async fn cdn_hash(&self, hash: &str) -> Result<Vec<u8>> {
        if hash.starts_with("http") {
            return Ok(self.http.get(hash).send().await?.error_for_status()?.bytes().await?.to_vec());
        }
        let mut h: u32 = 31;
        for b in hash.bytes().take(38) {
            h ^= b as u32;
        }
        let first = h % 8;
        let hosts = (0..8).map(|k| format!("t{}", (first + k) % 8)).chain(["tr".to_string()]);
        for host in hosts {
            let url = format!("https://{host}.rbxcdn.com/{hash}");
            if let Ok(resp) = self.http.get(&url).send().await {
                if resp.status().is_success() {
                    return Ok(resp.bytes().await?.to_vec());
                }
            }
        }
        bail!("No se pudo descargar {hash}")
    }

    /// Modelo 3D del avatar: obj, mtl y texturas (en base64) listos para dibujar.
    pub async fn avatar_3d(&self, cookie: &str, user_id: i64) -> Result<Value> {
        use base64::Engine;
        let url = format!("https://thumbnails.roblox.com/v1/users/avatar-3d?userId={user_id}");
        let mut meta: Value = Value::Null;
        for _ in 0..8 {
            meta = self.req(Method::GET, &url, Some(cookie)).send().await?.json().await?;
            if meta["state"] == "Completed" {
                break;
            }
            if meta["errors"].is_array() {
                bail!("Roblox: {}", meta["errors"][0]["message"].as_str().unwrap_or("error"));
            }
            tokio::time::sleep(Duration::from_millis(1500)).await;
        }
        let manifest_url = meta["imageUrl"].as_str().ok_or_else(|| anyhow!("Roblox aún no generó el modelo 3D ({})", meta["state"]))?;
        let manifest: Value = self.http.get(manifest_url).send().await?.json().await?;
        let obj = String::from_utf8_lossy(&self.cdn_hash(manifest["obj"].as_str().unwrap_or_default()).await?).into_owned();
        let mtl = String::from_utf8_lossy(&self.cdn_hash(manifest["mtl"].as_str().unwrap_or_default()).await?).into_owned();
        let mut textures = serde_json::Map::new();
        for t in manifest["textures"].as_array().into_iter().flatten().filter_map(|t| t.as_str()) {
            if let Ok(bytes) = self.cdn_hash(t).await {
                let mime = if bytes.starts_with(&[0x89, b'P']) { "image/png" } else { "image/jpeg" };
                let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                textures.insert(t.to_string(), json!(format!("data:{mime};base64,{b64}")));
            }
        }
        Ok(json!({ "obj": obj, "mtl": mtl, "textures": textures, "camera": manifest["camera"], "aabb": manifest["aabb"] }))
    }
}
