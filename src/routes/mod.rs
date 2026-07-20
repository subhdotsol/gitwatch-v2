pub mod auth;
pub mod github_webhook;

use axum::{routing::{get, post}, Router};

use crate::state::AppState;

pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/auth/github", get(auth::github_oauth_start))
        .route("/api/auth/github/callback", get(auth::github_oauth_callback))
        .route("/api/webhooks/github", post(github_webhook::github_webhook))
        .with_state(state)
}

async fn health() -> &'static str {
    "ok"
}
