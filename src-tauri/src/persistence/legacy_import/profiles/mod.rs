#[cfg(test)]
use std::{future::Future, pin::Pin};

#[cfg(test)]
use crate::persistence::runtime::PersistenceHandle;

#[cfg(test)]
use super::{import::import_additive_v1, LegacyReadSession, UpgradeError};

use super::LegacySchemaFingerprint;

mod profile_001;

#[cfg(test)]
pub(crate) type ImportFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), UpgradeError>> + Send + 'a>>;

pub(crate) struct LegacyProfileDescriptor {
    pub(crate) id: &'static str,
    pub(crate) base_schema_hash: &'static str,
    pub(crate) request_lifecycle_schema_hash: &'static str,
    #[cfg(test)]
    #[allow(
        dead_code,
        reason = "the released-schema integration target invokes the fixture-only importer descriptor"
    )]
    pub(crate) import:
        for<'a> fn(&'a mut LegacyReadSession, &'a PersistenceHandle, bool) -> ImportFuture<'a>,
}

#[derive(Clone, Copy)]
pub(crate) struct DetectedLegacyProfile {
    pub(super) descriptor: &'static LegacyProfileDescriptor,
    request_lifecycle: bool,
}

impl DetectedLegacyProfile {
    pub(crate) fn id(self) -> &'static str {
        self.descriptor.id
    }

    #[cfg(test)]
    pub(crate) fn base_schema_hash(self) -> &'static str {
        self.descriptor.base_schema_hash
    }

    pub(crate) fn has_request_lifecycle(self) -> bool {
        self.request_lifecycle
    }

    #[cfg(test)]
    pub(crate) fn request_lifecycle_schema_hash(self) -> Option<&'static str> {
        self.request_lifecycle
            .then_some(self.descriptor.request_lifecycle_schema_hash)
    }
}

pub(crate) fn all() -> &'static [LegacyProfileDescriptor] {
    &[profile_001::DESCRIPTOR]
}

pub(super) fn by_fingerprint(
    fingerprint: &LegacySchemaFingerprint,
) -> Option<DetectedLegacyProfile> {
    all().iter().find_map(|descriptor| {
        if descriptor.base_schema_hash != fingerprint.base_hash {
            return None;
        }
        let request_lifecycle = match fingerprint.request_lifecycle_hash.as_deref() {
            None => false,
            Some(hash) if hash == descriptor.request_lifecycle_schema_hash => true,
            Some(_) => return None,
        };
        Some(DetectedLegacyProfile {
            descriptor,
            request_lifecycle,
        })
    })
}

#[cfg(test)]
pub(super) fn additive_import<'a>(
    profile_id: &'static str,
    source: &'a mut LegacyReadSession,
    target: &'a PersistenceHandle,
    request_lifecycle: bool,
) -> ImportFuture<'a> {
    Box::pin(import_additive_v1(
        profile_id,
        source,
        target,
        None,
        request_lifecycle,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_001_accepts_only_absent_or_complete_request_lifecycle_capability() {
        let descriptor = &profile_001::DESCRIPTOR;
        let base_only = by_fingerprint(&LegacySchemaFingerprint {
            base_hash: descriptor.base_schema_hash.to_string(),
            request_lifecycle_hash: None,
        })
        .expect("base profile");
        assert_eq!(base_only.base_schema_hash(), descriptor.base_schema_hash);
        assert_eq!(base_only.request_lifecycle_schema_hash(), None);

        let with_capability = by_fingerprint(&LegacySchemaFingerprint {
            base_hash: descriptor.base_schema_hash.to_string(),
            request_lifecycle_hash: Some(descriptor.request_lifecycle_schema_hash.to_string()),
        })
        .expect("capability profile");
        assert_eq!(
            with_capability.request_lifecycle_schema_hash(),
            Some(descriptor.request_lifecycle_schema_hash)
        );

        assert!(by_fingerprint(&LegacySchemaFingerprint {
            base_hash: descriptor.base_schema_hash.to_string(),
            request_lifecycle_hash: Some("unknown-capability".to_string()),
        })
        .is_none());
    }
}
