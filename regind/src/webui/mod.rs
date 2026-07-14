//! Embedded web UI server (FEAT-087 / DISC-022): axum HTTP server inside
//! `regind`, gated behind the `webui` Cargo feature (off by default — see
//! `regind/Cargo.toml`'s doc comment and `regind/build.rs`). Three URL
//! namespaces (acceptance criterion 4): a public landing page at `/`,
//! public build-artifact/package-repo trees at `/artifacts` and `/repo`
//! (paths configurable, criterion 9), and a PAM-authenticated area under
//! `/regin/*` (SPA + REST/WebSocket API).
//!
//! **Auth boundary, reconciling criteria 4/5/10**: the SPA *shell*
//! (`/regin/`, its embedded CSS/JS) is served without server-side auth —
//! it has to be, so an unauthenticated browser can load the page and see
//! the login form (criterion 10: "Login form redirects to main view on
//! success" only makes sense if the shell itself is reachable first). What
//! criterion 5 actually gates is the **data surface**:
//! `/regin/api/*` (REST) and the WebSocket endpoints, all of which require
//! a valid bearer token/cookie except `/regin/api/health` and
//! `/regin/api/auth/login`. A strictly literal "every `/regin/*` path
//! requires auth" reading would make the login form itself unreachable
//! without already having a token — circular — so this is the coherent
//! resolution, not a scope-narrowing.

mod api;
mod artifacts;
mod auth;
mod pam_auth;
mod spa;
mod ws_chat;
mod ws_goal;
mod ws_terminal;

use axum::Router;
use axum::extract::{FromRequestParts, State};
use axum::http::request::Parts;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

/// Shared state for every web UI handler.
pub struct WebuiState {
    pub app: Arc<crate::AppState>,
    pub rate_limiter: Mutex<auth::RateLimiter>,
}

/// Start the HTTP server if `webui.enabled` is set. Best-effort: a bind
/// failure or a schema-init failure is logged and the daemon carries on
/// without the web UI, same fail-safe convention as MCP/plugin loading —
/// this is an optional feature, not core daemon functionality.
pub async fn maybe_start(app: Arc<crate::AppState>) {
    let (enabled, port) = {
        let db = app.db.lock().expect("DB poisoned");
        let enabled = regin_core::db::setting_get(&db, "webui.enabled").unwrap_or_default() == "true";
        let port: u16 = regin_core::db::setting_get(&db, "webui.port").ok().and_then(|v| v.parse().ok()).unwrap_or(8080);
        (enabled, port)
    };
    if !enabled {
        return;
    }

    {
        let db = app.db.lock().expect("DB poisoned");
        if let Err(e) = auth::ensure_webui_schema(&db) {
            tracing::error!("webui: failed to initialize schema: {e:#}");
            return;
        }
    }

    let (artifacts_url, artifacts_dir, repo_url, repo_dir) = {
        let db = app.db.lock().expect("DB poisoned");
        let artifacts_url = regin_core::db::setting_get(&db, "webui.public.artifacts").unwrap_or_default();
        let repo_url = regin_core::db::setting_get(&db, "webui.public.repo").unwrap_or_default();
        let configured_dir = regin_core::db::setting_get(&db, "webui.artifacts_dir").unwrap_or_default();
        let base = if configured_dir.is_empty() {
            regin_core::config::data_dir().map(|d| d.join("artifacts")).unwrap_or_else(|_| std::path::PathBuf::from("artifacts"))
        } else {
            std::path::PathBuf::from(configured_dir)
        };
        (artifacts_url, base.clone(), repo_url, base.join("repo"))
    };
    let _ = tokio::fs::create_dir_all(&artifacts_dir).await;
    let _ = tokio::fs::create_dir_all(&repo_dir).await;
    artifacts::regenerate_repo_metadata(&repo_dir).await;

    let state = Arc::new(WebuiState { app: app.clone(), rate_limiter: Mutex::new(auth::RateLimiter::new()) });
    let router = build_router(state, &artifacts_url, artifacts_dir, &repo_url, repo_dir);

    let listener = match tokio::net::TcpListener::bind(("127.0.0.1", port)).await {
        Ok(l) => l,
        Err(e) => {
            tracing::error!("webui: failed to bind 127.0.0.1:{port}: {e:#}");
            return;
        }
    };
    app.webui_listening.store(true, Ordering::SeqCst);
    tracing::info!("webui: listening on http://127.0.0.1:{port}/");

    let listening_flag = app.webui_listening.clone();
    tokio::spawn(async move {
        let _ = axum::serve(listener, router.into_make_service_with_connect_info::<std::net::SocketAddr>()).await;
        listening_flag.store(false, Ordering::SeqCst);
    });
}

fn build_router(state: Arc<WebuiState>, artifacts_url: &str, artifacts_dir: std::path::PathBuf, repo_url: &str, repo_dir: std::path::PathBuf) -> Router {
    Router::new()
        .route("/", get(spa::landing_page))
        .route("/regin/", get(spa::spa_shell))
        .route("/regin/api/health", get(api::health))
        .route("/regin/api/auth/login", post(api::login))
        .route("/regin/api/auth/refresh", post(api::refresh))
        .route("/regin/api/sessions", get(api::sessions))
        .route("/regin/api/memory", get(api::memory))
        .route("/regin/api/runs", get(api::runs))
        .route("/regin/api/config", get(api::config_snapshot))
        .route("/regin/api/tabs", get(api::tabs_list))
        .route("/regin/api/tabs/register", post(api::tabs_register))
        .route("/regin/api/chat", get(ws_chat::handler))
        .route("/regin/api/terminal", get(ws_terminal::handler))
        .route("/regin/api/goal", get(ws_goal::handler))
        .merge(artifacts::router(artifacts_url, artifacts_dir, repo_url, repo_dir))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Auth extractor (acceptance criterion 5)
// ---------------------------------------------------------------------------

/// An authenticated request: the PAM username the bearer token/cookie
/// resolved to. Handlers that need auth simply take this as a parameter —
/// axum's extractor mechanism means a handler that *doesn't* take it is
/// reachable unauthenticated by construction, so `/health`/`/auth/login`
/// just omit it.
pub struct AuthedUser(#[allow(dead_code)] pub String);

impl FromRequestParts<Arc<WebuiState>> for AuthedUser {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &Arc<WebuiState>) -> Result<Self, Self::Rejection> {
        let token = bearer_from_header(parts).or_else(|| token_from_query(parts)).or_else(|| token_from_cookie(parts));
        let Some(token) = token else {
            return Err(unauthorized());
        };
        let now = chrono::Utc::now();
        let username = {
            let db = state.app.db.lock().expect("DB poisoned");
            auth::validate_token(&db, &token, now).map_err(|_| unauthorized())?
        };
        username.map(AuthedUser).ok_or_else(unauthorized)
    }
}

fn unauthorized() -> Response {
    (StatusCode::UNAUTHORIZED, "unauthorized").into_response()
}

fn bearer_from_header(parts: &Parts) -> Option<String> {
    let raw = parts.headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    raw.strip_prefix("Bearer ").map(str::to_string)
}

/// Browsers' native `WebSocket` constructor can't set custom headers, so
/// the WS endpoints also accept `?token=...` — a well-known, unavoidable
/// constraint of browser WS auth, not a laxer check (still validated the
/// same way as the header).
fn token_from_query(parts: &Parts) -> Option<String> {
    let query = parts.uri.query()?;
    query.split('&').find_map(|pair| pair.strip_prefix("token=")).map(|v| v.to_string())
}

fn token_from_cookie(parts: &Parts) -> Option<String> {
    let raw = parts.headers.get(header::COOKIE)?.to_str().ok()?;
    raw.split(';').map(str::trim).find_map(|kv| kv.strip_prefix("regin_token=")).map(str::to_string)
}

// Re-exported for submodules (api.rs, ws_*.rs, artifacts.rs) without
// needing to repeat `use axum::extract::State` everywhere.
pub(crate) type SharedState = State<Arc<WebuiState>>;

/// Shared test scaffolding for every `webui` submodule's test module — one
/// place to build a fresh in-memory `AppState` + `WebuiState` instead of
/// repeating the field list in `api.rs`/`ws_chat.rs`/`ws_terminal.rs`/
/// `ws_goal.rs`.
#[cfg(test)]
pub(crate) mod test_support {
    use super::WebuiState;
    use std::sync::{Arc, Mutex};

    pub fn fresh_app_state() -> Arc<crate::AppState> {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        regin_core::db::init_schema(&db).unwrap();
        super::auth::ensure_webui_schema(&db).unwrap();
        let identity_db = rusqlite::Connection::open_in_memory().unwrap();
        regin_core::identity_db::init_identity_schema(&identity_db).unwrap();
        // `AppState`'s fields are private but visible here: they're defined
        // in the crate root (main.rs) and privacy in Rust extends to all
        // descendant modules, which `webui::test_support` is.
        Arc::new(crate::AppState {
            db: Mutex::new(db),
            identity_db: Mutex::new(identity_db),
            llm_override: None,
            undo: Mutex::new(regin_core::undo::UndoStore::new()),
            lsp: regin_core::lsp::LspContext::new(Arc::new(regin_core::lsp::ProcessLspSpawner)),
            task_limiter: regin_core::subagent::TaskLimiter::new(3),
            pending_permissions: Mutex::new(std::collections::HashMap::new()),
            mcp: regin_core::mcp::McpContext::new(Arc::new(regin_core::mcp::ProcessMcpSpawner)),
            plugins: regin_core::plugin::PluginHost::new(),
            references: Vec::new(),
            webui_listening: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        })
    }

    pub fn fresh_app_state_with_llm(llm: Arc<dyn regin_core::llm::LlmClient>) -> Arc<crate::AppState> {
        let app = fresh_app_state();
        // `AppState` fields are private-but-visible here (see above); we
        // can't mutate `llm_override` after construction (no interior
        // mutability), so rebuild with it set instead of trying to patch
        // the `Arc` in place.
        Arc::new(crate::AppState { llm_override: Some(llm), ..unwrap_arc(app) })
    }

    pub fn fresh_webui_state() -> Arc<WebuiState> {
        Arc::new(WebuiState { app: fresh_app_state(), rate_limiter: Mutex::new(super::auth::RateLimiter::new()) })
    }

    fn unwrap_arc(app: Arc<crate::AppState>) -> crate::AppState {
        Arc::try_unwrap(app).unwrap_or_else(|_| panic!("fresh_app_state's Arc should have exactly one owner"))
    }
}

/// End-to-end tests over a *real* bound `TcpListener` + `axum::serve`
/// (acceptance criterion 13: "WebSocket chat streaming test"; criterion
/// 14's `regin webui enable` + curl flow is covered by `regind`'s own
/// integration test suite, not here — this exercises the router/handler
/// wiring directly without going through the CLI).
#[cfg(test)]
mod integration_tests {
    use super::test_support;
    use super::*;
    use futures_util::{SinkExt, StreamExt};
    use regin_core::llm::{FakeLlm, LlmTurn};
    use std::net::SocketAddr;

    async fn spawn_test_server(app: Arc<crate::AppState>) -> SocketAddr {
        {
            let db = app.db.lock().unwrap();
            auth::ensure_webui_schema(&db).unwrap();
        }
        let state = Arc::new(WebuiState { app, rate_limiter: Mutex::new(auth::RateLimiter::new()) });
        let dir = std::env::temp_dir();
        let router = build_router(state, "", dir.clone(), "", dir);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, router.into_make_service_with_connect_info::<SocketAddr>()).await;
        });
        addr
    }

    #[tokio::test]
    async fn health_endpoint_responds_over_a_real_bound_listener() {
        let app = test_support::fresh_app_state();
        let addr = spawn_test_server(app).await;
        let resp = reqwest::get(format!("http://{addr}/regin/api/health")).await.unwrap();
        assert_eq!(resp.status(), 200);
    }

    #[tokio::test]
    async fn protected_endpoint_rejects_a_request_without_a_token() {
        let app = test_support::fresh_app_state();
        let addr = spawn_test_server(app).await;
        let resp = reqwest::get(format!("http://{addr}/regin/api/sessions")).await.unwrap();
        assert_eq!(resp.status(), 401);
    }

    #[tokio::test]
    async fn chat_websocket_streams_a_done_event_for_a_text_only_reply() {
        let fake = Arc::new(FakeLlm::new());
        fake.push_turn(LlmTurn::Text("hello from the web ui".to_string()));
        let app = test_support::fresh_app_state_with_llm(fake);
        let now = chrono::Utc::now();
        let token = {
            let db = app.db.lock().unwrap();
            auth::ensure_webui_schema(&db).unwrap();
            auth::issue_token(&db, "tester", now).unwrap()
        };
        let addr = spawn_test_server(app).await;

        let url = format!("ws://{addr}/regin/api/chat?token={token}");
        let (mut ws, _resp) = tokio_tungstenite::connect_async(url).await.unwrap();
        ws.send(tokio_tungstenite::tungstenite::Message::Text(serde_json::json!({"message": "hi"}).to_string())).await.unwrap();

        let msg = tokio::time::timeout(std::time::Duration::from_secs(5), ws.next()).await.expect("timed out waiting for a ws reply").unwrap().unwrap();
        let text = msg.into_text().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed["type"], "done");
        assert_eq!(parsed["text"], "hello from the web ui");
    }
}
