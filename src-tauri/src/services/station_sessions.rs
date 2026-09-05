use serde_json::Value;

const NEWAPI_REFRESH_COOKIE_NAMES: &[&str] = &["new_api_refresh"];
const NEWAPI_NON_AUTH_COOKIE_NAMES: &[&str] = &["new_api_has_session", "lang", "locale", "theme"];
const UNIX_SECONDS_UPPER_BOUND: i64 = 100_000_000_000;

pub(crate) fn merge_set_cookie_headers(
    existing: Option<&str>,
    set_cookie_headers: &[String],
) -> Option<String> {
    let mut pairs = existing
        .into_iter()
        .flat_map(|header| header.split(';'))
        .filter_map(non_empty_cookie_pair)
        .collect::<Vec<_>>();

    for header in set_cookie_headers {
        let Some((name, value)) = header.split(';').next().and_then(cookie_pair) else {
            continue;
        };
        let existing_index = pairs.iter().position(|(current, _)| current == &name);
        if value.is_empty() {
            if let Some(index) = existing_index {
                pairs.remove(index);
            }
        } else if let Some(index) = existing_index {
            pairs[index].1 = value;
        } else {
            pairs.push((name, value));
        }
    }

    (!pairs.is_empty()).then(|| {
        pairs
            .into_iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join("; ")
    })
}

pub(crate) fn newapi_cookie_can_authenticate(cookie: &str) -> bool {
    cookie_header_pairs(cookie).any(|(name, value)| {
        !value.is_empty()
            && !cookie_name_in(&name, NEWAPI_NON_AUTH_COOKIE_NAMES)
            && !cookie_name_in(&name, NEWAPI_REFRESH_COOKIE_NAMES)
    })
}

pub(crate) fn newapi_cookie_can_refresh(cookie: &str) -> bool {
    cookie_header_pairs(cookie).any(|(name, value)| {
        !value.is_empty() && cookie_name_in(&name, NEWAPI_REFRESH_COOKIE_NAMES)
    })
}

pub(crate) fn token_expires_at_from_payload(payload: &Value) -> Option<String> {
    access_expires_at_millis(payload)
        .map(|millis| millis.to_string())
        .or_else(|| relative_token_expires_at_from_payload(payload))
}

fn relative_token_expires_at_from_payload(payload: &Value) -> Option<String> {
    let seconds = payload
        .get("expires_in")
        .or_else(|| payload.get("expiresIn"))
        .and_then(u64_value)
        .or_else(|| {
            payload
                .get("data")
                .and_then(token_expiry_seconds_from_payload)
        })?;
    let milliseconds = i64::try_from(seconds.checked_mul(1_000)?).ok()?;
    let now = i64::try_from(crate::services::time::now_millis_for_services()).ok()?;
    Some(now.checked_add(milliseconds)?.to_string())
}

fn access_expires_at_millis(payload: &Value) -> Option<i64> {
    payload
        .get("access_expires_at")
        .or_else(|| payload.get("accessExpiresAt"))
        .and_then(unix_timestamp_to_millis)
        .or_else(|| payload.get("data").and_then(access_expires_at_millis))
}

fn token_expiry_seconds_from_payload(payload: &Value) -> Option<u64> {
    payload
        .get("expires_in")
        .or_else(|| payload.get("expiresIn"))
        .and_then(u64_value)
        .or_else(|| {
            payload
                .get("data")
                .and_then(token_expiry_seconds_from_payload)
        })
}

fn unix_timestamp_to_millis(value: &Value) -> Option<i64> {
    let timestamp = value
        .as_i64()
        .or_else(|| value.as_u64().and_then(|value| i64::try_from(value).ok()))
        .or_else(|| value.as_str()?.trim().parse::<i64>().ok())?;
    if timestamp <= 0 {
        return None;
    }
    if timestamp < UNIX_SECONDS_UPPER_BOUND {
        timestamp.checked_mul(1_000)
    } else {
        Some(timestamp)
    }
}

fn u64_value(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_str()?.trim().parse::<u64>().ok())
}

fn cookie_header_pairs(cookie: &str) -> impl Iterator<Item = (String, String)> + '_ {
    cookie.split(';').filter_map(non_empty_cookie_pair)
}

fn cookie_name_in(name: &str, names: &[&str]) -> bool {
    names
        .iter()
        .any(|candidate| name.eq_ignore_ascii_case(candidate))
}

fn non_empty_cookie_pair(value: &str) -> Option<(String, String)> {
    let (name, value) = cookie_pair(value)?;
    (!value.is_empty()).then_some((name, value))
}

fn cookie_pair(value: &str) -> Option<(String, String)> {
    let (name, value) = value.trim().split_once('=')?;
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    Some((name.to_string(), value.trim().to_string()))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn set_cookie_headers_replace_add_and_remove_named_cookies() {
        assert_eq!(
            merge_set_cookie_headers(
                Some("session=old; cf_clearance=clearance; theme=light"),
                &[
                    "session=rotated; Path=/; HttpOnly".to_string(),
                    "theme=; Max-Age=0; Path=/".to_string(),
                    "locale=zh; Path=/".to_string(),
                ],
            )
            .as_deref(),
            Some("session=rotated; cf_clearance=clearance; locale=zh")
        );
    }

    #[test]
    fn set_cookie_headers_can_create_a_cookie_header_without_an_existing_session() {
        assert_eq!(
            merge_set_cookie_headers(
                None,
                &[
                    "session=abc; Path=/; HttpOnly".to_string(),
                    "lang=zh; Path=/".to_string(),
                ],
            )
            .as_deref(),
            Some("session=abc; lang=zh")
        );
    }

    #[test]
    fn newapi_hint_and_refresh_cookies_are_not_collection_secrets() {
        assert!(!newapi_cookie_can_authenticate(
            "new_api_has_session=1; new_api_refresh=fixture-refresh"
        ));
        assert!(newapi_cookie_can_refresh(
            "new_api_has_session=1; new_api_refresh=fixture-refresh"
        ));
        assert!(newapi_cookie_can_authenticate("session=abc; lang=zh"));
        assert!(!newapi_cookie_can_authenticate(
            "new_api_has_session=1; lang=zh"
        ));
    }

    #[test]
    fn token_expiry_accepts_top_level_nested_and_string_durations() {
        let now = i64::try_from(crate::services::time::now_millis_for_services()).unwrap();
        for payload in [
            json!({"expires_in": 3600}),
            json!({"expiresIn": "3600"}),
            json!({"data": {"expires_in": 3600}}),
        ] {
            let expires_at = token_expires_at_from_payload(&payload)
                .and_then(|value| value.parse::<i64>().ok())
                .expect("absolute token expiry");
            assert!(expires_at >= now + 3_599_000);
            assert!(expires_at <= now + 3_601_000);
        }
    }

    #[test]
    fn token_expiry_accepts_newapi_access_expires_at_unix_seconds() {
        let expires_at = token_expires_at_from_payload(&json!({
            "data": {"access_expires_at": 1_700_000_000}
        }))
        .and_then(|value| value.parse::<i64>().ok())
        .expect("absolute token expiry");
        assert_eq!(expires_at, 1_700_000_000_000);
    }
}
