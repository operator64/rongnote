//! Thin HTTP client wrapping the rongnote-server REST API. Field names
//! match the server's wire format exactly so JSON round-trips work.

use anyhow::{anyhow, bail, Context, Result};
use reqwest::blocking::{Client, ClientBuilder};
use reqwest::cookie::CookieStore;
use reqwest::header::CONTENT_TYPE;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone)]
pub struct ApiClient {
    pub base: String,
    inner: Client,
    cookie_jar: Arc<reqwest::cookie::Jar>,
}

impl ApiClient {
    pub fn new(base: &str) -> Result<Self> {
        let jar = Arc::new(reqwest::cookie::Jar::default());
        let inner = ClientBuilder::new()
            .cookie_provider(jar.clone())
            .user_agent(concat!("rongnote-cli/", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Self {
            base: base.trim_end_matches('/').to_owned(),
            inner,
            cookie_jar: jar,
        })
    }

    /// Inject a previously-saved session cookie into the jar so subsequent
    /// requests are authenticated. The cookie line should look like
    /// `rongnote_session=<uuid>`.
    pub fn restore_cookie(&self, cookie_line: &str) -> Result<()> {
        let url = url::Url::parse(&self.base).context("base url")?;
        self.cookie_jar
            .add_cookie_str(&format!("{cookie_line}; Path=/"), &url);
        Ok(())
    }

    /// Pull whatever the server set as `rongnote_session` after login.
    pub fn current_session_cookie(&self) -> Option<String> {
        let url = url::Url::parse(&self.base).ok()?;
        // reqwest's cookie::Jar exposes cookies(&url) returning a header value.
        let header = self.cookie_jar.cookies(&url)?;
        let s = header.to_str().ok()?.to_owned();
        // The header is "k1=v1; k2=v2; …". Pick the rongnote_session pair.
        for kv in s.split(';') {
            let kv = kv.trim();
            if kv.starts_with("rongnote_session=") {
                return Some(kv.to_owned());
            }
        }
        None
    }

    fn request<T: for<'de> Deserialize<'de>>(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<&serde_json::Value>,
    ) -> Result<T> {
        let url = format!("{}{}", self.base, path);
        let mut req = self.inner.request(method, &url);
        if let Some(b) = body {
            req = req
                .header(CONTENT_TYPE, "application/json")
                .body(serde_json::to_vec(b)?);
        }
        let res = req.send().with_context(|| format!("request to {url}"))?;
        let status = res.status();
        let text = res.text().unwrap_or_default();
        if !status.is_success() {
            // Try to surface the server's {error, message}; fall back to raw.
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
                let msg = v
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("unknown error");
                bail!("{} {}: {}", status.as_u16(), status.canonical_reason().unwrap_or(""), msg);
            }
            bail!("{}: {}", status, text);
        }
        if status == StatusCode::NO_CONTENT || text.is_empty() {
            // Allow the caller to deserialize unit-like responses as ()
            // by going through serde_json::Value::Null.
            return serde_json::from_value(serde_json::Value::Null)
                .context("empty response not deserializable as T");
        }
        serde_json::from_str(&text).with_context(|| format!("decoding {path}"))
    }

    fn get<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Result<T> {
        self.request(reqwest::Method::GET, path, None)
    }
    fn post<T: for<'de> Deserialize<'de>>(&self, path: &str, body: &serde_json::Value) -> Result<T> {
        self.request(reqwest::Method::POST, path, Some(body))
    }
    #[allow(dead_code)]
    fn patch<T: for<'de> Deserialize<'de>>(&self, path: &str, body: &serde_json::Value) -> Result<T> {
        self.request(reqwest::Method::PATCH, path, Some(body))
    }
    fn delete(&self, path: &str) -> Result<()> {
        let _: serde_json::Value = self.request(reqwest::Method::DELETE, path, None)
            .or_else(|e| {
                // 204 returns nothing; tolerate "empty response" decode failure.
                if e.to_string().contains("empty response") {
                    Ok(serde_json::Value::Null)
                } else {
                    Err(e)
                }
            })?;
        Ok(())
    }

    // --- Endpoints ---

    pub fn precheck(&self, email: &str) -> Result<PrecheckResponse> {
        self.post("/api/v1/auth/precheck", &serde_json::json!({"email": email}))
    }

    pub fn login(&self, email: &str, auth_hash_b64: &str) -> Result<UserView> {
        self.post(
            "/api/v1/auth/login",
            &serde_json::json!({"email": email, "auth_hash": auth_hash_b64}),
        )
    }

    pub fn me(&self) -> Result<UserView> {
        self.get("/api/v1/auth/me")
    }

    pub fn logout(&self) -> Result<()> {
        let _: serde_json::Value = self.post("/api/v1/auth/logout", &serde_json::json!({}))
            .or_else(|e| {
                if e.to_string().contains("empty response") {
                    Ok(serde_json::Value::Null)
                } else {
                    Err(e)
                }
            })?;
        Ok(())
    }

    pub fn list_items(&self, opts: &ListItemsOptions) -> Result<Vec<ItemSummary>> {
        let mut qs = url::form_urlencoded::Serializer::new(String::new());
        if let Some(t) = &opts.type_ {
            qs.append_pair("type", t);
        }
        if opts.trash {
            qs.append_pair("trash", "true");
        }
        if let Some(s) = &opts.space_id {
            qs.append_pair("space_id", s);
        }
        let q = qs.finish();
        let path = if q.is_empty() {
            "/api/v1/items".to_owned()
        } else {
            format!("/api/v1/items?{q}")
        };
        self.get(&path)
    }

    pub fn get_item(&self, id: &str) -> Result<Item> {
        self.get(&format!("/api/v1/items/{id}"))
    }

    pub fn create_item(&self, input: &CreateItemInput) -> Result<Item> {
        self.post("/api/v1/items", &serde_json::to_value(input)?)
    }

    pub fn delete_item(&self, id: &str, hard: bool) -> Result<()> {
        let path = if hard {
            format!("/api/v1/items/{id}?hard=true")
        } else {
            format!("/api/v1/items/{id}")
        };
        self.delete(&path)
    }

    pub fn list_spaces(&self) -> Result<Vec<Space>> {
        self.get("/api/v1/spaces/")
    }

    pub fn list_members(&self, space_id: &str) -> Result<Vec<Member>> {
        self.get(&format!("/api/v1/spaces/{space_id}/members"))
    }
}

// --- Wire types matching server JSON. Some fields are unread by current
// commands but kept so JSON round-trips cleanly and so future subcommands
// don't have to re-derive the schema.
#[allow(dead_code)]

#[derive(Debug, Deserialize)]
pub struct PrecheckResponse {
    pub passphrase_salt: String,
    pub master_wrap_passphrase: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct UserView {
    pub id: String,
    pub email: String,
    pub passphrase_salt: String,
    pub master_wrap_passphrase: String,
    pub public_key: String,
    pub encrypted_private_key: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize, Clone)]
pub struct ItemSummary {
    pub id: String,
    #[serde(rename = "type")]
    pub type_: String,
    pub title: String,
    pub tags: Vec<String>,
    pub path: String,
    pub updated_at: String,
    #[serde(default)]
    pub due_at: Option<String>,
    pub done: bool,
    pub pinned: bool,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize, Clone)]
pub struct Item {
    pub id: String,
    pub space_id: String,
    #[serde(rename = "type")]
    pub type_: String,
    pub title: String,
    pub tags: Vec<String>,
    pub path: String,
    pub encrypted_body: Option<String>,
    pub wrapped_item_key: Option<String>,
    #[serde(default)]
    pub key_wrap: Option<String>,
    #[serde(default)]
    pub blob_sha256: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub deleted_at: Option<String>,
    #[serde(default)]
    pub due_at: Option<String>,
    pub done: bool,
    pub pinned: bool,
}

#[derive(Debug, Default)]
pub struct ListItemsOptions {
    pub type_: Option<String>,
    pub trash: bool,
    pub space_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CreateItemInput {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_: Option<String>,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encrypted_body: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wrapped_item_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub member_keys: Option<Vec<MemberKeyInput>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub space_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct MemberKeyInput {
    pub user_id: String,
    pub sealed_item_key: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct Space {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub owner_id: String,
    pub role: String,
    pub member_count: i64,
    pub created_at: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct Member {
    pub user_id: String,
    pub email: String,
    pub role: String,
    pub public_key: String,
    pub joined_at: String,
}

/// Helper to build &str representation of an item kind for filtering.
pub fn validate_item_type(t: &str) -> Result<()> {
    const VALID: &[&str] = &[
        "note", "task", "list", "secret", "snippet", "bookmark", "file", "event",
    ];
    if VALID.contains(&t) {
        Ok(())
    } else {
        Err(anyhow!(
            "unknown item type {t:?}; expected one of {:?}",
            VALID
        ))
    }
}
