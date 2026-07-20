use teloxide::{
    prelude::Requester,
    payloads::SendMessageSetters,
    types::{ChatId, LinkPreviewOptions, ParseMode},
    Bot,
};

use crate::github::RepoEvent;

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

pub fn format_webhook_event(
    event: &str,
    data: &serde_json::Value,
    owner: &str,
    repo: &str,
) -> Option<String> {
    let repo_link = format!(
        "<a href=\"https://github.com/{}/{}\">{}/{}</a>",
        esc(owner),
        esc(repo),
        esc(owner),
        esc(repo)
    );

    match event {
        "issues" => {
            let action = data["action"].as_str()?;
            let issue = &data["issue"];
            let title = issue["title"].as_str().unwrap_or("(no title)");
            let url = issue["html_url"].as_str().unwrap_or("");
            let number = issue["number"].as_u64().unwrap_or(0);
            let user = issue["user"]["login"].as_str().unwrap_or("unknown");

            let emoji = match action {
                "opened" => "🐛",
                "closed" => "✅",
                "assigned" => "👤",
                _ => return None,
            };

            Some(format!(
                "{emoji} <b>Issue #{number} {action}</b> on {repo_link}\n\
                 <a href=\"{url}\">{title}</a>\n\
                 by <b>{}</b>",
                esc(user),
                title = esc(title),
                url = url,
            ))
        }
        "pull_request" => {
            let action = data["action"].as_str()?;
            let pr = &data["pull_request"];
            let title = pr["title"].as_str().unwrap_or("(no title)");
            let url = pr["html_url"].as_str().unwrap_or("");
            let number = pr["number"].as_u64().unwrap_or(0);
            let user = pr["user"]["login"].as_str().unwrap_or("unknown");
            let merged = pr["merged"].as_bool().unwrap_or(false);

            let (emoji, label) = match action {
                "opened" => ("🔀", "opened"),
                "closed" => {
                    if merged {
                        ("🟣", "merged")
                    } else {
                        ("❌", "closed")
                    }
                }
                "assigned" => ("👤", "assigned"),
                _ => return None,
            };

            Some(format!(
                "{emoji} <b>PR #{number} {label}</b> on {repo_link}\n\
                 <a href=\"{url}\">{title}</a>\n\
                 by <b>{}</b>",
                esc(user),
                title = esc(title),
                url = url,
            ))
        }
        "push" => {
            let commits = data["commits"].as_array()?;
            if commits.is_empty() {
                return None;
            }
            let pusher = data["pusher"]["name"]
                .as_str()
                .or_else(|| data["sender"]["login"].as_str())
                .unwrap_or("unknown");
            let branch = data["ref"]
                .as_str()
                .unwrap_or("")
                .trim_start_matches("refs/heads/");
            let count = commits.len();

            let mut lines = vec![format!(
                "📦 <b>{count} commit{s} pushed</b> to {repo_link} (<code>{branch}</code>) by <b>{pusher}</b>",
                s = if count == 1 { "" } else { "s" },
                pusher = esc(pusher),
                branch = esc(branch),
            )];

            for c in commits.iter().take(3) {
                let msg = c["message"]
                    .as_str()
                    .unwrap_or("")
                    .lines()
                    .next()
                    .unwrap_or("");
                let sha = c["id"].as_str().unwrap_or("");
                let short_sha = &sha[..sha.len().min(7)];
                let url = c["url"].as_str().unwrap_or("");
                lines.push(format!(
                    "• <a href=\"{url}\"><code>{short_sha}</code></a> {}",
                    esc(msg)
                ));
            }

            if count > 3 {
                lines.push(format!("… and {} more", count - 3));
            }

            Some(lines.join("\n"))
        }
        "issue_comment" => {
            let action = data["action"].as_str()?;
            if action != "created" {
                return None;
            }
            let comment = &data["comment"];
            let issue = &data["issue"];
            let user = comment["user"]["login"].as_str().unwrap_or("unknown");
            let url = comment["html_url"].as_str().unwrap_or("");
            let issue_title = issue["title"].as_str().unwrap_or("(no title)");
            let issue_num = issue["number"].as_u64().unwrap_or(0);
            let body = comment["body"]
                .as_str()
                .unwrap_or("")
                .chars()
                .take(200)
                .collect::<String>();

            Some(format!(
                "💬 <b>New comment</b> on <a href=\"{url}\">#{issue_num}: {issue_title}</a> in {repo_link}\n\
                 by <b>{}</b>\n\
                 {}",
                esc(user),
                esc(&body),
                issue_title = esc(issue_title),
                url = url,
            ))
        }
        _ => None,
    }
}

pub fn format_polling_event(
    event: &RepoEvent,
    owner: &str,
    repo: &str,
    current_user: Option<&str>,
) -> Option<String> {
    let repo_link = format!(
        "<a href=\"https://github.com/{}/{}\">{}/{}</a>",
        esc(owner),
        esc(repo),
        esc(owner),
        esc(repo)
    );

    // Skip events from the current user (the watcher)
    if let Some(me) = current_user {
        if event.actor.login.eq_ignore_ascii_case(me) {
            return None;
        }
    }

    let payload = &event.payload;

    match event.event_type.as_str() {
        "IssuesEvent" => {
            let action = payload["action"].as_str()?;
            let issue = &payload["issue"];
            let title = issue["title"].as_str().unwrap_or("(no title)");
            let url = issue["html_url"].as_str().unwrap_or("");
            let number = issue["number"].as_u64().unwrap_or(0);

            let emoji = match action {
                "opened" => "🐛",
                "closed" => "✅",
                "assigned" => "👤",
                _ => return None,
            };

            Some(format!(
                "{emoji} <b>Issue #{number} {action}</b> on {repo_link}\n\
                 <a href=\"{url}\">{}</a>\n\
                 by <b>{}</b>",
                esc(title),
                esc(&event.actor.login),
                url = url,
            ))
        }
        "PullRequestEvent" => {
            let action = payload["action"].as_str()?;
            let pr = &payload["pull_request"];
            let title = pr["title"].as_str().unwrap_or("(no title)");
            let url = pr["html_url"].as_str().unwrap_or("");
            let number = pr["number"].as_u64().unwrap_or(0);
            let merged = pr["merged"].as_bool().unwrap_or(false);

            let (emoji, label) = match action {
                "opened" => ("🔀", "opened"),
                "closed" => {
                    if merged {
                        ("🟣", "merged")
                    } else {
                        ("❌", "closed")
                    }
                }
                _ => return None,
            };

            Some(format!(
                "{emoji} <b>PR #{number} {label}</b> on {repo_link}\n\
                 <a href=\"{url}\">{}</a>\n\
                 by <b>{}</b>",
                esc(title),
                esc(&event.actor.login),
                url = url,
            ))
        }
        "PushEvent" => {
            let commits = payload["commits"].as_array()?;
            if commits.is_empty() {
                return None;
            }
            let count = commits.len();
            let branch = payload["ref"]
                .as_str()
                .unwrap_or("")
                .trim_start_matches("refs/heads/");

            let mut lines = vec![format!(
                "📦 <b>{count} commit{s} pushed</b> to {repo_link} (<code>{branch}</code>) by <b>{}</b>",
                esc(&event.actor.login),
                s = if count == 1 { "" } else { "s" },
                branch = esc(branch),
            )];

            for c in commits.iter().take(3) {
                let msg = c["message"]
                    .as_str()
                    .unwrap_or("")
                    .lines()
                    .next()
                    .unwrap_or("");
                let sha = c["sha"].as_str().unwrap_or("");
                let short_sha = &sha[..sha.len().min(7)];
                lines.push(format!("• <code>{short_sha}</code> {}", esc(msg)));
            }

            if count > 3 {
                lines.push(format!("… and {} more", count - 3));
            }

            Some(lines.join("\n"))
        }
        "IssueCommentEvent" => {
            let action = payload["action"].as_str()?;
            if action != "created" {
                return None;
            }
            let comment = &payload["comment"];
            let issue = &payload["issue"];
            let url = comment["html_url"].as_str().unwrap_or("");
            let issue_title = issue["title"].as_str().unwrap_or("(no title)");
            let issue_num = issue["number"].as_u64().unwrap_or(0);
            let body = comment["body"]
                .as_str()
                .unwrap_or("")
                .chars()
                .take(200)
                .collect::<String>();

            Some(format!(
                "💬 <b>New comment</b> on <a href=\"{url}\">#{issue_num}: {}</a> in {repo_link}\n\
                 by <b>{}</b>\n\
                 {}",
                esc(issue_title),
                esc(&event.actor.login),
                esc(&body),
                url = url,
            ))
        }
        _ => None,
    }
}

pub async fn send_telegram(bot: &Bot, telegram_id: i64, html: &str) {
    let _ = bot
        .send_message(ChatId(telegram_id), html)
        .parse_mode(ParseMode::Html)
        .link_preview_options(LinkPreviewOptions {
            is_disabled: true,
            url: None,
            prefer_small_media: false,
            prefer_large_media: false,
            show_above_text: false,
        })
        .await;
}
