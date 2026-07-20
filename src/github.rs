use anyhow::Context;
use serde::{Deserialize, Serialize};
use tracing::warn;

#[derive(Deserialize, Debug)]
pub struct GitHubRepo {
    pub private: bool,
    pub permissions: Option<RepoPermissions>,
}

#[derive(Deserialize, Debug)]
pub struct RepoPermissions {
    pub admin: bool,
    pub push: bool,
}

#[derive(Deserialize, Debug)]
pub struct GitHubUser {
    pub login: String,
}

#[derive(Deserialize, Debug)]
pub struct RepoEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    pub actor: EventActor,
    pub payload: serde_json::Value,
    pub created_at: String,
}

#[derive(Deserialize, Debug)]
pub struct EventActor {
    pub login: String,
}

#[derive(Serialize)]
struct WebhookConfig {
    url: String,
    content_type: String,
    secret: String,
    insecure_ssl: String,
}

#[derive(Serialize)]
struct CreateWebhookRequest {
    name: String,
    active: bool,
    events: Vec<String>,
    config: WebhookConfig,
}

#[derive(Deserialize)]
struct CreateWebhookResponse {
    id: i64,
}

#[derive(Deserialize)]
struct AccessTokenResponse {
    access_token: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

pub async fn get_repo(
    client: &reqwest::Client,
    token: &str,
    owner: &str,
    repo: &str,
) -> anyhow::Result<GitHubRepo> {
    let url = format!("https://api.github.com/repos/{owner}/{repo}");
    let resp = client
        .get(&url)
        .header("Authorization", format!("Bearer {token}"))
        .header("Accept", "application/vnd.github.v3+json")
        .send()
        .await
        .context("failed to fetch repo")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("GitHub API error {status}: {body}");
    }

    let repo_data = resp
        .json::<GitHubRepo>()
        .await
        .context("failed to parse repo")?;
    Ok(repo_data)
}

pub async fn create_webhook(
    client: &reqwest::Client,
    token: &str,
    owner: &str,
    repo: &str,
    webhook_url: &str,
    secret: &str,
) -> anyhow::Result<Option<i64>> {
    let url = format!("https://api.github.com/repos/{owner}/{repo}/hooks");
    let body = CreateWebhookRequest {
        name: "web".to_string(),
        active: true,
        events: vec![
            "push".to_string(),
            "issues".to_string(),
            "pull_request".to_string(),
            "issue_comment".to_string(),
        ],
        config: WebhookConfig {
            url: webhook_url.to_string(),
            content_type: "json".to_string(),
            secret: secret.to_string(),
            insecure_ssl: "0".to_string(),
        },
    };

    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {token}"))
        .header("Accept", "application/vnd.github.v3+json")
        .json(&body)
        .send()
        .await;

    match resp {
        Err(e) => {
            warn!("Failed to create webhook for {owner}/{repo}: {e}");
            Ok(None)
        }
        Ok(r) => {
            if !r.status().is_success() {
                let status = r.status();
                let body = r.text().await.unwrap_or_default();
                warn!("GitHub webhook creation failed {status} for {owner}/{repo}: {body}");
                return Ok(None);
            }
            match r.json::<CreateWebhookResponse>().await {
                Ok(data) => Ok(Some(data.id)),
                Err(e) => {
                    warn!("Failed to parse webhook response: {e}");
                    Ok(None)
                }
            }
        }
    }
}

pub async fn delete_webhook(
    client: &reqwest::Client,
    token: &str,
    owner: &str,
    repo: &str,
    webhook_id: i64,
) -> anyhow::Result<()> {
    let url = format!("https://api.github.com/repos/{owner}/{repo}/hooks/{webhook_id}");
    let resp = client
        .delete(&url)
        .header("Authorization", format!("Bearer {token}"))
        .header("Accept", "application/vnd.github.v3+json")
        .send()
        .await
        .context("failed to delete webhook")?;

    if resp.status().as_u16() == 404 {
        // Already gone, that's fine
        return Ok(());
    }

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("Failed to delete webhook {status}: {body}");
    }

    Ok(())
}

pub async fn get_github_user(client: &reqwest::Client, token: &str) -> anyhow::Result<GitHubUser> {
    let resp = client
        .get("https://api.github.com/user")
        .header("Authorization", format!("Bearer {token}"))
        .header("Accept", "application/vnd.github.v3+json")
        .send()
        .await
        .context("failed to fetch github user")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("GitHub user API error {status}: {body}");
    }

    let user = resp
        .json::<GitHubUser>()
        .await
        .context("failed to parse github user")?;
    Ok(user)
}

pub async fn exchange_code_for_token(
    client: &reqwest::Client,
    client_id: &str,
    client_secret: &str,
    code: &str,
) -> anyhow::Result<String> {
    let params = [
        ("client_id", client_id),
        ("client_secret", client_secret),
        ("code", code),
    ];

    let resp = client
        .post("https://github.com/login/oauth/access_token")
        .header("Accept", "application/json")
        .form(&params)
        .send()
        .await
        .context("failed to exchange OAuth code")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("OAuth token exchange failed {status}: {body}");
    }

    let data = resp
        .json::<AccessTokenResponse>()
        .await
        .context("failed to parse token response")?;

    if let Some(err) = data.error {
        let desc = data.error_description.unwrap_or_default();
        anyhow::bail!("OAuth error: {err} - {desc}");
    }

    data.access_token
        .ok_or_else(|| anyhow::anyhow!("no access_token in response"))
}

pub async fn get_repo_events(
    client: &reqwest::Client,
    token: &str,
    owner: &str,
    repo: &str,
) -> anyhow::Result<Vec<RepoEvent>> {
    let url = format!("https://api.github.com/repos/{owner}/{repo}/events?per_page=100");
    let resp = client
        .get(&url)
        .header("Authorization", format!("Bearer {token}"))
        .header("Accept", "application/vnd.github.v3+json")
        .send()
        .await
        .context("failed to fetch repo events")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("GitHub events API error {status}: {body}");
    }

    let events = resp
        .json::<Vec<RepoEvent>>()
        .await
        .context("failed to parse events")?;
    Ok(events)
}
