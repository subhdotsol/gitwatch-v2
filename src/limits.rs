// Plan limits — extend here to add premium tiers
pub fn repo_limit(plan: &str) -> usize {
    match plan {
        "premium" => 5,
        _ => 2, // free
    }
}

pub fn plan_display_name(plan: &str) -> &'static str {
    match plan {
        "premium" => "Premium",
        _ => "Free",
    }
}

pub const MAX_PLATFORM_USERS: i64 = 1000;
