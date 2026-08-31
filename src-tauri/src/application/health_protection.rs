//! Routing protection policy and historical observation compatibility types.
//!
//! Station-key circuit transitions and Half-Open leases are owned by routing
//! v3. The scope type remains because supported databases can contain legacy
//! `probe_scope` evidence, while the V1 profile is still the compiled-policy
//! representation used during configuration upgrades.

use serde::{Deserialize, Serialize};

pub(crate) const HEALTH_PROTECTION_VERSION: &str = "health_protection_v1";
const MAX_PROFILE_SAMPLES: usize = 256;
const MAX_PROFILE_ENTRIES: usize = 4_096;
const MAX_PROFILE_WINDOW_MS: i64 = 24 * 60 * 60 * 1_000;
const MAX_PROFILE_COOLDOWN_MS: i64 = 24 * 60 * 60 * 1_000;

fn default_health_protection_enabled() -> bool {
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum HealthProtectionPreset {
    Conservative,
    Balanced,
    Aggressive,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HealthProtectionProfileV1 {
    pub(crate) version: String,
    #[serde(default = "default_health_protection_enabled")]
    pub(crate) enabled: bool,
    pub(crate) preset: HealthProtectionPreset,
    pub(crate) window_max_samples: usize,
    pub(crate) window_ms: i64,
    pub(crate) min_samples: usize,
    pub(crate) failure_threshold_percent: u8,
    pub(crate) base_cooldown_ms: i64,
    pub(crate) max_cooldown_ms: i64,
    pub(crate) half_open_successes_to_close: u8,
    pub(crate) max_entries: usize,
}

impl HealthProtectionProfileV1 {
    pub(crate) fn from_policy_config(
        config: &crate::models::routing_policy::ProtectionProfileConfigV2,
    ) -> Result<Self, HealthProtectionError> {
        config
            .validate()
            .map_err(|_| HealthProtectionError::InvalidProfile)?;
        let mut profile = Self::for_preset(HealthProtectionPreset::Balanced);
        profile.window_max_samples = usize::from(config.window_max_samples);
        profile.window_ms = config.window_millis();
        profile.min_samples = usize::from(config.min_samples);
        profile.failure_threshold_percent = config.failure_threshold_percent;
        profile.half_open_successes_to_close = config.half_open_successes_to_close;
        profile.enabled = config.enabled;
        profile.validate()?;
        Ok(profile)
    }

    pub(crate) fn for_preset(preset: HealthProtectionPreset) -> Self {
        let (
            window_max_samples,
            min_samples,
            threshold,
            base_cooldown_ms,
            max_cooldown_ms,
            half_open_successes_to_close,
        ) = match preset {
            HealthProtectionPreset::Conservative => (32, 5, 50, 60_000, 15 * 60 * 1_000, 3),
            HealthProtectionPreset::Balanced => (64, 5, 60, 30_000, 15 * 60 * 1_000, 2),
            HealthProtectionPreset::Aggressive => (128, 8, 75, 15_000, 5 * 60 * 1_000, 1),
        };
        Self {
            version: HEALTH_PROTECTION_VERSION.to_string(),
            enabled: true,
            preset,
            window_max_samples,
            window_ms: 5 * 60 * 1_000,
            min_samples,
            failure_threshold_percent: threshold,
            base_cooldown_ms,
            max_cooldown_ms,
            half_open_successes_to_close,
            max_entries: 1_024,
        }
    }

    pub(crate) fn validate(&self) -> Result<(), HealthProtectionError> {
        if self.version != HEALTH_PROTECTION_VERSION {
            return Err(HealthProtectionError::UnsupportedVersion);
        }
        if self.window_max_samples == 0 || self.window_max_samples > MAX_PROFILE_SAMPLES {
            return Err(HealthProtectionError::InvalidProfile);
        }
        if self.window_ms <= 0 || self.window_ms > MAX_PROFILE_WINDOW_MS {
            return Err(HealthProtectionError::InvalidProfile);
        }
        if self.min_samples == 0 || self.min_samples > self.window_max_samples {
            return Err(HealthProtectionError::InvalidProfile);
        }
        if !(1..=100).contains(&self.failure_threshold_percent) {
            return Err(HealthProtectionError::InvalidProfile);
        }
        if self.base_cooldown_ms <= 0
            || self.max_cooldown_ms < self.base_cooldown_ms
            || self.max_cooldown_ms > MAX_PROFILE_COOLDOWN_MS
        {
            return Err(HealthProtectionError::InvalidProfile);
        }
        if self.half_open_successes_to_close == 0
            || self.max_entries == 0
            || self.max_entries > MAX_PROFILE_ENTRIES
        {
            return Err(HealthProtectionError::InvalidProfile);
        }
        Ok(())
    }
}

impl Default for HealthProtectionProfileV1 {
    fn default() -> Self {
        Self::for_preset(HealthProtectionPreset::Balanced)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum HealthProtectionScopeKind {
    Credential,
    Account,
    Group,
    Endpoint,
    Model,
    CapacityDomain,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HealthProtectionScope {
    pub(crate) kind: HealthProtectionScopeKind,
    /// Historical evidence stores a commitment, never a raw endpoint,
    /// account, credential, or request identity.
    pub(crate) commitment: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HealthProtectionError {
    UnsupportedVersion,
    InvalidProfile,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_profile_is_valid() {
        HealthProtectionProfileV1::default()
            .validate()
            .expect("default profile must remain valid");
    }

    #[test]
    fn historical_probe_scope_shape_remains_decodable() {
        let scope: HealthProtectionScope = serde_json::from_value(serde_json::json!({
            "kind": "capacity_domain",
            "commitment": "capacity_domain:v1:fixture"
        }))
        .expect("decode historical probe scope");
        assert_eq!(scope.kind, HealthProtectionScopeKind::CapacityDomain);
        assert_eq!(scope.commitment, "capacity_domain:v1:fixture");
    }
}
