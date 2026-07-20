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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_plan_limit_is_two() {
        assert_eq!(repo_limit("free"), 2);
    }

    #[test]
    fn premium_plan_limit_is_five() {
        assert_eq!(repo_limit("premium"), 5);
    }

    #[test]
    fn unknown_plan_defaults_to_free_limit() {
        assert_eq!(repo_limit("enterprise"), 2);
        assert_eq!(repo_limit(""), 2);
        assert_eq!(repo_limit("PREMIUM"), 2); // case-sensitive
    }

    #[test]
    fn free_display_name() {
        assert_eq!(plan_display_name("free"), "Free");
    }

    #[test]
    fn premium_display_name() {
        assert_eq!(plan_display_name("premium"), "Premium");
    }

    #[test]
    fn unknown_plan_display_name_defaults_to_free() {
        assert_eq!(plan_display_name("other"), "Free");
    }
}
