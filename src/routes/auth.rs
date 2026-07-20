use axum::{
    extract::{Query, State},
    response::{Html, IntoResponse, Redirect},
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use crate::{bot::notify::send_telegram, db, github, state::AppState};

type HmacSha256 = Hmac<Sha256>;

#[derive(Deserialize)]
pub struct StartParams {
    pub telegram_id: i64,
}

#[derive(Deserialize)]
pub struct CallbackParams {
    pub code: String,
    pub state: String,
}

#[allow(dead_code)]
#[derive(Serialize, Deserialize)]
struct OAuthState {
    telegram_id: i64,
    ts: i64,
}

pub(crate) fn sign_state(data: &serde_json::Value, secret: &str) -> String {
    let payload = serde_json::to_string(data).unwrap_or_default();
    let payload_b64 = URL_SAFE_NO_PAD.encode(payload.as_bytes());

    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC can take key of any size");
    mac.update(payload_b64.as_bytes());
    let sig = mac.finalize().into_bytes();
    let sig_hex = hex::encode(sig);

    format!("{payload_b64}.{sig_hex}")
}

pub(crate) fn verify_state(state_str: &str, secret: &str) -> Option<serde_json::Value> {
    let parts: Vec<&str> = state_str.splitn(2, '.').collect();
    if parts.len() != 2 {
        return None;
    }
    let (payload_b64, sig_hex) = (parts[0], parts[1]);

    // Verify signature
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).ok()?;
    mac.update(payload_b64.as_bytes());
    let expected = mac.finalize().into_bytes();
    let expected_hex = hex::encode(expected);

    // Constant-time comparison
    if expected_hex != sig_hex {
        return None;
    }

    // Decode payload
    let payload_bytes = URL_SAFE_NO_PAD.decode(payload_b64).ok()?;
    let payload_str = String::from_utf8(payload_bytes).ok()?;
    let data: serde_json::Value = serde_json::from_str(&payload_str).ok()?;

    // Check timestamp is within 10 minutes
    let ts = data["ts"].as_i64()?;
    let now = chrono::Utc::now().timestamp();
    if (now - ts).abs() > 600 {
        return None;
    }

    Some(data)
}

pub async fn github_oauth_start(
    Query(params): Query<StartParams>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let state_data = serde_json::json!({
        "telegram_id": params.telegram_id,
        "ts": chrono::Utc::now().timestamp(),
    });
    let state_token = sign_state(&state_data, &state.config.github_webhook_secret);

    let auth_url = format!(
        "https://github.com/login/oauth/authorize?client_id={}&scope=repo,admin:repo_hook&state={}",
        state.config.github_client_id, state_token
    );

    Redirect::temporary(&auth_url)
}

pub async fn github_oauth_callback(
    Query(params): Query<CallbackParams>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    // Verify state
    let state_data = match verify_state(&params.state, &state.config.github_webhook_secret) {
        Some(d) => d,
        None => {
            return Html(
                "<html><body><h1>Error</h1><p>Invalid or expired OAuth state. Please try again.</p></body></html>"
                    .to_string(),
            );
        }
    };

    let telegram_id = match state_data["telegram_id"].as_i64() {
        Some(id) => id,
        None => {
            return Html(
                "<html><body><h1>Error</h1><p>Invalid state data.</p></body></html>".to_string(),
            );
        }
    };

    // Exchange code for token
    let token = match github::exchange_code_for_token(
        &state.http_client,
        &state.config.github_client_id,
        &state.config.github_client_secret,
        &params.code,
    )
    .await
    {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("OAuth token exchange failed: {e}");
            return Html(format!(
                "<html><body><h1>Error</h1><p>Failed to exchange code: {e}</p></body></html>"
            ));
        }
    };

    // Get GitHub user info
    let gh_user = match github::get_github_user(&state.http_client, &token).await {
        Ok(u) => u,
        Err(e) => {
            tracing::error!("Failed to get GitHub user: {e}");
            return Html(format!(
                "<html><body><h1>Error</h1><p>Failed to fetch GitHub user: {e}</p></body></html>"
            ));
        }
    };

    // Store in DB
    if let Err(e) = db::update_user_github(&state.db, telegram_id, &token, &gh_user.login).await {
        tracing::error!("Failed to update user GitHub: {e}");
        return Html(format!(
            "<html><body><h1>Error</h1><p>Database error: {e}</p></body></html>"
        ));
    }

    // Notify user on Telegram
    send_telegram(
        &state.bot,
        telegram_id,
        &format!(
            "✅ <b>GitHub connected!</b>\n\nConnected as <b>{}</b>.\n\nUse /watch owner/repo to start watching repositories.",
            gh_user.login
        ),
    )
    .await;

    // Redirect to success page
    Html(
        r#"<!-- success --><!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>GitHub Connected — GitWatch</title>
    <style>
        body { font-family: system-ui, sans-serif; display: flex; align-items: center;
               justify-content: center; height: 100vh; margin: 0; background: #f0f2f5; }
        .card { background: white; padding: 2rem 3rem; border-radius: 12px;
                box-shadow: 0 2px 12px rgba(0,0,0,.1); text-align: center; }
        h1 { color: #22863a; }
        p { color: #555; }
    </style>
</head>
<body>
    <div class="card">
        <h1>✅ GitHub Connected!</h1>
        <p>Return to Telegram to start watching repositories.</p>
    </div>
</body>
</html>"#
            .to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "test-webhook-secret-key";

    fn fresh_state_token(telegram_id: i64) -> String {
        let data = serde_json::json!({
            "telegram_id": telegram_id,
            "ts": chrono::Utc::now().timestamp(),
        });
        sign_state(&data, SECRET)
    }

    #[test]
    fn round_trip_sign_verify() {
        let token = fresh_state_token(123456789);
        let result = verify_state(&token, SECRET);
        assert!(result.is_some());
        assert_eq!(result.unwrap()["telegram_id"], 123456789_i64);
    }

    #[test]
    fn wrong_secret_fails_verification() {
        let token = fresh_state_token(42);
        assert!(verify_state(&token, "wrong-secret").is_none());
    }

    #[test]
    fn expired_timestamp_fails_verification() {
        let data = serde_json::json!({
            "telegram_id": 42_i64,
            "ts": chrono::Utc::now().timestamp() - 700, // 11+ min ago
        });
        let token = sign_state(&data, SECRET);
        assert!(verify_state(&token, SECRET).is_none());
    }

    #[test]
    fn tampered_payload_fails_verification() {
        let token = fresh_state_token(99);
        // Flip a character in the payload part (before the dot)
        let mut chars: Vec<char> = token.chars().collect();
        chars[3] = if chars[3] == 'A' { 'B' } else { 'A' };
        let tampered: String = chars.into_iter().collect();
        assert!(verify_state(&tampered, SECRET).is_none());
    }

    #[test]
    fn malformed_no_dot_fails() {
        assert!(verify_state("nodotinthisstring", SECRET).is_none());
    }

    #[test]
    fn empty_string_fails() {
        assert!(verify_state("", SECRET).is_none());
    }

    #[test]
    fn future_timestamp_within_limit_passes() {
        // Small clock skew forward should still pass
        let data = serde_json::json!({
            "telegram_id": 77_i64,
            "ts": chrono::Utc::now().timestamp() + 30,
        });
        let token = sign_state(&data, SECRET);
        assert!(verify_state(&token, SECRET).is_some());
    }

    #[test]
    fn different_telegram_ids_produce_different_tokens() {
        let t1 = fresh_state_token(111);
        let t2 = fresh_state_token(222);
        assert_ne!(t1, t2);
    }
}
