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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn allows_requests_under_limit() {
        let rl = RateLimiter::new();
        for _ in 0..5 {
            assert!(rl.check("user:1", 5, Duration::from_secs(60)).allowed);
        }
    }

    #[test]
    fn blocks_at_limit() {
        let rl = RateLimiter::new();
        for _ in 0..3 {
            rl.check("user:2", 3, Duration::from_secs(60));
        }
        let result = rl.check("user:2", 3, Duration::from_secs(60));
        assert!(!result.allowed);
        assert!(result.reset_in_ms > 0);
    }

    #[test]
    fn different_keys_are_independent() {
        let rl = RateLimiter::new();
        for _ in 0..3 {
            rl.check("user:a", 3, Duration::from_secs(60));
        }
        // user:a is blocked
        assert!(!rl.check("user:a", 3, Duration::from_secs(60)).allowed);
        // user:b is unaffected
        assert!(rl.check("user:b", 3, Duration::from_secs(60)).allowed);
    }

    #[test]
    fn window_expiry_allows_again() {
        let rl = RateLimiter::new();
        let window = Duration::from_millis(10);
        rl.check("user:c", 1, window);
        assert!(!rl.check("user:c", 1, window).allowed);
        std::thread::sleep(Duration::from_millis(15));
        assert!(rl.check("user:c", 1, window).allowed);
    }

    #[test]
    fn limit_of_one_allows_first_blocks_second() {
        let rl = RateLimiter::new();
        assert!(rl.check("user:d", 1, Duration::from_secs(60)).allowed);
        assert!(!rl.check("user:d", 1, Duration::from_secs(60)).allowed);
    }

    #[test]
    fn reset_in_ms_is_nonzero_when_blocked() {
        let rl = RateLimiter::new();
        rl.check("user:e", 1, Duration::from_secs(30));
        let result = rl.check("user:e", 1, Duration::from_secs(30));
        assert!(!result.allowed);
        assert!(result.reset_in_ms > 0 && result.reset_in_ms <= 30_000);
    }
}
