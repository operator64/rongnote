use std::sync::Arc;

use axum::{
    async_trait,
    extract::FromRequestParts,
    http::{header, request::Parts, HeaderValue},
};
use cookie::{Cookie, CookieJar, SameSite};
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{config::Config, error::AppError, AppState};

pub const COOKIE_NAME: &str = "rongnote_session";

#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: Uuid,
    #[allow(dead_code)] // used by future admin/logout-other-session flows
    pub session_id: Uuid,
    pub email: String,
}

pub fn parse_cookie_jar(headers: &axum::http::HeaderMap) -> CookieJar {
    let mut jar = CookieJar::new();
    if let Some(value) = headers.get(header::COOKIE).and_then(|v| v.to_str().ok()) {
        for raw in value.split(';') {
            if let Ok(cookie) = Cookie::parse_encoded(raw.trim().to_owned()) {
                jar.add_original(cookie);
            }
        }
    }
    jar
}

pub fn build_session_cookie<'a>(
    config: &Config,
    session_id: Uuid,
    expires_at: OffsetDateTime,
) -> Cookie<'a> {
    let mut cookie = Cookie::new(COOKIE_NAME, session_id.to_string());
    cookie.set_http_only(true);
    cookie.set_same_site(SameSite::Lax);
    cookie.set_secure(config.cookie_secure());
    cookie.set_path("/");
    cookie.set_expires(expires_at);
    cookie
}

pub fn build_clear_cookie<'a>(config: &Config) -> Cookie<'a> {
    let mut cookie = Cookie::new(COOKIE_NAME, "");
    cookie.set_http_only(true);
    cookie.set_same_site(SameSite::Lax);
    cookie.set_secure(config.cookie_secure());
    cookie.set_path("/");
    cookie.set_max_age(time::Duration::ZERO);
    cookie
}

pub fn cookie_to_header(cookie: &Cookie<'_>) -> HeaderValue {
    HeaderValue::from_str(&cookie.encoded().to_string())
        .expect("cookie should encode to a valid header value")
}

pub async fn create_session(
    pool: &PgPool,
    user_id: Uuid,
    ttl: std::time::Duration,
) -> Result<(Uuid, OffsetDateTime), sqlx::Error> {
    let expires_at = OffsetDateTime::now_utc() + time::Duration::seconds(ttl.as_secs() as i64);
    let row: (Uuid,) = sqlx::query_as(
        "INSERT INTO sessions (user_id, expires_at) VALUES ($1, $2) RETURNING id",
    )
    .bind(user_id)
    .bind(expires_at)
    .fetch_one(pool)
    .await?;
    Ok((row.0, expires_at))
}

pub async fn delete_session(pool: &PgPool, session_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM sessions WHERE id = $1")
        .bind(session_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn touch_and_load(
    pool: &PgPool,
    session_id: Uuid,
) -> Result<Option<AuthUser>, sqlx::Error> {
    let row = sqlx::query_as::<_, (Uuid, Uuid, String, OffsetDateTime)>(
        r#"
        UPDATE sessions
           SET last_seen_at = NOW()
         WHERE id = $1
           AND expires_at > NOW()
        RETURNING sessions.id, sessions.user_id,
                  (SELECT email FROM users WHERE users.id = sessions.user_id),
                  sessions.expires_at
        "#,
    )
    .bind(session_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|(session_id, user_id, email, _exp)| AuthUser {
        session_id,
        user_id,
        email,
    }))
}

#[async_trait]
impl FromRequestParts<Arc<AppState>> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        let jar = parse_cookie_jar(&parts.headers);
        let raw = jar
            .get(COOKIE_NAME)
            .ok_or(AppError::Unauthorized)?
            .value()
            .to_owned();
        let session_id = Uuid::parse_str(&raw).map_err(|_| AppError::Unauthorized)?;
        let user = touch_and_load(&state.pool, session_id)
            .await?
            .ok_or(AppError::Unauthorized)?;
        Ok(user)
    }
}
