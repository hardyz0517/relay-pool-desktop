use std::{future::Future, pin::Pin};

use crate::persistence::runtime::PersistenceHandle;

use super::{import::import_additive_v1, LegacyReadSession, UpgradeError};

mod profile_001;

pub(crate) type ImportFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), UpgradeError>> + Send + 'a>>;

pub(crate) struct LegacyProfileDescriptor {
    pub(crate) id: &'static str,
    pub(crate) schema_hash: &'static str,
    pub(crate) import:
        for<'a> fn(&'a mut LegacyReadSession, &'a PersistenceHandle) -> ImportFuture<'a>,
}

#[derive(Clone, Copy)]
pub(crate) struct DetectedLegacyProfile {
    pub(super) descriptor: &'static LegacyProfileDescriptor,
}

impl DetectedLegacyProfile {
    pub(crate) fn id(self) -> &'static str {
        self.descriptor.id
    }

    pub(crate) fn schema_hash(self) -> &'static str {
        self.descriptor.schema_hash
    }
}

pub(crate) fn all() -> &'static [LegacyProfileDescriptor] {
    &[
        profile_001::DESCRIPTOR,
    ]
}

pub(super) fn by_schema_hash(schema_hash: &str) -> Option<DetectedLegacyProfile> {
    all()
        .iter()
        .find(|profile| profile.schema_hash == schema_hash)
        .map(|descriptor| DetectedLegacyProfile { descriptor })
}

pub(super) fn additive_import<'a>(
    profile_id: &'static str,
    source: &'a mut LegacyReadSession,
    target: &'a PersistenceHandle,
) -> ImportFuture<'a> {
    Box::pin(import_additive_v1(profile_id, source, target, None))
}
