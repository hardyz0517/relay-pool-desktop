use std::collections::HashMap;

pub fn authorize_headers(headers: &HashMap<String, String>, configured_key: &str) -> bool {
    let Some(value) = headers.get("authorization") else {
        return false;
    };
    let Some(token) = value
        .strip_prefix("Bearer ")
        .or_else(|| value.strip_prefix("bearer "))
    else {
        return false;
    };
    constant_time_eq(token.trim().as_bytes(), configured_key.trim().as_bytes())
}

pub fn allowed_origin(origin: &str) -> Option<&str> {
    let origin = origin.trim();
    let loopback = origin == "http://localhost"
        || origin.starts_with("http://localhost:")
        || origin == "http://127.0.0.1"
        || origin.starts_with("http://127.0.0.1:");
    loopback.then_some(origin)
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        })
        == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bearer_auth_requires_exact_configured_key() {
        let headers = HashMap::from([(
            "authorization".to_string(),
            "Bearer relay-local-secret".to_string(),
        )]);
        assert!(authorize_headers(&headers, "relay-local-secret"));
        assert!(!authorize_headers(&headers, "relay-local-other"));
        assert!(!authorize_headers(&HashMap::new(), "relay-local-secret"));
    }

    #[test]
    fn cors_allows_loopback_origins_only() {
        assert_eq!(
            allowed_origin("http://127.0.0.1:3000"),
            Some("http://127.0.0.1:3000")
        );
        assert_eq!(
            allowed_origin("http://localhost:5173"),
            Some("http://localhost:5173")
        );
        assert_eq!(allowed_origin("https://attacker.example"), None);
    }
}
