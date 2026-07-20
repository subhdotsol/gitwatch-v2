use std::sync::Arc;

use sqlx::PgPool;
use teloxide::Bot;

use crate::{config::Config, rate_limit::RateLimiter};

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub bot: Bot,
    pub config: Arc<Config>,
    pub rate_limiter: Arc<RateLimiter>,
    pub http_client: reqwest::Client,
}
