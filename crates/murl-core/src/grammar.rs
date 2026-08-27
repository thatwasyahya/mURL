//! Shared micro-grammars used by both the mURL parser and the manifest
//! validator. One definition per identifier class, so the selector grammar
//! in `murl://…#role=docs` can never drift from the manifest's `role` field
//! grammar.

/// Resource ids: `[a-z0-9][a-z0-9_-]{0,63}`.
pub fn is_valid_resource_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes().enumerate().all(|(i, b)| {
            let alnum = b.is_ascii_lowercase() || b.is_ascii_digit();
            if i == 0 {
                alnum
            } else {
                alnum || b == b'-' || b == b'_'
            }
        })
}

/// Roles: `[a-z0-9][a-z0-9-]{0,31}`.
pub fn is_valid_role(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 32
        && s.bytes().enumerate().all(|(i, b)| {
            let alnum = b.is_ascii_lowercase() || b.is_ascii_digit();
            if i == 0 {
                alnum
            } else {
                alnum || b == b'-'
            }
        })
}

/// Tags: `[a-z0-9-]{1,32}`.
pub fn is_valid_tag(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 32
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids() {
        assert!(is_valid_resource_id("a"));
        assert!(is_valid_resource_id("monitoring-2_x"));
        assert!(!is_valid_resource_id(""));
        assert!(!is_valid_resource_id("-a"));
        assert!(!is_valid_resource_id("_a"));
        assert!(!is_valid_resource_id("UPPER"));
        assert!(!is_valid_resource_id(&"a".repeat(65)));
    }

    #[test]
    fn roles() {
        assert!(is_valid_role("docs"));
        assert!(is_valid_role("on-call"));
        assert!(!is_valid_role("-x"));
        assert!(!is_valid_role("x_y"));
        assert!(!is_valid_role(&"a".repeat(33)));
    }

    #[test]
    fn tags() {
        assert!(is_valid_tag("dev"));
        assert!(is_valid_tag("-odd-but-legal"));
        assert!(!is_valid_tag(""));
        assert!(!is_valid_tag("x_y"));
    }
}
