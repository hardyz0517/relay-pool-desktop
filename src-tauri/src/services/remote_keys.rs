use sha2::{Digest, Sha256};

pub fn api_key_fingerprint(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut hasher = Sha256::new();
    hasher.update(trimmed.as_bytes());
    Some(format!("{:x}", hasher.finalize()))
}

pub fn visible_mask_parts(masked: &str) -> Option<(String, String)> {
    let trimmed = masked.trim();
    let (prefix, suffix) = trimmed
        .split_once("****")
        .or_else(|| trimmed.split_once("..."))?;
    let prefix = prefix.trim().to_string();
    let suffix = suffix.trim().to_string();
    if prefix.len() < 3 || suffix.len() < 3 {
        return None;
    }
    Some((prefix, suffix))
}

pub fn masked_key_matches_full(masked: &str, full_key: &str) -> bool {
    visible_mask_parts(masked)
        .map(|(prefix, suffix)| full_key.starts_with(&prefix) && full_key.ends_with(&suffix))
        .unwrap_or(false)
}

pub fn remote_key_confidence(
    remote_fingerprint: Option<&str>,
    local_fingerprint: Option<&str>,
    remote_masked: Option<&str>,
    local_full_key: Option<&str>,
    same_group: bool,
    same_name: bool,
) -> f64 {
    if remote_fingerprint.is_some() && remote_fingerprint == local_fingerprint {
        return 1.0;
    }
    if let (Some(masked), Some(full_key)) = (remote_masked, local_full_key) {
        if masked_key_matches_full(masked, full_key) {
            return if same_group || same_name { 0.92 } else { 0.82 };
        }
    }
    match (same_group, same_name) {
        (true, true) => 0.72,
        (true, false) | (false, true) => 0.55,
        (false, false) => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprints_identical_keys_consistently() {
        assert_eq!(api_key_fingerprint("sk-a"), api_key_fingerprint("sk-a"));
        assert_ne!(api_key_fingerprint("sk-a"), api_key_fingerprint("sk-b"));
        assert_eq!(api_key_fingerprint("   "), None);
    }

    #[test]
    fn masked_key_match_requires_visible_prefix_and_suffix() {
        assert!(masked_key_matches_full("sk-live****cdef", "sk-live-123-cdef"));
        assert!(masked_key_matches_full(
            "sk-live-...cdef",
            "sk-live-123-cdef"
        ));
        assert!(!masked_key_matches_full("sk-live****zzzz", "sk-live-123-cdef"));
        assert!(!masked_key_matches_full(
            "sk-live-...zzzz",
            "sk-live-123-cdef"
        ));
        assert!(!masked_key_matches_full("sk****ef", "sk-live-123-cdef"));
        assert!(!masked_key_matches_full("sk-...ef", "sk-live-123-cdef"));
    }

    #[test]
    fn confidence_separates_high_and_possible_matches() {
        let fp = api_key_fingerprint("sk-live-123-cdef");
        assert_eq!(
            remote_key_confidence(fp.as_deref(), fp.as_deref(), None, None, false, false),
            1.0
        );
        assert!(
            remote_key_confidence(
                None,
                None,
                Some("sk-live****cdef"),
                Some("sk-live-123-cdef"),
                true,
                false
            ) >= 0.9
        );
        assert!(remote_key_confidence(None, None, None, None, true, true) < 0.8);
    }
}
