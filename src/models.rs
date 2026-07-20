use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct User {
    pub id: Uuid,
    pub telegram_id: i64,
    pub telegram_username: Option<String>,
    pub github_token: Option<String>,
    pub github_username: Option<String>,
    pub plan: String,
    pub plan_expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct WatchedRepo {
    pub id: Uuid,
    pub user_id: Uuid,
    pub owner: String,
    pub repo: String,
    pub webhook_id: Option<i64>,
    pub watch_mode: String,
    pub last_polled: Option<DateTime<Utc>>,
    pub active: bool,
    pub notify_issues: bool,
    pub notify_prs: bool,
    pub notify_commits: bool,
    pub notify_comments: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct WatchedRepoWithUser {
    // watched_repos fields
    pub id: Uuid,
    pub user_id: Uuid,
    pub owner: String,
    pub repo: String,
    pub webhook_id: Option<i64>,
    pub watch_mode: String,
    pub last_polled: Option<DateTime<Utc>>,
    pub active: bool,
    pub notify_issues: bool,
    pub notify_prs: bool,
    pub notify_commits: bool,
    pub notify_comments: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    // user fields (prefixed)
    pub user_telegram_id: i64,
    pub user_github_token: Option<String>,
    pub user_github_username: Option<String>,
}
