use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::{bot::notify, db, state::AppState};

type HmacSha256 = Hmac<Sha256>;

pub(crate) fn verify_signature(body: &[u8], signature: &str, secret: &str) -> bool {
    let sig = match signature.strip_prefix("sha256=") {
        Some(s) => s,
        None => return false,
    };

    let mut mac = match HmacSha256::new_from_slice(secret.as_bytes()) {
        Ok(m) => m,
        Err(_) => return false,
    };
    mac.update(body);
    let expected = mac.finalize().into_bytes();
    let expected_hex = hex::encode(expected);

    // Constant-time comparison
    expected_hex == sig
}

pub async fn github_webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    // Verify signature
    let signature = match headers
        .get("x-hub-signature-256")
        .and_then(|v| v.to_str().ok())
    {
        Some(s) => s.to_string(),
        None => {
            tracing::warn!("Missing x-hub-signature-256 header");
            return (StatusCode::UNAUTHORIZED, "Missing signature").into_response();
        }
    };

    if !verify_signature(&body, &signature, &state.config.github_webhook_secret) {
        tracing::warn!("Invalid webhook signature");
        return (StatusCode::UNAUTHORIZED, "Invalid signature").into_response();
    }

    // Get event type
    let event = match headers.get("x-github-event").and_then(|v| v.to_str().ok()) {
        Some(e) => e.to_string(),
        None => {
            return (StatusCode::BAD_REQUEST, "Missing x-github-event header").into_response();
        }
    };

    // Parse body
    let data: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(d) => d,
        Err(e) => {
            tracing::error!("Failed to parse webhook body: {e}");
            return (StatusCode::BAD_REQUEST, "Invalid JSON").into_response();
        }
    };

    // Extract repo info
    let owner = data["repository"]["owner"]["login"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let repo = data["repository"]["name"]
        .as_str()
        .unwrap_or("")
        .to_string();

    if owner.is_empty() || repo.is_empty() {
        return (StatusCode::OK, "ok").into_response();
    }

    // Find all watching users
    let watchers = match db::find_repos_by_owner_repo(&state.db, &owner, &repo).await {
        Ok(w) => w,
        Err(e) => {
            tracing::error!("DB error finding watchers: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "DB error").into_response();
        }
    };

    // Filter based on event type and user notification prefs
    for watcher in watchers {
        // Check per-event notification preferences
        let should_notify = match event.as_str() {
            "issues" => watcher.notify_issues,
            "pull_request" => watcher.notify_prs,
            "push" => watcher.notify_commits,
            "issue_comment" => watcher.notify_comments,
            _ => false,
        };

        if !should_notify {
            continue;
        }

        if let Some(message) = notify::format_webhook_event(&event, &data, &owner, &repo) {
            notify::send_telegram(&state.bot, watcher.user_telegram_id, &message).await;
        }
    }

    (StatusCode::OK, "ok").into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    fn make_sig(body: &[u8], secret: &str) -> String {
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body);
        format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
    }

    #[test]
    fn valid_signature_accepted() {
        let body = b"hello gitwatch";
        let sig = make_sig(body, "my-secret");
        assert!(verify_signature(body, &sig, "my-secret"));
    }

    #[test]
    fn wrong_secret_rejected() {
        let body = b"hello gitwatch";
        let sig = make_sig(body, "correct-secret");
        assert!(!verify_signature(body, &sig, "wrong-secret"));
    }

    #[test]
    fn missing_sha256_prefix_rejected() {
        // Raw hex without "sha256=" prefix
        let raw_hex = hex::encode(b"not-a-real-mac");
        assert!(!verify_signature(b"body", &raw_hex, "secret"));
    }

    #[test]
    fn tampered_body_rejected() {
        let original = b"original body";
        let sig = make_sig(original, "secret");
        assert!(!verify_signature(b"tampered body", &sig, "secret"));
    }

    #[test]
    fn empty_body_valid_signature_accepted() {
        let sig = make_sig(b"", "secret");
        assert!(verify_signature(b"", &sig, "secret"));
    }

    #[test]
    fn truncated_signature_rejected() {
        let body = b"data";
        let sig = make_sig(body, "secret");
        let truncated = &sig[..sig.len() - 4];
        assert!(!verify_signature(body, truncated, "secret"));
    }
}
