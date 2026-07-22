use super::LegacyProfileDescriptor;
#[cfg(test)]
use super::{additive_import, ImportFuture};
#[cfg(test)]
use crate::persistence::{legacy_import::LegacyReadSession, runtime::PersistenceHandle};

pub(super) const DESCRIPTOR: LegacyProfileDescriptor = LegacyProfileDescriptor {
    id: "profile_001",
    schema_hash: "859a884555aa27bb9ac2bc726f12f68b32db6b6f3fa269b64f371c1462aff94b",
    #[cfg(test)]
    import,
};

#[cfg(test)]
fn import<'a>(
    source: &'a mut LegacyReadSession,
    target: &'a PersistenceHandle,
) -> ImportFuture<'a> {
    additive_import(DESCRIPTOR.id, source, target)
}
