mod helpers;

use axum::{
    body::Body,
    http::{HeaderValue, Request, StatusCode},
};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use tower::ServiceExt; // for `.oneshot()`

use gitwatch_v2::routes::create_router;

fn sign_body(body: &[u8], secret: &str) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(body);
    format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
}

// ── Health check ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn health_endpoint_returns_200() {
    let state = helpers::setup().await;
    let app = create_router(state);

    let resp = app
        .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
}

// ── GitHub OAuth start ────────────────────────────────────────────────────────

#[tokio::test]
async fn oauth_start_redirects_to_github() {
    let state = helpers::setup().await;
    let app = create_router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/auth/github?telegram_id=123456789")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Should redirect (307) to GitHub's OAuth endpoint
    assert_eq!(resp.status(), StatusCode::TEMPORARY_REDIRECT);
    let location = resp.headers().get("location").unwrap().to_str().unwrap();
    assert!(
        location.starts_with("https://github.com/login/oauth/authorize"),
        "unexpected redirect: {location}"
    );
    assert!(location.contains("client_id="), "should include client_id");
    assert!(location.contains("state="), "should include HMAC state");
}

#[tokio::test]
async fn oauth_start_missing_telegram_id_returns_error() {
    let state = helpers::setup().await;
    let app = create_router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/auth/github") // no telegram_id
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // axum returns 400 when a required query param is missing
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

// ── GitHub webhook: signature verification ────────────────────────────────────

#[tokio::test]
async fn webhook_without_signature_returns_401() {
    let state = helpers::setup().await;
    let secret = state.config.github_webhook_secret.clone();
    let app = create_router(state);

    let body = r#"{"repository":{"owner":{"login":"o"},"name":"r"},"action":"opened"}"#;

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/webhooks/github")
                .header("x-github-event", "issues")
                .header("content-type", "application/json")
                // Intentionally omit x-hub-signature-256
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let _ = secret; // referenced to avoid unused warning
}

#[tokio::test]
async fn webhook_with_wrong_signature_returns_401() {
    let state = helpers::setup().await;
    let app = create_router(state);

    let body = r#"{"repository":{"owner":{"login":"o"},"name":"r"}}"#;
    let bad_sig = "sha256=deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef";

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/webhooks/github")
                .header("x-github-event", "push")
                .header("content-type", "application/json")
                .header("x-hub-signature-256", bad_sig)
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn webhook_with_valid_signature_no_watchers_returns_200() {
    let state = helpers::setup().await;
    let secret = state.config.github_webhook_secret.clone();
    let app = create_router(state);

    // A repo that nobody is watching
    let body = r#"{"repository":{"owner":{"login":"nonexistent-org-xyz"},"name":"nonexistent-repo-xyz"}}"#;
    let sig = sign_body(body.as_bytes(), &secret);

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/webhooks/github")
                .header("x-github-event", "push")
                .header("content-type", "application/json")
                .header("x-hub-signature-256", HeaderValue::from_str(&sig).unwrap())
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn webhook_missing_github_event_header_returns_400() {
    let state = helpers::setup().await;
    let secret = state.config.github_webhook_secret.clone();
    let app = create_router(state);

    let body = r#"{"repository":{"owner":{"login":"o"},"name":"r"}}"#;
    let sig = sign_body(body.as_bytes(), &secret);

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/webhooks/github")
                .header("content-type", "application/json")
                .header("x-hub-signature-256", HeaderValue::from_str(&sig).unwrap())
                // Omit x-github-event
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn webhook_invalid_json_returns_400() {
    let state = helpers::setup().await;
    let secret = state.config.github_webhook_secret.clone();
    let app = create_router(state);

    let body = b"not valid json {{";
    let sig = sign_body(body, &secret);

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/webhooks/github")
                .header("x-github-event", "push")
                .header("content-type", "application/json")
                .header("x-hub-signature-256", HeaderValue::from_str(&sig).unwrap())
                .body(Body::from(body.as_slice()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn webhook_missing_repository_field_returns_200() {
    // A ping event has no repository — we should handle it gracefully
    let state = helpers::setup().await;
    let secret = state.config.github_webhook_secret.clone();
    let app = create_router(state);

    let body = r#"{"zen":"Keep it logically awesome."}"#;
    let sig = sign_body(body.as_bytes(), &secret);

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/webhooks/github")
                .header("x-github-event", "ping")
                .header("content-type", "application/json")
                .header("x-hub-signature-256", HeaderValue::from_str(&sig).unwrap())
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
}

// ── Unknown routes ────────────────────────────────────────────────────────────

#[tokio::test]
async fn unknown_route_returns_404() {
    let state = helpers::setup().await;
    let app = create_router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/nonexistent")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
