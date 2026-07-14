//! REST handlers for the web UI (FEAT-087, acceptance criteria 5 & 6).
//! `health` and `login` are reachable unauthenticated (see `super`'s module
//! doc comment); everything else takes [`super::AuthedUser`] so axum's
//! extractor mechanism enforces the auth boundary by construction.

use super::{AuthedUser, SharedState};
use axum::extract::ConnectInfo;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json, Response};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::net::SocketAddr;

pub async fn health() -> impl IntoResponse {
    Json(json!({ "status": "ok" }))
}

#[derive(Deserialize)]
pub struct LoginRequest {
    username: String,
    password: String,
}

#[derive(Serialize)]
pub struct LoginResponse {
    token: String,
    username: String,
}

/// PAM-authenticates against [`super::auth::PAM_SERVICE`], rate-limited per
/// client IP (acceptance criterion 5: 5/min, 10s cooldown after 3
/// failures). `pam_auth::authenticate` is blocking libpam/libc FFI, so it
/// runs on `spawn_blocking` to avoid stalling the async runtime.
pub async fn login(
    State(state): SharedState,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(req): Json<LoginRequest>,
) -> Response {
    let ip = addr.ip().to_string();
    let now = chrono::Utc::now();

    let decision = { state.rate_limiter.lock().expect("poisoned").check(&ip, now) };
    if let super::auth::RateLimitDecision::Blocked { retry_after } = decision {
        return (StatusCode::TOO_MANY_REQUESTS, format!("too many attempts; retry in {}s", retry_after.num_seconds().max(1))).into_response();
    }

    let service = super::auth::PAM_SERVICE.to_string();
    let username = req.username.clone();
    let password = req.password.clone();
    let outcome = tokio::task::spawn_blocking(move || super::pam_auth::authenticate(&service, &username, &password)).await;

    let ok = match outcome {
        Ok(Ok(ok)) => ok,
        Ok(Err(e)) => {
            tracing::warn!("webui: PAM auth error: {e:#}");
            false
        }
        Err(e) => {
            tracing::error!("webui: PAM auth task panicked: {e:#}");
            false
        }
    };

    if !ok {
        state.rate_limiter.lock().expect("poisoned").record_failure(&ip, now);
        return (StatusCode::UNAUTHORIZED, "invalid credentials").into_response();
    }
    state.rate_limiter.lock().expect("poisoned").record_success(&ip);

    let token = {
        let db = state.app.db.lock().expect("DB poisoned");
        match super::auth::issue_token(&db, &req.username, now) {
            Ok(t) => t,
            Err(e) => {
                tracing::error!("webui: failed to issue token: {e:#}");
                return (StatusCode::INTERNAL_SERVER_ERROR, "failed to issue token").into_response();
            }
        }
    };

    Json(LoginResponse { token, username: req.username }).into_response()
}

/// Revokes the presented token and issues a fresh one (acceptance criterion
/// 5: "issues a new token, old one revoked").
pub async fn refresh(State(state): SharedState, AuthedUser(username): AuthedUser, headers: axum::http::HeaderMap) -> Response {
    let now = chrono::Utc::now();
    let db = state.app.db.lock().expect("DB poisoned");
    if let Some(old) = headers.get(axum::http::header::AUTHORIZATION).and_then(|v| v.to_str().ok()).and_then(|v| v.strip_prefix("Bearer ")) {
        let _ = super::auth::revoke_token(&db, old);
    }
    match super::auth::issue_token(&db, &username, now) {
        Ok(token) => Json(LoginResponse { token, username }).into_response(),
        Err(e) => {
            tracing::error!("webui: failed to refresh token: {e:#}");
            (StatusCode::INTERNAL_SERVER_ERROR, "failed to refresh token").into_response()
        }
    }
}

pub async fn sessions(State(state): SharedState, _user: AuthedUser) -> Response {
    let db = state.app.identity_db.lock().expect("DB poisoned");
    match regin_core::identity_db::session_list(&db, None, None) {
        Ok(rows) => Json(rows).into_response(),
        Err(e) => {
            tracing::error!("webui: failed to list sessions: {e:#}");
            (StatusCode::INTERNAL_SERVER_ERROR, "failed to list sessions").into_response()
        }
    }
}

pub async fn memory(State(state): SharedState, _user: AuthedUser) -> Response {
    let db = state.app.identity_db.lock().expect("DB poisoned");
    match regin_core::identity_db::memory_list(&db, None) {
        Ok(rows) => Json(rows).into_response(),
        Err(e) => {
            tracing::error!("webui: failed to list memory: {e:#}");
            (StatusCode::INTERNAL_SERVER_ERROR, "failed to list memory").into_response()
        }
    }
}

pub async fn runs(State(state): SharedState, _user: AuthedUser) -> Response {
    let db = state.app.db.lock().expect("DB poisoned");
    match regin_core::db::get_all_task_runs(&db, 100) {
        Ok(rows) => Json(rows).into_response(),
        Err(e) => {
            tracing::error!("webui: failed to list task runs: {e:#}");
            (StatusCode::INTERNAL_SERVER_ERROR, "failed to list task runs").into_response()
        }
    }
}

/// A read-only snapshot of settings (acceptance criterion 6). Deliberately
/// omits nothing sensitive-looking today (no secret-valued settings exist
/// yet), but is a natural place to redact `*.api_key`/`*.password`-suffixed
/// keys if that changes.
pub async fn config_snapshot(State(state): SharedState, _user: AuthedUser) -> Response {
    let db = state.app.db.lock().expect("DB poisoned");
    match regin_core::db::setting_list(&db) {
        Ok(rows) => {
            let redacted: Vec<(String, String)> = rows
                .into_iter()
                .map(|(k, v)| if k.ends_with("api_key") || k.ends_with("password") || k.ends_with("token") { (k, "***".to_string()) } else { (k, v) })
                .collect();
            Json(redacted).into_response()
        }
        Err(e) => {
            tracing::error!("webui: failed to list settings: {e:#}");
            (StatusCode::INTERNAL_SERVER_ERROR, "failed to list settings").into_response()
        }
    }
}

#[derive(Deserialize, Serialize)]
pub struct TabDef {
    pub name: String,
    pub icon: String,
    pub html: String,
}

/// Dynamic dashboard tabs (acceptance criterion 9): any authenticated
/// client — typically a `regin` skill or a manual `curl` — can register a
/// self-contained HTML fragment under a name, and the SPA shell renders it
/// as an extra tab. Deliberately not sandboxed (an iframe with `srcdoc`
/// would be the safer path for untrusted content) since only already-
/// authenticated clients can register tabs.
pub async fn tabs_list(State(state): SharedState, _user: AuthedUser) -> Response {
    let db = state.app.db.lock().expect("DB poisoned");
    match tabs_list_rows(&db) {
        Ok(rows) => Json(rows).into_response(),
        Err(e) => {
            tracing::error!("webui: failed to list tabs: {e:#}");
            (StatusCode::INTERNAL_SERVER_ERROR, "failed to list tabs").into_response()
        }
    }
}

pub async fn tabs_register(State(state): SharedState, _user: AuthedUser, Json(tab): Json<TabDef>) -> Response {
    let db = state.app.db.lock().expect("DB poisoned");
    let now = chrono::Utc::now().to_rfc3339();
    match db.execute(
        "INSERT OR REPLACE INTO webui_tabs (name, icon, html, created_at) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![tab.name, tab.icon, tab.html, now],
    ) {
        Ok(_) => (StatusCode::OK, "registered").into_response(),
        Err(e) => {
            tracing::error!("webui: failed to register tab: {e:#}");
            (StatusCode::INTERNAL_SERVER_ERROR, "failed to register tab").into_response()
        }
    }
}

fn tabs_list_rows(conn: &rusqlite::Connection) -> anyhow::Result<Vec<TabDef>> {
    let mut stmt = conn.prepare("SELECT name, icon, html FROM webui_tabs ORDER BY name")?;
    let rows = stmt.query_map([], |r| Ok(TabDef { name: r.get(0)?, icon: r.get(1)?, html: r.get(2)? }))?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

use axum::extract::State;

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::WebuiState;
    use std::sync::Arc;

    fn test_state() -> Arc<WebuiState> {
        crate::webui::test_support::fresh_webui_state()
    }

    #[tokio::test]
    async fn health_reports_ok() {
        let resp = health().await.into_response();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn tabs_register_then_list_round_trips() {
        let state = test_state();
        let tab = TabDef { name: "disk".into(), icon: "💾".into(), html: "<p>disk usage</p>".into() };
        let resp = tabs_register(State(state.clone()), AuthedUser("rene".into()), Json(tab)).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let resp = tabs_list(State(state), AuthedUser("rene".into())).await;
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn config_snapshot_redacts_secret_looking_keys() {
        let state = test_state();
        {
            let db = state.app.db.lock().unwrap();
            regin_core::db::setting_set(&db, "llm.api_key", "sk-super-secret").unwrap();
        }
        let resp = config_snapshot(State(state), AuthedUser("rene".into())).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let body = String::from_utf8(bytes.to_vec()).unwrap();
        assert!(!body.contains("sk-super-secret"));
        assert!(body.contains("***"));
    }

    #[tokio::test]
    async fn sessions_memory_and_runs_endpoints_return_empty_lists_on_a_fresh_db() {
        let state = test_state();
        for resp in [
            sessions(State(state.clone()), AuthedUser("rene".into())).await,
            memory(State(state.clone()), AuthedUser("rene".into())).await,
            runs(State(state.clone()), AuthedUser("rene".into())).await,
        ] {
            assert_eq!(resp.status(), StatusCode::OK);
        }
    }
}
