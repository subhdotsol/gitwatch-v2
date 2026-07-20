use teloxide::{
    payloads::SendMessageSetters,
    prelude::Requester,
    types::{ChatId, LinkPreviewOptions, ParseMode},
    Bot,
};

use crate::github::RepoEvent;

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
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

pub(crate) fn esc_html(s: &str) -> String {
    esc(s)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::github::{EventActor, RepoEvent};

    fn polling_event(event_type: &str, actor: &str, payload: serde_json::Value) -> RepoEvent {
        RepoEvent {
            event_type: event_type.to_string(),
            actor: EventActor {
                login: actor.to_string(),
            },
            payload,
            created_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    // ── format_webhook_event ──────────────────────────────────────────────────

    #[test]
    fn webhook_issue_opened() {
        let data = serde_json::json!({
            "action": "opened",
            "issue": {
                "number": 42,
                "title": "Something is broken",
                "html_url": "https://github.com/o/r/issues/42",
                "user": { "login": "alice" }
            }
        });
        let msg = format_webhook_event("issues", &data, "o", "r").unwrap();
        assert!(msg.contains("#42"), "should contain issue number");
        assert!(msg.contains("opened"));
        assert!(msg.contains("Something is broken"));
        assert!(msg.contains("alice"));
    }

    #[test]
    fn webhook_issue_closed() {
        let data = serde_json::json!({
            "action": "closed",
            "issue": {
                "number": 7,
                "title": "Done",
                "html_url": "https://github.com/o/r/issues/7",
                "user": { "login": "bob" }
            }
        });
        let msg = format_webhook_event("issues", &data, "o", "r").unwrap();
        assert!(msg.contains("closed"));
        assert!(msg.contains("#7"));
    }

    #[test]
    fn webhook_issue_reopened_returns_none() {
        let data = serde_json::json!({
            "action": "reopened",
            "issue": { "number": 1, "title": "x", "html_url": "", "user": { "login": "a" } }
        });
        assert!(format_webhook_event("issues", &data, "o", "r").is_none());
    }

    #[test]
    fn webhook_pr_opened() {
        let data = serde_json::json!({
            "action": "opened",
            "pull_request": {
                "number": 15,
                "title": "Add feature X",
                "html_url": "https://github.com/o/r/pull/15",
                "user": { "login": "dev" },
                "merged": false
            }
        });
        let msg = format_webhook_event("pull_request", &data, "o", "r").unwrap();
        assert!(msg.contains("PR #15"));
        assert!(msg.contains("opened"));
        assert!(msg.contains("Add feature X"));
    }

    #[test]
    fn webhook_pr_merged_shows_merged_label() {
        let data = serde_json::json!({
            "action": "closed",
            "pull_request": {
                "number": 20,
                "title": "Merge me",
                "html_url": "https://github.com/o/r/pull/20",
                "user": { "login": "dev" },
                "merged": true
            }
        });
        let msg = format_webhook_event("pull_request", &data, "o", "r").unwrap();
        assert!(msg.contains("merged"), "expected 'merged' in: {msg}");
    }

    #[test]
    fn webhook_pr_closed_not_merged() {
        let data = serde_json::json!({
            "action": "closed",
            "pull_request": {
                "number": 21,
                "title": "Abandon",
                "html_url": "",
                "user": { "login": "dev" },
                "merged": false
            }
        });
        let msg = format_webhook_event("pull_request", &data, "o", "r").unwrap();
        assert!(msg.contains("closed"));
        assert!(!msg.contains("merged"));
    }

    #[test]
    fn webhook_push_single_commit() {
        let data = serde_json::json!({
            "ref": "refs/heads/main",
            "pusher": { "name": "pusher" },
            "commits": [
                { "id": "abc1234", "message": "fix: the bug", "url": "https://github.com/o/r/commit/abc1234" }
            ]
        });
        let msg = format_webhook_event("push", &data, "o", "r").unwrap();
        assert!(msg.contains("1 commit"), "got: {msg}");
        assert!(msg.contains("main"));
        assert!(msg.contains("fix: the bug"));
    }

    #[test]
    fn webhook_push_truncates_beyond_three_commits() {
        let commits: Vec<serde_json::Value> = (0..5)
            .map(|i| {
                serde_json::json!({
                    "id": format!("sha{i}"),
                    "message": format!("commit {i}"),
                    "url": format!("https://github.com/o/r/commit/sha{i}")
                })
            })
            .collect();
        let data = serde_json::json!({
            "ref": "refs/heads/dev",
            "pusher": { "name": "p" },
            "commits": commits
        });
        let msg = format_webhook_event("push", &data, "o", "r").unwrap();
        assert!(msg.contains("5 commits"));
        assert!(msg.contains("and 2 more"));
    }

    #[test]
    fn webhook_push_empty_commits_returns_none() {
        let data = serde_json::json!({
            "ref": "refs/heads/main",
            "pusher": { "name": "p" },
            "commits": []
        });
        assert!(format_webhook_event("push", &data, "o", "r").is_none());
    }

    #[test]
    fn webhook_comment_created() {
        let data = serde_json::json!({
            "action": "created",
            "comment": {
                "id": 1,
                "html_url": "https://github.com/o/r/issues/5#issuecomment-1",
                "user": { "login": "commenter" },
                "body": "Looks good to me"
            },
            "issue": {
                "number": 5,
                "title": "The issue",
                "pull_request": null
            }
        });
        let msg = format_webhook_event("issue_comment", &data, "o", "r").unwrap();
        assert!(msg.contains("New comment"));
        assert!(msg.contains("commenter"));
        assert!(msg.contains("Looks good to me"));
    }

    #[test]
    fn webhook_comment_edited_returns_none() {
        let data = serde_json::json!({
            "action": "edited",
            "comment": { "html_url": "", "user": { "login": "x" }, "body": "edited" },
            "issue": { "number": 1, "title": "t", "pull_request": null }
        });
        assert!(format_webhook_event("issue_comment", &data, "o", "r").is_none());
    }

    #[test]
    fn webhook_unknown_event_returns_none() {
        assert!(format_webhook_event("star", &serde_json::json!({}), "o", "r").is_none());
        assert!(format_webhook_event("fork", &serde_json::json!({}), "o", "r").is_none());
    }

    #[test]
    fn html_special_chars_are_escaped() {
        let data = serde_json::json!({
            "action": "opened",
            "issue": {
                "number": 1,
                "title": "<script>alert('xss')</script>",
                "html_url": "https://github.com/o/r/issues/1",
                "user": { "login": "attacker" }
            }
        });
        let msg = format_webhook_event("issues", &data, "o", "r").unwrap();
        assert!(
            !msg.contains("<script>"),
            "raw script tag should not appear"
        );
        assert!(msg.contains("&lt;script&gt;"));
    }

    // ── format_polling_event ──────────────────────────────────────────────────

    #[test]
    fn polling_issue_opened() {
        let ev = polling_event(
            "IssuesEvent",
            "alice",
            serde_json::json!({
                "action": "opened",
                "issue": { "number": 3, "title": "Bug", "html_url": "https://github.com/o/r/issues/3" }
            }),
        );
        let msg = format_polling_event(&ev, "o", "r", None).unwrap();
        assert!(msg.contains("#3"));
        assert!(msg.contains("Bug"));
        assert!(msg.contains("alice"));
    }

    #[test]
    fn polling_skips_events_from_current_user() {
        let ev = polling_event(
            "IssuesEvent",
            "myself",
            serde_json::json!({
                "action": "opened",
                "issue": { "number": 1, "title": "My thing", "html_url": "" }
            }),
        );
        assert!(format_polling_event(&ev, "o", "r", Some("myself")).is_none());
    }

    #[test]
    fn polling_case_insensitive_user_skip() {
        let ev = polling_event(
            "IssuesEvent",
            "MyUser",
            serde_json::json!({
                "action": "opened",
                "issue": { "number": 1, "title": "t", "html_url": "" }
            }),
        );
        assert!(format_polling_event(&ev, "o", "r", Some("myuser")).is_none());
    }

    #[test]
    fn polling_pr_merged() {
        let ev = polling_event(
            "PullRequestEvent",
            "dev",
            serde_json::json!({
                "action": "closed",
                "pull_request": {
                    "number": 8,
                    "title": "Merge me",
                    "html_url": "https://github.com/o/r/pull/8",
                    "merged": true
                }
            }),
        );
        let msg = format_polling_event(&ev, "o", "r", None).unwrap();
        assert!(msg.contains("merged"));
    }

    #[test]
    fn polling_push_event() {
        let ev = polling_event(
            "PushEvent",
            "dev",
            serde_json::json!({
                "ref": "refs/heads/main",
                "commits": [
                    { "sha": "abc1234", "message": "chore: update deps" }
                ]
            }),
        );
        let msg = format_polling_event(&ev, "o", "r", None).unwrap();
        assert!(msg.contains("1 commit"));
        assert!(msg.contains("main"));
        assert!(msg.contains("chore: update deps"));
    }

    #[test]
    fn polling_push_empty_returns_none() {
        let ev = polling_event(
            "PushEvent",
            "dev",
            serde_json::json!({
                "ref": "refs/heads/main",
                "commits": []
            }),
        );
        assert!(format_polling_event(&ev, "o", "r", None).is_none());
    }

    #[test]
    fn polling_unknown_event_returns_none() {
        let ev = polling_event("WatchEvent", "user", serde_json::json!({}));
        assert!(format_polling_event(&ev, "o", "r", None).is_none());
    }

    // ── HTML escaping helper ──────────────────────────────────────────────────

    #[test]
    fn esc_html_escapes_all_special_chars() {
        assert_eq!(esc_html("a & b"), "a &amp; b");
        assert_eq!(esc_html("<tag>"), "&lt;tag&gt;");
        assert_eq!(esc_html("a<b>&c"), "a&lt;b&gt;&amp;c");
        assert_eq!(esc_html("plain text"), "plain text");
        assert_eq!(esc_html(""), "");
    }
}
