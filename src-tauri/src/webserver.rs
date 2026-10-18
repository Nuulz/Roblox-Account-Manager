//! Servidor web local para controlar las cuentas desde scripts (compatible con la API de la 3.x).
//! Rutas: /Método?Account=...&Password=...  ·  con prefijo /v2/ responde {"Success","Message"} en JSON.

use std::collections::HashMap;
use std::net::SocketAddr;

use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::http::{StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::Router;
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde_json::json;

use crate::ops::{self, JoinRequest};
use crate::roblox::api;
use crate::state::st;
use crate::storage;

static SHUTDOWN: Lazy<Mutex<Option<tokio::sync::oneshot::Sender<()>>>> = Lazy::new(|| Mutex::new(None));

/// Arranca, reinicia o detiene el servidor según la configuración actual.
pub fn apply() {
    if let Some(tx) = SHUTDOWN.lock().take() {
        let _ = tx.send(());
    }
    let ws = st().settings.read().web_server.clone();
    if !ws.enabled {
        return;
    }
    let ip = if ws.allow_external_connections { [0, 0, 0, 0] } else { [127, 0, 0, 1] };
    let addr = SocketAddr::from((ip, ws.port));
    let (tx, rx) = tokio::sync::oneshot::channel();
    *SHUTDOWN.lock() = Some(tx);
    tauri::async_runtime::spawn(async move {
        let listener = match tokio::net::TcpListener::bind(addr).await {
            Ok(l) => l,
            Err(e) => {
                st().toast("error", format!("Servidor web: no se pudo usar el puerto {}: {e}", addr.port()));
                return;
            }
        };
        log::info!("Servidor web escuchando en {addr}");
        let app = Router::new().fallback(handle).with_state(());
        let _ = axum::serve(listener, app).with_graceful_shutdown(async { let _ = rx.await; }).await;
        log::info!("Servidor web detenido");
    });
}

struct Reply {
    v2: bool,
    code: StatusCode,
    message: String,
    raw: Option<String>,
}

impl IntoResponse for Reply {
    fn into_response(self) -> Response {
        let ok = self.code.is_success();
        let body = if self.v2 {
            json!({ "Success": ok, "Message": self.message }).to_string()
        } else {
            self.raw.unwrap_or(self.message.clone())
        };
        let mut res = (self.code, [("content-type", "text/plain; charset=utf-8")], body.clone()).into_response();
        if !ok {
            if let Ok(v) = body.replace(['\r', '\n'], " ").chars().take(300).collect::<String>().parse() {
                res.headers_mut().insert("ws-error", v);
            }
        }
        res
    }
}

async fn handle(State(()): State<()>, uri: Uri, Query(q): Query<HashMap<String, String>>, body: Bytes) -> Reply {
    let path = uri.path();
    let v2 = path.starts_with("/v2/");
    let path = if v2 { &path[3..] } else { path };
    let body = String::from_utf8_lossy(&body).to_string();

    let r = |code: StatusCode, msg: String, raw: Option<&str>| Reply { v2, code, message: msg, raw: raw.map(str::to_string) };
    let ok = |msg: String| r(StatusCode::OK, msg, None);
    let bad = |msg: &str| r(StatusCode::BAD_REQUEST, msg.to_string(), None);
    let denied = |m: &str| r(StatusCode::UNAUTHORIZED, format!("Method `{m}` not allowed"), Some("Method not allowed"));

    if path == "/favicon.ico" {
        return ok(String::new());
    }
    if path == "/Running" {
        return r(StatusCode::OK, "Roblox Account Manager is running".into(), Some("true"));
    }
    if !st().is_unlocked() {
        return r(StatusCode::SERVICE_UNAVAILABLE, "Accounts are locked".into(), None);
    }

    let ws = st().settings.read().web_server.clone();
    let method = path.trim_start_matches('/');
    let pass_ok = ws.password.chars().count() >= 6 && q.get("Password").map(|p| *p == ws.password).unwrap_or(false);
    let invalid_pass = || r(StatusCode::UNAUTHORIZED, "Invalid Password, make sure your password contains 6 or more characters".into(), Some("Invalid Password"));

    if ws.every_request_requires_password && !pass_ok {
        return invalid_pass();
    }
    if matches!(method, "GetCookie" | "GetAccounts" | "GetAccountsJson" | "LaunchAccount" | "FollowUser" | "ImportCookie") && !pass_ok {
        return invalid_pass();
    }

    match method {
        "GetAccounts" | "GetAccountsJson" => {
            if !ws.allow_get_accounts {
                return denied(method);
            }
            let group = q.get("Group").filter(|g| !g.is_empty());
            let list: Vec<_> = st().accounts.read().iter().filter(|a| group.map(|g| a.group == *g).unwrap_or(true)).cloned().collect();
            if method == "GetAccounts" {
                let names = list.iter().map(|a| a.username.as_str()).collect::<Vec<_>>().join(",");
                return r(StatusCode::OK, names.clone(), Some(&names));
            }
            let cookies = ws.allow_get_cookie && q.get("IncludeCookies").map(|v| v == "true").unwrap_or(false);
            let arr: Vec<_> = list
                .iter()
                .map(|a| {
                    json!({
                        "Username": a.username, "UserID": a.user_id, "Alias": a.alias, "Description": a.description,
                        "Group": a.group, "LastUsed": a.last_use.timestamp(), "Fields": a.fields,
                        "Cookie": if cookies { Some(a.security_token.clone()) } else { None },
                    })
                })
                .collect();
            return ok(serde_json::to_string(&arr).unwrap_or_default());
        }
        "ImportCookie" => {
            let cookie = q.get("Cookie").cloned().unwrap_or(body.clone());
            return match ops::add_account(&cookie, "").await {
                Ok(_) => r(StatusCode::OK, "Cookie successfully imported".into(), Some("true")),
                Err(e) => r(StatusCode::BAD_REQUEST, format!("[ImportCookie] {e}"), Some("false")),
            };
        }
        _ => {}
    }

    let Some(key) = q.get("Account").filter(|a| !a.is_empty()) else { return bad("Empty Account") };
    let Some(acc) = st().find_by_name_or_id(key) else {
        return r(StatusCode::BAD_REQUEST, "Invalid Account".into(), Some("Invalid Account"));
    };
    let id = acc.user_id;
    let cookie = acc.security_token.clone();
    let edited = |msg: String| {
        storage::save();
        st().emit_accounts();
        ok(msg)
    };

    match method {
        "GetCookie" => {
            if !ws.allow_get_cookie {
                return denied(method);
            }
            ok(cookie)
        }
        "LaunchAccount" => {
            if !ws.allow_launch_account {
                return denied(method);
            }
            let Some(place_id) = q.get("PlaceId").and_then(|p| p.parse::<i64>().ok()) else {
                return r(StatusCode::BAD_REQUEST, "Invalid PlaceId provided".into(), Some("Invalid PlaceId"));
            };
            let req = JoinRequest {
                place_id,
                job_id: q.get("JobId").cloned().unwrap_or_default(),
                follow_user: q.get("FollowUser").map(|v| v == "true").unwrap_or(false),
                join_vip: q.get("JoinVIP").map(|v| v == "true").unwrap_or(false),
            };
            tauri::async_runtime::spawn(async move {
                if let Err(e) = ops::join_server(id, req).await {
                    st().toast("error", e);
                }
            });
            ok(format!("Launched {} to {place_id}", acc.username))
        }
        "FollowUser" => {
            if !ws.allow_launch_account {
                return denied(method);
            }
            let Some(user) = q.get("Username").filter(|u| !u.is_empty()) else { return bad("Invalid Username Parameter") };
            match api().user_id(user, Some(&cookie)).await {
                Ok(target) => {
                    let req = JoinRequest { place_id: target, follow_user: true, ..Default::default() };
                    tauri::async_runtime::spawn(async move {
                        if let Err(e) = ops::join_server(id, req).await {
                            st().toast("error", e);
                        }
                    });
                    ok(format!("Joining {user}'s game on {}", acc.username))
                }
                Err(e) => bad(&format!("Failed to get UserId: {e}")),
            }
        }
        "GetCSRFToken" => match api().csrf(&cookie).await {
            Ok(t) => ok(t),
            Err(e) => bad(&e.to_string()),
        },
        "GetAlias" => ok(acc.alias),
        "GetDescription" => ok(acc.description),
        "GetField" => ok(q.get("Field").map(|f| acc.field(f)).unwrap_or_default()),
        "SetField" | "RemoveField" | "SetAlias" | "SetDescription" | "AppendDescription" if !ws.allow_account_editing => denied(method),
        "SetField" => {
            let (Some(f), Some(v)) = (q.get("Field"), q.get("Value")) else { return bad("Field and Value required") };
            st().with_account(id, |a| a.fields.insert(f.clone(), v.clone()));
            edited(format!("Set Field {f} to {v} for {}", acc.username))
        }
        "RemoveField" => {
            let Some(f) = q.get("Field") else { return bad("Field required") };
            st().with_account(id, |a| a.fields.remove(f));
            edited(format!("Removed Field {f} from {}", acc.username))
        }
        "SetAlias" if !body.is_empty() => {
            st().with_account(id, |a| a.alias = body.clone());
            edited(format!("Set Alias of {} to {body}", acc.username))
        }
        "SetDescription" if !body.is_empty() => {
            st().with_account(id, |a| a.description = body.clone());
            edited(format!("Set Description of {} to {body}", acc.username))
        }
        "AppendDescription" if !body.is_empty() => {
            st().with_account(id, |a| a.description.push_str(&body));
            edited(format!("Appended Description of {} with {body}", acc.username))
        }
        "SetAvatar" => match serde_json::from_str::<serde_json::Value>(&body) {
            Ok(v) => match api().set_avatar(&cookie, &v).await {
                Ok(_) => ok(format!("Set avatar of {}", acc.username)),
                Err(e) => bad(&e.to_string()),
            },
            Err(_) => bad("Body must be JSON"),
        },
        "BlockUser" | "UnblockUser" => {
            let Some(uid) = q.get("UserId").and_then(|u| u.parse::<i64>().ok()) else { return bad("Invalid UserId") };
            match api().set_blocked(&cookie, uid, method == "BlockUser").await {
                Ok((code, msg)) => r(StatusCode::from_u16(code).unwrap_or(StatusCode::OK), msg, None),
                Err(e) => r(StatusCode::INTERNAL_SERVER_ERROR, e.to_string(), None),
            }
        }
        "GetBlockedList" => match api().blocked_list(&cookie).await {
            Ok(v) => ok(v.to_string()),
            Err(e) => r(StatusCode::INTERNAL_SERVER_ERROR, e.to_string(), None),
        },
        "UnblockEveryone" => match api().blocked_ids(&cookie).await {
            Ok(ids) => {
                let mut n = 0;
                for uid in ids {
                    if api().set_blocked(&cookie, uid, false).await.is_ok() {
                        n += 1;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
                }
                ok(format!("Unblocked {n} users"))
            }
            Err(e) => bad(&e.to_string()),
        },
        _ => r(StatusCode::NOT_FOUND, "404 not found".into(), None),
    }
}
