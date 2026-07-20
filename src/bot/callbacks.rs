use teloxide::{
    prelude::*,
    types::{InlineKeyboardButton, InlineKeyboardMarkup},
};
use uuid::Uuid;

use crate::{db, models::WatchedRepo, state::AppState};

pub fn preferences_keyboard(repo: &WatchedRepo) -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new(vec![
        vec![
            InlineKeyboardButton::callback(
                format!("{} Issues", if repo.notify_issues { "✅" } else { "❌" }),
                format!("notify:issues:{}", repo.id),
            ),
            InlineKeyboardButton::callback(
                format!("{} PRs", if repo.notify_prs { "✅" } else { "❌" }),
                format!("notify:prs:{}", repo.id),
            ),
        ],
        vec![
            InlineKeyboardButton::callback(
                format!("{} Commits", if repo.notify_commits { "✅" } else { "❌" }),
                format!("notify:commits:{}", repo.id),
            ),
            InlineKeyboardButton::callback(
                format!(
                    "{} Comments",
                    if repo.notify_comments { "✅" } else { "❌" }
                ),
                format!("notify:comments:{}", repo.id),
            ),
        ],
    ])
}

pub async fn handle_callback(bot: Bot, q: CallbackQuery, state: AppState) -> anyhow::Result<()> {
    let data = match &q.data {
        Some(d) => d.clone(),
        None => {
            bot.answer_callback_query(&q.id).await?;
            return Ok(());
        }
    };

    let parts: Vec<&str> = data.splitn(3, ':').collect();

    if parts.len() < 2 {
        bot.answer_callback_query(&q.id)
            .text("Unknown action")
            .await?;
        return Ok(());
    }

    match parts[0] {
        "notify" => {
            if parts.len() < 3 {
                bot.answer_callback_query(&q.id)
                    .text("Invalid callback")
                    .await?;
                return Ok(());
            }
            let field = parts[1];
            let repo_id = match Uuid::parse_str(parts[2]) {
                Ok(id) => id,
                Err(_) => {
                    bot.answer_callback_query(&q.id)
                        .text("Invalid repo ID")
                        .await?;
                    return Ok(());
                }
            };

            match db::toggle_notify_field(&state.db, repo_id, field).await {
                Ok(updated_repo) => {
                    let keyboard = preferences_keyboard(&updated_repo);
                    let status = match field {
                        "issues" => {
                            if updated_repo.notify_issues {
                                "Issues notifications enabled"
                            } else {
                                "Issues notifications disabled"
                            }
                        }
                        "prs" => {
                            if updated_repo.notify_prs {
                                "PR notifications enabled"
                            } else {
                                "PR notifications disabled"
                            }
                        }
                        "commits" => {
                            if updated_repo.notify_commits {
                                "Commit notifications enabled"
                            } else {
                                "Commit notifications disabled"
                            }
                        }
                        "comments" => {
                            if updated_repo.notify_comments {
                                "Comment notifications enabled"
                            } else {
                                "Comment notifications disabled"
                            }
                        }
                        _ => "Updated",
                    };

                    if let Some(msg) = &q.message {
                        let _ = bot
                            .edit_message_reply_markup(msg.chat().id, msg.id())
                            .reply_markup(keyboard)
                            .await;
                    }

                    bot.answer_callback_query(&q.id).text(status).await?;
                }
                Err(e) => {
                    tracing::error!("Failed to toggle notify field: {e}");
                    bot.answer_callback_query(&q.id)
                        .text("Failed to update setting")
                        .await?;
                }
            }
        }
        "manage" => {
            if parts.len() < 2 {
                bot.answer_callback_query(&q.id)
                    .text("Invalid callback")
                    .await?;
                return Ok(());
            }
            let repo_id = match Uuid::parse_str(parts[1]) {
                Ok(id) => id,
                Err(_) => {
                    bot.answer_callback_query(&q.id)
                        .text("Invalid repo ID")
                        .await?;
                    return Ok(());
                }
            };

            match db::find_watched_repo_by_id(&state.db, repo_id).await {
                Ok(Some(repo)) => {
                    let keyboard = preferences_keyboard(&repo);
                    let text = format!(
                        "⚙️ <b>Preferences for {}/{}</b>\n\nToggle notifications:",
                        repo.owner, repo.repo
                    );

                    if let Some(msg) = &q.message {
                        let _ = bot
                            .edit_message_text(msg.chat().id, msg.id(), &text)
                            .parse_mode(teloxide::types::ParseMode::Html)
                            .reply_markup(keyboard)
                            .await;
                    }

                    bot.answer_callback_query(&q.id).await?;
                }
                Ok(None) => {
                    bot.answer_callback_query(&q.id)
                        .text("Repo not found")
                        .await?;
                }
                Err(e) => {
                    tracing::error!("Failed to find repo: {e}");
                    bot.answer_callback_query(&q.id)
                        .text("Error loading repo")
                        .await?;
                }
            }
        }
        _ => {
            bot.answer_callback_query(&q.id)
                .text("Unknown action")
                .await?;
        }
    }

    Ok(())
}
