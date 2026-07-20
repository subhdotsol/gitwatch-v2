use dashmap::DashMap;
use std::time::{Duration, Instant};

pub struct RateLimiter {
    // key -> list of request timestamps in current window
    requests: DashMap<String, Vec<Instant>>,
}

pub struct RateLimitResult {
    pub allowed: bool,
    pub reset_in_ms: u64,
}

impl RateLimiter {
    pub fn new() -> Self {
        Self {
            requests: DashMap::new(),
        }
    }

    pub fn check(&self, key: &str, max_requests: usize, window: Duration) -> RateLimitResult {
        let now = Instant::now();
        let window_start = now - window;

        let mut entry = self.requests.entry(key.to_string()).or_default();
        entry.retain(|&t| t > window_start);

        if entry.len() >= max_requests {
            let oldest = entry[0];
            let reset_in = window.saturating_sub(now - oldest);
            return RateLimitResult {
                allowed: false,
                reset_in_ms: reset_in.as_millis() as u64,
            };
        }

        entry.push(now);
        RateLimitResult {
            allowed: true,
            reset_in_ms: 0,
        }
    }
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}
