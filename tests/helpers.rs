#![allow(dead_code)]

use std::sync::Arc;

use sqlx::postgres::PgPoolOptions;
use teloxide::Bot;

use gitwatch_v2::{config::Config, rate_limit::RateLimiter, state::AppState};

pub async fn setup() -> AppState {
    dotenvy::dotenv().ok();

    let config = Config::from_env().expect("Config must load from .env for integration tests");

    let test_db_url = std::env::var("TEST_DATABASE_URL")
        .expect("TEST_DATABASE_URL must be set for integration tests");

    let db = PgPoolOptions::new()
        .max_connections(5)
        .connect(&test_db_url)
        .await
        .expect("TEST_DATABASE_URL connection failed");

    sqlx::migrate!("./migrations")
        .run(&db)
        .await
        .expect("Migrations");

    let bot = Bot::new(&config.telegram_bot_token);
    let http_client = reqwest::Client::builder()
        .user_agent("gitwatch-v2-test/0.1")
        .build()
        .unwrap();

    AppState {
        db,
        bot,
        config: Arc::new(config),
        rate_limiter: Arc::new(RateLimiter::new()),
        http_client,
    }
}

/// Returns a large Telegram ID in a test-only range (>9_000_000_000)
/// unlikely to conflict with real users.
pub fn rand_telegram_id() -> i64 {
    9_000_000_000i64 + (rand::random::<u32>() % 999_999_999) as i64
}

/// Delete a test user and their repos by telegram_id.
pub async fn cleanup_user(pool: &sqlx::PgPool, telegram_id: i64) {
    // watched_repos cascade-deletes on user delete
    let _ = sqlx::query("DELETE FROM users WHERE telegram_id = $1")
        .bind(telegram_id)
        .execute(pool)
        .await;
}
