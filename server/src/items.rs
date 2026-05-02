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
        .route("/:id/move", axum::routing::post(move_item))
        .route("/:id/versions", get(list_versions))
        .route("/:id/versions/:version", get(get_version))
        .route(
            "/:id/versions/:version/restore",
            axum::routing::post(restore_version),
        )
}

// --- Allowed types ---

const ITEM_TYPES: &[&str] = &[
    "note", "secret", "file", "event", "task", "snippet", "bookmark", "list",
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
    /// Only meaningful for type='event'. Stored UTC, rendered local on the
    /// client. Plaintext on the server (calendar range queries need it).
    #[serde(default, with = "time::serde::rfc3339::option", skip_serializing_if = "Option::is_none")]
    start_at: Option<OffsetDateTime>,
    #[serde(default, with = "time::serde::rfc3339::option", skip_serializing_if = "Option::is_none")]
    end_at: Option<OffsetDateTime>,
    #[serde(default)]
    all_day: bool,
    done: bool,
    pinned: bool,
}

/// Server is opaque on encrypted_body / wrapped_item_key. Title, tags, path,
/// type stay plaintext (search index, sidebar grouping). For type='file',
/// blob_sha256 points at the encrypted bytes on disk.
///
/// `wrapped_item_key` is overloaded: for personal-space items it's the
/// item_key wrapped under the user's master_key (secretbox). For team-space
/// items, the get/create/update handlers populate it with the caller's
/// sealed-box wrap from `item_member_keys`. `key_wrap` tells the client
/// which decryption to use.
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
    /// 'master' (personal space, secretbox), 'sealed' (team space, sealed_box),
    /// or null when the item has no body yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    key_wrap: Option<String>,
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
    #[serde(default, with = "time::serde::rfc3339::option", skip_serializing_if = "Option::is_none")]
    start_at: Option<OffsetDateTime>,
    #[serde(default, with = "time::serde::rfc3339::option", skip_serializing_if = "Option::is_none")]
    end_at: Option<OffsetDateTime>,
    #[serde(default)]
    all_day: bool,
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

/// Sealed-box wrap of an item key for one specific member of a team space.
/// `sealed_item_key` = libsodium crypto_box_seal(item_key, member_public_key).
#[derive(Debug, Deserialize)]
pub struct MemberKeyInput {
    user_id: Uuid,
    #[serde(with = "crate::b64")]
    sealed_item_key: Vec<u8>,
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
    /// Required for team-space items when encrypted_body is set. Must contain
    /// one entry per current member of the space (caller included).
    #[serde(default)]
    member_keys: Option<Vec<MemberKeyInput>>,
    #[serde(default, with = "crate::b64::hex_option")]
    blob_sha256: Option<Vec<u8>>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default = "default_path")]
    path: String,
    #[serde(default, with = "crate::b64::date_iso_option")]
    due_at: Option<time::Date>,
    /// Event timing fields (only meaningful for type='event').
    #[serde(default, with = "time::serde::rfc3339::option")]
    start_at: Option<OffsetDateTime>,
    #[serde(default, with = "time::serde::rfc3339::option")]
    end_at: Option<OffsetDateTime>,
    #[serde(default)]
    all_day: bool,
    #[serde(default)]
    done: bool,
    /// Optional space target. Falls back to the user's personal space.
    space_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateItemBody {
    title: Option<String>,
    #[serde(default, with = "crate::b64::option", skip_serializing_if = "Option::is_none")]
    encrypted_body: Option<Vec<u8>>,
    #[serde(default, with = "crate::b64::option", skip_serializing_if = "Option::is_none")]
    wrapped_item_key: Option<Vec<u8>>,
    /// For team-space body updates: a fresh per-member wrap of the (rotated)
    /// item_key. Required when update_body=true on a team-space item.
    #[serde(default)]
    member_keys: Option<Vec<MemberKeyInput>>,
    #[serde(default)]
    update_body: bool,
    tags: Option<Vec<String>>,
    path: Option<String>,
    /// Set this true to apply due_at (including clearing to NULL).
    #[serde(default)]
    update_due_at: bool,
    #[serde(default, with = "crate::b64::date_iso_option")]
    due_at: Option<time::Date>,
    /// Set this true to apply start_at + end_at + all_day (incl clearing).
    #[serde(default)]
    update_event_time: bool,
    #[serde(default, with = "time::serde::rfc3339::option")]
    start_at: Option<OffsetDateTime>,
    #[serde(default, with = "time::serde::rfc3339::option")]
    end_at: Option<OffsetDateTime>,
    #[serde(default)]
    all_day: bool,
    done: Option<bool>,
    pinned: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct MoveItemBody {
    target_space_id: Uuid,
    /// Required when the target is a personal space — secretbox wrap of the
    /// item_key under the *new owner's* master_key.
    #[serde(default, with = "crate::b64::option")]
    wrapped_item_key: Option<Vec<u8>>,
    /// Required when the target is a team space — sealed-box wraps for every
    /// current member. Same shape as create/update.
    #[serde(default)]
    member_keys: Option<Vec<MemberKeyInput>>,
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    #[serde(rename = "type")]
    type_: Option<String>,
    #[serde(default)]
    trash: bool,
    /// Optional space filter. If absent, defaults to the user's personal space.
    space_id: Option<Uuid>,
    /// Calendar range filter (UTC, RFC3339). When both are set, results are
    /// limited to items whose start_at falls in [start_after, start_before).
    /// Typically used together with type='event'.
    #[serde(default, with = "time::serde::rfc3339::option")]
    start_after: Option<OffsetDateTime>,
    #[serde(default, with = "time::serde::rfc3339::option")]
    start_before: Option<OffsetDateTime>,
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

/// Resolve the target space for a request: explicit space_id if the user
/// is a member of it, else the personal space.
async fn resolve_space(
    state: &Arc<AppState>,
    user: &AuthUser,
    explicit: Option<Uuid>,
) -> AppResult<Uuid> {
    if let Some(id) = explicit {
        let row: Option<(Uuid,)> = sqlx::query_as(
            "SELECT space_id FROM memberships WHERE user_id = $1 AND space_id = $2",
        )
        .bind(user.user_id)
        .bind(id)
        .fetch_optional(&state.pool)
        .await?;
        return row.map(|r| r.0).ok_or(AppError::Forbidden);
    }
    resolve_default_space(state, user).await
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

/// Authorisation gate for any mutation against an existing item.
///   owner / editor → ok
///   viewer         → forbidden
///   kiosk          → ok if item.created_by == user
///                    OR item.type ∈ {'list', 'task'}
///
/// Lists + tasks carve-out lets the always-on kiosk display tick off the
/// shared shopping list and household chores even though the items
/// belong to another member. Everything else (events, notes, secrets)
/// stays locked to the original creator from the kiosk's side.
async fn assert_can_modify(
    state: &Arc<AppState>,
    user: &AuthUser,
    item_id: Uuid,
    space_id: Uuid,
    item_type: &str,
) -> AppResult<()> {
    let role = assert_member(state, user, space_id).await?;
    if role == "viewer" {
        return Err(AppError::Forbidden);
    }
    if role == "kiosk" {
        if matches!(item_type, "list" | "task") {
            return Ok(());
        }
        let row: Option<(Uuid,)> =
            sqlx::query_as("SELECT created_by FROM items WHERE id = $1")
                .bind(item_id)
                .fetch_optional(&state.pool)
                .await?;
        let created_by = row.ok_or(AppError::NotFound)?.0;
        if created_by != user.user_id {
            return Err(AppError::Forbidden);
        }
    }
    Ok(())
}

async fn space_kind(state: &Arc<AppState>, space_id: Uuid) -> AppResult<String> {
    let row: (String,) = sqlx::query_as("SELECT kind FROM spaces WHERE id = $1")
        .bind(space_id)
        .fetch_one(&state.pool)
        .await?;
    Ok(row.0)
}

/// All read paths route through this so the (item, current user) pair gets
/// the right wrap: master-key wrap for personal-space items, sealed-box
/// wrap from item_member_keys for team-space items.
const ITEM_VIEW_SELECT: &str = r#"
    SELECT i.id, i.space_id, i.type, i.title, i.tags, i.path,
           i.encrypted_body,
           COALESCE(i.wrapped_item_key, mk.sealed_item_key) AS wrapped_item_key,
           CASE
             WHEN i.wrapped_item_key IS NOT NULL THEN 'master'
             WHEN mk.sealed_item_key IS NOT NULL THEN 'sealed'
             ELSE NULL
           END AS key_wrap,
           i.blob_sha256,
           i.created_at, i.updated_at, i.deleted_at,
           i.due_at, i.start_at, i.end_at, i.all_day, i.done, i.pinned
      FROM items i
      JOIN memberships m ON m.space_id = i.space_id
      LEFT JOIN item_member_keys mk
             ON mk.item_id = i.id AND mk.user_id = $2
"#;

async fn load_item_for_user(
    state: &Arc<AppState>,
    user_id: Uuid,
    item_id: Uuid,
) -> AppResult<Option<ItemView>> {
    let q = format!("{ITEM_VIEW_SELECT} WHERE i.id = $1 AND m.user_id = $2");
    let row = sqlx::query_as::<_, ItemView>(&q)
        .bind(item_id)
        .bind(user_id)
        .fetch_optional(&state.pool)
        .await?;
    Ok(row)
}

/// Verify that the supplied member_keys cover every current member of the
/// space exactly once. Returns Ok with Vec ready for INSERT, or BadRequest.
async fn validate_member_keys(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    space_id: Uuid,
    keys: &[MemberKeyInput],
) -> AppResult<()> {
    let members: Vec<(Uuid,)> = sqlx::query_as(
        "SELECT user_id FROM memberships WHERE space_id = $1",
    )
    .bind(space_id)
    .fetch_all(&mut **tx)
    .await?;

    let mut expected: std::collections::HashSet<Uuid> =
        members.into_iter().map(|r| r.0).collect();
    for k in keys {
        if !expected.remove(&k.user_id) {
            return Err(AppError::BadRequest(format!(
                "member_keys: user {} is not a current member of the space",
                k.user_id
            )));
        }
    }
    if !expected.is_empty() {
        return Err(AppError::BadRequest(
            "member_keys must cover every current member of the space".into(),
        ));
    }
    Ok(())
}

// --- Handlers ---

async fn list(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<Vec<ItemSummary>>> {
    let space_id = resolve_space(&state, &user, q.space_id).await?;
    if let Some(t) = &q.type_ {
        validate_type(t)?;
    }
    let rows = sqlx::query_as::<_, ItemSummary>(
        r#"
        SELECT id, type, title, tags, path, updated_at,
               due_at, start_at, end_at, all_day, done, pinned
          FROM items
         WHERE space_id = $1
           AND ($2::text IS NULL OR type = $2)
           AND CASE WHEN $3::bool THEN deleted_at IS NOT NULL ELSE deleted_at IS NULL END
           AND ($4::timestamptz IS NULL OR (start_at IS NOT NULL AND start_at >= $4))
           AND ($5::timestamptz IS NULL OR (start_at IS NOT NULL AND start_at <  $5))
         ORDER BY pinned DESC,
                  COALESCE(start_at, deleted_at, updated_at) DESC
        "#,
    )
    .bind(space_id)
    .bind(q.type_.as_deref())
    .bind(q.trash)
    .bind(q.start_after)
    .bind(q.start_before)
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

    let space_id = resolve_space(&state, &user, body.space_id).await?;
    let role = assert_member(&state, &user, space_id).await?;
    if role == "viewer" {
        return Err(AppError::Forbidden);
    }
    let kind = space_kind(&state, space_id).await?;
    let is_team = kind == "team";

    // Wrap-shape rules differ by space kind:
    //   personal: encrypted_body + wrapped_item_key (master-key secretbox)
    //   team:     encrypted_body + member_keys (sealed_box per member)
    let has_body = body.encrypted_body.is_some();
    let stored_wrap: Option<&[u8]> = if is_team {
        if body.wrapped_item_key.is_some() {
            return Err(AppError::BadRequest(
                "team-space items use member_keys, not wrapped_item_key".into(),
            ));
        }
        if has_body && body.member_keys.as_ref().map_or(true, |v| v.is_empty()) {
            return Err(AppError::BadRequest(
                "team-space items with encrypted_body require member_keys".into(),
            ));
        }
        if !has_body && body.member_keys.is_some() {
            return Err(AppError::BadRequest(
                "member_keys without encrypted_body".into(),
            ));
        }
        None
    } else {
        if body.member_keys.is_some() {
            return Err(AppError::BadRequest(
                "member_keys not allowed on personal-space items".into(),
            ));
        }
        if has_body != body.wrapped_item_key.is_some() {
            return Err(AppError::BadRequest(
                "encrypted_body and wrapped_item_key must be set together".into(),
            ));
        }
        body.wrapped_item_key.as_deref()
    };

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

    let inserted_id: (Uuid,) = sqlx::query_as(
        r#"
        INSERT INTO items (
            space_id, type, title, encrypted_body, wrapped_item_key, blob_sha256,
            tags, path, created_by, due_at, done,
            start_at, end_at, all_day
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
        RETURNING id
        "#,
    )
    .bind(space_id)
    .bind(&body.type_)
    .bind(body.title.trim())
    .bind(body.encrypted_body.as_deref())
    .bind(stored_wrap)
    .bind(&body.blob_sha256)
    .bind(&body.tags)
    .bind(&body.path)
    .bind(user.user_id)
    .bind(body.due_at)
    .bind(body.done)
    .bind(body.start_at)
    .bind(body.end_at)
    .bind(body.all_day)
    .fetch_one(&mut *tx)
    .await?;

    if is_team && has_body {
        let keys = body.member_keys.as_deref().unwrap_or(&[]);
        validate_member_keys(&mut tx, space_id, keys).await?;
        for k in keys {
            sqlx::query(
                r#"
                INSERT INTO item_member_keys (item_id, user_id, sealed_item_key)
                VALUES ($1, $2, $3)
                "#,
            )
            .bind(inserted_id.0)
            .bind(k.user_id)
            .bind(&k.sealed_item_key)
            .execute(&mut *tx)
            .await?;
        }
    }

    tx.commit().await?;

    let item = load_item_for_user(&state, user.user_id, inserted_id.0)
        .await?
        .ok_or_else(|| AppError::Other(anyhow::anyhow!("item vanished after insert")))?;
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
    let item = load_item_for_user(&state, user.user_id, id)
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
    let existing = load_item_for_user(&state, user.user_id, id)
        .await?
        .ok_or(AppError::NotFound)?;

    assert_can_modify(
        &state,
        &user,
        existing.id(),
        existing.space_id(),
        existing.type_str(),
    )
    .await?;

    if existing.deleted_at.is_some() {
        return Err(AppError::BadRequest(
            "item is in trash; restore before editing".into(),
        ));
    }

    let kind = space_kind(&state, existing.space_id).await?;
    let is_team = kind == "team";

    // Body-update validation, branched on space kind.
    //   personal: item_key is rotated each save, fresh wrapped_item_key
    //             stored in items.wrapped_item_key.
    //   team:     item_key is *reused* across saves (the client decrypts the
    //             current sealed wrap, re-encrypts the body with the same
    //             key). This keeps version-history snapshots decryptable
    //             without snapshotting per-member key wraps. New member
    //             wraps come in via the Phase C invite endpoint, not here.
    if body.update_body {
        if body.encrypted_body.is_none() {
            return Err(AppError::BadRequest(
                "update_body=true requires encrypted_body".into(),
            ));
        }
        if is_team {
            if body.wrapped_item_key.is_some() {
                return Err(AppError::BadRequest(
                    "team-space items use the existing item_member_keys; do not send wrapped_item_key on update".into(),
                ));
            }
            if body.member_keys.is_some() {
                return Err(AppError::BadRequest(
                    "team-space body update reuses existing member keys; do not send member_keys".into(),
                ));
            }
        } else {
            if body.member_keys.is_some() {
                return Err(AppError::BadRequest(
                    "member_keys not allowed on personal-space items".into(),
                ));
            }
            if body.wrapped_item_key.is_none() {
                return Err(AppError::BadRequest(
                    "personal-space body update requires wrapped_item_key".into(),
                ));
            }
        }
    }

    // Snapshot the current body before overwriting. Only when the body
    // actually changes (update_body=true) — otherwise tag/path/done/pinned
    // toggles would each create a noise version.
    if body.update_body {
        snapshot_version(&state.pool, &existing, user.user_id).await.ok();
    }

    let mut tx = state.pool.begin().await?;

    let stored_wrap: Option<&[u8]> = if body.update_body && !is_team {
        body.wrapped_item_key.as_deref()
    } else if body.update_body && is_team {
        None // team items keep wrapped_item_key NULL
    } else {
        // No body update: leave wrapped_item_key untouched (CASE WHEN false branch).
        None
    };

    sqlx::query(
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
               start_at          = CASE WHEN $12::bool THEN $13 ELSE start_at END,
               end_at            = CASE WHEN $12::bool THEN $14 ELSE end_at END,
               all_day           = CASE WHEN $12::bool THEN $15 ELSE all_day END,
               updated_at        = NOW()
         WHERE id = $1
        "#,
    )
    .bind(id)
    .bind(body.title.as_deref().map(str::trim))
    .bind(body.update_body)
    .bind(body.encrypted_body.as_deref())
    .bind(stored_wrap)
    .bind(body.tags.as_deref())
    .bind(body.path.as_deref())
    .bind(body.update_due_at)
    .bind(body.due_at)
    .bind(body.done)
    .bind(body.pinned)
    .bind(body.update_event_time)
    .bind(body.start_at)
    .bind(body.end_at)
    .bind(body.all_day)
    .execute(&mut *tx)
    .await?;

    // Team-space body updates: nothing to do here — the existing rows in
    // item_member_keys still wrap the unchanged item_key.

    tx.commit().await?;

    let updated = load_item_for_user(&state, user.user_id, id)
        .await?
        .ok_or(AppError::NotFound)?;
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

/// Move an item to a different space. The caller has to re-wrap the
/// item_key for the target space's kind: secretbox under master_key for
/// personal targets, sealed_box per current member for team targets.
async fn move_item(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<MoveItemBody>,
) -> AppResult<Json<ItemView>> {
    let existing = load_item_for_user(&state, user.user_id, id)
        .await?
        .ok_or(AppError::NotFound)?;
    if existing.deleted_at.is_some() {
        return Err(AppError::BadRequest(
            "item is in trash; restore before moving".into(),
        ));
    }

    let from_role = assert_member(&state, &user, existing.space_id).await?;
    if matches!(from_role.as_str(), "viewer" | "kiosk") {
        return Err(AppError::Forbidden);
    }
    if existing.space_id == body.target_space_id {
        return Err(AppError::BadRequest(
            "item already in that space".into(),
        ));
    }

    let to_role = assert_member(&state, &user, body.target_space_id).await?;
    if matches!(to_role.as_str(), "viewer" | "kiosk") {
        return Err(AppError::Forbidden);
    }

    let target_kind = space_kind(&state, body.target_space_id).await?;
    let target_is_team = target_kind == "team";

    // Wrap-shape rules mirror create/update for the target kind.
    let stored_wrap: Option<&[u8]> = if target_is_team {
        if body.wrapped_item_key.is_some() {
            return Err(AppError::BadRequest(
                "team target uses member_keys, not wrapped_item_key".into(),
            ));
        }
        if existing.encrypted_body.is_some()
            && body.member_keys.as_ref().map_or(true, |v| v.is_empty())
        {
            return Err(AppError::BadRequest(
                "moving an item with body to a team space requires member_keys".into(),
            ));
        }
        None
    } else {
        if body.member_keys.is_some() {
            return Err(AppError::BadRequest(
                "personal target uses wrapped_item_key, not member_keys".into(),
            ));
        }
        if existing.encrypted_body.is_some() && body.wrapped_item_key.is_none() {
            return Err(AppError::BadRequest(
                "moving an item with body to a personal space requires wrapped_item_key".into(),
            ));
        }
        body.wrapped_item_key.as_deref()
    };

    let mut tx = state.pool.begin().await?;

    // Update space + wrap. For team targets we clear wrapped_item_key (the
    // wraps go to item_member_keys). For personal targets we clear all
    // item_member_keys rows and write the master-key wrap.
    sqlx::query(
        r#"
        UPDATE items
           SET space_id          = $2,
               wrapped_item_key  = $3,
               updated_at        = NOW()
         WHERE id = $1
        "#,
    )
    .bind(id)
    .bind(body.target_space_id)
    .bind(stored_wrap)
    .execute(&mut *tx)
    .await?;

    // Always clear the old per-member wraps; if the source was a team space
    // these are stale, if the source was personal they were empty anyway.
    sqlx::query("DELETE FROM item_member_keys WHERE item_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;

    if target_is_team {
        if let Some(keys) = body.member_keys.as_deref() {
            validate_member_keys(&mut tx, body.target_space_id, keys).await?;
            for k in keys {
                sqlx::query(
                    r#"
                    INSERT INTO item_member_keys (item_id, user_id, sealed_item_key)
                    VALUES ($1, $2, $3)
                    "#,
                )
                .bind(id)
                .bind(k.user_id)
                .bind(&k.sealed_item_key)
                .execute(&mut *tx)
                .await?;
            }
        }
    }

    tx.commit().await?;

    let updated = load_item_for_user(&state, user.user_id, id)
        .await?
        .ok_or(AppError::NotFound)?;
    crate::audit::record_item(
        &state.pool,
        user.user_id,
        updated.space_id(),
        updated.id(),
        "item.move",
        Some(serde_json::json!({
            "from_space_id": existing.space_id,
            "to_space_id": body.target_space_id,
        })),
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
    let existing = load_item_for_user(&state, user.user_id, id)
        .await?
        .ok_or(AppError::NotFound)?;
    if existing.deleted_at.is_some() {
        return Err(AppError::NotFound);
    }

    assert_can_modify(
        &state,
        &user,
        existing.id(),
        existing.space_id(),
        existing.type_str(),
    )
    .await?;

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

    // For team-space items the snapshot's wrapped_item_key is NULL (item
    // key wraps live in item_member_keys, not item_versions). We never
    // rotate the item_key on snapshot/restore, so the active sealed wraps
    // still match the restored body. Just don't overwrite wrapped_item_key
    // back to NULL → write whatever the snapshot stored.
    sqlx::query(
        r#"
        UPDATE items
           SET title             = $2,
               encrypted_body    = $3,
               wrapped_item_key  = $4,
               updated_at        = NOW()
         WHERE id = $1
        "#,
    )
    .bind(id)
    .bind(&snap_title)
    .bind(&snap_body)
    .bind(&snap_key)
    .execute(&state.pool)
    .await?;

    let updated = load_item_for_user(&state, user.user_id, id)
        .await?
        .ok_or(AppError::NotFound)?;

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
    if matches!(role.as_str(), "viewer" | "kiosk") {
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
    // Restore-from-trash and version-restore are both edit-shaped — kiosk
    // can only do them on their own items, so ban the role outright (the
    // dashboard never trashes anything anyway).
    if matches!(role.as_str(), "viewer" | "kiosk") {
        return Err(AppError::Forbidden);
    }

    sqlx::query(
        r#"
        UPDATE items
           SET deleted_at = NULL,
               updated_at = NOW()
         WHERE id = $1
        "#,
    )
    .bind(id)
    .execute(&state.pool)
    .await?;
    let restored = load_item_for_user(&state, user.user_id, id)
        .await?
        .ok_or(AppError::NotFound)?;
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
