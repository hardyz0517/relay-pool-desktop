use super::LegacyProfileDescriptor;
#[cfg(test)]
use super::{additive_import, ImportFuture};
#[cfg(test)]
use crate::persistence::{legacy_import::LegacyReadSession, runtime::PersistenceHandle};

pub(super) const DESCRIPTOR: LegacyProfileDescriptor = LegacyProfileDescriptor {
    id: "profile_001",
    base_schema_hash: "0ed1e6119418312648ca378d80abfe016f24d5bd7df1cb72006bd8ab39b69358",
    request_lifecycle_schema_hash:
        "e04a8e99ad24a1f218de9d095ba83999095868d25fdb2751fdc2c576bbc48ba1",
    #[cfg(test)]
    import,
};

#[cfg(test)]
fn import<'a>(
    source: &'a mut LegacyReadSession,
    target: &'a PersistenceHandle,
    request_lifecycle: bool,
) -> ImportFuture<'a> {
    additive_import(DESCRIPTOR.id, source, target, request_lifecycle)
}
