//! Audit log: who did what when. Per spec §3, secret reads are required;
//! everything else is best-effort. Inserts are non-fatal — if logging fails
//! we still let the action through.

use std::sync::Arc;

use axum::{
    extract::{Query, State},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{error::AppResult, session::AuthUser, AppState};

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/", get(list))
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct AuditEntry {
    id: Uuid,
    user_id: Option<Uuid>,
    space_id: Option<Uuid>,
    item_id: Option<Uuid>,
    /// Title of the referenced item if it still exists (null after hard-delete).
    item_title: Option<String>,
    item_type: Option<String>,
    action: String,
    meta: Option<JsonValue>,
    #[serde(with = "time::serde::rfc3339")]
    ts: OffsetDateTime,
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    #[serde(default = "default_limit")]
    limit: i64,
}
fn default_limit() -> i64 {
    100
}

async fn list(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<Vec<AuditEntry>>> {
    let limit = q.limit.clamp(1, 500);
    let rows = sqlx::query_as::<_, AuditEntry>(
        r#"
        SELECT a.id, a.user_id, a.space_id, a.item_id,
               i.title AS item_title,
               i.type AS item_type,
               a.action, a.meta, a.ts
          FROM audit_log a
          LEFT JOIN items i ON i.id = a.item_id
         WHERE a.user_id = $1
         ORDER BY a.ts DESC
         LIMIT $2
        "#,
    )
    .bind(user.user_id)
    .bind(limit)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(rows))
}

/// Fire-and-forget audit insert. Errors are swallowed (logged at warn).
pub async fn record(
    pool: &PgPool,
    user_id: Option<Uuid>,
    space_id: Option<Uuid>,
    item_id: Option<Uuid>,
    action: &str,
    meta: Option<JsonValue>,
) {
    let res = sqlx::query(
        "INSERT INTO audit_log (user_id, space_id, item_id, action, meta) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(user_id)
    .bind(space_id)
    .bind(item_id)
    .bind(action)
    .bind(meta)
    .execute(pool)
    .await;
    if let Err(e) = res {
        tracing::warn!(error = %e, action, "audit log insert failed");
    }
}

/// Convenience for handlers that already have a user.
pub async fn record_user(pool: &PgPool, user_id: Uuid, action: &str, meta: Option<JsonValue>) {
    record(pool, Some(user_id), None, None, action, meta).await;
}

pub async fn record_item(
    pool: &PgPool,
    user_id: Uuid,
    space_id: Uuid,
    item_id: Uuid,
    action: &str,
    meta: Option<JsonValue>,
) {
    record(pool, Some(user_id), Some(space_id), Some(item_id), action, meta).await;
}
