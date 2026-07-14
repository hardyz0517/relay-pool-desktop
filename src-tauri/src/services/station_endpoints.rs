use url::Url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StationEndpointUrls {
    pub website_url: String,
    pub api_base_url: String,
}

pub fn normalize_station_endpoints(
    website_url: &str,
    api_base_url: &str,
) -> Result<StationEndpointUrls, String> {
    Ok(StationEndpointUrls {
        website_url: normalize_endpoint_url(website_url, "前端网址", false)?,
        api_base_url: normalize_endpoint_url(api_base_url, "API Base URL", true)?,
    })
}

pub fn build_management_url(base: &str, path: &str) -> Result<String, String> {
    append_resource(base, path.trim_start_matches('/'))
}

pub fn build_api_url(base: &str, local_path: &str) -> Result<String, String> {
    let resource = local_path
        .strip_prefix("/v1/")
        .or_else(|| local_path.strip_prefix("v1/"))
        .unwrap_or_else(|| local_path.trim_start_matches('/'));
    if !is_valid_resource_path(resource) {
        return Err("上游 API 资源路径无效".to_string());
    }
    append_resource(base, resource)
}

pub fn same_origin(left: &str, right: &str) -> Result<bool, String> {
    let left = Url::parse(left).map_err(|error| format!("URL 无效: {error}"))?;
    let right = Url::parse(right).map_err(|error| format!("URL 无效: {error}"))?;
    Ok(origins_match(&left, &right))
}

pub(crate) fn legacy_api_base_url(website_url: &str) -> Result<String, String> {
    let normalized = normalize_endpoint_url(website_url, "前端网址", false)?;
    let url = Url::parse(&normalized).map_err(|error| format!("前端网址无效: {error}"))?;
    if last_path_segment(&url).is_some_and(is_version_segment) {
        return Ok(normalized);
    }
    append_resource(&normalized, "v1")
}

pub(crate) fn legacy_website_url(api_base_url: &str) -> Result<String, String> {
    let normalized = normalize_endpoint_url(api_base_url, "API Base URL", false)?;
    let mut url = Url::parse(&normalized).map_err(|error| format!("API Base URL 无效: {error}"))?;
    if last_path_segment(&url).is_some_and(is_version_segment) {
        url.path_segments_mut()
            .map_err(|_| "API Base URL 无法作为层级网址".to_string())?
            .pop();
    }
    normalized_url_string(url)
}

pub(crate) fn url_belongs_to_base(candidate: &str, base: &str) -> bool {
    let Ok(candidate) = Url::parse(candidate) else {
        return false;
    };
    let Ok(base) = Url::parse(base) else {
        return false;
    };
    if !origins_match(&candidate, &base) {
        return false;
    }

    let candidate_segments = meaningful_path_segments(&candidate);
    let base_segments = meaningful_path_segments(&base);
    candidate_segments.starts_with(&base_segments)
}

fn normalize_endpoint_url(
    value: &str,
    label: &str,
    reject_resource: bool,
) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(format!("{label}不能为空"));
    }

    let mut url = Url::parse(value).map_err(|error| format!("{label}无效: {error}"))?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err(format!("{label}必须是有效的 HTTP(S) 网址"));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(format!("{label}不能包含用户凭据"));
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(format!("{label}不能包含查询参数或片段"));
    }

    trim_trailing_path_slashes(&mut url);
    if reject_resource && is_final_api_resource(&url) {
        return Err(format!("{label}必须是 API 命名空间，不能是最终资源网址"));
    }
    normalized_url_string(url)
}

fn append_resource(base: &str, resource: &str) -> Result<String, String> {
    let (resource_path, resource_query) = split_resource_query(resource)?;
    if !is_valid_resource_path(resource_path) {
        return Err("资源路径无效".to_string());
    }
    let preserve_trailing_path_slash = resource_path.ends_with('/');

    let normalized = normalize_endpoint_url(base, "基础网址", false)?;
    let mut url = Url::parse(&normalized).map_err(|error| format!("基础网址无效: {error}"))?;
    {
        let mut segments = url
            .path_segments_mut()
            .map_err(|_| "基础网址无法追加资源路径".to_string())?;
        for segment in resource_path
            .split('/')
            .filter(|segment| !segment.is_empty())
        {
            segments.push(segment);
        }
    }
    if preserve_trailing_path_slash && !url.path().ends_with('/') {
        let mut path = url.path().to_string();
        path.push('/');
        url.set_path(&path);
    }
    url.set_query(resource_query);
    let mut value = normalized_url_string(url)?;
    if preserve_trailing_path_slash && resource_query.is_none() && !value.ends_with('/') {
        value.push('/');
    }
    Ok(value)
}

fn split_resource_query(resource: &str) -> Result<(&str, Option<&str>), String> {
    if resource.contains('#') {
        return Err("资源路径无效".to_string());
    }
    let (path, query) = match resource.split_once('?') {
        Some((path, query)) => (path, Some(query)),
        None => (resource, None),
    };
    Ok((path, query))
}

fn is_valid_resource_path(resource: &str) -> bool {
    !resource.is_empty()
        && !resource.contains("://")
        && resource
            .split('/')
            .all(|segment| !matches!(segment, "." | ".."))
}

fn is_final_api_resource(url: &Url) -> bool {
    let segments = meaningful_path_segments(url);
    matches!(segments.last(), Some(&"responses") | Some(&"models"))
        || segments.ends_with(&["chat", "completions"])
}

fn last_path_segment(url: &Url) -> Option<&str> {
    url.path_segments()?
        .filter(|segment| !segment.is_empty())
        .next_back()
}

fn is_version_segment(segment: &str) -> bool {
    segment
        .strip_prefix('v')
        .is_some_and(|version| !version.is_empty() && version.chars().all(|ch| ch.is_ascii_digit()))
}

fn meaningful_path_segments(url: &Url) -> Vec<&str> {
    url.path_segments()
        .into_iter()
        .flatten()
        .filter(|segment| !segment.is_empty())
        .collect()
}

fn origins_match(left: &Url, right: &Url) -> bool {
    is_http_origin(left)
        && is_http_origin(right)
        && left.scheme() == right.scheme()
        && left.host_str() == right.host_str()
        && left.port_or_known_default() == right.port_or_known_default()
}

fn is_http_origin(url: &Url) -> bool {
    matches!(url.scheme(), "http" | "https") && url.host_str().is_some()
}

fn trim_trailing_path_slashes(url: &mut Url) {
    let path = url.path().trim_end_matches('/').to_string();
    url.set_path(&path);
}

fn normalized_url_string(url: Url) -> Result<String, String> {
    let value = url.to_string();
    let normalized = value.trim_end_matches('/');
    if normalized.is_empty() {
        return Err("URL 无效".to_string());
    }
    Ok(normalized.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_resources_from_complete_api_namespaces() {
        assert_eq!(
            build_management_url("https://relay.example/api", "/user/self").unwrap(),
            "https://relay.example/api/user/self"
        );
        assert_eq!(
            build_management_url(
                "https://relay.example/console",
                "/api/token/?p=1&page_size=100",
            )
            .unwrap(),
            "https://relay.example/console/api/token/?p=1&page_size=100"
        );
        assert_eq!(
            build_api_url("https://relay.example/v1", "/v1/responses").unwrap(),
            "https://relay.example/v1/responses"
        );
        assert_eq!(
            build_api_url("https://ark.example/api/v3", "/v1/chat/completions",).unwrap(),
            "https://ark.example/api/v3/chat/completions"
        );
        assert_eq!(
            build_api_url("https://relay.example/proxy/v1", "/v1/models").unwrap(),
            "https://relay.example/proxy/v1/models"
        );
    }

    #[test]
    fn dual_origin_builders_keep_management_and_api_namespaces_disjoint() {
        let management = build_management_url(
            "https://console.example/app",
            "/api/token/?p=1&page_size=100",
        )
        .unwrap();
        let api = build_api_url("https://api.example/provider/api/v3", "/v1/usage").unwrap();

        assert_eq!(
            management,
            "https://console.example/app/api/token/?p=1&page_size=100"
        );
        assert_eq!(api, "https://api.example/provider/api/v3/usage");
        assert!(!same_origin(&management, &api).unwrap());
    }

    #[test]
    fn rejects_final_resource_urls_as_api_bases() {
        let error = normalize_station_endpoints(
            "https://relay.example",
            "https://api.example/v1/responses",
        )
        .expect_err("final response URL must not be accepted as a base");
        assert!(error.contains("API Base URL"));
    }

    #[test]
    fn derives_legacy_versioned_namespaces_without_corrupting_provider_paths() {
        assert_eq!(
            legacy_api_base_url("https://relay.example").unwrap(),
            "https://relay.example/v1"
        );
        assert_eq!(
            legacy_api_base_url("https://relay.example/proxy").unwrap(),
            "https://relay.example/proxy/v1"
        );
        assert_eq!(
            legacy_api_base_url("https://ark.example/api/v3").unwrap(),
            "https://ark.example/api/v3"
        );
        assert_eq!(
            legacy_website_url("https://relay.example/v1").unwrap(),
            "https://relay.example"
        );
    }

    #[test]
    fn base_membership_uses_origin_and_path_boundaries() {
        assert!(url_belongs_to_base(
            "https://relay.example/api/user/self",
            "https://relay.example/api",
        ));
        assert!(!url_belongs_to_base(
            "https://relay.example.evil.test/api/user/self",
            "https://relay.example",
        ));
        assert!(!url_belongs_to_base(
            "https://relay.example/apix/user/self",
            "https://relay.example/api",
        ));
    }

    #[test]
    fn opaque_urls_do_not_share_an_http_origin() {
        assert!(!same_origin("data:text/plain,left", "data:text/plain,right").unwrap());
    }

    #[test]
    fn opaque_urls_do_not_belong_to_hierarchical_bases() {
        assert!(!url_belongs_to_base(
            "data:text/plain,left",
            "data:text/plain,right",
        ));
    }

    #[test]
    fn rejects_invalid_station_endpoint_components() {
        let cases = [
            (
                "credentials",
                "https://user:secret@relay.example",
                "https://api.example/v1",
            ),
            (
                "query",
                "https://relay.example?tenant=one",
                "https://api.example/v1",
            ),
            (
                "fragment",
                "https://relay.example",
                "https://api.example/v1#responses",
            ),
            (
                "unsupported scheme",
                "ftp://relay.example",
                "https://api.example/v1",
            ),
        ];

        for (name, website_url, api_base_url) in cases {
            assert!(
                normalize_station_endpoints(website_url, api_base_url).is_err(),
                "{name} must be rejected"
            );
        }
    }

    #[test]
    fn rejects_traversal_resource_paths() {
        for path in ["/v1/../responses", "/v1/./models", "../chat/completions"] {
            assert!(
                build_api_url("https://relay.example/v1", path).is_err(),
                "{path} must be rejected"
            );
        }
    }

    #[test]
    fn differing_ports_do_not_share_an_origin_or_base() {
        let cases = [
            ("https://relay.example:443", "https://relay.example:8443"),
            ("http://relay.example:80", "http://relay.example:8080"),
        ];

        for (left, right) in cases {
            assert!(!same_origin(left, right).unwrap());
            assert!(!url_belongs_to_base(left, right));
        }
    }

    #[test]
    fn normalizes_endpoint_urls_and_compares_origins() {
        assert_eq!(
            normalize_station_endpoints(
                " https://relay.example/ ",
                " https://api.example/proxy/v1/ ",
            )
            .unwrap(),
            StationEndpointUrls {
                website_url: "https://relay.example".to_string(),
                api_base_url: "https://api.example/proxy/v1".to_string(),
            }
        );
        assert!(same_origin(
            "https://relay.example/path",
            "https://relay.example:443/other",
        )
        .unwrap());
        assert!(!same_origin("https://relay.example/path", "http://relay.example/path",).unwrap());
    }
}
