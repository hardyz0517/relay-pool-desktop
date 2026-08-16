use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    app_runtime_events::EVENT_DESCRIPTORS as APP_EVENT_DESCRIPTORS,
    commands::runtime_events::EVENT_DESCRIPTORS as FRONTEND_EVENT_DESCRIPTORS,
    ipc::runtime_events::EVENT_DESCRIPTORS as IPC_EVENT_DESCRIPTORS,
    outbound::runtime_events::EVENT_DESCRIPTORS as OUTBOUND_EVENT_DESCRIPTORS,
    persistence::runtime_events::EVENT_DESCRIPTORS as PERSISTENCE_EVENT_DESCRIPTORS,
    services::{
        monitoring::runtime_events::EVENT_DESCRIPTORS as MONITORING_EVENT_DESCRIPTORS,
        portable_migration::runtime_events::EVENT_DESCRIPTORS as MIGRATION_EVENT_DESCRIPTORS,
        proxy::runtime_events::EVENT_DESCRIPTORS as PROXY_EVENT_DESCRIPTORS,
        station_collectors::runtime_events::EVENT_DESCRIPTORS as COLLECTOR_EVENT_DESCRIPTORS,
        updater::runtime_events::EVENT_DESCRIPTORS as UPDATER_EVENT_DESCRIPTORS,
    },
};

use super::descriptor::{EventDescriptor, Lifecycle, SamplingPolicy};
use super::event::{Component, DetailKind, EventLevel, EventOutcome, RuntimeEvent};
use super::runtime_events::EVENT_DESCRIPTORS as RUNTIME_EVENT_DESCRIPTORS;
use super::subject::{is_stable_token, SubjectKind};

pub(crate) const CATALOG_MANIFEST_VERSION: u16 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ManifestEventDescriptor {
    pub(crate) code: String,
    pub(crate) owner: String,
    pub(crate) event_schema_version: u16,
    pub(crate) detail_schema_version: u16,
    pub(crate) component: Component,
    pub(crate) level: EventLevel,
    pub(crate) outcomes: Vec<EventOutcome>,
    pub(crate) details: Vec<DetailKind>,
    pub(crate) subjects: Vec<SubjectKind>,
    pub(crate) sampling: SamplingPolicy,
    pub(crate) support_bundle: bool,
    pub(crate) message_key: String,
    pub(crate) lifecycle: ManifestLifecycle,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub(crate) enum ManifestLifecycle {
    Active,
    Deprecated {
        replaced_by: String,
        sunset_version: u16,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CatalogManifest {
    pub(crate) manifest_version: u16,
    pub(crate) manifest_id: String,
    pub(crate) events: Vec<ManifestEventDescriptor>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CatalogError {
    EmptyCode,
    InvalidCode,
    InvalidOwner,
    InvalidMessageKey,
    UnsupportedSchema,
    MissingOutcome,
    MissingDetail,
    DuplicateCode,
    UnknownReplacement,
    ReplacementCycle,
}

impl std::fmt::Display for CatalogError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::EmptyCode => "catalog event code is empty",
            Self::InvalidCode => "catalog event code is not a stable token",
            Self::InvalidOwner => "catalog owner is not a stable token",
            Self::InvalidMessageKey => "catalog message key is not a stable token",
            Self::UnsupportedSchema => "catalog schema version is unsupported",
            Self::MissingOutcome => "catalog event has no allowed outcome",
            Self::MissingDetail => "catalog event has no allowed detail",
            Self::DuplicateCode => "catalog contains duplicate event code",
            Self::UnknownReplacement => "catalog replacement does not exist",
            Self::ReplacementCycle => "catalog replacement chain contains a cycle",
        })
    }
}

impl std::error::Error for CatalogError {}

pub(crate) struct Catalog;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EventCompatibility {
    pub(crate) message_key: String,
    pub(crate) replaced_by: Option<String>,
    pub(crate) manifest_source: ManifestSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ManifestSource {
    Current,
    Previous,
}

impl Catalog {
    /// Resolve a compiled owner descriptor for contract tests. Production
    /// producers receive their descriptor from an owner-local named handle.
    #[cfg(test)]
    pub(crate) fn descriptor(code: &'static str) -> &'static EventDescriptor {
        OWNER_EVENT_DESCRIPTOR_SLICES
            .iter()
            .flat_map(|slice| slice.iter())
            .find(|descriptor| descriptor.code == code)
            .unwrap_or_else(|| panic!("runtime event descriptor is missing: {code}"))
    }

    pub(crate) fn accepts_event(event: &RuntimeEvent) -> bool {
        let Some(descriptor) = OWNER_EVENT_DESCRIPTOR_SLICES
            .iter()
            .flat_map(|slice| slice.iter())
            .find(|descriptor| descriptor.code == event.event_code.as_str())
        else {
            return false;
        };
        descriptor.level == event.level
            && descriptor.component == event.component
            && descriptor.outcomes.contains(&event.outcome)
            && descriptor.details.contains(&event.detail.kind())
            && event.subject.as_ref().map_or_else(
                || descriptor.subjects.contains(&SubjectKind::None),
                |subject| descriptor.subjects.contains(&subject.kind()),
            )
    }

    /// Resolve the descriptor that was declared for the manifest recorded by
    /// a segment. Only the current build and the validated current/previous
    /// snapshots are accepted; arbitrary files in the log directory cannot
    /// expand the compatibility set.
    pub(crate) fn compatibility_for_event(
        root: &Path,
        manifest_id: &str,
        event: &RuntimeEvent,
    ) -> Option<EventCompatibility> {
        let (manifest, manifest_source) = manifest_for_id(root, manifest_id)?;
        let descriptor = manifest
            .events
            .iter()
            .find(|descriptor| descriptor.code == event.event_code.as_str())?;
        if descriptor.event_schema_version != event.schema_version
            || descriptor.component != event.component
            || descriptor.level != event.level
            || !descriptor.outcomes.contains(&event.outcome)
            || !descriptor.details.contains(&event.detail.kind())
            || !event.subject.as_ref().map_or_else(
                || descriptor.subjects.contains(&SubjectKind::None),
                |subject| descriptor.subjects.contains(&subject.kind()),
            )
        {
            return None;
        }
        let replaced_by = match &descriptor.lifecycle {
            ManifestLifecycle::Active => None,
            ManifestLifecycle::Deprecated { replaced_by, .. } => Some(replaced_by.clone()),
        };
        Some(EventCompatibility {
            message_key: descriptor.message_key.clone(),
            replaced_by,
            manifest_source,
        })
    }

    pub(crate) fn build(
        owner_slices: &[&'static [EventDescriptor]],
    ) -> Result<CatalogManifest, CatalogError> {
        let mut by_code = BTreeMap::<&'static str, EventDescriptor>::new();
        for slice in owner_slices {
            for descriptor in *slice {
                validate_descriptor(descriptor)?;
                if by_code.insert(descriptor.code, *descriptor).is_some() {
                    return Err(CatalogError::DuplicateCode);
                }
            }
        }

        for descriptor in by_code.values() {
            if let Lifecycle::Deprecated { replaced_by, .. } = descriptor.lifecycle {
                if !by_code.contains_key(replaced_by) {
                    return Err(CatalogError::UnknownReplacement);
                }
                let mut seen = BTreeSet::new();
                let mut current = descriptor.code;
                while let Some(next) =
                    by_code
                        .get(current)
                        .and_then(|entry| match entry.lifecycle {
                            Lifecycle::Deprecated { replaced_by, .. } => Some(replaced_by),
                            Lifecycle::Active => None,
                        })
                {
                    if !seen.insert(current) {
                        return Err(CatalogError::ReplacementCycle);
                    }
                    current = next;
                }
            }
        }

        let events = by_code
            .values()
            .map(|descriptor| ManifestEventDescriptor {
                code: descriptor.code.to_owned(),
                owner: descriptor.owner.to_owned(),
                event_schema_version: descriptor.event_schema_version,
                detail_schema_version: descriptor.detail_schema_version,
                component: descriptor.component,
                level: descriptor.level,
                outcomes: descriptor.outcomes.to_vec(),
                details: descriptor.details.to_vec(),
                subjects: descriptor.subjects.to_vec(),
                sampling: descriptor.sampling,
                support_bundle: descriptor.support_bundle,
                message_key: descriptor.message_key.to_owned(),
                lifecycle: manifest_lifecycle(descriptor.lifecycle),
            })
            .collect::<Vec<_>>();
        validate_manifest_events(&events)?;
        let unsigned = serde_json::to_vec(&serde_json::json!({
            "manifestVersion": CATALOG_MANIFEST_VERSION,
            "events": events,
        }))
        .expect("catalog manifest is serializable");
        let digest = Sha256::digest(unsigned);
        Ok(CatalogManifest {
            manifest_version: CATALOG_MANIFEST_VERSION,
            manifest_id: format!("{digest:x}"),
            events,
        })
    }

    pub(crate) fn core_manifest_id() -> String {
        Self::build(OWNER_EVENT_DESCRIPTOR_SLICES)
            .map(|manifest| manifest.manifest_id)
            .unwrap_or_else(|_| "invalid-runtime-manifest".to_owned())
    }

    /// Validate a persisted manifest snapshot before allowing it to explain
    /// historical segments. The id is recomputed from the unsigned payload so
    /// an arbitrary file in the log directory cannot widen the compatibility
    /// set.
    pub(crate) fn validate_snapshot(bytes: &[u8]) -> Option<String> {
        let manifest = serde_json::from_slice::<CatalogManifest>(bytes).ok()?;
        if manifest.manifest_version != CATALOG_MANIFEST_VERSION {
            return None;
        }
        validate_manifest_events(&manifest.events).ok()?;
        let unsigned = serde_json::to_vec(&serde_json::json!({
            "manifestVersion": manifest.manifest_version,
            "events": manifest.events,
        }))
        .ok()?;
        let digest = Sha256::digest(unsigned);
        let expected = format!("{digest:x}");
        (manifest.manifest_id == expected).then_some(expected)
    }
}

fn manifest_for_id(root: &Path, manifest_id: &str) -> Option<(CatalogManifest, ManifestSource)> {
    let current = Catalog::build(OWNER_EVENT_DESCRIPTOR_SLICES).ok()?;
    if manifest_id == current.manifest_id || {
        #[cfg(test)]
        {
            manifest_id == "runtime-test-manifest-v1"
        }
        #[cfg(not(test))]
        {
            false
        }
    } {
        return Some((current, ManifestSource::Current));
    }
    for (name, source) in [
        ("manifest.json", ManifestSource::Current),
        ("manifest.previous.json", ManifestSource::Previous),
    ] {
        let Ok(bytes) = std::fs::read(root.join(name)) else {
            continue;
        };
        let Some(validated_id) = Catalog::validate_snapshot(&bytes) else {
            continue;
        };
        if validated_id != manifest_id {
            continue;
        }
        return serde_json::from_slice(&bytes)
            .ok()
            .map(|manifest| (manifest, source));
    }
    None
}

fn manifest_lifecycle(lifecycle: Lifecycle) -> ManifestLifecycle {
    match lifecycle {
        Lifecycle::Active => ManifestLifecycle::Active,
        Lifecycle::Deprecated {
            replaced_by,
            sunset_version,
        } => ManifestLifecycle::Deprecated {
            replaced_by: replaced_by.to_owned(),
            sunset_version,
        },
    }
}

fn validate_manifest_events(events: &[ManifestEventDescriptor]) -> Result<(), CatalogError> {
    if events.is_empty() {
        return Err(CatalogError::MissingOutcome);
    }
    let mut codes = BTreeSet::new();
    for event in events {
        if !codes.insert(event.code.as_str()) {
            return Err(CatalogError::DuplicateCode);
        }
        if event.code.is_empty() || !is_stable_token(&event.code) {
            return Err(if event.code.is_empty() {
                CatalogError::EmptyCode
            } else {
                CatalogError::InvalidCode
            });
        }
        if !is_stable_token(&event.owner) {
            return Err(CatalogError::InvalidOwner);
        }
        if !is_stable_token(&event.message_key) {
            return Err(CatalogError::InvalidMessageKey);
        }
        if event.event_schema_version != 1 || event.detail_schema_version != 1 {
            return Err(CatalogError::UnsupportedSchema);
        }
        if event.outcomes.is_empty() {
            return Err(CatalogError::MissingOutcome);
        }
        if event.details.is_empty() {
            return Err(CatalogError::MissingDetail);
        }
    }
    for event in events {
        let ManifestLifecycle::Deprecated { replaced_by, .. } = &event.lifecycle else {
            continue;
        };
        let Some(_) = events
            .iter()
            .find(|candidate| candidate.code == *replaced_by)
        else {
            return Err(CatalogError::UnknownReplacement);
        };
        let mut seen = BTreeSet::new();
        let mut current = event.code.as_str();
        while let Some(next) = events
            .iter()
            .find(|candidate| candidate.code == current)
            .and_then(|candidate| match &candidate.lifecycle {
                ManifestLifecycle::Deprecated { replaced_by, .. } => Some(replaced_by.as_str()),
                ManifestLifecycle::Active => None,
            })
        {
            if !seen.insert(current) {
                return Err(CatalogError::ReplacementCycle);
            }
            current = next;
        }
    }
    Ok(())
}

fn validate_descriptor(descriptor: &EventDescriptor) -> Result<(), CatalogError> {
    if descriptor.code.is_empty() {
        return Err(CatalogError::EmptyCode);
    }
    if !is_stable_token(descriptor.code) {
        return Err(CatalogError::InvalidCode);
    }
    if !is_stable_token(descriptor.owner) {
        return Err(CatalogError::InvalidOwner);
    }
    if !is_stable_token(descriptor.message_key) {
        return Err(CatalogError::InvalidMessageKey);
    }
    if descriptor.event_schema_version != 1 || descriptor.detail_schema_version != 1 {
        return Err(CatalogError::UnsupportedSchema);
    }
    if descriptor.outcomes.is_empty() {
        return Err(CatalogError::MissingOutcome);
    }
    if descriptor.details.is_empty() {
        return Err(CatalogError::MissingDetail);
    }
    Ok(())
}

pub(crate) const OWNER_EVENT_DESCRIPTOR_SLICES: &[&[EventDescriptor]] = &[
    RUNTIME_EVENT_DESCRIPTORS,
    APP_EVENT_DESCRIPTORS,
    FRONTEND_EVENT_DESCRIPTORS,
    IPC_EVENT_DESCRIPTORS,
    COLLECTOR_EVENT_DESCRIPTORS,
    MIGRATION_EVENT_DESCRIPTORS,
    MONITORING_EVENT_DESCRIPTORS,
    OUTBOUND_EVENT_DESCRIPTORS,
    PERSISTENCE_EVENT_DESCRIPTORS,
    PROXY_EVENT_DESCRIPTORS,
    UPDATER_EVENT_DESCRIPTORS,
];

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use sha2::{Digest, Sha256};

    use super::{Catalog, ManifestLifecycle, OWNER_EVENT_DESCRIPTOR_SLICES};

    #[test]
    fn emit_runtime_event_catalog() {
        let Some(output_dir) = std::env::var_os("RELAY_POOL_RUNTIME_EVENT_CATALOG_OUT") else {
            return;
        };
        let output_dir = PathBuf::from(output_dir);
        fs::create_dir_all(&output_dir).expect("catalog generator output directory");
        let manifest = Catalog::build(OWNER_EVENT_DESCRIPTOR_SLICES).expect("runtime catalog");
        let mut bytes = serde_json::to_vec_pretty(&manifest).expect("runtime catalog json");
        bytes.push(b'\n');
        fs::write(output_dir.join("runtime-event-catalog.v1.json"), bytes)
            .expect("runtime catalog artifact");
    }

    #[test]
    fn validates_previous_manifest_with_deprecated_replacement() {
        let baseline = Catalog::build(OWNER_EVENT_DESCRIPTOR_SLICES).expect("runtime catalog");
        let mut active = baseline.events[0].clone();
        active.code = "runtime.compat.current".to_owned();
        active.message_key = active.code.clone();
        active.lifecycle = ManifestLifecycle::Active;
        let mut deprecated = active.clone();
        deprecated.code = "runtime.compat.previous".to_owned();
        deprecated.message_key = deprecated.code.clone();
        deprecated.lifecycle = ManifestLifecycle::Deprecated {
            replaced_by: active.code.clone(),
            sunset_version: 2,
        };
        let events = vec![active, deprecated];
        let unsigned = serde_json::to_vec(&serde_json::json!({
            "manifestVersion": 1,
            "events": events,
        }))
        .expect("unsigned manifest");
        let manifest_id = format!("{:x}", Sha256::digest(unsigned));
        let bytes = serde_json::to_vec(&serde_json::json!({
            "manifestVersion": 1,
            "manifestId": manifest_id,
            "events": events,
        }))
        .expect("manifest");
        assert!(Catalog::validate_snapshot(&bytes).is_some());
    }

    #[test]
    fn rejects_previous_manifest_with_unknown_replacement_even_with_valid_hash() {
        let baseline = Catalog::build(OWNER_EVENT_DESCRIPTOR_SLICES).expect("runtime catalog");
        let mut event = baseline.events[0].clone();
        event.code = "runtime.compat.previous".to_owned();
        event.message_key = event.code.clone();
        event.lifecycle = ManifestLifecycle::Deprecated {
            replaced_by: "runtime.compat.missing".to_owned(),
            sunset_version: 2,
        };
        let events = vec![event];
        let unsigned = serde_json::to_vec(&serde_json::json!({
            "manifestVersion": 1,
            "events": events,
        }))
        .expect("unsigned manifest");
        let manifest_id = format!("{:x}", Sha256::digest(unsigned));
        let bytes = serde_json::to_vec(&serde_json::json!({
            "manifestVersion": 1,
            "manifestId": manifest_id,
            "events": events,
        }))
        .expect("manifest");
        assert!(Catalog::validate_snapshot(&bytes).is_none());
    }

    #[test]
    fn rejects_previous_manifest_with_unsupported_event_schema() {
        let baseline = Catalog::build(OWNER_EVENT_DESCRIPTOR_SLICES).expect("runtime catalog");
        let mut event = baseline.events[0].clone();
        event.event_schema_version = 2;
        let events = vec![event];
        let unsigned = serde_json::to_vec(&serde_json::json!({
            "manifestVersion": 1,
            "events": events,
        }))
        .expect("unsigned manifest");
        let manifest_id = format!("{:x}", Sha256::digest(unsigned));
        let bytes = serde_json::to_vec(&serde_json::json!({
            "manifestVersion": 1,
            "manifestId": manifest_id,
            "events": events,
        }))
        .expect("manifest");
        assert!(Catalog::validate_snapshot(&bytes).is_none());
    }

    #[test]
    fn rejects_replacement_cycles_even_with_valid_hash() {
        let baseline = Catalog::build(OWNER_EVENT_DESCRIPTOR_SLICES).expect("runtime catalog");
        let mut first = baseline.events[0].clone();
        first.code = "runtime.compat.first".to_owned();
        first.message_key = first.code.clone();
        first.lifecycle = ManifestLifecycle::Deprecated {
            replaced_by: "runtime.compat.second".to_owned(),
            sunset_version: 2,
        };
        let mut second = first.clone();
        second.code = "runtime.compat.second".to_owned();
        second.message_key = second.code.clone();
        second.lifecycle = ManifestLifecycle::Deprecated {
            replaced_by: first.code.clone(),
            sunset_version: 2,
        };
        let events = vec![first, second];
        let unsigned = serde_json::to_vec(&serde_json::json!({
            "manifestVersion": 1,
            "events": events,
        }))
        .expect("unsigned manifest");
        let manifest_id = format!("{:x}", Sha256::digest(unsigned));
        let bytes = serde_json::to_vec(&serde_json::json!({
            "manifestVersion": 1,
            "manifestId": manifest_id,
            "events": events,
        }))
        .expect("manifest");
        assert!(Catalog::validate_snapshot(&bytes).is_none());
    }
}
