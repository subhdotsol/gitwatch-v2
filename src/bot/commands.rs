use std::time::Duration;

use chrono::Utc;
use teloxide::{prelude::*, types::ParseMode, utils::command::BotCommands};

use crate::{
    bot::{callbacks::preferences_keyboard, notify::send_telegram},
    db, github, limits,
    state::AppState,
};

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase", description = "GitWatch commands")]
pub enum Command {
    #[command(description = "Connect your GitHub account")]
    Start,
    #[command(description = "Watch a repository")]
    Watch(String),
    #[command(description = "View all watched repositories")]
    Watchlist,
    #[command(description = "Stop watching a repository")]
    Unwatch(String),
    #[command(description = "Check your plan and usage")]
    Status,
    #[command(description = "Disconnect GitHub and remove all watches")]
    Disconnect,
    #[command(description = "Show help message")]
    Help,
    // Admin commands
    #[command(description = "Admin: Approve premium")]
    Approve(String),
    #[command(description = "Admin: Downgrade user")]
    Downgrade(String),
    #[command(description = "Admin: Reject payment")]
    Reject(String),
    #[command(description = "Admin: View stats")]
    Stats,
}

fn is_admin(state: &AppState, telegram_id: i64) -> bool {
    state
        .config
        .admin_telegram_id
        .map(|id| id == telegram_id)
        .unwrap_or(false)
}

fn parse_owner_repo(input: &str) -> Option<(String, String)> {
    // Strip URL prefix if present
    let cleaned = input
        .trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_start_matches("github.com/")
        .trim_end_matches('/');

    // Strip .git suffix
    let cleaned = cleaned.trim_end_matches(".git");

    // Split by /
    let parts: Vec<&str> = cleaned.splitn(2, '/').collect();
    if parts.len() == 2 && !parts[0].is_empty() && !parts[1].is_empty() {
        Some((parts[0].to_string(), parts[1].to_string()))
    } else {
        None
    }
}

fn parse_user_target(input: &str) -> String {
    // Accept @username or plain username or numeric id
    input.trim().trim_start_matches('@').to_string()
}

pub async fn handle_command(
    bot: Bot,
    msg: Message,
    cmd: Command,
    state: AppState,
) -> anyhow::Result<()> {
    let from = match msg.from.as_ref() {
        Some(u) => u,
        None => return Ok(()),
    };
    let telegram_id = from.id.0 as i64;
    let username = from.username.as_deref();

    match cmd {
        Command::Start => {
            // Upsert user
            let user = match db::upsert_user(&state.db, telegram_id, username).await {
                Ok(u) => u,
                Err(e) => {
                    tracing::error!("upsert_user error: {e}");
                    bot.send_message(msg.chat.id, "❌ Internal error, please try again.")
                        .await?;
                    return Ok(());
                }
            };

            // Check for payload after "/start "
            let text = msg.text().unwrap_or("");
            let payload = text.strip_prefix("/start ").unwrap_or("").trim();

            if payload == "connected" && user.github_username.is_some() {
                let gh_user = user.github_username.as_deref().unwrap_or("");
                bot.send_message(
                    msg.chat.id,
                    format!(
                        "✅ Welcome back! You're connected as <b>{}</b>.\n\nUse /watch owner/repo to start watching.",
                        gh_user
                    ),
                )
                .parse_mode(ParseMode::Html)
                .await?;
                return Ok(());
            }

            let display_name = from
                .username
                .as_deref()
                .map(|u| format!("@{u}"))
                .unwrap_or_else(|| from.first_name.clone());

            let auth_url = format!(
                "{}/api/auth/github?telegram_id={}",
                state.config.app_url, telegram_id
            );

            bot.send_message(
                msg.chat.id,
                format!(
                    "👋 Welcome to GitWatch, <b>{}</b>!\n\n\
                     Connect your GitHub account:\n\
                     <a href=\"{auth_url}\">Authorize GitHub</a>\n\n\
                     Features:\n\
                     • Real-time notifications via webhooks\n\
                     • Per-repo notification preferences\n\
                     • Polling fallback for private repos\n\n\
                     Free plan: watch up to {} repositories.",
                    display_name,
                    limits::repo_limit("free")
                ),
            )
            .parse_mode(ParseMode::Html)
            .await?;
        }

        Command::Watch(input) => {
            // Rate limit: 5 per 60s per user
            let rl = state.rate_limiter.check(
                &format!("watch:{telegram_id}"),
                5,
                Duration::from_secs(60),
            );
            if !rl.allowed {
                bot.send_message(
                    msg.chat.id,
                    format!("⏳ Slow down! Try again in {}s.", rl.reset_in_ms / 1000 + 1),
                )
                .await?;
                return Ok(());
            }

            let user = match db::find_user_by_telegram_id(&state.db, telegram_id).await? {
                Some(u) => u,
                None => {
                    bot.send_message(msg.chat.id, "Please use /start first.")
                        .await?;
                    return Ok(());
                }
            };

            if user.github_token.is_none() {
                bot.send_message(
                    msg.chat.id,
                    format!(
                        "❌ GitHub not connected. Use /start to connect.\n{}/api/auth/github?telegram_id={}",
                        state.config.app_url, telegram_id
                    ),
                )
                .await?;
                return Ok(());
            }

            // Check repo limit
            let count = db::count_active_repos_for_user(&state.db, user.id).await?;
            let limit = limits::repo_limit(&user.plan) as i64;
            if count >= limit {
                bot.send_message(
                    msg.chat.id,
                    format!(
                        "❌ You've reached your {} plan limit of {} repos.\n\
                         Unwatch a repo first with /unwatch, or upgrade to Premium for more.",
                        limits::plan_display_name(&user.plan),
                        limit
                    ),
                )
                .await?;
                return Ok(());
            }

            let (owner, repo) = match parse_owner_repo(&input) {
                Some(r) => r,
                None => {
                    bot.send_message(
                        msg.chat.id,
                        "❌ Invalid format. Use: /watch owner/repo or /watch https://github.com/owner/repo",
                    )
                    .await?;
                    return Ok(());
                }
            };

            // Send processing message
            let processing_msg = bot
                .send_message(msg.chat.id, format!("⏳ Processing {owner}/{repo}..."))
                .await?;

            // Check if already watching
            if let Some(existing) = db::find_watched_repo(&state.db, user.id, &owner, &repo).await?
            {
                if existing.active {
                    let keyboard = preferences_keyboard(&existing);
                    bot.edit_message_text(
                        msg.chat.id,
                        processing_msg.id,
                        format!(
                            "ℹ️ Already watching <b>{owner}/{repo}</b>.\n\nManage preferences:",
                        ),
                    )
                    .parse_mode(ParseMode::Html)
                    .reply_markup(keyboard)
                    .await?;
                    return Ok(());
                }
            }

            let token = user.github_token.as_deref().unwrap();

            // Verify repo exists
            let gh_repo = match github::get_repo(&state.http_client, token, &owner, &repo).await {
                Ok(r) => r,
                Err(e) => {
                    bot.edit_message_text(
                        msg.chat.id,
                        processing_msg.id,
                        format!("❌ Could not access {owner}/{repo}: {e}"),
                    )
                    .await?;
                    return Ok(());
                }
            };

            if gh_repo.private {
                bot.edit_message_text(
                    msg.chat.id,
                    processing_msg.id,
                    "❌ Private repositories are not supported on the free plan.",
                )
                .await?;
                return Ok(());
            }

            // Determine watch mode
            let has_admin = gh_repo
                .permissions
                .as_ref()
                .map(|p| p.admin || p.push)
                .unwrap_or(false);

            let (webhook_id, watch_mode) = if has_admin {
                let webhook_url = format!("{}/api/webhooks/github", state.config.app_url);
                let wh_id = github::create_webhook(
                    &state.http_client,
                    token,
                    &owner,
                    &repo,
                    &webhook_url,
                    &state.config.github_webhook_secret,
                )
                .await
                .unwrap_or(None);
                if wh_id.is_some() {
                    (wh_id, "webhook")
                } else {
                    (None, "polling")
                }
            } else {
                (None, "polling")
            };

            let watched =
                db::create_watched_repo(&state.db, user.id, &owner, &repo, webhook_id, watch_mode)
                    .await?;

            let mode_label = if watch_mode == "webhook" {
                "Real-time (webhook)"
            } else {
                "Polling (every 5 min)"
            };

            let keyboard = preferences_keyboard(&watched);

            bot.edit_message_text(
                msg.chat.id,
                processing_msg.id,
                format!(
                    "✅ Now watching <b>{owner}/{repo}</b>\n\
                     Mode: {mode_label}\n\n\
                     Select notification preferences:"
                ),
            )
            .parse_mode(ParseMode::Html)
            .reply_markup(keyboard)
            .await?;
        }

        Command::Watchlist => {
            let user = match db::find_user_by_telegram_id(&state.db, telegram_id).await? {
                Some(u) => u,
                None => {
                    bot.send_message(msg.chat.id, "Please use /start first.")
                        .await?;
                    return Ok(());
                }
            };

            let repos = db::get_user_watched_repos(&state.db, user.id).await?;

            if repos.is_empty() {
                bot.send_message(
                    msg.chat.id,
                    "You're not watching any repositories.\n\nUse /watch owner/repo to start.",
                )
                .await?;
                return Ok(());
            }

            let mut lines = vec![format!(
                "📋 <b>Your watched repos</b> ({}/{} used):\n",
                repos.len(),
                limits::repo_limit(&user.plan)
            )];

            for (i, r) in repos.iter().enumerate() {
                let mode_icon = if r.watch_mode == "webhook" {
                    "⚡"
                } else {
                    "🔄"
                };
                lines.push(format!(
                    "{}. {mode_icon} <code>{}/{}</code>",
                    i + 1,
                    r.owner,
                    r.repo
                ));
            }

            lines.push("\n⚡ = webhook  🔄 = polling".to_string());

            bot.send_message(msg.chat.id, lines.join("\n"))
                .parse_mode(ParseMode::Html)
                .await?;
        }

        Command::Unwatch(input) => {
            let user = match db::find_user_by_telegram_id(&state.db, telegram_id).await? {
                Some(u) => u,
                None => {
                    bot.send_message(msg.chat.id, "Please use /start first.")
                        .await?;
                    return Ok(());
                }
            };

            let (owner, repo) = match parse_owner_repo(&input) {
                Some(r) => r,
                None => {
                    bot.send_message(msg.chat.id, "❌ Invalid format. Use: /unwatch owner/repo")
                        .await?;
                    return Ok(());
                }
            };

            let watched = match db::find_watched_repo(&state.db, user.id, &owner, &repo).await? {
                Some(r) if r.active => r,
                _ => {
                    bot.send_message(
                        msg.chat.id,
                        format!("❌ You're not watching {owner}/{repo}."),
                    )
                    .await?;
                    return Ok(());
                }
            };

            // Delete webhook if applicable
            if let (Some(webhook_id), Some(token)) = (watched.webhook_id, &user.github_token) {
                if let Err(e) =
                    github::delete_webhook(&state.http_client, token, &owner, &repo, webhook_id)
                        .await
                {
                    tracing::warn!("Failed to delete webhook {webhook_id}: {e}");
                }
            }

            db::deactivate_watched_repo(&state.db, watched.id).await?;

            bot.send_message(
                msg.chat.id,
                format!("✅ Stopped watching <b>{owner}/{repo}</b>."),
            )
            .parse_mode(ParseMode::Html)
            .await?;
        }

        Command::Status => {
            let user = match db::find_user_by_telegram_id(&state.db, telegram_id).await? {
                Some(u) => u,
                None => {
                    bot.send_message(msg.chat.id, "No account found. Use /start to get started.")
                        .await?;
                    return Ok(());
                }
            };

            if user.github_token.is_none() {
                bot.send_message(
                    msg.chat.id,
                    format!(
                        "❌ GitHub not connected.\n\nConnect here: {}/api/auth/github?telegram_id={}",
                        state.config.app_url, telegram_id
                    ),
                )
                .await?;
                return Ok(());
            }

            let count = db::count_active_repos_for_user(&state.db, user.id).await?;
            let limit = limits::repo_limit(&user.plan);
            let plan_name = limits::plan_display_name(&user.plan);
            let gh_username = user.github_username.as_deref().unwrap_or("unknown");

            let expires_str = user
                .plan_expires_at
                .map(|e| format!("\nPlan expires: {}", e.format("%Y-%m-%d")))
                .unwrap_or_default();

            bot.send_message(
                msg.chat.id,
                format!(
                    "📊 <b>Your Status</b>\n\n\
                     GitHub: <b>{gh_username}</b>\n\
                     Plan: <b>{plan_name}</b>{expires_str}\n\
                     Repos: <b>{count}/{limit}</b>",
                ),
            )
            .parse_mode(ParseMode::Html)
            .await?;
        }

        Command::Disconnect => {
            let user = match db::find_user_by_telegram_id(&state.db, telegram_id).await? {
                Some(u) => u,
                None => {
                    bot.send_message(msg.chat.id, "No account found.").await?;
                    return Ok(());
                }
            };

            // Delete all webhooks first
            let repos = db::get_user_watched_repos(&state.db, user.id).await?;
            for r in repos {
                if let (Some(webhook_id), Some(token)) = (r.webhook_id, &user.github_token) {
                    if let Err(e) = github::delete_webhook(
                        &state.http_client,
                        token,
                        &r.owner,
                        &r.repo,
                        webhook_id,
                    )
                    .await
                    {
                        tracing::warn!("Failed to delete webhook {webhook_id}: {e}");
                    }
                }
            }

            db::delete_all_user_repos(&state.db, user.id).await?;
            db::disconnect_user_github(&state.db, user.id).await?;

            bot.send_message(
                msg.chat.id,
                "✅ Disconnected GitHub. All watches have been removed.\n\nUse /start to reconnect.",
            )
            .await?;
        }

        Command::Help => {
            bot.send_message(
                msg.chat.id,
                "<b>GitWatch Commands</b>\n\n\
                 /start — Connect your GitHub account\n\
                 /watch owner/repo — Start watching a repository\n\
                 /watchlist — View all watched repositories\n\
                 /unwatch owner/repo — Stop watching a repository\n\
                 /status — Check your plan and usage\n\
                 /disconnect — Remove GitHub connection and all watches\n\
                 /help — Show this message\n\n\
                 <b>Watch Modes</b>\n\
                 ⚡ Webhook — Real-time notifications (requires repo admin/push access)\n\
                 🔄 Polling — Checked every 5 minutes (always available)",
            )
            .parse_mode(ParseMode::Html)
            .await?;
        }

        // Admin commands
        Command::Approve(input) => {
            if !is_admin(&state, telegram_id) {
                bot.send_message(msg.chat.id, "❌ Admin only.").await?;
                return Ok(());
            }

            let target = parse_user_target(&input);
            let user = if let Ok(id) = target.parse::<i64>() {
                db::find_user_by_telegram_id_exact(&state.db, id).await?
            } else {
                db::find_user_by_username(&state.db, &target).await?
            };

            match user {
                None => {
                    bot.send_message(msg.chat.id, format!("❌ User not found: {target}"))
                        .await?;
                }
                Some(u) => {
                    let expires = Utc::now() + chrono::Duration::days(30);
                    db::set_user_plan(&state.db, u.id, "premium", Some(expires)).await?;

                    // Notify user
                    send_telegram(
                        &bot,
                        u.telegram_id,
                        &format!(
                            "🎉 <b>Premium activated!</b>\n\nYour account has been upgraded to Premium.\n\
                             You can now watch up to {} repositories.\nExpires: {}",
                            limits::repo_limit("premium"),
                            expires.format("%Y-%m-%d")
                        ),
                    )
                    .await;

                    bot.send_message(
                        msg.chat.id,
                        format!(
                            "✅ Approved premium for {target} until {}",
                            expires.format("%Y-%m-%d")
                        ),
                    )
                    .await?;
                }
            }
        }

        Command::Downgrade(input) => {
            if !is_admin(&state, telegram_id) {
                bot.send_message(msg.chat.id, "❌ Admin only.").await?;
                return Ok(());
            }

            let target = parse_user_target(&input);
            let user = if let Ok(id) = target.parse::<i64>() {
                db::find_user_by_telegram_id_exact(&state.db, id).await?
            } else {
                db::find_user_by_username(&state.db, &target).await?
            };

            match user {
                None => {
                    bot.send_message(msg.chat.id, format!("❌ User not found: {target}"))
                        .await?;
                }
                Some(u) => {
                    db::set_user_plan(&state.db, u.id, "free", None).await?;

                    send_telegram(
                        &bot,
                        u.telegram_id,
                        "ℹ️ Your account has been downgraded to the Free plan.\n\
                         You can watch up to 2 repositories.",
                    )
                    .await;

                    bot.send_message(msg.chat.id, format!("✅ Downgraded {target} to free plan."))
                        .await?;
                }
            }
        }

        Command::Reject(input) => {
            if !is_admin(&state, telegram_id) {
                bot.send_message(msg.chat.id, "❌ Admin only.").await?;
                return Ok(());
            }

            // Format: /reject @username reason text here
            let parts: Vec<&str> = input.splitn(2, ' ').collect();
            let target_str = parts[0].trim();
            let reason = if parts.len() > 1 { parts[1].trim() } else { "" };

            let target = parse_user_target(target_str);
            let user = if let Ok(id) = target.parse::<i64>() {
                db::find_user_by_telegram_id_exact(&state.db, id).await?
            } else {
                db::find_user_by_username(&state.db, &target).await?
            };

            match user {
                None => {
                    bot.send_message(msg.chat.id, format!("❌ User not found: {target}"))
                        .await?;
                }
                Some(u) => {
                    let reason_text = if reason.is_empty() {
                        "No reason provided.".to_string()
                    } else {
                        reason.to_string()
                    };

                    send_telegram(
                        &bot,
                        u.telegram_id,
                        &format!(
                            "❌ <b>Payment rejected</b>\n\nYour premium upgrade request was not approved.\n\
                             Reason: {reason_text}\n\nPlease contact support if you have questions."
                        ),
                    )
                    .await;

                    bot.send_message(msg.chat.id, format!("✅ Rejection sent to {target}."))
                        .await?;
                }
            }
        }

        Command::Stats => {
            if !is_admin(&state, telegram_id) {
                bot.send_message(msg.chat.id, "❌ Admin only.").await?;
                return Ok(());
            }

            let stats = db::get_platform_stats(&state.db).await?;
            let premium_count = db::count_premium_users(&state.db).await?;

            bot.send_message(
                msg.chat.id,
                format!(
                    "📊 <b>Platform Stats</b>\n\n\
                     Total users: <b>{}</b>\n\
                     Premium users: <b>{}</b>\n\
                     Total repos: <b>{}</b>\n\
                     Active repos: <b>{}</b>",
                    stats.total_users, premium_count, stats.total_repos, stats.active_repos
                ),
            )
            .parse_mode(ParseMode::Html)
            .await?;
        }
    }

    Ok(())
}
