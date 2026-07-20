CREATE TABLE IF NOT EXISTS users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    telegram_id BIGINT UNIQUE NOT NULL,
    telegram_username TEXT,
    github_token TEXT,
    github_username TEXT,
    plan TEXT NOT NULL DEFAULT 'free',
    plan_expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS watched_repos (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    owner TEXT NOT NULL,
    repo TEXT NOT NULL,
    webhook_id BIGINT,
    watch_mode TEXT NOT NULL DEFAULT 'polling',
    last_polled TIMESTAMPTZ,
    active BOOLEAN NOT NULL DEFAULT TRUE,
    notify_issues BOOLEAN NOT NULL DEFAULT TRUE,
    notify_prs BOOLEAN NOT NULL DEFAULT TRUE,
    notify_commits BOOLEAN NOT NULL DEFAULT TRUE,
    notify_comments BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(user_id, owner, repo)
);
