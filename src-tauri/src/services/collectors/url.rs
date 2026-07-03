#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectorBaseUrls {
    pub upstream_api_base_url: String,
    pub management_base_url: String,
}

pub fn collector_base_urls(base_url: &str) -> CollectorBaseUrls {
    let trimmed = base_url.trim().trim_end_matches('/').to_string();
    let management = trimmed
        .strip_suffix("/v1")
        .or_else(|| trimmed.strip_suffix("/compatible-mode/v1"))
        .unwrap_or(trimmed.as_str())
        .trim_end_matches('/')
        .to_string();
    let upstream = if trimmed.ends_with("/v1") {
        trimmed
    } else {
        format!("{management}/v1")
    };
    CollectorBaseUrls {
        upstream_api_base_url: upstream,
        management_base_url: management,
    }
}

pub fn join_url(base: &str, path: &str) -> String {
    format!(
        "{}/{}",
        base.trim_end_matches('/'),
        path.trim_start_matches('/')
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collector_base_urls_strip_v1_for_management() {
        let urls = collector_base_urls("https://relay.example.com/v1");
        assert_eq!(urls.upstream_api_base_url, "https://relay.example.com/v1");
        assert_eq!(urls.management_base_url, "https://relay.example.com");
        assert_eq!(
            join_url(&urls.management_base_url, "/api/v1/groups/available"),
            "https://relay.example.com/api/v1/groups/available"
        );
    }

    #[test]
    fn collector_base_urls_add_v1_for_root_input() {
        let urls = collector_base_urls("https://relay.example.com");
        assert_eq!(urls.upstream_api_base_url, "https://relay.example.com/v1");
        assert_eq!(urls.management_base_url, "https://relay.example.com");
    }
}
