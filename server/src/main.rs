use std::sync::Arc;

use axum::{
    http::{header, HeaderValue, Method, StatusCode},
    routing::get,
    Router,
};
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
mod static_assets;

use config::Config;
use passkey::PasskeyService;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: Config,
    pub passkey: Arc<PasskeyService>,
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

    let state = Arc::new(AppState {
        pool,
        config: config.clone(),
        passkey: passkey_service,
    });

    let auth_routes = auth::router().nest("/passkey", passkey::router());

    let api = Router::new()
        .nest("/auth", auth_routes)
        .nest("/items", items::router())
        .nest("/files", files::router())
        .nest("/audit_log", audit::router())
        .nest("/export", export::router())
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

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,tower_http=info,sqlx=warn"));
    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_target(false))
        .init();
}

/// In dev the SvelteKit server runs on a separate origin (5173), so we allow
/// it via CORS. In production the SPA is served from the same origin so CORS
/// is effectively a no-op.
fn cors_layer(config: &Config) -> CorsLayer {
    if config.is_production {
        return CorsLayer::new();
    }
    let dev_origins = [
        HeaderValue::from_static("http://localhost:5173"),
        HeaderValue::from_static("http://127.0.0.1:5173"),
    ];
    CorsLayer::new()
        .allow_origin(AllowOrigin::list(dev_origins))
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
