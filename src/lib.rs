#![allow(dead_code)]

pub mod bot;
pub mod config;
pub mod db;
pub mod error;
pub mod github;
pub mod limits;
pub mod models;
pub mod poller;
pub mod rate_limit;
pub mod routes;
pub mod state;

use std::sync::Arc;

use sqlx::postgres::PgPoolOptions;
use teloxide::Bot;
use tracing_subscriber::EnvFilter;

pub async fn run() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let config = config::Config::from_env()?;
    let port = config.port;
    let interval = config.cron_interval_secs;
    let config = Arc::new(config);

    let db = PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(std::time::Duration::from_secs(30))
        .connect(&config.database_url)
        .await?;

    sqlx::migrate!("./migrations").run(&db).await?;

    let bot = Bot::new(&config.telegram_bot_token);

    let http_client = reqwest::Client::builder()
        .user_agent("gitwatch-v2/0.1")
        .build()?;

    let app_state = state::AppState {
        db,
        bot,
        config,
        rate_limiter: Arc::new(rate_limit::RateLimiter::new()),
        http_client,
    };

    let poller_state = app_state.clone();
    tokio::spawn(async move {
        poller::run_poller(poller_state, interval).await;
    });

    let bot_state = app_state.clone();
    tokio::spawn(async move {
        bot::run_bot(bot_state).await;
    });

    let router = routes::create_router(app_state);
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}")).await?;
    tracing::info!("Server listening on port {port}");
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    tracing::info!("Shutdown complete");
    Ok(())
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("failed to install Ctrl+C handler");
    tracing::info!("Shutting down...");
}
