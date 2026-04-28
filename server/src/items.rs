use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    error::{AppError, AppResult},
    session::AuthUser,
    AppState,
};

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/:id", get(get_one).patch(update).delete(delete_one))
        .route("/:id/restore", axum::routing::post(restore))
        .route("/:id/versions", get(list_versions))
        .route("/:id/versions/:version", get(get_version))
        .route(
            "/:id/versions/:version/restore",
            axum::routing::post(restore_version),
        )
}

// --- Allowed types ---

const ITEM_TYPES: &[&str] = &[
    "note", "secret", "file", "event", "task", "snippet", "bookmark",
];

fn validate_type(t: &str) -> AppResult<()> {
    if ITEM_TYPES.contains(&t) {
        Ok(())
    } else {
        Err(AppError::BadRequest(format!("invalid item type: {t}")))
    }
}

// --- Wire types ---

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ItemSummary {
    id: Uuid,
    #[sqlx(rename = "type")]
    #[serde(rename = "type")]
    type_: String,
    title: String,
    tags: Vec<String>,
    path: String,
    #[serde(with = "time::serde::rfc3339")]
    updated_at: OffsetDateTime,
    /// Only meaningful for type='task' (date-only).
    #[serde(default, with = "crate::b64::date_iso_option", skip_serializing_if = "Option::is_none")]
    due_at: Option<time::Date>,
    done: bool,
    pinned: bool,
}

/// Server is opaque on encrypted_body / wrapped_item_key. Title, tags, path,
/// type stay plaintext (search index, sidebar grouping). For type='file',
/// blob_sha256 points at the encrypted bytes on disk.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ItemView {
    id: Uuid,
    space_id: Uuid,
    #[sqlx(rename = "type")]
    #[serde(rename = "type")]
    type_: String,
    title: String,
    tags: Vec<String>,
    path: String,
    #[serde(with = "crate::b64::option")]
    encrypted_body: Option<Vec<u8>>,
    #[serde(with = "crate::b64::option")]
    wrapped_item_key: Option<Vec<u8>>,
    /// Hex-encoded over the wire. Only set for type='file'.
    #[serde(default, with = "crate::b64::hex_option", skip_serializing_if = "Option::is_none")]
    blob_sha256: Option<Vec<u8>>,
    #[serde(with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    updated_at: OffsetDateTime,
    #[serde(default, with = "time::serde::rfc3339::option", skip_serializing_if = "Option::is_none")]
    deleted_at: Option<OffsetDateTime>,
    #[serde(default, with = "crate::b64::date_iso_option", skip_serializing_if = "Option::is_none")]
    due_at: Option<time::Date>,
    #[serde(default)]
    done: bool,
    #[serde(default)]
    pinned: bool,
}

impl ItemView {
    pub fn id(&self) -> Uuid {
        self.id
    }
    pub fn space_id(&self) -> Uuid {
        self.space_id
    }
    pub fn type_str(&self) -> &str {
        &self.type_
    }
    pub fn title_str(&self) -> &str {
        &self.title
    }
    pub fn encrypted_body_bytes(&self) -> Option<&[u8]> {
        self.encrypted_body.as_deref()
    }
    pub fn wrapped_item_key_bytes(&self) -> Option<&[u8]> {
        self.wrapped_item_key.as_deref()
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateItemBody {
    #[serde(default = "default_type", rename = "type")]
    type_: String,
    title: String,
    #[serde(default, with = "crate::b64::option")]
    encrypted_body: Option<Vec<u8>>,
    #[serde(default, with = "crate::b64::option")]
    wrapped_item_key: Option<Vec<u8>>,
    #[serde(default, with = "crate::b64::hex_option")]
    blob_sha256: Option<Vec<u8>>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default = "default_path")]
    path: String,
    #[serde(default, with = "crate::b64::date_iso_option")]
    due_at: Option<time::Date>,
    #[serde(default)]
    done: bool,
}

#[derive(Debug, Deserialize)]
pub struct UpdateItemBody {
    title: Option<String>,
    #[serde(default, with = "crate::b64::option", skip_serializing_if = "Option::is_none")]
    encrypted_body: Option<Vec<u8>>,
    #[serde(default, with = "crate::b64::option", skip_serializing_if = "Option::is_none")]
    wrapped_item_key: Option<Vec<u8>>,
    #[serde(default)]
    update_body: bool,
    tags: Option<Vec<String>>,
    path: Option<String>,
    /// Set this true to apply due_at (including clearing to NULL).
    #[serde(default)]
    update_due_at: bool,
    #[serde(default, with = "crate::b64::date_iso_option")]
    due_at: Option<time::Date>,
    done: Option<bool>,
    pinned: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    #[serde(rename = "type")]
    type_: Option<String>,
    #[serde(default)]
    trash: bool,
}

#[derive(Debug, Deserialize)]
pub struct DeleteQuery {
    #[serde(default)]
    hard: bool,
}

fn default_type() -> String {
    "note".to_owned()
}

fn default_path() -> String {
    "/".to_owned()
}

// --- Authz helpers ---

async fn resolve_default_space(state: &Arc<AppState>, user: &AuthUser) -> AppResult<Uuid> {
    let row: (Uuid,) = sqlx::query_as(
        r#"
        SELECT s.id
          FROM spaces s
          JOIN memberships m ON m.space_id = s.id
         WHERE m.user_id = $1
           AND s.kind = 'personal'
         LIMIT 1
        "#,
    )
    .bind(user.user_id)
    .fetch_one(&state.pool)
    .await?;
    Ok(row.0)
}

async fn assert_member(
    state: &Arc<AppState>,
    user: &AuthUser,
    space_id: Uuid,
) -> AppResult<String> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT role FROM memberships WHERE user_id = $1 AND space_id = $2")
            .bind(user.user_id)
            .bind(space_id)
            .fetch_optional(&state.pool)
            .await?;
    row.map(|r| r.0).ok_or(AppError::Forbidden)
}

// --- Handlers ---

async fn list(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<Vec<ItemSummary>>> {
    let space_id = resolve_default_space(&state, &user).await?;
    if let Some(t) = &q.type_ {
        validate_type(t)?;
    }
    let rows = sqlx::query_as::<_, ItemSummary>(
        r#"
        SELECT id, type, title, tags, path, updated_at, due_at, done, pinned
          FROM items
         WHERE space_id = $1
           AND ($2::text IS NULL OR type = $2)
           AND CASE WHEN $3::bool THEN deleted_at IS NOT NULL ELSE deleted_at IS NULL END
         ORDER BY pinned DESC, COALESCE(deleted_at, updated_at) DESC
        "#,
    )
    .bind(space_id)
    .bind(q.type_.as_deref())
    .bind(q.trash)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(rows))
}

async fn create(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(body): Json<CreateItemBody>,
) -> AppResult<impl IntoResponse> {
    if body.title.trim().is_empty() {
        return Err(AppError::BadRequest("title required".into()));
    }
    validate_type(&body.type_)?;
    if body.encrypted_body.is_some() != body.wrapped_item_key.is_some() {
        return Err(AppError::BadRequest(
            "encrypted_body and wrapped_item_key must be set together".into(),
        ));
    }

    if body.blob_sha256.is_some() && body.type_ != "file" {
        return Err(AppError::BadRequest(
            "blob_sha256 only allowed on type='file'".into(),
        ));
    }
    if body.type_ == "file" && body.blob_sha256.is_none() {
        return Err(AppError::BadRequest(
            "type='file' requires blob_sha256".into(),
        ));
    }

    let space_id = resolve_default_space(&state, &user).await?;
    let role = assert_member(&state, &user, space_id).await?;
    if role == "viewer" {
        return Err(AppError::Forbidden);
    }

    let mut tx = state.pool.begin().await?;

    if let Some(sha) = &body.blob_sha256 {
        let updated = sqlx::query(
            "UPDATE files_blobs SET refcount = refcount + 1 WHERE sha256 = $1",
        )
        .bind(sha)
        .execute(&mut *tx)
        .await?;
        if updated.rows_affected() == 0 {
            return Err(AppError::BadRequest(
                "blob_sha256 not found — upload it first".into(),
            ));
        }
    }

    let item = sqlx::query_as::<_, ItemView>(
        r#"
        INSERT INTO items (
            space_id, type, title, encrypted_body, wrapped_item_key, blob_sha256,
            tags, path, created_by, due_at, done
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
        RETURNING id, space_id, type, title, tags, path,
                  encrypted_body, wrapped_item_key, blob_sha256,
                  created_at, updated_at, deleted_at, due_at, done, pinned
        "#,
    )
    .bind(space_id)
    .bind(&body.type_)
    .bind(body.title.trim())
    .bind(&body.encrypted_body)
    .bind(&body.wrapped_item_key)
    .bind(&body.blob_sha256)
    .bind(&body.tags)
    .bind(&body.path)
    .bind(user.user_id)
    .bind(body.due_at)
    .bind(body.done)
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;
    crate::audit::record_item(
        &state.pool,
        user.user_id,
        item.space_id(),
        item.id(),
        "item.create",
        Some(serde_json::json!({"type": item.type_str()})),
    )
    .await;
    Ok((StatusCode::CREATED, Json(item)))
}

async fn get_one(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<ItemView>> {
    let item = sqlx::query_as::<_, ItemView>(
        r#"
        SELECT i.id, i.space_id, i.type, i.title, i.tags, i.path,
               i.encrypted_body, i.wrapped_item_key, i.blob_sha256,
               i.created_at, i.updated_at, i.deleted_at, i.due_at, i.done, i.pinned
          FROM items i
          JOIN memberships m ON m.space_id = i.space_id
         WHERE i.id = $1 AND m.user_id = $2
        "#,
    )
    .bind(id)
    .bind(user.user_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::NotFound)?;
    if item.type_str() == "secret" {
        crate::audit::record_item(
            &state.pool,
            user.user_id,
            item.space_id(),
            item.id(),
            "secret.read",
            None,
        )
        .await;
    }
    Ok(Json(item))
}

async fn update(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateItemBody>,
) -> AppResult<Json<ItemView>> {
    let existing = sqlx::query_as::<_, ItemView>(
        r#"
        SELECT i.id, i.space_id, i.type, i.title, i.tags, i.path,
               i.encrypted_body, i.wrapped_item_key, i.blob_sha256,
               i.created_at, i.updated_at, i.deleted_at, i.due_at, i.done, i.pinned
          FROM items i
          JOIN memberships m ON m.space_id = i.space_id
         WHERE i.id = $1 AND m.user_id = $2
        "#,
    )
    .bind(id)
    .bind(user.user_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::NotFound)?;

    let role = assert_member(&state, &user, existing.space_id).await?;
    if role == "viewer" {
        return Err(AppError::Forbidden);
    }

    if existing.deleted_at.is_some() {
        return Err(AppError::BadRequest(
            "item is in trash; restore before editing".into(),
        ));
    }

    if body.update_body
        && body.encrypted_body.is_some() != body.wrapped_item_key.is_some()
    {
        return Err(AppError::BadRequest(
            "encrypted_body and wrapped_item_key must be set together".into(),
        ));
    }

    // Snapshot the current body before overwriting. Only when the body
    // actually changes (update_body=true) — otherwise tag/path/done/pinned
    // toggles would each create a noise version.
    if body.update_body {
        snapshot_version(&state.pool, &existing, user.user_id).await.ok();
    }

    let updated = sqlx::query_as::<_, ItemView>(
        r#"
        UPDATE items
           SET title             = COALESCE($2, title),
               encrypted_body    = CASE WHEN $3::bool THEN $4 ELSE encrypted_body END,
               wrapped_item_key  = CASE WHEN $3::bool THEN $5 ELSE wrapped_item_key END,
               tags              = COALESCE($6, tags),
               path              = COALESCE($7, path),
               due_at            = CASE WHEN $8::bool THEN $9 ELSE due_at END,
               done              = COALESCE($10, done),
               pinned            = COALESCE($11, pinned),
               updated_at        = NOW()
         WHERE id = $1
        RETURNING id, space_id, type, title, tags, path,
                  encrypted_body, wrapped_item_key, blob_sha256,
                  created_at, updated_at, deleted_at, due_at, done, pinned
        "#,
    )
    .bind(id)
    .bind(body.title.as_deref().map(str::trim))
    .bind(body.update_body)
    .bind(body.encrypted_body.as_deref())
    .bind(body.wrapped_item_key.as_deref())
    .bind(body.tags.as_deref())
    .bind(body.path.as_deref())
    .bind(body.update_due_at)
    .bind(body.due_at)
    .bind(body.done)
    .bind(body.pinned)
    .fetch_one(&state.pool)
    .await?;
    crate::audit::record_item(
        &state.pool,
        user.user_id,
        updated.space_id(),
        updated.id(),
        "item.update",
        Some(serde_json::json!({"type": updated.type_str()})),
    )
    .await;
    Ok(Json(updated))
}

async fn snapshot_version(
    pool: &sqlx::PgPool,
    existing: &ItemView,
    user_id: Uuid,
) -> sqlx::Result<()> {
    let next: (i32,) = sqlx::query_as(
        "SELECT COALESCE(MAX(version), 0) + 1 FROM item_versions WHERE item_id = $1",
    )
    .bind(existing.id())
    .fetch_one(pool)
    .await?;
    sqlx::query(
        r#"
        INSERT INTO item_versions
            (item_id, version, title, encrypted_body, wrapped_item_key, created_by)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(existing.id())
    .bind(next.0)
    .bind(existing.title_str())
    .bind(existing.encrypted_body_bytes())
    .bind(existing.wrapped_item_key_bytes())
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(())
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct VersionSummary {
    id: Uuid,
    version: i32,
    title: String,
    #[serde(with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,
    created_by: Option<Uuid>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct VersionDetail {
    id: Uuid,
    version: i32,
    title: String,
    #[serde(with = "crate::b64::option")]
    encrypted_body: Option<Vec<u8>>,
    #[serde(with = "crate::b64::option")]
    wrapped_item_key: Option<Vec<u8>>,
    #[serde(with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,
}

async fn list_versions(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Vec<VersionSummary>>> {
    let _: (Uuid,) = sqlx::query_as(
        r#"
        SELECT i.id FROM items i
          JOIN memberships m ON m.space_id = i.space_id
         WHERE i.id = $1 AND m.user_id = $2
        "#,
    )
    .bind(id)
    .bind(user.user_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|_| AppError::NotFound)?;

    let rows = sqlx::query_as::<_, VersionSummary>(
        r#"
        SELECT id, version, title, created_at, created_by
          FROM item_versions
         WHERE item_id = $1
         ORDER BY version DESC
        "#,
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(rows))
}

async fn get_version(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path((id, version)): Path<(Uuid, i32)>,
) -> AppResult<Json<VersionDetail>> {
    let _: (Uuid,) = sqlx::query_as(
        r#"
        SELECT i.id FROM items i
          JOIN memberships m ON m.space_id = i.space_id
         WHERE i.id = $1 AND m.user_id = $2
        "#,
    )
    .bind(id)
    .bind(user.user_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|_| AppError::NotFound)?;

    let row: Option<VersionDetail> = sqlx::query_as(
        r#"
        SELECT id, version, title, encrypted_body, wrapped_item_key, created_at
          FROM item_versions
         WHERE item_id = $1 AND version = $2
        "#,
    )
    .bind(id)
    .bind(version)
    .fetch_optional(&state.pool)
    .await?;
    Ok(Json(row.ok_or(AppError::NotFound)?))
}

async fn restore_version(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path((id, version)): Path<(Uuid, i32)>,
) -> AppResult<Json<ItemView>> {
    // Authz: load current item for the user; member of space.
    let existing = sqlx::query_as::<_, ItemView>(
        r#"
        SELECT i.id, i.space_id, i.type, i.title, i.tags, i.path,
               i.encrypted_body, i.wrapped_item_key, i.blob_sha256,
               i.created_at, i.updated_at, i.deleted_at, i.due_at, i.done, i.pinned
          FROM items i
          JOIN memberships m ON m.space_id = i.space_id
         WHERE i.id = $1 AND m.user_id = $2 AND i.deleted_at IS NULL
        "#,
    )
    .bind(id)
    .bind(user.user_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::NotFound)?;

    // Snapshot current state before restoring (so the restore is itself
    // versioned + reversible).
    snapshot_version(&state.pool, &existing, user.user_id).await.ok();

    let snap: Option<(String, Option<Vec<u8>>, Option<Vec<u8>>)> = sqlx::query_as(
        r#"
        SELECT title, encrypted_body, wrapped_item_key
          FROM item_versions
         WHERE item_id = $1 AND version = $2
        "#,
    )
    .bind(id)
    .bind(version)
    .fetch_optional(&state.pool)
    .await?;
    let (snap_title, snap_body, snap_key) = snap.ok_or(AppError::NotFound)?;

    let updated = sqlx::query_as::<_, ItemView>(
        r#"
        UPDATE items
           SET title             = $2,
               encrypted_body    = $3,
               wrapped_item_key  = $4,
               updated_at        = NOW()
         WHERE id = $1
        RETURNING id, space_id, type, title, tags, path,
                  encrypted_body, wrapped_item_key, blob_sha256,
                  created_at, updated_at, deleted_at, due_at, done, pinned
        "#,
    )
    .bind(id)
    .bind(&snap_title)
    .bind(&snap_body)
    .bind(&snap_key)
    .fetch_one(&state.pool)
    .await?;

    crate::audit::record_item(
        &state.pool,
        user.user_id,
        updated.space_id(),
        updated.id(),
        "item.restore_version",
        Some(serde_json::json!({"version": version})),
    )
    .await;

    Ok(Json(updated))
}

async fn delete_one(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Query(q): Query<DeleteQuery>,
) -> AppResult<StatusCode> {
    let row: Option<(Uuid, String, Option<Vec<u8>>)> = sqlx::query_as(
        r#"
        SELECT i.space_id, m.role, i.blob_sha256
          FROM items i
          JOIN memberships m ON m.space_id = i.space_id
         WHERE i.id = $1 AND m.user_id = $2
        "#,
    )
    .bind(id)
    .bind(user.user_id)
    .fetch_optional(&state.pool)
    .await?;
    let (space_id, role, blob_sha256) = row.ok_or(AppError::NotFound)?;
    if role == "viewer" {
        return Err(AppError::Forbidden);
    }
    if q.hard {
        let mut tx = state.pool.begin().await?;
        sqlx::query("DELETE FROM items WHERE id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        let mut delete_blob_path: Option<std::path::PathBuf> = None;
        if let Some(sha) = blob_sha256 {
            let new_refcount: Option<(i32,)> = sqlx::query_as(
                r#"
                UPDATE files_blobs
                   SET refcount = GREATEST(refcount - 1, 0)
                 WHERE sha256 = $1
                RETURNING refcount
                "#,
            )
            .bind(&sha)
            .fetch_optional(&mut *tx)
            .await?;
            if let Some((rc,)) = new_refcount {
                if rc == 0 {
                    sqlx::query("DELETE FROM files_blobs WHERE sha256 = $1 AND refcount = 0")
                        .bind(&sha)
                        .execute(&mut *tx)
                        .await?;
                    delete_blob_path = Some(crate::files::blob_path_for(
                        &state.config.data_dir,
                        &sha,
                    ));
                }
            }
        }
        tx.commit().await?;
        if let Some(path) = delete_blob_path {
            tokio::fs::remove_file(&path).await.ok();
        }
        crate::audit::record_item(
            &state.pool,
            user.user_id,
            space_id,
            id,
            "item.hard_delete",
            None,
        )
        .await;
    } else {
        sqlx::query("UPDATE items SET deleted_at = NOW() WHERE id = $1 AND deleted_at IS NULL")
            .bind(id)
            .execute(&state.pool)
            .await?;
        crate::audit::record_item(
            &state.pool,
            user.user_id,
            space_id,
            id,
            "item.soft_delete",
            None,
        )
        .await;
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn restore(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<ItemView>> {
    let role: Option<(String,)> = sqlx::query_as(
        r#"
        SELECT m.role
          FROM items i
          JOIN memberships m ON m.space_id = i.space_id
         WHERE i.id = $1 AND m.user_id = $2
        "#,
    )
    .bind(id)
    .bind(user.user_id)
    .fetch_optional(&state.pool)
    .await?;
    let role = role.ok_or(AppError::NotFound)?.0;
    if role == "viewer" {
        return Err(AppError::Forbidden);
    }

    let restored = sqlx::query_as::<_, ItemView>(
        r#"
        UPDATE items
           SET deleted_at = NULL,
               updated_at = NOW()
         WHERE id = $1
        RETURNING id, space_id, type, title, tags, path,
                  encrypted_body, wrapped_item_key, blob_sha256,
                  created_at, updated_at, deleted_at, due_at, done, pinned
        "#,
    )
    .bind(id)
    .fetch_one(&state.pool)
    .await?;
    crate::audit::record_item(
        &state.pool,
        user.user_id,
        restored.space_id(),
        restored.id(),
        "item.restore",
        None,
    )
    .await;
    Ok(Json(restored))
}
