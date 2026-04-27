use std::time::Duration;

#[derive(Debug, Clone)]
pub struct Config {
    pub bind_addr: String,
    pub database_url: String,
    pub public_url: String,
    pub data_dir: String,
    pub session_ttl: Duration,
    pub is_production: bool,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let bind_addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".into());
        let database_url = std::env::var("DATABASE_URL")
            .map_err(|_| anyhow::anyhow!("DATABASE_URL must be set"))?;
        let public_url =
            std::env::var("PUBLIC_URL").unwrap_or_else(|_| "http://localhost:8080".into());
        let data_dir = std::env::var("DATA_DIR").unwrap_or_else(|_| "./data".into());
        let session_ttl_str = std::env::var("SESSION_TTL").unwrap_or_else(|_| "30d".into());
        let session_ttl = humantime::parse_duration(&session_ttl_str)
            .map_err(|e| anyhow::anyhow!("SESSION_TTL invalid: {e}"))?;
        let is_production = std::env::var("APP_ENV")
            .map(|v| v.eq_ignore_ascii_case("production"))
            .unwrap_or(false);

        Ok(Self {
            bind_addr,
            database_url,
            public_url,
            data_dir,
            session_ttl,
            is_production,
        })
    }

    /// Whether the cookie Secure flag should be set. PUBLIC_URL must be https
    /// AND we must be in production mode for browsers to accept the cookie at
    /// localhost during dev.
    pub fn cookie_secure(&self) -> bool {
        self.is_production && self.public_url.starts_with("https://")
    }
}
