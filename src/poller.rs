use std::time::Duration;

use chrono::Utc;
use tokio::time::{self, MissedTickBehavior};
use tracing::{error, info, warn};

use crate::{bot::notify, db, github, state::AppState};

pub async fn run_poller(state: AppState, interval_secs: u64) {
    let mut ticker = time::interval(Duration::from_secs(interval_secs));
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

    info!("Poller started, interval: {}s", interval_secs);

    loop {
        ticker.tick().await;
        if let Err(e) = poll_all(&state).await {
            error!("Polling cycle error: {e}");
        }
    }
}

async fn poll_all(state: &AppState) -> anyhow::Result<()> {
    let repos = db::get_polling_repos(&state.db, 500).await?;
    info!("Polling {} repos", repos.len());

    // Process in chunks of 10 concurrently
    for chunk in repos.chunks(10) {
        let futures: Vec<_> = chunk.iter().map(|repo| poll_repo(state, repo)).collect();

        futures::future::join_all(futures).await;
    }

    Ok(())
}

async fn poll_repo(state: &AppState, repo: &crate::models::WatchedRepoWithUser) {
    let token = match &repo.user_github_token {
        Some(t) => t.clone(),
        None => {
            warn!(
                "No GitHub token for user {} watching {}/{}",
                repo.user_telegram_id, repo.owner, repo.repo
            );
            return;
        }
    };

    let events_result =
        github::get_repo_events(&state.http_client, &token, &repo.owner, &repo.repo).await;

    match events_result {
        Err(e) => {
            let err_str = e.to_string();
            // Check for auth / not found errors
            if err_str.contains("401") || err_str.contains("403") {
                warn!(
                    "Auth error polling {}/{} for user {}: {}",
                    repo.owner, repo.repo, repo.user_telegram_id, e
                );
                // Deactivate and notify user
                if let Err(de) = db::deactivate_watched_repo(&state.db, repo.id).await {
                    error!("Failed to deactivate repo {}: {de}", repo.id);
                }
                notify::send_telegram(
                    &state.bot,
                    repo.user_telegram_id,
                    &format!(
                        "⚠️ Stopped watching <b>{}/{}</b> — GitHub token expired or access revoked.\n\
                         Please reconnect with /start.",
                        repo.owner, repo.repo
                    ),
                )
                .await;
            } else if err_str.contains("404") {
                warn!("Repo not found {}/{}: {}", repo.owner, repo.repo, e);
                if let Err(de) = db::deactivate_watched_repo(&state.db, repo.id).await {
                    error!("Failed to deactivate repo {}: {de}", repo.id);
                }
                notify::send_telegram(
                    &state.bot,
                    repo.user_telegram_id,
                    &format!(
                        "⚠️ Stopped watching <b>{}/{}</b> — repository not found or no longer accessible.",
                        repo.owner, repo.repo
                    ),
                )
                .await;
            } else {
                error!("Poll error for {}/{}: {e}", repo.owner, repo.repo);
            }
        }
        Ok(events) => {
            let since = repo.last_polled;
            let current_user = repo.user_github_username.as_deref();

            // First poll: establish baseline so we don't flood with historical events.
            if since.is_none() {
                if let Err(e) = db::update_last_polled(&state.db, repo.id).await {
                    error!("Failed to set initial last_polled for {}/{}: {e}", repo.owner, repo.repo);
                }
                return;
            }

            for event in &events {
                // Parse event timestamp
                let event_time = chrono::DateTime::parse_from_rfc3339(&event.created_at)
                    .ok()
                    .map(|t| t.with_timezone(&Utc));

                // Skip events older than or equal to last_polled
                if let (Some(since), Some(event_time)) = (since, event_time) {
                    if event_time <= since {
                        continue;
                    }
                }

                // Apply per-field notification preferences
                let should_notify = match event.event_type.as_str() {
                    "IssuesEvent" => repo.notify_issues,
                    "PullRequestEvent" => repo.notify_prs,
                    "PushEvent" => repo.notify_commits,
                    "IssueCommentEvent" => repo.notify_comments,
                    _ => false,
                };

                if !should_notify {
                    continue;
                }

                if let Some(message) =
                    notify::format_polling_event(event, &repo.owner, &repo.repo, current_user)
                {
                    notify::send_telegram(&state.bot, repo.user_telegram_id, &message).await;
                }
            }

            // Update last_polled
            if let Err(e) = db::update_last_polled(&state.db, repo.id).await {
                error!("Failed to update last_polled for {}: {e}", repo.id);
            }
        }
    }
}
