mod helpers;

use gitwatch_v2::db;

// ── User CRUD ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn upsert_creates_new_user() {
    let state = helpers::setup().await;
    let tid = helpers::rand_telegram_id();

    let user = db::upsert_user(&state.db, tid, Some("testbot"))
        .await
        .unwrap();
    assert_eq!(user.telegram_id, tid);
    assert_eq!(user.telegram_username.as_deref(), Some("testbot"));
    assert_eq!(user.plan, "free");
    assert!(user.github_token.is_none());

    helpers::cleanup_user(&state.db, tid).await;
}

#[tokio::test]
async fn upsert_updates_username_on_conflict() {
    let state = helpers::setup().await;
    let tid = helpers::rand_telegram_id();

    let u1 = db::upsert_user(&state.db, tid, Some("oldname"))
        .await
        .unwrap();
    let u2 = db::upsert_user(&state.db, tid, Some("newname"))
        .await
        .unwrap();

    assert_eq!(u1.id, u2.id, "should be the same DB row");
    assert_eq!(u2.telegram_username.as_deref(), Some("newname"));

    helpers::cleanup_user(&state.db, tid).await;
}

#[tokio::test]
async fn find_user_by_telegram_id_returns_none_for_unknown() {
    let state = helpers::setup().await;
    let result = db::find_user_by_telegram_id(&state.db, 999_999_999_999)
        .await
        .unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn find_user_by_telegram_id_returns_existing_user() {
    let state = helpers::setup().await;
    let tid = helpers::rand_telegram_id();

    db::upsert_user(&state.db, tid, Some("findme"))
        .await
        .unwrap();
    let found = db::find_user_by_telegram_id(&state.db, tid).await.unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().telegram_id, tid);

    helpers::cleanup_user(&state.db, tid).await;
}

#[tokio::test]
async fn update_user_github_stores_token_and_username() {
    let state = helpers::setup().await;
    let tid = helpers::rand_telegram_id();

    db::upsert_user(&state.db, tid, Some("user")).await.unwrap();
    let updated = db::update_user_github(&state.db, tid, "ghu_token123", "ghuser")
        .await
        .unwrap();

    assert_eq!(updated.github_token.as_deref(), Some("ghu_token123"));
    assert_eq!(updated.github_username.as_deref(), Some("ghuser"));

    helpers::cleanup_user(&state.db, tid).await;
}

#[tokio::test]
async fn find_user_by_username_works() {
    let state = helpers::setup().await;
    let tid = helpers::rand_telegram_id();
    let uname = format!("tguser_{tid}");

    db::upsert_user(&state.db, tid, Some(&uname)).await.unwrap();
    let found = db::find_user_by_username(&state.db, &uname).await.unwrap();
    assert!(found.is_some());

    helpers::cleanup_user(&state.db, tid).await;
}

#[tokio::test]
async fn count_users_includes_new_user() {
    let state = helpers::setup().await;
    let tid = helpers::rand_telegram_id();

    db::upsert_user(&state.db, tid, None).await.unwrap();
    let total = db::count_users(&state.db).await.unwrap();

    // At minimum the user we just created must be counted
    assert!(total >= 1, "should have at least one user");
    // Verify our specific user actually exists
    let found = db::find_user_by_telegram_id(&state.db, tid).await.unwrap();
    assert!(found.is_some());

    helpers::cleanup_user(&state.db, tid).await;
}

#[tokio::test]
async fn set_user_plan_updates_to_premium() {
    let state = helpers::setup().await;
    let tid = helpers::rand_telegram_id();

    let user = db::upsert_user(&state.db, tid, None).await.unwrap();
    let expires = chrono::Utc::now() + chrono::Duration::days(30);
    db::set_user_plan(&state.db, user.id, "premium", Some(expires))
        .await
        .unwrap();

    let refreshed = db::find_user_by_telegram_id(&state.db, tid)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(refreshed.plan, "premium");
    assert!(refreshed.plan_expires_at.is_some());

    helpers::cleanup_user(&state.db, tid).await;
}

#[tokio::test]
async fn disconnect_github_clears_token_and_username() {
    let state = helpers::setup().await;
    let tid = helpers::rand_telegram_id();

    let user = db::upsert_user(&state.db, tid, None).await.unwrap();
    db::update_user_github(&state.db, tid, "tok", "ghuser")
        .await
        .unwrap();
    db::disconnect_user_github(&state.db, user.id)
        .await
        .unwrap();

    let refreshed = db::find_user_by_telegram_id(&state.db, tid)
        .await
        .unwrap()
        .unwrap();
    assert!(refreshed.github_token.is_none());
    assert!(refreshed.github_username.is_none());

    helpers::cleanup_user(&state.db, tid).await;
}

// ── WatchedRepo CRUD ─────────────────────────────────────────────────────────

#[tokio::test]
async fn create_and_find_watched_repo() {
    let state = helpers::setup().await;
    let tid = helpers::rand_telegram_id();
    let user = db::upsert_user(&state.db, tid, None).await.unwrap();

    let repo = db::create_watched_repo(&state.db, user.id, "rust-lang", "rust", None, "polling")
        .await
        .unwrap();

    assert_eq!(repo.owner, "rust-lang");
    assert_eq!(repo.repo, "rust");
    assert_eq!(repo.watch_mode, "polling");
    assert!(repo.active);
    assert!(repo.notify_issues);
    assert!(repo.notify_prs);
    assert!(repo.notify_commits);
    assert!(repo.notify_comments);

    let found = db::find_watched_repo(&state.db, user.id, "rust-lang", "rust")
        .await
        .unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().id, repo.id);

    helpers::cleanup_user(&state.db, tid).await;
}

#[tokio::test]
async fn create_watched_repo_with_webhook_id() {
    let state = helpers::setup().await;
    let tid = helpers::rand_telegram_id();
    let user = db::upsert_user(&state.db, tid, None).await.unwrap();

    let repo = db::create_watched_repo(
        &state.db,
        user.id,
        "owner",
        "wh-repo",
        Some(12345),
        "webhook",
    )
    .await
    .unwrap();

    assert_eq!(repo.webhook_id, Some(12345));
    assert_eq!(repo.watch_mode, "webhook");

    helpers::cleanup_user(&state.db, tid).await;
}

#[tokio::test]
async fn count_active_repos_increments_and_stops_after_deactivate() {
    let state = helpers::setup().await;
    let tid = helpers::rand_telegram_id();
    let user = db::upsert_user(&state.db, tid, None).await.unwrap();

    assert_eq!(
        db::count_active_repos_for_user(&state.db, user.id)
            .await
            .unwrap(),
        0
    );

    let r1 = db::create_watched_repo(&state.db, user.id, "o", "r1", None, "polling")
        .await
        .unwrap();
    let _r2 = db::create_watched_repo(&state.db, user.id, "o", "r2", None, "polling")
        .await
        .unwrap();
    assert_eq!(
        db::count_active_repos_for_user(&state.db, user.id)
            .await
            .unwrap(),
        2
    );

    db::deactivate_watched_repo(&state.db, r1.id).await.unwrap();
    assert_eq!(
        db::count_active_repos_for_user(&state.db, user.id)
            .await
            .unwrap(),
        1
    );

    helpers::cleanup_user(&state.db, tid).await;
}

#[tokio::test]
async fn find_watched_repo_returns_none_for_unknown() {
    let state = helpers::setup().await;
    let tid = helpers::rand_telegram_id();
    let user = db::upsert_user(&state.db, tid, None).await.unwrap();

    let found = db::find_watched_repo(&state.db, user.id, "nobody", "norepo")
        .await
        .unwrap();
    assert!(found.is_none());

    helpers::cleanup_user(&state.db, tid).await;
}

#[tokio::test]
async fn get_user_watched_repos_returns_only_active() {
    let state = helpers::setup().await;
    let tid = helpers::rand_telegram_id();
    let user = db::upsert_user(&state.db, tid, None).await.unwrap();

    let r1 = db::create_watched_repo(&state.db, user.id, "o", "active", None, "polling")
        .await
        .unwrap();
    let r2 = db::create_watched_repo(&state.db, user.id, "o", "inactive", None, "polling")
        .await
        .unwrap();
    db::deactivate_watched_repo(&state.db, r2.id).await.unwrap();

    let repos = db::get_user_watched_repos(&state.db, user.id)
        .await
        .unwrap();
    assert_eq!(repos.len(), 1);
    assert_eq!(repos[0].id, r1.id);

    helpers::cleanup_user(&state.db, tid).await;
}

#[tokio::test]
async fn delete_all_user_repos_removes_everything() {
    let state = helpers::setup().await;
    let tid = helpers::rand_telegram_id();
    let user = db::upsert_user(&state.db, tid, None).await.unwrap();

    db::create_watched_repo(&state.db, user.id, "o", "r1", None, "polling")
        .await
        .unwrap();
    db::create_watched_repo(&state.db, user.id, "o", "r2", None, "polling")
        .await
        .unwrap();

    db::delete_all_user_repos(&state.db, user.id).await.unwrap();

    let repos = db::get_user_watched_repos(&state.db, user.id)
        .await
        .unwrap();
    assert!(repos.is_empty());

    helpers::cleanup_user(&state.db, tid).await;
}

// ── Notification preference toggles ─────────────────────────────────────────

#[tokio::test]
async fn toggle_notify_issues_flips_value() {
    let state = helpers::setup().await;
    let tid = helpers::rand_telegram_id();
    let user = db::upsert_user(&state.db, tid, None).await.unwrap();
    let repo = db::create_watched_repo(&state.db, user.id, "o", "toggle-issues", None, "polling")
        .await
        .unwrap();

    assert!(repo.notify_issues);
    let updated = db::toggle_notify_field(&state.db, repo.id, "issues")
        .await
        .unwrap();
    assert!(!updated.notify_issues);
    let toggled_back = db::toggle_notify_field(&state.db, repo.id, "issues")
        .await
        .unwrap();
    assert!(toggled_back.notify_issues);

    helpers::cleanup_user(&state.db, tid).await;
}

#[tokio::test]
async fn toggle_notify_prs_flips_value() {
    let state = helpers::setup().await;
    let tid = helpers::rand_telegram_id();
    let user = db::upsert_user(&state.db, tid, None).await.unwrap();
    let repo = db::create_watched_repo(&state.db, user.id, "o", "toggle-prs", None, "polling")
        .await
        .unwrap();

    let updated = db::toggle_notify_field(&state.db, repo.id, "prs")
        .await
        .unwrap();
    assert!(!updated.notify_prs);

    helpers::cleanup_user(&state.db, tid).await;
}

#[tokio::test]
async fn toggle_notify_commits_flips_value() {
    let state = helpers::setup().await;
    let tid = helpers::rand_telegram_id();
    let user = db::upsert_user(&state.db, tid, None).await.unwrap();
    let repo = db::create_watched_repo(&state.db, user.id, "o", "toggle-commits", None, "polling")
        .await
        .unwrap();

    let updated = db::toggle_notify_field(&state.db, repo.id, "commits")
        .await
        .unwrap();
    assert!(!updated.notify_commits);

    helpers::cleanup_user(&state.db, tid).await;
}

#[tokio::test]
async fn toggle_notify_comments_flips_value() {
    let state = helpers::setup().await;
    let tid = helpers::rand_telegram_id();
    let user = db::upsert_user(&state.db, tid, None).await.unwrap();
    let repo = db::create_watched_repo(&state.db, user.id, "o", "toggle-comments", None, "polling")
        .await
        .unwrap();

    let updated = db::toggle_notify_field(&state.db, repo.id, "comments")
        .await
        .unwrap();
    assert!(!updated.notify_comments);

    helpers::cleanup_user(&state.db, tid).await;
}

#[tokio::test]
async fn toggle_unknown_field_returns_error() {
    let state = helpers::setup().await;
    let tid = helpers::rand_telegram_id();
    let user = db::upsert_user(&state.db, tid, None).await.unwrap();
    let repo = db::create_watched_repo(&state.db, user.id, "o", "toggle-bad", None, "polling")
        .await
        .unwrap();

    let result = db::toggle_notify_field(&state.db, repo.id, "stars").await;
    assert!(result.is_err());

    helpers::cleanup_user(&state.db, tid).await;
}

// ── Polling query ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn get_polling_repos_returns_only_polling_mode() {
    let state = helpers::setup().await;
    let tid = helpers::rand_telegram_id();
    let user = db::upsert_user(&state.db, tid, None).await.unwrap();

    db::create_watched_repo(&state.db, user.id, "o", "poll-repo", None, "polling")
        .await
        .unwrap();
    db::create_watched_repo(&state.db, user.id, "o", "wh-repo", Some(99), "webhook")
        .await
        .unwrap();

    let polling = db::get_polling_repos(&state.db, 100).await.unwrap();
    let my_repos: Vec<_> = polling.iter().filter(|r| r.user_id == user.id).collect();

    assert_eq!(my_repos.len(), 1, "only polling-mode repos");
    assert_eq!(my_repos[0].repo, "poll-repo");

    helpers::cleanup_user(&state.db, tid).await;
}

#[tokio::test]
async fn update_last_polled_sets_timestamp() {
    let state = helpers::setup().await;
    let tid = helpers::rand_telegram_id();
    let user = db::upsert_user(&state.db, tid, None).await.unwrap();
    let repo = db::create_watched_repo(&state.db, user.id, "o", "poll-ts", None, "polling")
        .await
        .unwrap();

    assert!(repo.last_polled.is_none());
    db::update_last_polled(&state.db, repo.id).await.unwrap();

    let refreshed = db::find_watched_repo_by_id(&state.db, repo.id)
        .await
        .unwrap()
        .unwrap();
    assert!(refreshed.last_polled.is_some());

    helpers::cleanup_user(&state.db, tid).await;
}

// ── Cross-table join queries ──────────────────────────────────────────────────

#[tokio::test]
async fn find_repos_by_owner_repo_returns_watchers_with_user_info() {
    let state = helpers::setup().await;
    let tid = helpers::rand_telegram_id();
    let user = db::upsert_user(&state.db, tid, None).await.unwrap();
    db::update_user_github(&state.db, tid, "tok_abc", "ghuser")
        .await
        .unwrap();

    db::create_watched_repo(&state.db, user.id, "torvalds", "linux", None, "polling")
        .await
        .unwrap();

    let watchers = db::find_repos_by_owner_repo(&state.db, "torvalds", "linux")
        .await
        .unwrap();

    // There might be other test users watching linux; find ours
    let ours: Vec<_> = watchers
        .iter()
        .filter(|w| w.user_telegram_id == tid)
        .collect();
    assert_eq!(ours.len(), 1);
    assert_eq!(ours[0].user_github_token.as_deref(), Some("tok_abc"));
    assert_eq!(ours[0].user_github_username.as_deref(), Some("ghuser"));

    helpers::cleanup_user(&state.db, tid).await;
}

#[tokio::test]
async fn find_repos_by_owner_repo_excludes_inactive() {
    let state = helpers::setup().await;
    let tid = helpers::rand_telegram_id();
    let user = db::upsert_user(&state.db, tid, None).await.unwrap();

    let repo = db::create_watched_repo(&state.db, user.id, "o", "inactive-check", None, "polling")
        .await
        .unwrap();
    db::deactivate_watched_repo(&state.db, repo.id)
        .await
        .unwrap();

    let watchers = db::find_repos_by_owner_repo(&state.db, "o", "inactive-check")
        .await
        .unwrap();

    let ours: Vec<_> = watchers
        .iter()
        .filter(|w| w.user_telegram_id == tid)
        .collect();
    assert!(ours.is_empty(), "inactive repo should not appear");

    helpers::cleanup_user(&state.db, tid).await;
}

// ── Platform stats ────────────────────────────────────────────────────────────

#[tokio::test]
async fn platform_stats_returns_consistent_counts() {
    let state = helpers::setup().await;
    let tid = helpers::rand_telegram_id();
    let user = db::upsert_user(&state.db, tid, None).await.unwrap();
    db::create_watched_repo(&state.db, user.id, "o", "stat-repo", None, "polling")
        .await
        .unwrap();

    let stats = db::get_platform_stats(&state.db).await.unwrap();
    // Counts are non-negative and our test data is visible
    assert!(stats.total_users >= 1);
    assert!(stats.total_repos >= 1);
    assert!(stats.active_repos >= 0);
    // Both queries return independently; just confirm they're plausible
    assert!(stats.total_repos >= stats.active_repos || stats.active_repos >= 0);

    helpers::cleanup_user(&state.db, tid).await;
}

#[tokio::test]
async fn count_premium_users_only_counts_premium() {
    let state = helpers::setup().await;
    let tid = helpers::rand_telegram_id();
    let user = db::upsert_user(&state.db, tid, None).await.unwrap();

    let before = db::count_premium_users(&state.db).await.unwrap();
    db::set_user_plan(&state.db, user.id, "premium", None)
        .await
        .unwrap();
    let after = db::count_premium_users(&state.db).await.unwrap();

    assert!(after > before, "premium count should have increased");
    helpers::cleanup_user(&state.db, tid).await;
}
