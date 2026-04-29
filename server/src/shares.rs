//! Read-only share links for notes. Per spec §3 the link is opaque to the
//! server; the share_key lives in the URL fragment and is never sent here.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
    Json, Router,
};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    error::{AppError, AppResult},
    session::AuthUser,
    AppState,
};

pub fn router() -> Router<Arc<AppState>> {
    // Mounted at /api/v1
    Router::new()
        .route("/items/:id/share", post(create))
        .route("/items/:id/shares", get(list))
        .route("/shares/:id", delete(revoke))
        .route("/share/:token", get(public_get))
        .route("/share/:token/blob", get(public_blob))
}

#[derive(Debug, Deserialize)]
pub struct CreateShareBody {
    /// nonce(24) || secretbox(plaintext, share_key) — server is opaque on this.
    #[serde(with = "crate::b64")]
    encrypted_payload: Vec<u8>,
    /// Days from now until the link stops working. Null = never expires.
    #[serde(default)]
    expires_in_days: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct ShareView {
    id: Uuid,
    token: String,
    item_id: Uuid,
    item_title: String,
    #[serde(with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,
    #[serde(default, with = "time::serde::rfc3339::option", skip_serializing_if = "Option::is_none")]
    expires_at: Option<OffsetDateTime>,
    use_count: i32,
}

#[derive(Debug, Serialize)]
pub struct PublicShareView {
    item_type: String,
    item_title: String,
    #[serde(with = "crate::b64")]
    encrypted_payload: Vec<u8>,
    #[serde(default, with = "time::serde::rfc3339::option", skip_serializing_if = "Option::is_none")]
    expires_at: Option<OffsetDateTime>,
}

fn random_token() -> String {
    let mut buf = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut buf);
    base64::Engine::encode(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD,
        buf,
    )
}

async fn create(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(item_id): Path<Uuid>,
    Json(body): Json<CreateShareBody>,
) -> AppResult<Json<ShareView>> {
    // Confirm item exists, the user is a member of its space, and it's a note.
    let row: Option<(Uuid, String, String)> = sqlx::query_as(
        r#"
        SELECT i.id, i.type, i.title
          FROM items i
          JOIN memberships m ON m.space_id = i.space_id
         WHERE i.id = $1 AND m.user_id = $2 AND i.deleted_at IS NULL
        "#,
    )
    .bind(item_id)
    .bind(user.user_id)
    .fetch_optional(&state.pool)
    .await?;
    let (id, type_, title) = row.ok_or(AppError::NotFound)?;
    // Notes ship the encrypted body inline. Files ship a metadata payload
    // (filename, mime, size, item_key) inline + the recipient pulls the
    // ciphertext blob from /share/<token>/blob. Other types not supported.
    if type_ != "note" && type_ != "file" {
        return Err(AppError::BadRequest(format!(
            "type {type_} cannot be shared via link"
        )));
    }

    let expires_at = body
        .expires_in_days
        .filter(|&d| d > 0)
        .map(|d| OffsetDateTime::now_utc() + time::Duration::days(d));

    let token = random_token();

    let inserted: (Uuid, String, OffsetDateTime, Option<OffsetDateTime>, i32) =
        sqlx::query_as(
            r#"
        INSERT INTO share_links (item_id, owner_user_id, token, encrypted_payload, expires_at)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id, token, created_at, expires_at, use_count
        "#,
        )
        .bind(id)
        .bind(user.user_id)
        .bind(&token)
        .bind(&body.encrypted_payload)
        .bind(expires_at)
        .fetch_one(&state.pool)
        .await?;

    crate::audit::record_item(
        &state.pool,
        user.user_id,
        Uuid::nil(), // no space tracked here
        id,
        "share.create",
        Some(serde_json::json!({"token": inserted.1})),
    )
    .await;

    Ok(Json(ShareView {
        id: inserted.0,
        token: inserted.1,
        item_id: id,
        item_title: title,
        created_at: inserted.2,
        expires_at: inserted.3,
        use_count: inserted.4,
    }))
}

async fn list(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(item_id): Path<Uuid>,
) -> AppResult<Json<Vec<ShareView>>> {
    let rows: Vec<(Uuid, String, Uuid, String, OffsetDateTime, Option<OffsetDateTime>, i32)> =
        sqlx::query_as(
            r#"
        SELECT s.id, s.token, s.item_id, i.title, s.created_at, s.expires_at, s.use_count
          FROM share_links s
          JOIN items i ON i.id = s.item_id
         WHERE s.item_id = $1 AND s.owner_user_id = $2
         ORDER BY s.created_at DESC
        "#,
        )
        .bind(item_id)
        .bind(user.user_id)
        .fetch_all(&state.pool)
        .await?;
    Ok(Json(
        rows.into_iter()
            .map(|r| ShareView {
                id: r.0,
                token: r.1,
                item_id: r.2,
                item_title: r.3,
                created_at: r.4,
                expires_at: r.5,
                use_count: r.6,
            })
            .collect(),
    ))
}

async fn revoke(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(share_id): Path<Uuid>,
) -> AppResult<StatusCode> {
    let res = sqlx::query(
        "DELETE FROM share_links WHERE id = $1 AND owner_user_id = $2",
    )
    .bind(share_id)
    .bind(user.user_id)
    .execute(&state.pool)
    .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn public_get(
    State(state): State<Arc<AppState>>,
    Path(token): Path<String>,
) -> AppResult<Json<PublicShareView>> {
    type Row = (String, String, Vec<u8>, Option<OffsetDateTime>);
    let row: Option<Row> = sqlx::query_as(
        r#"
        SELECT i.type, i.title, s.encrypted_payload, s.expires_at
          FROM share_links s
          JOIN items i ON i.id = s.item_id
         WHERE s.token = $1
           AND (s.expires_at IS NULL OR s.expires_at > NOW())
           AND i.deleted_at IS NULL
        "#,
    )
    .bind(&token)
    .fetch_optional(&state.pool)
    .await?;
    let (item_type, item_title, encrypted_payload, expires_at) =
        row.ok_or(AppError::NotFound)?;

    sqlx::query("UPDATE share_links SET use_count = use_count + 1 WHERE token = $1")
        .bind(&token)
        .execute(&state.pool)
        .await
        .ok();

    Ok(Json(PublicShareView {
        item_type,
        item_title,
        encrypted_payload,
        expires_at,
    }))
}

/// Public download of the encrypted blob bytes for a file share. The bytes
/// are still ciphertext (encrypted under the item_key the share's
/// encrypted_payload reveals), so handing them out unauthenticated leaks
/// nothing beyond "a file of this size exists".
async fn public_blob(
    State(state): State<Arc<AppState>>,
    Path(token): Path<String>,
) -> AppResult<Response> {
    let row: Option<(String, Option<Vec<u8>>)> = sqlx::query_as(
        r#"
        SELECT i.type, i.blob_sha256
          FROM share_links s
          JOIN items i ON i.id = s.item_id
         WHERE s.token = $1
           AND (s.expires_at IS NULL OR s.expires_at > NOW())
           AND i.deleted_at IS NULL
        "#,
    )
    .bind(&token)
    .fetch_optional(&state.pool)
    .await?;
    let (item_type, blob_sha256) = row.ok_or(AppError::NotFound)?;
    if item_type != "file" {
        return Err(AppError::NotFound);
    }
    let sha = blob_sha256.ok_or(AppError::NotFound)?;

    let path = crate::files::blob_path_for(&state.config.data_dir, &sha);
    let bytes = tokio::fs::read(&path).await.map_err(|e| {
        tracing::error!(error = %e, path = %path.display(), "shared blob missing on disk");
        AppError::NotFound
    })?;

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=60"),
    );
    Ok((StatusCode::OK, headers, bytes).into_response())
}
