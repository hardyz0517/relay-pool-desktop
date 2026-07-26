#![allow(
    dead_code,
    reason = "Stage 19.A freezes provider evidence contracts before production driver cutover"
)]

use serde_json::Value;

const MAX_EVIDENCE_ITEMS: usize = 8;
const MAX_FIELD_BYTES: usize = 96;
const MAX_DETAIL_BYTES: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndpointEvidence {
    pub role: EndpointRole,
    pub method: String,
    pub url: Option<String>,
    pub status_code: Option<u16>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointRole {
    ApiBase,
    Website,
    Balance,
    Groups,
    Models,
    RemoteKeys,
    Authorization,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceSet {
    entries: Vec<EndpointEvidence>,
}

impl EvidenceSet {
    pub fn new(entries: impl IntoIterator<Item = EndpointEvidence>) -> Self {
        let entries = entries
            .into_iter()
            .take(MAX_EVIDENCE_ITEMS)
            .map(EndpointEvidence::sanitized)
            .collect();
        Self { entries }
    }

    pub fn empty() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn entries(&self) -> &[EndpointEvidence] {
        &self.entries
    }
}

impl EndpointEvidence {
    pub fn new(
        role: EndpointRole,
        method: impl Into<String>,
        url: Option<String>,
        status_code: Option<u16>,
        detail: Option<String>,
    ) -> Self {
        Self {
            role,
            method: method.into(),
            url,
            status_code,
            detail,
        }
        .sanitized()
    }

    fn sanitized(self) -> Self {
        Self {
            role: self.role,
            method: truncate_field(&redact_text(self.method.trim())),
            url: self.url.map(|url| truncate_field(&redact_text(&url))),
            status_code: self.status_code,
            detail: self
                .detail
                .map(|detail| truncate_detail(&redact_text(&detail))),
        }
    }
}

pub fn redact_text(text: &str) -> String {
    crate::services::secrets::mask::redact_text(text)
}

pub fn redact_value(value: &Value) -> Value {
    crate::services::secrets::mask::redact_value(value)
}

fn truncate_field(value: &str) -> String {
    truncate_bytes(value, MAX_FIELD_BYTES)
}

fn truncate_detail(value: &str) -> String {
    truncate_bytes(value, MAX_DETAIL_BYTES)
}

fn truncate_bytes(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }
    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &value[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evidence_is_redacted_and_bounded() {
        let entries = (0..12).map(|index| {
            EndpointEvidence::new(
                EndpointRole::ApiBase,
                "GET",
                Some(format!(
                    "https://example.test/v1/models?api_key=sk-p8-secret-plaintext-canary-{index}&padding={}",
                    "x".repeat(200)
                )),
                Some(200),
                Some(format!(
                    "Authorization: Bearer sk-p8-secret-plaintext-canary-{index} {}",
                    "y".repeat(800)
                )),
            )
        });

        let evidence = EvidenceSet::new(entries);

        assert_eq!(evidence.entries().len(), 8);
        for entry in evidence.entries() {
            let url = entry.url.as_ref().expect("url");
            assert!(!url.contains("sk-p8-secret-plaintext-canary"));
            assert!(url.len() <= MAX_FIELD_BYTES + 3);
            let detail = entry.detail.as_ref().expect("detail");
            assert!(!detail.contains("sk-p8-secret-plaintext-canary"));
            assert!(detail.len() <= MAX_DETAIL_BYTES + 3);
        }
    }
}
