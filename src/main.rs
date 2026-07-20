#![allow(dead_code)]

use std::sync::Arc;

use sqlx::postgres::PgPoolOptions;
use teloxide::Bot;
use tracing_subscriber::EnvFilter;

mod bot;
mod config;
mod db;
mod error;
mod github;
mod limits;
mod models;
mod poller;
mod rate_limit;
mod routes;
mod state;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load .env
    dotenvy::dotenv().ok();

    // Tracing
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    // Config
    let config = config::Config::from_env()?;
    let port = config.port;
    let interval = config.cron_interval_secs;
    let config = Arc::new(config);

    // DB pool
    let db = PgPoolOptions::new()
        .max_connections(10)
        .connect(&config.database_url)
        .await?;

    // Run migrations
    sqlx::migrate!("./migrations").run(&db).await?;

    // Bot
    let bot = Bot::new(&config.telegram_bot_token);

    // HTTP client
    let http_client = reqwest::Client::builder()
        .user_agent("gitwatch-v2/0.1")
        .build()?;

    // Shared state
    let app_state = state::AppState {
        db,
        bot: bot.clone(),
        config: config.clone(),
        rate_limiter: Arc::new(rate_limit::RateLimiter::new()),
        http_client,
    };

    // Spawn poller
    let poller_state = app_state.clone();
    tokio::spawn(async move {
        poller::run_poller(poller_state, interval).await;
    });

    // Spawn Telegram bot
    let bot_state = app_state.clone();
    tokio::spawn(async move {
        bot::run_bot(bot_state).await;
    });

    // Start HTTP server
    let router = routes::create_router(app_state);
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}")).await?;
    tracing::info!("Server listening on port {port}");
    axum::serve(listener, router).await?;

    Ok(())
}
