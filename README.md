# GitWatch v2

Get Telegram notifications for GitHub activity — issues, PRs, pushes, and comments — on any public repository.

Built with Rust (axum · tokio · sqlx · teloxide).

---

## How it works

1. You message the bot on Telegram and click "Authorize GitHub"
2. GitHub OAuth connects your account
3. `/watch owner/repo` starts watching a repo
4. If you have admin/push access, a real-time webhook is created on the repo
5. If not, the app polls the GitHub Events API every 5 minutes as a fallback
6. Notifications arrive in Telegram as formatted HTML messages

---

## Prerequisites

| Tool | Purpose |
|------|---------|
| Rust (stable) | Build the app |
| Docker | Run PostgreSQL locally |
| ngrok | Expose localhost to GitHub webhooks during dev |
| A Telegram bot token | From [@BotFather](https://t.me/BotFather) |
| A GitHub OAuth App | From github.com/settings/developers |

---

## Local setup

### 1 — Clone and enter the project

```bash
cd gitwatch-v2
```

### 2 — Start PostgreSQL

```bash
docker compose up -d
```

This starts `postgres:16` on port 5432 with user/pass/db all set to `gitwatch`.

### 3 — Start ngrok

GitHub webhooks need a public HTTPS URL to reach your local machine.

```bash
ngrok http 3000
```

Copy the `https://xxxx.ngrok-free.app` URL — you'll need it in the next step.

### 4 — Create a GitHub OAuth App

Go to **github.com → Settings → Developer settings → OAuth Apps → New OAuth App**

| Field | Value |
|-------|-------|
| Application name | GitWatch (dev) |
| Homepage URL | `https://your-ngrok-url` |
| Authorization callback URL | `https://your-ngrok-url/api/auth/github/callback` |

Save — you'll get a **Client ID** and **Client Secret**.

### 5 — Fill in `.env`

```bash
cp .env.example .env
```

Edit `.env`:

```env
TELEGRAM_BOT_TOKEN=        # from @BotFather
GITHUB_CLIENT_ID=          # from the OAuth App
GITHUB_CLIENT_SECRET=      # from the OAuth App
GITHUB_WEBHOOK_SECRET=     # run: openssl rand -base64 32
APP_URL=                   # your ngrok URL, e.g. https://xxxx.ngrok-free.app
DATABASE_URL=postgres://gitwatch:gitwatch@localhost:5432/gitwatch
TEST_DATABASE_URL=postgres://gitwatch:gitwatch@localhost:5432/gitwatch_test
ADMIN_TELEGRAM_ID=         # your Telegram numeric ID (find it via @userinfobot)
CRON_INTERVAL_SECS=300     # polling interval in seconds (300 = 5 min)
PORT=3000
RUST_LOG=info
```

### 6 — Register the Telegram webhook

Tell Telegram where to send bot updates (replace with your ngrok URL):

```bash
curl -X POST "https://api.telegram.org/bot<TELEGRAM_BOT_TOKEN>/setWebhook" \
     -d "url=https://your-ngrok-url/api/telegram/webhook"
```

> **Local dev alternative:** If you'd rather not set a webhook, the bot already runs in long-polling mode automatically — skip this step entirely.

### 7 — Run the app

```bash
cargo run
```

You should see:

```
INFO gitwatch_v2::poller: Poller started, interval: 300s
INFO gitwatch_v2: Server listening on port 3000
```

---

## Bot commands

| Command | Description |
|---------|-------------|
| `/start` | Connect your GitHub account |
| `/watch owner/repo` | Watch a repository (also accepts full GitHub URLs) |
| `/watchlist` | List all watched repositories |
| `/unwatch owner/repo` | Stop watching a repository |
| `/status` | Show your plan and repo usage |
| `/disconnect` | Remove GitHub connection and all watches |
| `/help` | Show help |

**Admin only** (requires `ADMIN_TELEGRAM_ID`):

| Command | Description |
|---------|-------------|
| `/approve @user` | Upgrade user to Premium |
| `/downgrade @user` | Revert user to Free plan |
| `/reject @user reason` | Notify user their payment was rejected |
| `/stats` | Platform-wide statistics |

---

## Watch modes

**Webhook** (real-time) — used when you have admin or push access to the repo. GitWatch registers a webhook on GitHub; events arrive the moment they happen.

**Polling** — used for repos where you don't have admin access. GitWatch checks the GitHub Events API on every cron tick (`CRON_INTERVAL_SECS`). The minimum meaningful interval is 60 s; GitHub caches the Events API so very short intervals have no effect.

---

## Plan limits

| Plan | Max watched repos |
|------|------------------|
| Free | 2 |
| Premium | 5 |

Plans are managed manually via admin commands. To add more tiers, edit `src/limits.rs`.

---

## Project structure

```
src/
  main.rs              entry point
  lib.rs               crate root, wires everything together
  config.rs            typed env-var config
  state.rs             AppState shared across axum handlers and bot
  models.rs            sqlx row types
  db.rs                all database query functions
  github.rs            GitHub API client helpers
  limits.rs            per-plan repo limits (edit here to add tiers)
  rate_limit.rs        in-memory sliding-window rate limiter
  poller.rs            background tokio task for polling repos
  routes/
    mod.rs             axum router
    auth.rs            GitHub OAuth start + callback
    github_webhook.rs  receive and verify GitHub webhook events
  bot/
    mod.rs             teloxide dispatcher
    commands.rs        all bot command handlers
    callbacks.rs       inline keyboard (notification preference toggles)
    notify.rs          message formatters + send_telegram helper
migrations/
  001_init.sql         users and watched_repos tables
tests/
  helpers.rs           shared test setup (uses TEST_DATABASE_URL)
  db_tests.rs          DB integration tests (26 tests)
  http_tests.rs        HTTP handler tests (10 tests)
```

---

## Running tests

```bash
# Start the DB first
docker compose up -d

# Create the test database (one-time)
docker exec gitwatch-v2-postgres-1 psql -U gitwatch -c "CREATE DATABASE gitwatch_test;"

# Run all tests
cargo test
```

Tests split into:
- **Unit tests** (47) — message formatting, HMAC signatures, rate limiter, plan limits — no DB needed
- **DB integration tests** (26) — all CRUD and query functions — uses `TEST_DATABASE_URL`
- **HTTP integration tests** (10) — axum route handlers — uses `TEST_DATABASE_URL`

---

## Production deployment

1. Point `APP_URL` to your real domain
2. Set `CRON_INTERVAL_SECS=300`
3. Register the Telegram webhook to `https://yourdomain.com/api/telegram/webhook`
4. Run behind a reverse proxy (nginx / Caddy) with TLS — GitHub requires HTTPS for webhooks
5. Use a managed Postgres or keep Docker with a persistent volume (already configured in `docker-compose.yml`)

Build a release binary:

```bash
cargo build --release
./target/release/gitwatch-v2
```
