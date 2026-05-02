use std::sync::Arc;

use axum::{
    extract::State,
    http::{header, Method, StatusCode},
    routing::get,
    Json, Router,
};
use serde::Serialize;
use sqlx::PgPool;
use tower_http::{
    cors::{AllowOrigin, CorsLayer},
    trace::TraceLayer,
};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

mod audit;
mod auth;
mod b64;
mod config;
mod db;
mod error;
mod export;
mod files;
mod items;
mod passkey;
mod session;
mod shares;
mod spaces;
mod static_assets;
mod transit;

use config::Config;
use passkey::PasskeyService;
use transit::TransitCache;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: Config,
    pub passkey: Arc<PasskeyService>,
    /// Shared outbound HTTP client for the VRR EFA proxy. Reuses
    /// connection pooling + DNS cache across requests.
    pub http: reqwest::Client,
    pub transit_cache: TransitCache,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Best-effort: load .env from the workspace root (one level above
    // server/). Silent if it isn't there.
    let _ = dotenvy::from_path("../.env");
    let _ = dotenvy::dotenv();

    init_tracing();

    let config = Config::from_env()?;
    tracing::info!(addr = %config.bind_addr, "starting rongnote-server");

    let pool = db::connect(&config.database_url).await?;
    db::migrate(&pool).await?;

    let passkey_service = Arc::new(PasskeyService::new(&config)?);

    let http = reqwest::Client::builder()
        .user_agent("rongnote/1.0 (+https://github.com/operator64/rongnote)")
        .timeout(std::time::Duration::from_secs(8))
        .build()?;

    let state = Arc::new(AppState {
        pool,
        config: config.clone(),
        passkey: passkey_service,
        http,
        transit_cache: transit::new_cache(),
    });

    let auth_routes = auth::router().nest("/passkey", passkey::router());

    let api = Router::new()
        .route("/config", get(public_config))
        .nest("/auth", auth_routes)
        .nest("/items", items::router())
        .nest("/files", files::router())
        .nest("/audit_log", audit::router())
        .nest("/export", export::router())
        .nest("/spaces", spaces::router())
        .nest("/transit", transit::router())
        .merge(shares::router());

    let app = Router::new()
        .route("/healthz", get(healthz))
        .nest("/api/v1", api)
        .fallback(static_assets::handler)
        .with_state(state)
        .layer(TraceLayer::new_for_http())
        .layer(cors_layer(&config));

    let listener = tokio::net::TcpListener::bind(&config.bind_addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn healthz() -> (StatusCode, &'static str) {
    (StatusCode::OK, "ok")
}

#[derive(Serialize)]
struct PublicConfig {
    registration_open: bool,
}

/// Public, unauthenticated config the SPA needs to render correctly
/// before the user has logged in. Specifically: whether to show the
/// register form.
async fn public_config(State(state): State<Arc<AppState>>) -> Json<PublicConfig> {
    Json(PublicConfig {
        registration_open: state.config.registration_open,
    })
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,tower_http=info,sqlx=warn"));
    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_target(false))
        .init();
}

/// CORS rules:
/// - dev SPA on http://localhost:5173 — origin allowlisted (dev only)
/// - browser extension popups on moz-extension:// + chrome-extension:// —
///   allowed in any env, since the extension's UUID isn't known up front.
/// - prod same-origin SPA — no CORS interaction needed.
///
/// Credentials are allowed across the board so cookie-based sessions work
/// from the extension popup.
fn cors_layer(config: &Config) -> CorsLayer {
    let allow_dev_spa = !config.is_production;
    let allow_origin = AllowOrigin::predicate(move |origin, _req| {
        if let Ok(s) = origin.to_str() {
            if s.starts_with("moz-extension://") || s.starts_with("chrome-extension://") {
                return true;
            }
            if allow_dev_spa
                && (s == "http://localhost:5173" || s == "http://127.0.0.1:5173")
            {
                return true;
            }
        }
        false
    });
    CorsLayer::new()
        .allow_origin(allow_origin)
        .allow_credentials(true)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([header::CONTENT_TYPE, header::ACCEPT])
}
