use std::{collections::HashSet, fmt};

use serde::Serialize;
use sha2::{Digest, Sha256};

pub const STATION_PUBLISHED_STATUS_SOURCE_KIND: &str = "sub2api_channel_monitors";
pub const MAX_PUBLISHED_STATUS_MONITORS: usize = 512;
pub const MAX_PUBLISHED_STATUS_SAMPLES_PER_MODEL: usize = 60;
pub const MAX_PUBLISHED_STATUS_TIMELINE_INPUT: usize = 240;
pub const MAX_PUBLISHED_STATUS_EXTRA_MODELS: usize = 32;
pub const MAX_PUBLISHED_STATUS_RESPONSE_BODY_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_PUBLISHED_STATUS_STATION_ID_BYTES: usize = 128;
pub const MAX_PUBLISHED_STATUS_MONITOR_ID_BYTES: usize = 128;
pub const MAX_PUBLISHED_STATUS_NAME_BYTES: usize = 128;
pub const MAX_PUBLISHED_STATUS_PROVIDER_BYTES: usize = 128;
pub const MAX_PUBLISHED_STATUS_GROUP_BYTES: usize = 128;
pub const MAX_PUBLISHED_STATUS_MODEL_BYTES: usize = 128;
pub const MAX_PUBLISHED_STATUS_SOURCE_STATUS_BYTES: usize = 64;
pub const MAX_PUBLISHED_STATUS_SAFE_MESSAGE_BYTES: usize = 512;
pub const MAX_PUBLISHED_STATUS_LATENCY_MS: i64 = 3_600_000;
pub const MAX_PUBLISHED_STATUS_TIMESTAMP_MS: i64 = 8_640_000_000_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub enum PublishedStatusSourceState {
    Available,
    Empty,
    Unsupported,
    AuthorizationRequired,
    Degraded,
    Failed,
}

impl PublishedStatusSourceState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Available => "available",
            Self::Empty => "empty",
            Self::Unsupported => "unsupported",
            Self::AuthorizationRequired => "authorization_required",
            Self::Degraded => "degraded",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub enum PublishedStatusCompleteness {
    Complete,
    Partial,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub enum PublishedSampleOutcome {
    Available,
    Degraded,
    Unavailable,
    Unknown,
}

impl PublishedSampleOutcome {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Available => "available",
            Self::Degraded => "degraded",
            Self::Unavailable => "unavailable",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub enum PublishedMonitorIdentityKind {
    UpstreamId,
    DerivedFallback,
}

impl PublishedMonitorIdentityKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UpstreamId => "upstream_id",
            Self::DerivedFallback => "derived_fallback",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PublishedMonitorSampleFact {
    pub model: String,
    pub outcome: PublishedSampleOutcome,
    pub source_status: String,
    pub latency_ms: Option<i64>,
    pub ping_latency_ms: Option<i64>,
    pub checked_at_ms: i64,
    pub safe_message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PublishedMonitorFact {
    pub upstream_monitor_id: String,
    pub identity_kind: PublishedMonitorIdentityKind,
    pub name: String,
    pub provider: String,
    pub group_name: Option<String>,
    pub primary_model: String,
    pub extra_models: Vec<String>,
    pub current_outcome: PublishedSampleOutcome,
    pub source_status: String,
    pub current_latency_ms: Option<i64>,
    pub current_ping_latency_ms: Option<i64>,
    pub upstream_checked_at_ms: Option<i64>,
    pub samples: Vec<PublishedMonitorSampleFact>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PublishedStatusBatch {
    pub station_id: String,
    pub endpoint_revision: i64,
    pub source_kind: String,
    pub source_state: PublishedStatusSourceState,
    pub completeness: PublishedStatusCompleteness,
    pub monitors: Vec<PublishedMonitorFact>,
    pub collected_at_ms: i64,
    pub safe_error_kind: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PublishedStatusValidationError {
    EmptyField(&'static str),
    FieldTooLong {
        field: &'static str,
        max_bytes: usize,
    },
    ControlCharacter(&'static str),
    InvalidEndpointRevision,
    InvalidCollectedAt,
    TooManyMonitors(usize),
    DuplicateMonitorIdentity(String),
    TooManyExtraModels(usize),
    DuplicateExtraModel(String),
    InvalidLatency(&'static str),
    InvalidCheckedAt,
    TooManySamples(usize),
    UnorderedSamples,
    DuplicateSample {
        model: String,
        checked_at_ms: i64,
    },
}

impl fmt::Display for PublishedStatusValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyField(field) => write!(formatter, "{field} must not be empty"),
            Self::FieldTooLong { field, max_bytes } => {
                write!(formatter, "{field} exceeds {max_bytes} bytes")
            }
            Self::ControlCharacter(field) => {
                write!(formatter, "{field} contains a control character")
            }
            Self::InvalidEndpointRevision => {
                formatter.write_str("endpoint revision must be positive")
            }
            Self::InvalidCollectedAt => formatter.write_str("collected time must be non-negative"),
            Self::TooManyMonitors(value) => write!(
                formatter,
                "monitor count exceeds {MAX_PUBLISHED_STATUS_MONITORS}: {value}"
            ),
            Self::DuplicateMonitorIdentity(value) => {
                write!(formatter, "duplicate monitor identity: {value}")
            }
            Self::TooManyExtraModels(value) => write!(
                formatter,
                "extra model count exceeds {MAX_PUBLISHED_STATUS_EXTRA_MODELS}: {value}"
            ),
            Self::DuplicateExtraModel(value) => write!(formatter, "duplicate extra model: {value}"),
            Self::InvalidLatency(field) => write!(
                formatter,
                "{field} must be within 0..={MAX_PUBLISHED_STATUS_LATENCY_MS} milliseconds"
            ),
            Self::InvalidCheckedAt => formatter.write_str("checked time must be non-negative"),
            Self::TooManySamples(value) => write!(
                formatter,
                "sample count exceeds {MAX_PUBLISHED_STATUS_SAMPLES_PER_MODEL}: {value}"
            ),
            Self::UnorderedSamples => {
                formatter.write_str("samples must be ordered by checked time")
            }
            Self::DuplicateSample {
                model,
                checked_at_ms,
            } => write!(formatter, "duplicate sample: {model} at {checked_at_ms}"),
        }
    }
}

impl std::error::Error for PublishedStatusValidationError {}

impl PublishedStatusBatch {
    pub fn validate(&self) -> Result<(), PublishedStatusValidationError> {
        validate_required(
            &self.station_id,
            "station_id",
            MAX_PUBLISHED_STATUS_STATION_ID_BYTES,
        )?;
        if self.endpoint_revision < 1 {
            return Err(PublishedStatusValidationError::InvalidEndpointRevision);
        }
        validate_required(
            &self.source_kind,
            "source_kind",
            MAX_PUBLISHED_STATUS_SOURCE_STATUS_BYTES,
        )?;
        if self.collected_at_ms < 0 {
            return Err(PublishedStatusValidationError::InvalidCollectedAt);
        }
        if self.monitors.len() > MAX_PUBLISHED_STATUS_MONITORS {
            return Err(PublishedStatusValidationError::TooManyMonitors(
                self.monitors.len(),
            ));
        }
        if let Some(error_kind) = self.safe_error_kind.as_deref() {
            validate_required(
                error_kind,
                "safe_error_kind",
                MAX_PUBLISHED_STATUS_SOURCE_STATUS_BYTES,
            )?;
        }

        let mut identities = HashSet::with_capacity(self.monitors.len());
        for monitor in &self.monitors {
            monitor.validate()?;
            if !identities.insert(monitor.upstream_monitor_id.as_str()) {
                return Err(PublishedStatusValidationError::DuplicateMonitorIdentity(
                    monitor.upstream_monitor_id.clone(),
                ));
            }
        }
        Ok(())
    }
}

impl PublishedMonitorFact {
    pub fn validate(&self) -> Result<(), PublishedStatusValidationError> {
        validate_required(
            &self.upstream_monitor_id,
            "upstream_monitor_id",
            MAX_PUBLISHED_STATUS_MONITOR_ID_BYTES,
        )?;
        validate_required(&self.name, "name", MAX_PUBLISHED_STATUS_NAME_BYTES)?;
        validate_required(
            &self.provider,
            "provider",
            MAX_PUBLISHED_STATUS_PROVIDER_BYTES,
        )?;
        if let Some(group_name) = self.group_name.as_deref() {
            validate_required(group_name, "group_name", MAX_PUBLISHED_STATUS_GROUP_BYTES)?;
        }
        validate_required(
            &self.primary_model,
            "primary_model",
            MAX_PUBLISHED_STATUS_MODEL_BYTES,
        )?;
        validate_required(
            &self.source_status,
            "source_status",
            MAX_PUBLISHED_STATUS_SOURCE_STATUS_BYTES,
        )?;
        validate_latency(self.current_latency_ms, "current_latency_ms")?;
        validate_latency(self.current_ping_latency_ms, "current_ping_latency_ms")?;
        if let Some(checked_at_ms) = self.upstream_checked_at_ms {
            validate_checked_at(checked_at_ms)?;
        }
        if self.extra_models.len() > MAX_PUBLISHED_STATUS_EXTRA_MODELS {
            return Err(PublishedStatusValidationError::TooManyExtraModels(
                self.extra_models.len(),
            ));
        }
        let mut extra_models = HashSet::with_capacity(self.extra_models.len());
        for model in &self.extra_models {
            validate_required(model, "extra_model", MAX_PUBLISHED_STATUS_MODEL_BYTES)?;
            if !extra_models.insert(model.as_str()) {
                return Err(PublishedStatusValidationError::DuplicateExtraModel(
                    model.clone(),
                ));
            }
        }
        if self.samples.len() > MAX_PUBLISHED_STATUS_SAMPLES_PER_MODEL {
            return Err(PublishedStatusValidationError::TooManySamples(
                self.samples.len(),
            ));
        }

        let mut previous = None;
        let mut samples = HashSet::with_capacity(self.samples.len());
        for sample in &self.samples {
            sample.validate()?;
            let order_key = (sample.checked_at_ms, sample.model.as_str());
            if previous.is_some_and(|previous| previous > order_key) {
                return Err(PublishedStatusValidationError::UnorderedSamples);
            }
            previous = Some(order_key);
            let identity = (sample.model.as_str(), sample.checked_at_ms);
            if !samples.insert(identity) {
                return Err(PublishedStatusValidationError::DuplicateSample {
                    model: sample.model.clone(),
                    checked_at_ms: sample.checked_at_ms,
                });
            }
        }
        Ok(())
    }
}

impl PublishedMonitorSampleFact {
    pub fn validate(&self) -> Result<(), PublishedStatusValidationError> {
        validate_required(
            &self.model,
            "sample_model",
            MAX_PUBLISHED_STATUS_MODEL_BYTES,
        )?;
        validate_required(
            &self.source_status,
            "sample_source_status",
            MAX_PUBLISHED_STATUS_SOURCE_STATUS_BYTES,
        )?;
        validate_latency(self.latency_ms, "latency_ms")?;
        validate_latency(self.ping_latency_ms, "ping_latency_ms")?;
        validate_checked_at(self.checked_at_ms)?;
        if let Some(message) = self.safe_message.as_deref() {
            validate_required(
                message,
                "safe_message",
                MAX_PUBLISHED_STATUS_SAFE_MESSAGE_BYTES,
            )?;
        }
        Ok(())
    }
}

pub fn derived_monitor_identity(
    name: &str,
    provider: &str,
    group_name: Option<&str>,
    primary_model: &str,
) -> String {
    let seed = [
        normalize_identity_component(name),
        normalize_identity_component(provider),
        group_name
            .map(normalize_identity_component)
            .unwrap_or_default(),
        normalize_identity_component(primary_model),
    ]
    .join("\n");
    format!("derived:{:x}", Sha256::digest(seed.as_bytes()))
}

fn normalize_identity_component(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn validate_required(
    value: &str,
    field: &'static str,
    max_bytes: usize,
) -> Result<(), PublishedStatusValidationError> {
    if value.trim().is_empty() {
        return Err(PublishedStatusValidationError::EmptyField(field));
    }
    if value.len() > max_bytes {
        return Err(PublishedStatusValidationError::FieldTooLong { field, max_bytes });
    }
    if value.chars().any(char::is_control) {
        return Err(PublishedStatusValidationError::ControlCharacter(field));
    }
    Ok(())
}

fn validate_latency(
    value: Option<i64>,
    field: &'static str,
) -> Result<(), PublishedStatusValidationError> {
    if value.is_some_and(|value| !(0..=MAX_PUBLISHED_STATUS_LATENCY_MS).contains(&value)) {
        return Err(PublishedStatusValidationError::InvalidLatency(field));
    }
    Ok(())
}

fn validate_checked_at(value: i64) -> Result<(), PublishedStatusValidationError> {
    ((0..=MAX_PUBLISHED_STATUS_TIMESTAMP_MS).contains(&value))
        .then_some(())
        .ok_or(PublishedStatusValidationError::InvalidCheckedAt)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(checked_at_ms: i64) -> PublishedMonitorSampleFact {
        PublishedMonitorSampleFact {
            model: "gpt-fixture".to_string(),
            outcome: PublishedSampleOutcome::Available,
            source_status: "healthy".to_string(),
            latency_ms: Some(20),
            ping_latency_ms: Some(3),
            checked_at_ms,
            safe_message: None,
        }
    }

    fn monitor() -> PublishedMonitorFact {
        PublishedMonitorFact {
            upstream_monitor_id: "monitor-fixture".to_string(),
            identity_kind: PublishedMonitorIdentityKind::UpstreamId,
            name: "Fixture Monitor".to_string(),
            provider: "fixture".to_string(),
            group_name: Some("default".to_string()),
            primary_model: "gpt-fixture".to_string(),
            extra_models: vec!["gpt-extra".to_string()],
            current_outcome: PublishedSampleOutcome::Available,
            source_status: "healthy".to_string(),
            current_latency_ms: Some(20),
            current_ping_latency_ms: Some(3),
            upstream_checked_at_ms: Some(1_700_000_000_000),
            samples: vec![sample(1_700_000_000_000)],
        }
    }

    fn batch(monitors: Vec<PublishedMonitorFact>) -> PublishedStatusBatch {
        PublishedStatusBatch {
            station_id: "station-fixture".to_string(),
            endpoint_revision: 1,
            source_kind: STATION_PUBLISHED_STATUS_SOURCE_KIND.to_string(),
            source_state: PublishedStatusSourceState::Available,
            completeness: PublishedStatusCompleteness::Complete,
            monitors,
            collected_at_ms: 1_700_000_001_000,
            safe_error_kind: None,
        }
    }

    #[test]
    fn derived_identity_is_stable_and_marks_a_distinct_namespace() {
        let first =
            derived_monitor_identity("  Main   Monitor ", "OpenAI", Some("Default"), "GPT-4");
        let second = derived_monitor_identity("main monitor", "openai", Some("default"), "gpt-4");

        assert_eq!(first, second);
        assert!(first.starts_with("derived:"));
    }

    #[test]
    fn batch_rejects_duplicate_monitor_identity() {
        let first = monitor();
        let second = monitor();

        assert!(matches!(
            batch(vec![first, second]).validate(),
            Err(PublishedStatusValidationError::DuplicateMonitorIdentity(_))
        ));
    }

    #[test]
    fn monitor_rejects_unbounded_or_unsorted_samples() {
        let mut unordered = monitor();
        unordered.samples = vec![sample(2), sample(1)];
        assert_eq!(
            unordered.validate(),
            Err(PublishedStatusValidationError::UnorderedSamples)
        );

        let mut oversized = monitor();
        oversized.samples = (0..=MAX_PUBLISHED_STATUS_SAMPLES_PER_MODEL as i64)
            .map(sample)
            .collect();
        assert!(matches!(
            oversized.validate(),
            Err(PublishedStatusValidationError::TooManySamples(_))
        ));
    }

    #[test]
    fn monitor_field_and_timestamp_bounds_match_persistence_contract() {
        let mut too_long_name = monitor();
        too_long_name.name = "x".repeat(MAX_PUBLISHED_STATUS_NAME_BYTES + 1);
        assert!(matches!(
            too_long_name.validate(),
            Err(PublishedStatusValidationError::FieldTooLong { field: "name", .. })
        ));

        let mut out_of_range_sample = monitor();
        out_of_range_sample.samples = vec![sample(MAX_PUBLISHED_STATUS_TIMESTAMP_MS + 1)];
        assert_eq!(
            out_of_range_sample.validate(),
            Err(PublishedStatusValidationError::InvalidCheckedAt)
        );
    }

    #[test]
    fn safe_message_rejects_control_characters_and_overlong_values() {
        let mut invalid_control = sample(1);
        invalid_control.safe_message = Some("untrusted\u{0000}message".to_string());
        assert_eq!(
            invalid_control.validate(),
            Err(PublishedStatusValidationError::ControlCharacter(
                "safe_message"
            ))
        );

        let mut invalid_length = sample(1);
        invalid_length.safe_message = Some("x".repeat(MAX_PUBLISHED_STATUS_SAFE_MESSAGE_BYTES + 1));
        assert!(matches!(
            invalid_length.validate(),
            Err(PublishedStatusValidationError::FieldTooLong {
                field: "safe_message",
                ..
            })
        ));
    }
}
