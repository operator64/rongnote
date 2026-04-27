use std::sync::Arc;

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use axum::{
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    error::{AppError, AppResult},
    session::{
        build_clear_cookie, build_session_cookie, cookie_to_header, create_session,
        delete_session, parse_cookie_jar, AuthUser, COOKIE_NAME,
    },
    AppState,
};

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/register", post(register))
        .route("/precheck", post(precheck))
        .route("/login", post(login))
        .route("/logout", post(logout))
        .route("/me", get(me))
        .route("/recovery_init", post(recovery_init))
        .route("/reset_passphrase", post(reset_passphrase))
}

// --- Wire types ---

#[derive(Debug, Deserialize)]
pub struct RegisterBody {
    email: String,
    #[serde(with = "crate::b64")]
    passphrase_salt: Vec<u8>,
    #[serde(with = "crate::b64")]
    recovery_salt: Vec<u8>,
    #[serde(with = "crate::b64")]
    master_wrap_passphrase: Vec<u8>,
    #[serde(with = "crate::b64")]
    master_wrap_recovery: Vec<u8>,
    /// BLAKE2b-keyed(master_key, "rongnote-auth-v1"). Server stores Argon2id
    /// of this, never the raw value.
    #[serde(with = "crate::b64")]
    auth_hash: Vec<u8>,
    #[serde(with = "crate::b64")]
    public_key: Vec<u8>,
    #[serde(with = "crate::b64")]
    encrypted_private_key: Vec<u8>,
}

#[derive(Debug, Deserialize)]
pub struct EmailBody {
    email: String,
}

#[derive(Debug, Serialize)]
pub struct PrecheckResponse {
    #[serde(with = "crate::b64")]
    passphrase_salt: Vec<u8>,
    #[serde(with = "crate::b64")]
    master_wrap_passphrase: Vec<u8>,
}

#[derive(Debug, Serialize)]
pub struct RecoveryInitResponse {
    #[serde(with = "crate::b64")]
    recovery_salt: Vec<u8>,
    #[serde(with = "crate::b64")]
    master_wrap_recovery: Vec<u8>,
}

#[derive(Debug, Deserialize)]
pub struct LoginBody {
    email: String,
    #[serde(with = "crate::b64")]
    auth_hash: Vec<u8>,
}

#[derive(Debug, Deserialize)]
pub struct ResetPassphraseBody {
    email: String,
    /// Proves the caller knows master_key (via passphrase OR recovery code).
    #[serde(with = "crate::b64")]
    auth_hash: Vec<u8>,
    #[serde(with = "crate::b64")]
    new_passphrase_salt: Vec<u8>,
    #[serde(with = "crate::b64")]
    new_master_wrap_passphrase: Vec<u8>,
}

#[derive(Debug, Serialize)]
pub struct UserView {
    id: Uuid,
    email: String,
    #[serde(with = "crate::b64")]
    passphrase_salt: Vec<u8>,
    #[serde(with = "crate::b64")]
    master_wrap_passphrase: Vec<u8>,
    #[serde(with = "crate::b64")]
    public_key: Vec<u8>,
    #[serde(with = "crate::b64")]
    encrypted_private_key: Vec<u8>,
}

impl UserView {
    pub fn new(
        id: Uuid,
        email: String,
        passphrase_salt: Vec<u8>,
        master_wrap_passphrase: Vec<u8>,
        public_key: Vec<u8>,
        encrypted_private_key: Vec<u8>,
    ) -> Self {
        Self {
            id,
            email,
            passphrase_salt,
            master_wrap_passphrase,
            public_key,
            encrypted_private_key,
        }
    }
}

// --- Helpers ---

const SALT_LEN: usize = 16;
const KEY_LEN: usize = 32;
const WRAP_LEN: usize = 24 + 32 + 16; // nonce + key + Poly1305 tag
const ENCRYPTED_PRIVKEY_LEN: usize = 24 + 32 + 16;

fn validate_email(email: &str) -> AppResult<String> {
    let email = email.trim().to_lowercase();
    if email.is_empty() || !email.contains('@') {
        return Err(AppError::BadRequest("invalid email".into()));
    }
    Ok(email)
}

fn check_len(name: &str, bytes: &[u8], expected: usize) -> AppResult<()> {
    if bytes.len() != expected {
        return Err(AppError::BadRequest(format!(
            "{name} must be {expected} bytes, got {}",
            bytes.len()
        )));
    }
    Ok(())
}

fn server_hash(auth_hash: &[u8]) -> AppResult<String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(auth_hash, &salt)
        .map(|h| h.to_string())
        .map_err(|e| AppError::Other(anyhow::anyhow!("server hash failed: {e}")))
}

fn server_verify(auth_hash: &[u8], stored: &str) -> bool {
    let parsed = match PasswordHash::new(stored) {
        Ok(h) => h,
        Err(_) => return false,
    };
    Argon2::default().verify_password(auth_hash, &parsed).is_ok()
}

// --- Handlers ---

async fn register(
    State(state): State<Arc<AppState>>,
    Json(body): Json<RegisterBody>,
) -> AppResult<impl IntoResponse> {
    let email = validate_email(&body.email)?;
    check_len("passphrase_salt", &body.passphrase_salt, SALT_LEN)?;
    check_len("recovery_salt", &body.recovery_salt, SALT_LEN)?;
    check_len("master_wrap_passphrase", &body.master_wrap_passphrase, WRAP_LEN)?;
    check_len("master_wrap_recovery", &body.master_wrap_recovery, WRAP_LEN)?;
    check_len("auth_hash", &body.auth_hash, KEY_LEN)?;
    check_len("public_key", &body.public_key, KEY_LEN)?;
    check_len(
        "encrypted_private_key",
        &body.encrypted_private_key,
        ENCRYPTED_PRIVKEY_LEN,
    )?;

    let stored_hash = server_hash(&body.auth_hash)?;

    let mut tx = state.pool.begin().await?;

    let exists: Option<(Uuid,)> = sqlx::query_as("SELECT id FROM users WHERE email = $1")
        .bind(&email)
        .fetch_optional(&mut *tx)
        .await?;
    if exists.is_some() {
        return Err(AppError::Conflict("email already registered".into()));
    }

    let user_id: (Uuid,) = sqlx::query_as(
        r#"
        INSERT INTO users (
            email, passphrase_salt, recovery_salt, auth_hash,
            public_key, encrypted_private_key,
            master_wrap_passphrase, master_wrap_recovery
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        RETURNING id
        "#,
    )
    .bind(&email)
    .bind(&body.passphrase_salt)
    .bind(&body.recovery_salt)
    .bind(&stored_hash)
    .bind(&body.public_key)
    .bind(&body.encrypted_private_key)
    .bind(&body.master_wrap_passphrase)
    .bind(&body.master_wrap_recovery)
    .fetch_one(&mut *tx)
    .await?;
    let user_id = user_id.0;

    let space: (Uuid,) = sqlx::query_as(
        "INSERT INTO spaces (name, kind, owner_id) VALUES ($1, 'personal', $2) RETURNING id",
    )
    .bind("Personal")
    .bind(user_id)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO memberships (user_id, space_id, role) VALUES ($1, $2, 'owner')",
    )
    .bind(user_id)
    .bind(space.0)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    crate::audit::record_user(&state.pool, user_id, "auth.register", None).await;

    let view = UserView {
        id: user_id,
        email,
        passphrase_salt: body.passphrase_salt,
        master_wrap_passphrase: body.master_wrap_passphrase,
        public_key: body.public_key,
        encrypted_private_key: body.encrypted_private_key,
    };
    issue_session_response(&state, view, StatusCode::CREATED).await
}

async fn precheck(
    State(state): State<Arc<AppState>>,
    Json(body): Json<EmailBody>,
) -> AppResult<Json<PrecheckResponse>> {
    let email = validate_email(&body.email)?;
    let row: Option<(Vec<u8>, Vec<u8>)> = sqlx::query_as(
        "SELECT passphrase_salt, master_wrap_passphrase FROM users WHERE email = $1",
    )
    .bind(&email)
    .fetch_optional(&state.pool)
    .await?;
    let (passphrase_salt, master_wrap_passphrase) = row.ok_or(AppError::NotFound)?;
    Ok(Json(PrecheckResponse {
        passphrase_salt,
        master_wrap_passphrase,
    }))
}

async fn recovery_init(
    State(state): State<Arc<AppState>>,
    Json(body): Json<EmailBody>,
) -> AppResult<Json<RecoveryInitResponse>> {
    let email = validate_email(&body.email)?;
    let row: Option<(Vec<u8>, Vec<u8>)> = sqlx::query_as(
        "SELECT recovery_salt, master_wrap_recovery FROM users WHERE email = $1",
    )
    .bind(&email)
    .fetch_optional(&state.pool)
    .await?;
    let (recovery_salt, master_wrap_recovery) = row.ok_or(AppError::NotFound)?;
    Ok(Json(RecoveryInitResponse {
        recovery_salt,
        master_wrap_recovery,
    }))
}

async fn login(
    State(state): State<Arc<AppState>>,
    Json(body): Json<LoginBody>,
) -> AppResult<impl IntoResponse> {
    let email = validate_email(&body.email)?;
    check_len("auth_hash", &body.auth_hash, KEY_LEN)?;

    type Row = (Uuid, String, Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>);
    let row: Option<Row> = sqlx::query_as(
        r#"
        SELECT id, auth_hash, passphrase_salt, master_wrap_passphrase,
               public_key, encrypted_private_key
          FROM users WHERE email = $1
        "#,
    )
    .bind(&email)
    .fetch_optional(&state.pool)
    .await?;

    let (user_id, stored, passphrase_salt, master_wrap_passphrase, public_key, encrypted_private_key) =
        match row {
            Some(r) => r,
            None => {
                let _ = server_hash(&body.auth_hash);
                return Err(AppError::Unauthorized);
            }
        };

    if !server_verify(&body.auth_hash, &stored) {
        return Err(AppError::Unauthorized);
    }

    crate::audit::record_user(
        &state.pool,
        user_id,
        "auth.login",
        Some(serde_json::json!({"method": "passphrase"})),
    )
    .await;

    issue_session_response(
        &state,
        UserView {
            id: user_id,
            email,
            passphrase_salt,
            master_wrap_passphrase,
            public_key,
            encrypted_private_key,
        },
        StatusCode::OK,
    )
    .await
}

async fn reset_passphrase(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ResetPassphraseBody>,
) -> AppResult<StatusCode> {
    let email = validate_email(&body.email)?;
    check_len("auth_hash", &body.auth_hash, KEY_LEN)?;
    check_len("new_passphrase_salt", &body.new_passphrase_salt, SALT_LEN)?;
    check_len(
        "new_master_wrap_passphrase",
        &body.new_master_wrap_passphrase,
        WRAP_LEN,
    )?;

    let stored: Option<(String,)> =
        sqlx::query_as("SELECT auth_hash FROM users WHERE email = $1")
            .bind(&email)
            .fetch_optional(&state.pool)
            .await?;
    let stored = match stored {
        Some(s) => s.0,
        None => {
            let _ = server_hash(&body.auth_hash);
            return Err(AppError::Unauthorized);
        }
    };

    if !server_verify(&body.auth_hash, &stored) {
        return Err(AppError::Unauthorized);
    }

    sqlx::query(
        r#"
        UPDATE users
           SET passphrase_salt = $2,
               master_wrap_passphrase = $3
         WHERE email = $1
        "#,
    )
    .bind(&email)
    .bind(&body.new_passphrase_salt)
    .bind(&body.new_master_wrap_passphrase)
    .execute(&state.pool)
    .await?;

    // Optionally invalidate all existing sessions on passphrase reset.
    sqlx::query("DELETE FROM sessions WHERE user_id = (SELECT id FROM users WHERE email = $1)")
        .bind(&email)
        .execute(&state.pool)
        .await?;

    let user_id: Option<(Uuid,)> = sqlx::query_as("SELECT id FROM users WHERE email = $1")
        .bind(&email)
        .fetch_optional(&state.pool)
        .await?;
    if let Some((uid,)) = user_id {
        crate::audit::record_user(&state.pool, uid, "auth.passphrase_reset", None).await;
    }

    Ok(StatusCode::NO_CONTENT)
}

async fn logout(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> AppResult<impl IntoResponse> {
    let jar = parse_cookie_jar(&headers);
    if let Some(c) = jar.get(COOKIE_NAME) {
        if let Ok(id) = Uuid::parse_str(c.value()) {
            // Capture the user before destroying the session so we can audit-log it.
            let uid: Option<(Uuid,)> =
                sqlx::query_as("SELECT user_id FROM sessions WHERE id = $1")
                    .bind(id)
                    .fetch_optional(&state.pool)
                    .await
                    .ok()
                    .flatten();
            delete_session(&state.pool, id).await.ok();
            if let Some((uid,)) = uid {
                crate::audit::record_user(&state.pool, uid, "auth.logout", None).await;
            }
        }
    }
    let mut response_headers = HeaderMap::new();
    response_headers.insert(
        header::SET_COOKIE,
        cookie_to_header(&build_clear_cookie(&state.config)),
    );
    Ok((StatusCode::NO_CONTENT, response_headers))
}

async fn me(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
) -> AppResult<Json<UserView>> {
    let row: (Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>) = sqlx::query_as(
        r#"
        SELECT passphrase_salt, master_wrap_passphrase, public_key, encrypted_private_key
          FROM users WHERE id = $1
        "#,
    )
    .bind(user.user_id)
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(UserView {
        id: user.user_id,
        email: user.email,
        passphrase_salt: row.0,
        master_wrap_passphrase: row.1,
        public_key: row.2,
        encrypted_private_key: row.3,
    }))
}

async fn issue_session_response(
    state: &Arc<AppState>,
    view: UserView,
    status: StatusCode,
) -> AppResult<axum::response::Response> {
    let (session_id, expires_at) =
        create_session(&state.pool, view.id, state.config.session_ttl).await?;
    let cookie = build_session_cookie(&state.config, session_id, expires_at);
    let mut headers = HeaderMap::new();
    headers.insert(header::SET_COOKIE, cookie_to_header(&cookie));
    Ok((status, headers, Json(view)).into_response())
}
