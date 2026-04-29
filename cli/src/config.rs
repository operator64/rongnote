//! Persisted CLI session: server URL, login email, session cookie, plus
//! the unwrapped master_key + privkey so subsequent invocations don't
//! re-prompt for the passphrase.
//!
//! The same security caveat as the browser's sessionStorage applies: a
//! local attacker with read access to the file can decrypt the user's
//! data. We chmod the file 600 on Unix; on Windows we rely on the user
//! profile's ACLs. Set $RONGNOTE_NO_PERSIST=1 to disable on-disk caching
//! entirely (every command will re-prompt).

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use zeroize::Zeroize;

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct Session {
    pub server: String,
    pub email: String,
    pub user_id: String,
    /// "rongnote_session=<uuid>" — replayed into the cookie jar.
    pub cookie: String,
    /// base64 of the unwrapped master_key. ⚠ secret.
    pub master_key_b64: String,
    /// base64 of our X25519 public key.
    pub public_key_b64: String,
    /// base64 of the unwrapped X25519 private key. ⚠ secret.
    pub private_key_b64: String,
    /// Optional active space id; falls back to the server-default personal.
    #[serde(default)]
    pub active_space_id: Option<String>,
}

impl Session {
    pub fn path() -> Result<PathBuf> {
        if let Ok(p) = std::env::var("RONGNOTE_SESSION_PATH") {
            return Ok(PathBuf::from(p));
        }
        let proj = directories::ProjectDirs::from("", "rongnote", "rongnote")
            .ok_or_else(|| anyhow!("can't resolve config dir"))?;
        let dir = proj.config_dir();
        std::fs::create_dir_all(dir)
            .with_context(|| format!("creating config dir {}", dir.display()))?;
        Ok(dir.join("session.json"))
    }

    pub fn load() -> Result<Option<Session>> {
        if std::env::var("RONGNOTE_NO_PERSIST").ok().as_deref() == Some("1") {
            return Ok(None);
        }
        let p = Self::path()?;
        if !p.exists() {
            return Ok(None);
        }
        let raw = std::fs::read_to_string(&p)
            .with_context(|| format!("reading {}", p.display()))?;
        let s: Session = serde_json::from_str(&raw)
            .with_context(|| format!("parsing {}", p.display()))?;
        Ok(Some(s))
    }

    pub fn save(&self) -> Result<()> {
        if std::env::var("RONGNOTE_NO_PERSIST").ok().as_deref() == Some("1") {
            return Ok(());
        }
        let p = Self::path()?;
        let tmp = p.with_extension("json.tmp");
        let raw = serde_json::to_vec_pretty(self)?;
        std::fs::write(&tmp, &raw).with_context(|| format!("writing {}", tmp.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
        }
        std::fs::rename(&tmp, &p).with_context(|| format!("renaming to {}", p.display()))?;
        Ok(())
    }

    pub fn clear() -> Result<()> {
        let p = Self::path()?;
        if p.exists() {
            // Zeroize-overwrite before delete is overkill; the OS will reuse
            // the blocks. Just unlink.
            std::fs::remove_file(&p)?;
        }
        Ok(())
    }
}

/// Drop hook so the master_key/private_key b64 strings don't linger in
/// memory when a Session is dropped.
impl Drop for Session {
    fn drop(&mut self) {
        self.master_key_b64.zeroize();
        self.private_key_b64.zeroize();
        self.cookie.zeroize();
    }
}
