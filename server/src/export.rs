//! `GET /api/v1/export` — assemble a self-contained tar archive of the
//! current user's data. Bytes inside are already opaque ciphertext (item
//! bodies, file blobs); the manifest + key material is in the clear so a
//! restore tool can rewire everything. For full at-rest privacy the user
//! pipes the result through `age -p` locally.

use std::sync::Arc;

use axum::{
    extract::State,
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use serde::Serialize;
use serde_json::Value as JsonValue;
use sqlx::types::Json as SqlxJson;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use uuid::Uuid;

use crate::{
    error::{AppError, AppResult},
    session::AuthUser,
    AppState,
};

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/", get(export))
}

#[derive(Debug, Serialize)]
struct Manifest {
    /// Bumped when the format changes incompatibly.
    format_version: u32,
    exported_at: String,
    user_id: Uuid,
    item_count: usize,
    blob_count: usize,
    note: &'static str,
}

#[derive(Debug, Serialize)]
struct UserExport {
    id: Uuid,
    email: String,
    /// Hex-encoded for portability.
    passphrase_salt_hex: String,
    recovery_salt_hex: String,
    master_wrap_passphrase_hex: String,
    master_wrap_recovery_hex: String,
    public_key_hex: String,
    encrypted_private_key_hex: String,
}

#[derive(Debug, Serialize)]
struct ItemExport {
    id: Uuid,
    space_id: Uuid,
    space_name: String,
    space_kind: String,
    #[serde(rename = "type")]
    type_: String,
    title: String,
    tags: Vec<String>,
    path: String,
    encrypted_body_b64: Option<String>,
    wrapped_item_key_b64: Option<String>,
    blob_sha256_hex: Option<String>,
    due_at: Option<String>,
    done: bool,
    created_at: String,
    updated_at: String,
    deleted_at: Option<String>,
}

async fn export(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
) -> AppResult<Response> {
    use base64::{engine::general_purpose::STANDARD, Engine};

    // --- User record ---
    type UserRow = (
        Uuid,
        String,
        Vec<u8>, // passphrase_salt
        Vec<u8>, // recovery_salt
        Vec<u8>, // master_wrap_passphrase
        Vec<u8>, // master_wrap_recovery
        Vec<u8>, // public_key
        Vec<u8>, // encrypted_private_key
    );
    let row: UserRow = sqlx::query_as(
        r#"
        SELECT id, email, passphrase_salt, recovery_salt,
               master_wrap_passphrase, master_wrap_recovery,
               public_key, encrypted_private_key
          FROM users WHERE id = $1
        "#,
    )
    .bind(user.user_id)
    .fetch_one(&state.pool)
    .await?;
    let user_export = UserExport {
        id: row.0,
        email: row.1,
        passphrase_salt_hex: hex::encode(&row.2),
        recovery_salt_hex: hex::encode(&row.3),
        master_wrap_passphrase_hex: hex::encode(&row.4),
        master_wrap_recovery_hex: hex::encode(&row.5),
        public_key_hex: hex::encode(&row.6),
        encrypted_private_key_hex: hex::encode(&row.7),
    };

    // --- Items (live + trashed) ---
    type ItemRow = (
        Uuid,
        Uuid,
        String,
        String,
        String,
        String,
        Vec<String>,
        String,
        Option<Vec<u8>>,
        Option<Vec<u8>>,
        Option<Vec<u8>>,
        Option<time::Date>,
        bool,
        OffsetDateTime,
        OffsetDateTime,
        Option<OffsetDateTime>,
    );
    // For team-space items the master-key wrap is NULL on the item row; the
    // user's sealed wrap lives in item_member_keys. COALESCE so the export
    // contains *whatever the calling user can decrypt*.
    let item_rows: Vec<ItemRow> = sqlx::query_as(
        r#"
        SELECT i.id, i.space_id, s.name, s.kind, i.type, i.title, i.tags, i.path,
               i.encrypted_body,
               COALESCE(i.wrapped_item_key, mk.sealed_item_key) AS wrapped_item_key,
               i.blob_sha256,
               i.due_at, i.done, i.created_at, i.updated_at, i.deleted_at
          FROM items i
          JOIN spaces s ON s.id = i.space_id
          JOIN memberships m ON m.space_id = s.id
          LEFT JOIN item_member_keys mk
                 ON mk.item_id = i.id AND mk.user_id = $1
         WHERE m.user_id = $1
         ORDER BY i.created_at
        "#,
    )
    .bind(user.user_id)
    .fetch_all(&state.pool)
    .await?;

    let items: Vec<ItemExport> = item_rows
        .into_iter()
        .map(|r| ItemExport {
            id: r.0,
            space_id: r.1,
            space_name: r.2,
            space_kind: r.3,
            type_: r.4,
            title: r.5,
            tags: r.6,
            path: r.7,
            encrypted_body_b64: r.8.as_ref().map(|v| STANDARD.encode(v)),
            wrapped_item_key_b64: r.9.as_ref().map(|v| STANDARD.encode(v)),
            blob_sha256_hex: r.10.as_ref().map(hex::encode),
            due_at: r.11.and_then(|d| {
                d.format(&time::macros::format_description!("[year]-[month]-[day]"))
                    .ok()
            }),
            done: r.12,
            created_at: r.13.format(&Rfc3339).unwrap_or_default(),
            updated_at: r.14.format(&Rfc3339).unwrap_or_default(),
            deleted_at: r.15.and_then(|t| t.format(&Rfc3339).ok()),
        })
        .collect();

    // --- Blob hashes referenced by user's items ---
    let blob_rows: Vec<(Vec<u8>,)> = sqlx::query_as(
        r#"
        SELECT DISTINCT i.blob_sha256
          FROM items i
          JOIN memberships m ON m.space_id = i.space_id
         WHERE m.user_id = $1 AND i.blob_sha256 IS NOT NULL
        "#,
    )
    .bind(user.user_id)
    .fetch_all(&state.pool)
    .await?;
    let blob_hashes: Vec<Vec<u8>> = blob_rows.into_iter().map(|(h,)| h).collect();

    let manifest = Manifest {
        format_version: 1,
        exported_at: OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .unwrap_or_default(),
        user_id: user.user_id,
        item_count: items.len(),
        blob_count: blob_hashes.len(),
        note:
            "Item bodies + blobs are encrypted with per-item keys wrapped via the user's master_key. \
             Restore on a fresh server, then sign in with the same passphrase to decrypt. \
             For full at-rest privacy of this archive, pipe through `age -p`.",
    };

    // --- Build tar in memory. Spec OK at MVP scale (<1 GB usually). ---
    let mut tar_buf: Vec<u8> = Vec::new();
    {
        let mut builder = tar::Builder::new(&mut tar_buf);

        let manifest_bytes = serde_json::to_vec_pretty(&manifest)
            .map_err(|e| AppError::Other(anyhow::anyhow!("manifest: {e}")))?;
        append_bytes(&mut builder, "manifest.json", &manifest_bytes)?;

        let user_bytes = serde_json::to_vec_pretty(&user_export)
            .map_err(|e| AppError::Other(anyhow::anyhow!("user export: {e}")))?;
        append_bytes(&mut builder, "users/me.json", &user_bytes)?;

        let mut items_jsonl = Vec::with_capacity(items.len() * 256);
        for item in &items {
            serde_json::to_writer(&mut items_jsonl, item)
                .map_err(|e| AppError::Other(anyhow::anyhow!("item write: {e}")))?;
            items_jsonl.push(b'\n');
        }
        append_bytes(&mut builder, "items.jsonl", &items_jsonl)?;

        for sha in &blob_hashes {
            let path = crate::files::blob_path_for(&state.config.data_dir, sha);
            let bytes = match tokio::fs::read(&path).await {
                Ok(b) => b,
                Err(e) => {
                    tracing::warn!(error = %e, sha = %hex::encode(sha), "blob missing during export");
                    continue;
                }
            };
            let arc_path = format!("blobs/{}", hex::encode(sha));
            append_bytes(&mut builder, &arc_path, &bytes)?;
        }

        builder
            .finish()
            .map_err(|e| AppError::Other(anyhow::anyhow!("tar finish: {e}")))?;
    }

    let _ = SqlxJson::<JsonValue>; // silence import if unused elsewhere

    crate::audit::record_user(
        &state.pool,
        user.user_id,
        "export",
        Some(serde_json::json!({
            "items": items.len(),
            "blobs": blob_hashes.len()
        })),
    )
    .await;

    let date = time::OffsetDateTime::now_utc()
        .format(&time::macros::format_description!(
            "[year][month][day]-[hour][minute]"
        ))
        .unwrap_or_else(|_| "backup".into());
    let filename = format!("rongnote-backup-{date}.tar");

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/x-tar"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{filename}\""))
            .unwrap_or(HeaderValue::from_static("attachment")),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));

    Ok((StatusCode::OK, headers, tar_buf).into_response())
}

fn append_bytes(
    builder: &mut tar::Builder<&mut Vec<u8>>,
    path: &str,
    bytes: &[u8],
) -> AppResult<()> {
    let mut header = tar::Header::new_gnu();
    header.set_size(bytes.len() as u64);
    header.set_mode(0o644);
    header.set_mtime(
        OffsetDateTime::now_utc()
            .unix_timestamp()
            .max(0) as u64,
    );
    header.set_cksum();
    builder
        .append_data(&mut header, path, bytes)
        .map_err(|e| AppError::Other(anyhow::anyhow!("tar append {path}: {e}")))?;
    Ok(())
}
