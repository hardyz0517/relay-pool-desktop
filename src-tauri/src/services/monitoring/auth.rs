#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthBoundaryViolation {
    ForbiddenHeader(String),
}

pub fn normalize_header_name(name: &str) -> String {
    name.trim().to_ascii_lowercase()
}

pub fn validate_profile_header_name(name: &str) -> Result<(), AuthBoundaryViolation> {
    let normalized = normalize_header_name(name);
    if forbidden_profile_headers().contains(&normalized.as_str()) {
        return Err(AuthBoundaryViolation::ForbiddenHeader(normalized));
    }
    Ok(())
}

pub fn forbidden_profile_headers() -> &'static [&'static str] {
    &[
        "authorization",
        "proxy-authorization",
        "x-api-key",
        "api-key",
        "anthropic-api-key",
        "cookie",
        "set-cookie",
        "host",
        "x-forwarded-host",
        "x-forwarded-proto",
    ]
}
