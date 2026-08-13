use sha2::{Digest, Sha256};

const CAPACITY_DOMAIN_SCHEMA_VERSION: u8 = 1;

/// Trusted upstream identity used to decide whether two capacity failures are
/// correlated. Station, endpoint, and credential identities intentionally do
/// not participate in equality: multiple relays may still reach the same
/// provider deployment.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct ProviderCapacityDomain {
    provider_family: String,
    upstream_model_family: String,
    deployment: Option<String>,
    region: Option<String>,
}

impl ProviderCapacityDomain {
    pub(crate) fn from_trusted_identity(
        provider_family: impl AsRef<str>,
        upstream_model_family: impl AsRef<str>,
        deployment: Option<&str>,
        region: Option<&str>,
    ) -> Option<Self> {
        let provider_family = normalize_identity(provider_family.as_ref())?;
        let upstream_model_family = normalize_identity(upstream_model_family.as_ref())?;
        Some(Self {
            provider_family,
            upstream_model_family,
            deployment: deployment.and_then(normalize_identity),
            region: region.and_then(normalize_identity),
        })
    }

    /// A fixed-size, versioned value suitable for trace/replay equality. It
    /// contains neither station/key IDs nor endpoint URLs.
    pub(crate) fn commitment(&self) -> CapacityDomainCommitment {
        let mut digest = Sha256::new();
        digest.update([CAPACITY_DOMAIN_SCHEMA_VERSION]);
        update_component(&mut digest, &self.provider_family);
        update_component(&mut digest, &self.upstream_model_family);
        update_optional_component(&mut digest, self.deployment.as_deref());
        update_optional_component(&mut digest, self.region.as_deref());
        CapacityDomainCommitment {
            schema_version: CAPACITY_DOMAIN_SCHEMA_VERSION,
            digest_hex: encode_hex(&digest.finalize()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct CapacityDomainCommitment {
    pub(crate) schema_version: u8,
    pub(crate) digest_hex: String,
}

impl CapacityDomainCommitment {
    /// Rehydrates the canonical, non-secret wire representation carried by a
    /// classified failure. Rejecting malformed or future-version values keeps
    /// retry-domain admission fail closed.
    pub(crate) fn from_canonical(value: &str) -> Option<Self> {
        let (version, digest_hex) = value.strip_prefix('v')?.split_once(':')?;
        let schema_version = version.parse::<u8>().ok()?;
        if schema_version != CAPACITY_DOMAIN_SCHEMA_VERSION
            || digest_hex.len() != 64
            || !digest_hex.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return None;
        }
        Some(Self {
            schema_version,
            digest_hex: digest_hex.to_ascii_lowercase(),
        })
    }
}

fn normalize_identity(value: &str) -> Option<String> {
    let normalized = value.trim().to_ascii_lowercase();
    (!normalized.is_empty()).then_some(normalized)
}

fn update_component(digest: &mut Sha256, value: &str) {
    digest.update((value.len() as u64).to_be_bytes());
    digest.update(value.as_bytes());
}

fn update_optional_component(digest: &mut Sha256, value: Option<&str>) {
    match value {
        Some(value) => {
            digest.update([1]);
            update_component(digest, value);
        }
        None => digest.update([0]),
    }
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn station_and_key_provenance_cannot_split_a_provider_domain() {
        let first = ProviderCapacityDomain::from_trusted_identity(
            "OpenAI",
            "gpt-5-codex",
            Some("primary"),
            Some("US"),
        )
        .expect("trusted identity");
        let second = ProviderCapacityDomain::from_trusted_identity(
            " openai ",
            "GPT-5-CODEX",
            Some("PRIMARY"),
            Some("us"),
        )
        .expect("trusted identity");

        assert_eq!(first, second);
        assert_eq!(first.commitment(), second.commitment());
    }

    #[test]
    fn a_different_authoritative_deployment_is_a_different_domain() {
        let first = ProviderCapacityDomain::from_trusted_identity(
            "openai",
            "gpt-5-codex",
            Some("deployment-a"),
            None,
        )
        .expect("trusted identity");
        let second = ProviderCapacityDomain::from_trusted_identity(
            "openai",
            "gpt-5-codex",
            Some("deployment-b"),
            None,
        )
        .expect("trusted identity");

        assert_ne!(first, second);
        assert_ne!(first.commitment(), second.commitment());
    }

    #[test]
    fn missing_provider_or_model_identity_fails_closed() {
        assert!(ProviderCapacityDomain::from_trusted_identity("", "gpt-5", None, None).is_none());
        assert!(ProviderCapacityDomain::from_trusted_identity("openai", " ", None, None).is_none());
    }
}
