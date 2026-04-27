//! Encrypted file blob storage.
//!
//! Server stores ciphertext bytes content-addressed by sha256 of the
//! ciphertext. Refcount tracks how many items point at each blob. The actual
//! file metadata (filename, MIME type, plaintext size) is encrypted on the
//! item row, never seen by the server.

use std::path::PathBuf;
use std::sync::Arc;

use axum::{
    extract::{DefaultBodyLimit, Multipart, Path as AxumPath, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    error::{AppError, AppResult},
    session::AuthUser,
    AppState,
};

const MAX_BLOB_BYTES: usize = 50 * 1024 * 1024; // 50 MB per file (v0.7 cap).

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", post(upload).layer(DefaultBodyLimit::max(MAX_BLOB_BYTES + 1024 * 1024)))
        .route("/:hex_sha256", get(download))
}

#[derive(Debug, Serialize)]
pub struct UploadResponse {
    sha256: String,
    size: i64,
    already_existed: bool,
}

pub fn blob_path_for(data_dir: &str, sha256: &[u8]) -> PathBuf {
    let hex = hex::encode(sha256);
    let mut p = PathBuf::from(data_dir);
    p.push("blobs");
    p.push(&hex[..2]);
    p.push(&hex);
    p
}

async fn upload(
    State(state): State<Arc<AppState>>,
    _user: AuthUser,
    mut multipart: Multipart,
) -> AppResult<Json<UploadResponse>> {
    let mut blob_bytes: Option<Vec<u8>> = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("multipart: {e}")))?
    {
        let name = field.name().unwrap_or("").to_owned();
        if name == "blob" {
            let data = field
                .bytes()
                .await
                .map_err(|e| AppError::BadRequest(format!("read blob: {e}")))?;
            if data.len() > MAX_BLOB_BYTES {
                return Err(AppError::BadRequest(format!(
                    "blob exceeds {MAX_BLOB_BYTES} bytes"
                )));
            }
            blob_bytes = Some(data.to_vec());
        }
    }
    let blob = blob_bytes.ok_or_else(|| AppError::BadRequest("missing 'blob' field".into()))?;

    let mut hasher = Sha256::new();
    hasher.update(&blob);
    let sha256 = hasher.finalize().to_vec();

    // Insert the row first (or no-op if already there). Refcount stays at 0
    // until an item is created against it; an orphaned blob with refcount 0
    // is collected by a later sweep.
    let existed: Option<(i64,)> =
        sqlx::query_as("SELECT size FROM files_blobs WHERE sha256 = $1")
            .bind(&sha256)
            .fetch_optional(&state.pool)
            .await?;

    let already_existed = existed.is_some();
    if !already_existed {
        let size_i64 = blob.len() as i64;
        sqlx::query(
            "INSERT INTO files_blobs (sha256, size) VALUES ($1, $2)
             ON CONFLICT (sha256) DO NOTHING",
        )
        .bind(&sha256)
        .bind(size_i64)
        .execute(&state.pool)
        .await?;

        let path = blob_path_for(&state.config.data_dir, &sha256);
        if let Some(dir) = path.parent() {
            tokio::fs::create_dir_all(dir).await.map_err(|e| {
                AppError::Other(anyhow::anyhow!("mkdir {}: {e}", dir.display()))
            })?;
        }
        // Write to a temp file first then rename to avoid torn writes.
        let tmp = path.with_extension("tmp");
        tokio::fs::write(&tmp, &blob)
            .await
            .map_err(|e| AppError::Other(anyhow::anyhow!("write tmp: {e}")))?;
        tokio::fs::rename(&tmp, &path)
            .await
            .map_err(|e| AppError::Other(anyhow::anyhow!("rename: {e}")))?;
    }

    let size: i64 = sqlx::query_scalar("SELECT size FROM files_blobs WHERE sha256 = $1")
        .bind(&sha256)
        .fetch_one(&state.pool)
        .await?;

    Ok(Json(UploadResponse {
        sha256: hex::encode(&sha256),
        size,
        already_existed,
    }))
}

async fn download(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    AxumPath(hex_sha256): AxumPath<String>,
) -> AppResult<Response> {
    let sha256 = hex::decode(&hex_sha256)
        .map_err(|_| AppError::BadRequest("invalid sha256 hex".into()))?;
    if sha256.len() != 32 {
        return Err(AppError::BadRequest("sha256 must be 32 bytes".into()));
    }

    // Authz: the requesting user must own at least one item in a space they're
    // a member of that references this blob. (For v0.7 personal-only spaces,
    // this collapses to "user has an item with this blob".)
    let permitted: Option<(Uuid,)> = sqlx::query_as(
        r#"
        SELECT i.id
          FROM items i
          JOIN memberships m ON m.space_id = i.space_id
         WHERE i.blob_sha256 = $1
           AND m.user_id = $2
         LIMIT 1
        "#,
    )
    .bind(&sha256)
    .bind(user.user_id)
    .fetch_optional(&state.pool)
    .await?;
    if permitted.is_none() {
        return Err(AppError::NotFound);
    }

    let path = blob_path_for(&state.config.data_dir, &sha256);
    let bytes = tokio::fs::read(&path).await.map_err(|e| {
        tracing::error!(error = %e, path = %path.display(), "blob file missing on disk");
        AppError::NotFound
    })?;

    let mut headers = HeaderMap::new();
    // The bytes are ciphertext — opaque to the browser. Tell it that and let
    // the SPA decrypt + reinterpret.
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=300"),
    );
    Ok((StatusCode::OK, headers, bytes).into_response())
}
