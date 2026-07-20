use anyhow::{Context, Result};

#[derive(Clone, Debug)]
pub struct Config {
    pub telegram_bot_token: String,
    pub github_client_id: String,
    pub github_client_secret: String,
    pub github_webhook_secret: String,
    pub app_url: String,
    pub database_url: String,
    pub admin_telegram_id: Option<i64>,
    pub cron_interval_secs: u64,
    pub port: u16,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            telegram_bot_token: env_var("TELEGRAM_BOT_TOKEN")?,
            github_client_id: env_var("GITHUB_CLIENT_ID")?,
            github_client_secret: env_var("GITHUB_CLIENT_SECRET")?,
            github_webhook_secret: env_var("GITHUB_WEBHOOK_SECRET")?,
            app_url: env_var("APP_URL")?,
            database_url: env_var("DATABASE_URL")?,
            admin_telegram_id: std::env::var("ADMIN_TELEGRAM_ID")
                .ok()
                .and_then(|v| v.parse().ok()),
            cron_interval_secs: std::env::var("CRON_INTERVAL_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(300),
            port: std::env::var("PORT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(3000),
        })
    }
}

fn env_var(key: &str) -> Result<String> {
    std::env::var(key).with_context(|| format!("missing env var: {key}"))
}
