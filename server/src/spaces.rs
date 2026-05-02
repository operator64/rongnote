//! Team spaces + memberships.
//!
//! Phase A: schema + management endpoints. Items still use master_key wraps
//! exclusively. Phase B will switch team-space items to sealed_box-per-member
//! using the X25519 public keys we already collect at registration.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, patch, post},
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
        .route("/:id", get(get_one).delete(delete_space))
        .route("/:id/members", get(list_members).post(add_member))
        .route("/:id/members/:user_id", patch(set_role).delete(remove_member))
        // Resolve a user by email (returns id + pubkey for sealing). Auth-required
        // so randos can't enumerate the directory.
        .route("/lookup_user", post(lookup_user))
}

// --- Wire types ---

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct SpaceView {
    id: Uuid,
    name: String,
    kind: String,
    owner_id: Uuid,
    role: String,
    member_count: i64,
    #[serde(with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,
}

#[derive(Debug, Deserialize)]
pub struct CreateSpaceBody {
    name: String,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct MemberView {
    user_id: Uuid,
    email: String,
    role: String,
    /// X25519 public key for sealing item keys (Phase B).
    #[serde(with = "crate::b64")]
    public_key: Vec<u8>,
    #[serde(with = "time::serde::rfc3339")]
    joined_at: OffsetDateTime,
}

#[derive(Debug, Deserialize)]
pub struct AddMemberBody {
    email: String,
    /// 'editor' | 'viewer'. Owners are set by the create-space path only.
    role: String,
    /// Optional: sealed-box wraps of every existing item key for the new
    /// member. Required when the space already contains items (the inviter
    /// has computed the wraps from items they themselves can decrypt).
    /// Each entry is the wrap of one item's `item_key` to the invitee's
    /// public key, used to populate `item_member_keys`.
    #[serde(default)]
    item_keys: Vec<NewMemberItemKey>,
}

#[derive(Debug, Deserialize)]
pub struct NewMemberItemKey {
    item_id: Uuid,
    #[serde(with = "crate::b64")]
    sealed_item_key: Vec<u8>,
}

#[derive(Debug, Deserialize)]
pub struct SetRoleBody {
    role: String,
}

#[derive(Debug, Deserialize)]
pub struct LookupUserBody {
    email: String,
}

#[derive(Debug, Serialize)]
pub struct LookupUserView {
    id: Uuid,
    email: String,
    #[serde(with = "crate::b64")]
    public_key: Vec<u8>,
}

// --- Helpers ---

fn validate_role(role: &str) -> AppResult<()> {
    if matches!(role, "owner" | "editor" | "viewer" | "kiosk") {
        Ok(())
    } else {
        Err(AppError::BadRequest(
            "role must be one of owner|editor|viewer|kiosk".into(),
        ))
    }
}

async fn require_owner(
    state: &Arc<AppState>,
    user: &AuthUser,
    space_id: Uuid,
) -> AppResult<()> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT role FROM memberships WHERE space_id = $1 AND user_id = $2",
    )
    .bind(space_id)
    .bind(user.user_id)
    .fetch_optional(&state.pool)
    .await?;
    match row.map(|r| r.0).as_deref() {
        Some("owner") => Ok(()),
        Some(_) => Err(AppError::Forbidden),
        None => Err(AppError::NotFound),
    }
}

// --- Handlers ---

async fn list(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
) -> AppResult<Json<Vec<SpaceView>>> {
    let rows = sqlx::query_as::<_, SpaceView>(
        r#"
        SELECT s.id, s.name, s.kind, s.owner_id, m.role,
               (SELECT COUNT(*) FROM memberships WHERE space_id = s.id) AS member_count,
               s.created_at
          FROM spaces s
          JOIN memberships m ON m.space_id = s.id
         WHERE m.user_id = $1
         ORDER BY (s.kind = 'personal') DESC, s.created_at ASC
        "#,
    )
    .bind(user.user_id)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(rows))
}

async fn create(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(body): Json<CreateSpaceBody>,
) -> AppResult<Json<SpaceView>> {
    let name = body.name.trim();
    if name.is_empty() {
        return Err(AppError::BadRequest("space name required".into()));
    }
    if name.len() > 80 {
        return Err(AppError::BadRequest("space name too long".into()));
    }

    let mut tx = state.pool.begin().await?;
    let space: (Uuid, OffsetDateTime) = sqlx::query_as(
        r#"
        INSERT INTO spaces (name, kind, owner_id)
        VALUES ($1, 'team', $2)
        RETURNING id, created_at
        "#,
    )
    .bind(name)
    .bind(user.user_id)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO memberships (user_id, space_id, role) VALUES ($1, $2, 'owner')",
    )
    .bind(user.user_id)
    .bind(space.0)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    crate::audit::record(
        &state.pool,
        Some(user.user_id),
        Some(space.0),
        None,
        "space.create",
        Some(serde_json::json!({"name": name})),
    )
    .await;

    Ok(Json(SpaceView {
        id: space.0,
        name: name.to_owned(),
        kind: "team".into(),
        owner_id: user.user_id,
        role: "owner".into(),
        member_count: 1,
        created_at: space.1,
    }))
}

async fn get_one(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<SpaceView>> {
    let row = sqlx::query_as::<_, SpaceView>(
        r#"
        SELECT s.id, s.name, s.kind, s.owner_id, m.role,
               (SELECT COUNT(*) FROM memberships WHERE space_id = s.id) AS member_count,
               s.created_at
          FROM spaces s
          JOIN memberships m ON m.space_id = s.id
         WHERE s.id = $1 AND m.user_id = $2
        "#,
    )
    .bind(id)
    .bind(user.user_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::NotFound)?;
    Ok(Json(row))
}

async fn delete_space(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    require_owner(&state, &user, id).await?;
    // Deny deleting the personal space — that's the user's whole vault.
    let kind: (String,) = sqlx::query_as("SELECT kind FROM spaces WHERE id = $1")
        .bind(id)
        .fetch_one(&state.pool)
        .await?;
    if kind.0 == "personal" {
        return Err(AppError::BadRequest(
            "personal spaces cannot be deleted".into(),
        ));
    }
    sqlx::query("DELETE FROM spaces WHERE id = $1")
        .bind(id)
        .execute(&state.pool)
        .await?;
    crate::audit::record(
        &state.pool,
        Some(user.user_id),
        Some(id),
        None,
        "space.delete",
        None,
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_members(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Vec<MemberView>>> {
    // Authz: caller must be a member.
    let _: (String,) = sqlx::query_as(
        "SELECT role FROM memberships WHERE space_id = $1 AND user_id = $2",
    )
    .bind(id)
    .bind(user.user_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|_| AppError::NotFound)?;

    let rows = sqlx::query_as::<_, MemberView>(
        r#"
        SELECT m.user_id, u.email, m.role, u.public_key, m.created_at AS joined_at
          FROM memberships m
          JOIN users u ON u.id = m.user_id
         WHERE m.space_id = $1
         ORDER BY (m.role = 'owner') DESC, m.created_at ASC
        "#,
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(rows))
}

async fn add_member(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<AddMemberBody>,
) -> AppResult<Json<MemberView>> {
    require_owner(&state, &user, id).await?;
    if body.role == "owner" {
        return Err(AppError::BadRequest(
            "owners are set at create time, not invited".into(),
        ));
    }
    validate_role(&body.role)?;
    let email = body.email.trim().to_lowercase();
    if email.is_empty() || !email.contains('@') {
        return Err(AppError::BadRequest("invalid email".into()));
    }

    let lookup: Option<(Uuid, Vec<u8>, OffsetDateTime)> = sqlx::query_as(
        "SELECT id, public_key, created_at FROM users WHERE email = $1",
    )
    .bind(&email)
    .fetch_optional(&state.pool)
    .await?;
    let (target_user_id, public_key, _) =
        lookup.ok_or_else(|| AppError::NotFound)?;

    // Bundle membership insert + item-key re-wraps in one transaction so a
    // partial state (member added but not yet able to decrypt) is impossible.
    let mut tx = state.pool.begin().await?;

    let inserted: Option<(OffsetDateTime,)> = sqlx::query_as(
        r#"
        INSERT INTO memberships (user_id, space_id, role)
        VALUES ($1, $2, $3)
        ON CONFLICT (user_id, space_id) DO NOTHING
        RETURNING created_at
        "#,
    )
    .bind(target_user_id)
    .bind(id)
    .bind(&body.role)
    .fetch_optional(&mut *tx)
    .await?;
    let joined_at = inserted
        .map(|r| r.0)
        .ok_or_else(|| AppError::Conflict("user is already a member".into()))?;

    // Re-wraps: every supplied item_id must belong to this space and have a
    // body (encrypted_body IS NOT NULL, signalling there's a key to share).
    if !body.item_keys.is_empty() {
        let space_items: Vec<(Uuid,)> = sqlx::query_as(
            "SELECT id FROM items WHERE space_id = $1 AND encrypted_body IS NOT NULL",
        )
        .bind(id)
        .fetch_all(&mut *tx)
        .await?;
        let allowed: std::collections::HashSet<Uuid> =
            space_items.into_iter().map(|r| r.0).collect();
        for k in &body.item_keys {
            if !allowed.contains(&k.item_id) {
                return Err(AppError::BadRequest(format!(
                    "item_keys: item {} not in this space or has no body",
                    k.item_id
                )));
            }
            sqlx::query(
                r#"
                INSERT INTO item_member_keys (item_id, user_id, sealed_item_key)
                VALUES ($1, $2, $3)
                ON CONFLICT (item_id, user_id) DO NOTHING
                "#,
            )
            .bind(k.item_id)
            .bind(target_user_id)
            .bind(&k.sealed_item_key)
            .execute(&mut *tx)
            .await?;
        }
    }

    tx.commit().await?;

    crate::audit::record(
        &state.pool,
        Some(user.user_id),
        Some(id),
        None,
        "space.invite",
        Some(serde_json::json!({
            "invited_email": email,
            "role": body.role,
            "rewrapped_items": body.item_keys.len(),
        })),
    )
    .await;

    Ok(Json(MemberView {
        user_id: target_user_id,
        email,
        role: body.role,
        public_key,
        joined_at,
    }))
}

async fn set_role(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path((space_id, target_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<SetRoleBody>,
) -> AppResult<StatusCode> {
    require_owner(&state, &user, space_id).await?;
    if body.role == "owner" {
        return Err(AppError::BadRequest(
            "ownership transfer not supported in v1".into(),
        ));
    }
    validate_role(&body.role)?;
    if target_id == user.user_id {
        return Err(AppError::BadRequest("cannot change your own role".into()));
    }
    let res = sqlx::query(
        "UPDATE memberships SET role = $3 WHERE space_id = $1 AND user_id = $2",
    )
    .bind(space_id)
    .bind(target_id)
    .bind(&body.role)
    .execute(&state.pool)
    .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn remove_member(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path((space_id, target_id)): Path<(Uuid, Uuid)>,
) -> AppResult<StatusCode> {
    require_owner(&state, &user, space_id).await?;
    if target_id == user.user_id {
        return Err(AppError::BadRequest(
            "owners can't remove themselves; delete the space instead".into(),
        ));
    }
    // Drop any sealed wraps for this user in this space (Phase B will use them).
    sqlx::query(
        r#"
        DELETE FROM item_member_keys
         WHERE user_id = $1
           AND item_id IN (SELECT id FROM items WHERE space_id = $2)
        "#,
    )
    .bind(target_id)
    .bind(space_id)
    .execute(&state.pool)
    .await
    .ok();

    let res = sqlx::query(
        "DELETE FROM memberships WHERE space_id = $1 AND user_id = $2",
    )
    .bind(space_id)
    .bind(target_id)
    .execute(&state.pool)
    .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    crate::audit::record(
        &state.pool,
        Some(user.user_id),
        Some(space_id),
        None,
        "space.remove_member",
        Some(serde_json::json!({"removed_user_id": target_id})),
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

async fn lookup_user(
    State(state): State<Arc<AppState>>,
    _user: AuthUser,
    Json(body): Json<LookupUserBody>,
) -> AppResult<Json<LookupUserView>> {
    let email = body.email.trim().to_lowercase();
    if email.is_empty() || !email.contains('@') {
        return Err(AppError::BadRequest("invalid email".into()));
    }
    let row: Option<(Uuid, Vec<u8>)> =
        sqlx::query_as("SELECT id, public_key FROM users WHERE email = $1")
            .bind(&email)
            .fetch_optional(&state.pool)
            .await?;
    let (id, public_key) = row.ok_or(AppError::NotFound)?;
    Ok(Json(LookupUserView {
        id,
        email,
        public_key,
    }))
}
