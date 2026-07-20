use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::{User, WatchedRepo, WatchedRepoWithUser};

pub struct PlatformStats {
    pub total_users: i64,
    pub total_repos: i64,
    pub active_repos: i64,
}

pub async fn find_user_by_telegram_id(pool: &PgPool, telegram_id: i64) -> anyhow::Result<Option<User>> {
    let user = sqlx::query_as::<_, User>(
        "SELECT * FROM users WHERE telegram_id = $1",
    )
    .bind(telegram_id)
    .fetch_optional(pool)
    .await?;
    Ok(user)
}

pub async fn find_user_by_telegram_id_exact(pool: &PgPool, id: i64) -> anyhow::Result<Option<User>> {
    find_user_by_telegram_id(pool, id).await
}

pub async fn upsert_user(pool: &PgPool, telegram_id: i64, username: Option<&str>) -> anyhow::Result<User> {
    let user = sqlx::query_as::<_, User>(
        r#"
        INSERT INTO users (telegram_id, telegram_username)
        VALUES ($1, $2)
        ON CONFLICT (telegram_id) DO UPDATE
            SET telegram_username = COALESCE($2, users.telegram_username),
                updated_at = NOW()
        RETURNING *
        "#,
    )
    .bind(telegram_id)
    .bind(username)
    .fetch_one(pool)
    .await?;
    Ok(user)
}

pub async fn update_user_github(
    pool: &PgPool,
    telegram_id: i64,
    token: &str,
    username: &str,
) -> anyhow::Result<User> {
    let user = sqlx::query_as::<_, User>(
        r#"
        UPDATE users
        SET github_token = $2, github_username = $3, updated_at = NOW()
        WHERE telegram_id = $1
        RETURNING *
        "#,
    )
    .bind(telegram_id)
    .bind(token)
    .bind(username)
    .fetch_one(pool)
    .await?;
    Ok(user)
}

pub async fn count_users(pool: &PgPool) -> anyhow::Result<i64> {
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
        .fetch_one(pool)
        .await?;
    Ok(row.0)
}

pub async fn find_watched_repo(
    pool: &PgPool,
    user_id: Uuid,
    owner: &str,
    repo: &str,
) -> anyhow::Result<Option<WatchedRepo>> {
    let r = sqlx::query_as::<_, WatchedRepo>(
        "SELECT * FROM watched_repos WHERE user_id = $1 AND owner = $2 AND repo = $3",
    )
    .bind(user_id)
    .bind(owner)
    .bind(repo)
    .fetch_optional(pool)
    .await?;
    Ok(r)
}

pub async fn find_watched_repo_by_id(pool: &PgPool, id: Uuid) -> anyhow::Result<Option<WatchedRepo>> {
    let r = sqlx::query_as::<_, WatchedRepo>("SELECT * FROM watched_repos WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(r)
}

pub async fn count_active_repos_for_user(pool: &PgPool, user_id: Uuid) -> anyhow::Result<i64> {
    let row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM watched_repos WHERE user_id = $1 AND active = true",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}

pub async fn create_watched_repo(
    pool: &PgPool,
    user_id: Uuid,
    owner: &str,
    repo: &str,
    webhook_id: Option<i64>,
    watch_mode: &str,
) -> anyhow::Result<WatchedRepo> {
    let r = sqlx::query_as::<_, WatchedRepo>(
        r#"
        INSERT INTO watched_repos (user_id, owner, repo, webhook_id, watch_mode)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (user_id, owner, repo) DO UPDATE
            SET webhook_id = $4, watch_mode = $5, active = true, updated_at = NOW()
        RETURNING *
        "#,
    )
    .bind(user_id)
    .bind(owner)
    .bind(repo)
    .bind(webhook_id)
    .bind(watch_mode)
    .fetch_one(pool)
    .await?;
    Ok(r)
}

pub async fn get_user_watched_repos(pool: &PgPool, user_id: Uuid) -> anyhow::Result<Vec<WatchedRepo>> {
    let repos = sqlx::query_as::<_, WatchedRepo>(
        "SELECT * FROM watched_repos WHERE user_id = $1 AND active = true ORDER BY created_at ASC",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    Ok(repos)
}

pub async fn deactivate_watched_repo(pool: &PgPool, id: Uuid) -> anyhow::Result<()> {
    sqlx::query("UPDATE watched_repos SET active = false, updated_at = NOW() WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn delete_all_user_repos(pool: &PgPool, user_id: Uuid) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM watched_repos WHERE user_id = $1")
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn disconnect_user_github(pool: &PgPool, user_id: Uuid) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE users SET github_token = NULL, github_username = NULL, updated_at = NOW() WHERE id = $1",
    )
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn find_repos_by_owner_repo(
    pool: &PgPool,
    owner: &str,
    repo: &str,
) -> anyhow::Result<Vec<WatchedRepoWithUser>> {
    let repos = sqlx::query_as::<_, WatchedRepoWithUser>(
        r#"
        SELECT
            wr.id, wr.user_id, wr.owner, wr.repo, wr.webhook_id, wr.watch_mode,
            wr.last_polled, wr.active, wr.notify_issues, wr.notify_prs,
            wr.notify_commits, wr.notify_comments, wr.created_at, wr.updated_at,
            u.telegram_id as user_telegram_id,
            u.github_token as user_github_token,
            u.github_username as user_github_username
        FROM watched_repos wr
        JOIN users u ON u.id = wr.user_id
        WHERE wr.owner = $1 AND wr.repo = $2 AND wr.active = true
        "#,
    )
    .bind(owner)
    .bind(repo)
    .fetch_all(pool)
    .await?;
    Ok(repos)
}

pub async fn get_polling_repos(pool: &PgPool, limit: i64) -> anyhow::Result<Vec<WatchedRepoWithUser>> {
    let repos = sqlx::query_as::<_, WatchedRepoWithUser>(
        r#"
        SELECT
            wr.id, wr.user_id, wr.owner, wr.repo, wr.webhook_id, wr.watch_mode,
            wr.last_polled, wr.active, wr.notify_issues, wr.notify_prs,
            wr.notify_commits, wr.notify_comments, wr.created_at, wr.updated_at,
            u.telegram_id as user_telegram_id,
            u.github_token as user_github_token,
            u.github_username as user_github_username
        FROM watched_repos wr
        JOIN users u ON u.id = wr.user_id
        WHERE wr.watch_mode = 'polling' AND wr.active = true
        ORDER BY wr.last_polled ASC NULLS FIRST
        LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(repos)
}

pub async fn update_last_polled(pool: &PgPool, id: Uuid) -> anyhow::Result<()> {
    sqlx::query("UPDATE watched_repos SET last_polled = NOW(), updated_at = NOW() WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn toggle_notify_field(pool: &PgPool, repo_id: Uuid, field: &str) -> anyhow::Result<WatchedRepo> {
    let r = match field {
        "issues" => {
            sqlx::query_as::<_, WatchedRepo>(
                "UPDATE watched_repos SET notify_issues = NOT notify_issues, updated_at = NOW() WHERE id = $1 RETURNING *",
            )
            .bind(repo_id)
            .fetch_one(pool)
            .await?
        }
        "prs" => {
            sqlx::query_as::<_, WatchedRepo>(
                "UPDATE watched_repos SET notify_prs = NOT notify_prs, updated_at = NOW() WHERE id = $1 RETURNING *",
            )
            .bind(repo_id)
            .fetch_one(pool)
            .await?
        }
        "commits" => {
            sqlx::query_as::<_, WatchedRepo>(
                "UPDATE watched_repos SET notify_commits = NOT notify_commits, updated_at = NOW() WHERE id = $1 RETURNING *",
            )
            .bind(repo_id)
            .fetch_one(pool)
            .await?
        }
        "comments" => {
            sqlx::query_as::<_, WatchedRepo>(
                "UPDATE watched_repos SET notify_comments = NOT notify_comments, updated_at = NOW() WHERE id = $1 RETURNING *",
            )
            .bind(repo_id)
            .fetch_one(pool)
            .await?
        }
        _ => {
            return Err(anyhow::anyhow!("unknown notify field: {field}"));
        }
    };
    Ok(r)
}

pub async fn get_platform_stats(pool: &PgPool) -> anyhow::Result<PlatformStats> {
    let total_users: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
        .fetch_one(pool)
        .await?;
    let total_repos: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM watched_repos")
        .fetch_one(pool)
        .await?;
    let active_repos: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM watched_repos WHERE active = true")
            .fetch_one(pool)
            .await?;
    Ok(PlatformStats {
        total_users: total_users.0,
        total_repos: total_repos.0,
        active_repos: active_repos.0,
    })
}

pub async fn set_user_plan(
    pool: &PgPool,
    user_id: Uuid,
    plan: &str,
    expires_at: Option<DateTime<Utc>>,
) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE users SET plan = $2, plan_expires_at = $3, updated_at = NOW() WHERE id = $1",
    )
    .bind(user_id)
    .bind(plan)
    .bind(expires_at)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn count_premium_users(pool: &PgPool) -> anyhow::Result<i64> {
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users WHERE plan = 'premium'")
        .fetch_one(pool)
        .await?;
    Ok(row.0)
}

pub async fn find_user_by_username(pool: &PgPool, username: &str) -> anyhow::Result<Option<User>> {
    let user = sqlx::query_as::<_, User>(
        "SELECT * FROM users WHERE telegram_username = $1",
    )
    .bind(username)
    .fetch_optional(pool)
    .await?;
    Ok(user)
}
