//! WebAuthn / Passkeys.
//!
//! Server only handles proof-of-possession via webauthn-rs. The PRF output is
//! never sent back here — that lives strictly between the authenticator and
//! the browser. Server stores `master_wrap_passkey`, an opaque blob the
//! client wraps with its PRF-derived KEK; only the authenticator can produce
//! the matching key to unwrap.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::{
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use uuid::Uuid;
use webauthn_rs::prelude::*;

use crate::{
    config::Config,
    error::{AppError, AppResult},
    session::{
        build_session_cookie, cookie_to_header, create_session, AuthUser,
    },
    AppState,
};

const CHALLENGE_TTL: Duration = Duration::from_secs(5 * 60);

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/register/begin", post(register_begin))
        .route("/register/finish", post(register_finish))
        .route("/login/begin", post(login_begin))
        .route("/login/finish", post(login_finish))
        .route("/", axum::routing::get(list))
        .route("/:id", axum::routing::delete(delete_one))
}

pub struct PasskeyService {
    webauthn: Arc<Webauthn>,
    pending_reg:
        Mutex<HashMap<Uuid, (Instant, Uuid /* user_id */, PasskeyRegistration)>>,
    pending_auth: Mutex<HashMap<Uuid, (Instant, DiscoverableAuthentication)>>,
}

impl PasskeyService {
    pub fn new(config: &Config) -> anyhow::Result<Self> {
        let url = url::Url::parse(&config.public_url)?;
        let rp_id = url
            .host_str()
            .ok_or_else(|| anyhow::anyhow!("PUBLIC_URL has no host"))?;
        let webauthn = WebauthnBuilder::new(rp_id, &url)?
            .rp_name("rongnote")
            .build()?;
        Ok(Self {
            webauthn: Arc::new(webauthn),
            pending_reg: Mutex::new(HashMap::new()),
            pending_auth: Mutex::new(HashMap::new()),
        })
    }

    fn gc(&self) {
        let now = Instant::now();
        self.pending_reg
            .lock()
            .retain(|_, (t, _, _)| now.duration_since(*t) < CHALLENGE_TTL);
        self.pending_auth
            .lock()
            .retain(|_, (t, _)| now.duration_since(*t) < CHALLENGE_TTL);
    }
}

// --- Wire types ---

#[derive(Debug, Serialize)]
pub struct RegisterBeginResponse {
    state_id: Uuid,
    options: CreationChallengeResponse,
}

#[derive(Debug, Deserialize)]
pub struct RegisterFinishBody {
    state_id: Uuid,
    response: RegisterPublicKeyCredential,
    #[serde(with = "crate::b64")]
    master_wrap_passkey: Vec<u8>,
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PasskeyView {
    id: Uuid,
    name: String,
    #[serde(with = "time::serde::rfc3339")]
    created_at: time::OffsetDateTime,
}

#[derive(Debug, Serialize)]
pub struct LoginBeginResponse {
    state_id: Uuid,
    options: RequestChallengeResponse,
}

#[derive(Debug, Deserialize)]
pub struct LoginFinishBody {
    state_id: Uuid,
    response: PublicKeyCredential,
}

#[derive(Debug, Serialize)]
pub struct LoginFinishResponse {
    user: crate::auth::UserView,
    /// nonce(24) || secretbox of master_key with PRF-derived KEK.
    #[serde(with = "crate::b64")]
    master_wrap_passkey: Vec<u8>,
}

// --- Handlers ---

async fn register_begin(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
) -> AppResult<Json<RegisterBeginResponse>> {
    state.passkey.gc();

    // Exclude any already-registered credentials for this user.
    let rows: Vec<(Vec<u8>,)> =
        sqlx::query_as("SELECT credential_id FROM passkeys WHERE user_id = $1")
            .bind(user.user_id)
            .fetch_all(&state.pool)
            .await?;
    let exclude: Vec<CredentialID> = rows
        .into_iter()
        .map(|(b,)| CredentialID::from(b))
        .collect();
    let exclude_opt = if exclude.is_empty() {
        None
    } else {
        Some(exclude)
    };

    let (options, registration) = state
        .passkey
        .webauthn
        .start_passkey_registration(user.user_id, &user.email, &user.email, exclude_opt)
        .map_err(|e| AppError::Other(anyhow::anyhow!("webauthn begin: {e}")))?;

    let state_id = Uuid::new_v4();
    state
        .passkey
        .pending_reg
        .lock()
        .insert(state_id, (Instant::now(), user.user_id, registration));

    Ok(Json(RegisterBeginResponse { state_id, options }))
}

async fn register_finish(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(body): Json<RegisterFinishBody>,
) -> AppResult<Json<PasskeyView>> {
    if body.master_wrap_passkey.len() != 24 + 32 + 16 {
        return Err(AppError::BadRequest(
            "master_wrap_passkey must be 72 bytes".into(),
        ));
    }

    let registration = state
        .passkey
        .pending_reg
        .lock()
        .remove(&body.state_id)
        .map(|(_, uid, reg)| (uid, reg))
        .ok_or_else(|| AppError::BadRequest("unknown or expired state_id".into()))?;
    if registration.0 != user.user_id {
        return Err(AppError::Forbidden);
    }

    let passkey = state
        .passkey
        .webauthn
        .finish_passkey_registration(&body.response, &registration.1)
        .map_err(|e| AppError::BadRequest(format!("attestation invalid: {e}")))?;

    let credential_id: Vec<u8> = passkey.cred_id().as_ref().to_vec();
    let credential_json = serde_json::to_value(&passkey)
        .map_err(|e| AppError::Other(anyhow::anyhow!("serialize passkey: {e}")))?;

    let name = body
        .name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("passkey")
        .to_owned();

    let row: (Uuid, String, time::OffsetDateTime) = sqlx::query_as(
        r#"
        INSERT INTO passkeys (user_id, credential_id, name, credential, master_wrap_passkey)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id, name, created_at
        "#,
    )
    .bind(user.user_id)
    .bind(&credential_id)
    .bind(&name)
    .bind(&credential_json)
    .bind(&body.master_wrap_passkey)
    .fetch_one(&state.pool)
    .await?;

    crate::audit::record_user(
        &state.pool,
        user.user_id,
        "auth.passkey_register",
        Some(serde_json::json!({ "name": row.1 })),
    )
    .await;

    Ok(Json(PasskeyView {
        id: row.0,
        name: row.1,
        created_at: row.2,
    }))
}

async fn login_begin(
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<LoginBeginResponse>> {
    state.passkey.gc();

    let (options, auth) = state
        .passkey
        .webauthn
        .start_discoverable_authentication()
        .map_err(|e| AppError::Other(anyhow::anyhow!("discoverable begin: {e}")))?;

    let state_id = Uuid::new_v4();
    state
        .passkey
        .pending_auth
        .lock()
        .insert(state_id, (Instant::now(), auth));

    Ok(Json(LoginBeginResponse { state_id, options }))
}

async fn login_finish(
    State(state): State<Arc<AppState>>,
    Json(body): Json<LoginFinishBody>,
) -> AppResult<axum::response::Response> {
    let auth = state
        .passkey
        .pending_auth
        .lock()
        .remove(&body.state_id)
        .map(|(_, a)| a)
        .ok_or_else(|| AppError::BadRequest("unknown or expired state_id".into()))?;

    // Identify the credential — webauthn-rs gives us the cred_id + user_uuid
    // from the discoverable response before we verify.
    let (user_uuid, credential_id_raw) = state
        .passkey
        .webauthn
        .identify_discoverable_authentication(&body.response)
        .map_err(|e| AppError::BadRequest(format!("invalid response: {e}")))?;

    // Look up the credential by its id and the user; use the stored Passkey
    // for verification.
    let credential_id_bytes: Vec<u8> = credential_id_raw.as_ref().to_vec();
    let row: Option<(Uuid, JsonValue, Vec<u8>)> = sqlx::query_as(
        r#"
        SELECT user_id, credential, master_wrap_passkey
          FROM passkeys
         WHERE credential_id = $1
        "#,
    )
    .bind(&credential_id_bytes)
    .fetch_optional(&state.pool)
    .await?;
    let (db_user_id, cred_json, master_wrap_passkey) =
        row.ok_or(AppError::Unauthorized)?;
    if db_user_id != user_uuid {
        return Err(AppError::Unauthorized);
    }

    let stored: Passkey = serde_json::from_value(cred_json)
        .map_err(|e| AppError::Other(anyhow::anyhow!("deserialize passkey: {e}")))?;

    let result = state
        .passkey
        .webauthn
        .finish_discoverable_authentication(&body.response, auth, &[(&stored).into()])
        .map_err(|e| AppError::Unauthorized.with_message_of(format!("auth failed: {e}")))?;

    // Update sign count + last_used.
    let updated_passkey = {
        let mut p = stored.clone();
        p.update_credential(&result);
        serde_json::to_value(&p).unwrap_or(JsonValue::Null)
    };
    sqlx::query(
        r#"
        UPDATE passkeys
           SET credential = $2,
               last_used_at = NOW()
         WHERE credential_id = $1
        "#,
    )
    .bind(&credential_id_bytes)
    .bind(&updated_passkey)
    .execute(&state.pool)
    .await
    .ok();

    // Load user view + open a session.
    let user_row: (String, Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>) = sqlx::query_as(
        r#"
        SELECT email, passphrase_salt, master_wrap_passphrase,
               public_key, encrypted_private_key
          FROM users WHERE id = $1
        "#,
    )
    .bind(db_user_id)
    .fetch_one(&state.pool)
    .await?;

    let view = crate::auth::UserView::new(
        db_user_id,
        user_row.0,
        user_row.1,
        user_row.2,
        user_row.3,
        user_row.4,
    );

    let (session_id, expires_at) =
        create_session(&state.pool, db_user_id, state.config.session_ttl).await?;
    let cookie = build_session_cookie(&state.config, session_id, expires_at);

    crate::audit::record_user(
        &state.pool,
        db_user_id,
        "auth.login",
        Some(serde_json::json!({"method": "passkey"})),
    )
    .await;

    let mut headers = HeaderMap::new();
    headers.insert(header::SET_COOKIE, cookie_to_header(&cookie));

    let body = LoginFinishResponse {
        user: view,
        master_wrap_passkey,
    };
    Ok((StatusCode::OK, headers, Json(body)).into_response())
}

// --- List + delete (registered passkeys) ---

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct PasskeyListItem {
    id: Uuid,
    name: String,
    #[serde(with = "time::serde::rfc3339")]
    created_at: time::OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option", skip_serializing_if = "Option::is_none")]
    last_used_at: Option<time::OffsetDateTime>,
}

async fn list(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
) -> AppResult<Json<Vec<PasskeyListItem>>> {
    let rows = sqlx::query_as::<_, PasskeyListItem>(
        r#"
        SELECT id, name, created_at, last_used_at
          FROM passkeys
         WHERE user_id = $1
         ORDER BY created_at ASC
        "#,
    )
    .bind(user.user_id)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(rows))
}

async fn delete_one(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
) -> AppResult<StatusCode> {
    let row = sqlx::query_as::<_, (String,)>(
        "DELETE FROM passkeys WHERE id = $1 AND user_id = $2 RETURNING name",
    )
    .bind(id)
    .bind(user.user_id)
    .fetch_optional(&state.pool)
    .await?;
    let name = row.map(|r| r.0).ok_or(AppError::NotFound)?;
    crate::audit::record_user(
        &state.pool,
        user.user_id,
        "auth.passkey_delete",
        Some(serde_json::json!({"name": name})),
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

// --- Helpers ---

trait WithMessage {
    fn with_message_of(self, msg: String) -> Self;
}
impl WithMessage for AppError {
    fn with_message_of(self, msg: String) -> Self {
        match self {
            AppError::Unauthorized => {
                tracing::debug!(detail = %msg, "passkey auth rejected");
                AppError::Unauthorized
            }
            other => other,
        }
    }
}
